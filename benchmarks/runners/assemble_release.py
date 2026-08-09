#!/usr/bin/env python3
"""Derive public tables and checksums from immutable peer run artifacts."""

from __future__ import annotations

import argparse
import csv
import hashlib
import importlib.metadata
import json
import platform
import shutil
import statistics
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

import tiktoken

from public_artifacts import (
    LOCAL_FILE_URI_TOKEN,
    PublicArtifactSanitizer,
    default_aliases,
    parse_alias,
    verify_release,
)


TOKENIZER_VERSION = importlib.metadata.version("tiktoken")
if TOKENIZER_VERSION != "0.12.0":
    raise RuntimeError(
        f"release assembly requires tiktoken 0.12.0, found {TOKENIZER_VERSION}"
    )
ENCODING = tiktoken.get_encoding("o200k_base")


def digest(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def regular_summary(run: dict[str, Any], source_file: str) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    optimizer = run["optimizer"]
    for surface in ("prompt_context", "tool_output"):
        relevant = [case for case in run.get("cases", []) if case.get("surface") == surface]
        eligible = [case for case in relevant if case.get("status") != "unsupported"]
        completed = [case for case in eligible if case.get("status") == "ok" and "score" in case]
        if not eligible:
            continue
        independent_counts = [
            (
                len(ENCODING.encode(str(case["input_text"]))),
                len(ENCODING.encode(str(case["output_text"]))),
            )
            for case in completed
        ]
        input_tokens = sum(before for before, _ in independent_counts)
        output_tokens = sum(after for _, after in independent_counts)
        token_delta = input_tokens - output_tokens
        latencies = [float(case.get("elapsed_ms", 0.0)) for case in completed]
        coverage_complete = len(completed) == len(eligible)
        preservation_passed = sum(
            case["score"]["preservation_proxy_pass"] for case in completed
        )
        preservation_complete = coverage_complete and preservation_passed == len(completed)
        raw_reduction = (
            round(100 * token_delta / input_tokens, 4) if input_tokens else 0.0
        )
        zero_reduction_cases = sum(
            case["score"]["token_delta"] == 0 for case in completed
        )
        worker_count_disagreements = sum(
            case["score"]["input"]["tokens"] != independent[0]
            or case["score"]["output"]["tokens"] != independent[1]
            for case, independent in zip(completed, independent_counts, strict=True)
        )
        rows.append(
            {
                "optimizer_id": optimizer["id"],
                "optimizer_name": optimizer["name"],
                "version": optimizer.get("version", "unknown"),
                "setting": optimizer.get("setting", "unknown"),
                "class": optimizer.get("class", "unknown"),
                "surface": surface,
                "eligible_cases": len(eligible),
                "completed_cases": len(completed),
                "failed_cases": sum(case.get("status") == "failed" for case in eligible),
                "input_tokens": input_tokens,
                "output_tokens": output_tokens,
                "token_delta": token_delta,
                "token_reduction_percent": raw_reduction,
                "raw_payload_reduction_percent": raw_reduction,
                "preservation_proxy_passed": preservation_passed,
                "negative_reduction_cases": sum(
                    case["score"]["token_delta"] < 0 for case in completed
                ),
                "zero_reduction_cases": zero_reduction_cases,
                "positive_reduction_cases": sum(
                    case["score"]["token_delta"] > 0 for case in completed
                ),
                "no_op_case_percent": (
                    round(100 * zero_reduction_cases / len(completed), 4)
                    if completed
                    else None
                ),
                "coverage_complete": coverage_complete,
                "preservation_complete": preservation_complete,
                "independent_recount": "tiktoken:o200k_base@0.12.0",
                "worker_count_disagreements": worker_count_disagreements,
                "qualified_payload_reduction_percent": (
                    raw_reduction if preservation_complete else None
                ),
                "headline_token_reduction_percent": (
                    raw_reduction if preservation_complete else None
                ),
                "median_latency_ms": round(statistics.median(latencies), 3)
                if latencies
                else None,
                "source_run": source_file,
            }
        )
    return rows


def fmt_percent(value: float) -> str:
    return f"{value:.2f}%"


def markdown_table(rows: list[dict[str, Any]]) -> str:
    lines = [
        "| Optimizer / setting | Completed / eligible | Input tokens | Output tokens | Raw payload reduction | Proxy-qualified reduction | Preservation proxy | No-op cases | Failures |",
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |",
    ]
    for row in rows:
        raw_reduction = (
            fmt_percent(row["raw_payload_reduction_percent"])
            if row["coverage_complete"]
            else "N/A (incomplete)"
        )
        qualified_reduction = (
            fmt_percent(row["qualified_payload_reduction_percent"])
            if row["qualified_payload_reduction_percent"] is not None
            else "Not qualified"
        )
        lines.append(
            "| {name} — `{setting}` | {done} / {eligible} | {before:,} | {after:,} | {raw_reduction} | {qualified_reduction} | {gates} / {done} | {no_ops} / {done} | {failures} |".format(
                name=row["optimizer_name"],
                setting=row["setting"],
                done=row["completed_cases"],
                eligible=row["eligible_cases"],
                before=row["input_tokens"],
                after=row["output_tokens"],
                raw_reduction=raw_reduction,
                qualified_reduction=qualified_reduction,
                gates=row["preservation_proxy_passed"],
                no_ops=row["zero_reduction_cases"],
                failures=row["failed_cases"],
            )
        )
    return "\n".join(lines)


def caveman_summary(run: dict[str, Any]) -> dict[str, Any]:
    cases = run.get("cases", [])
    totals = {
        "cases": len(cases),
        "baseline_output_tokens": sum(case["tokens"]["baseline_output"] for case in cases),
        "terse_output_tokens": sum(case["tokens"]["terse_output"] for case in cases),
        "caveman_output_tokens": sum(case["tokens"]["caveman_output"] for case in cases),
        "caveman_input_overhead_tokens": sum(
            case["tokens"]["caveman_input_overhead_vs_terse"] for case in cases
        ),
        "additional_output_saving_vs_terse": sum(
            case["tokens"]["additional_output_saving_vs_terse"] for case in cases
        ),
        "net_token_saving_vs_terse_single_turn": sum(
            case["tokens"]["net_token_saving_vs_terse_single_turn"] for case in cases
        ),
        "quality_scored_cases": 0,
        "fresh_model_execution": run.get("evidence", {}).get("fresh_model_execution", False),
        "snapshot_metadata": run.get("evidence", {}).get("snapshot_metadata", {}),
    }
    terse = totals["terse_output_tokens"]
    totals["additional_output_reduction_percent_vs_terse"] = round(
        100 * totals["additional_output_saving_vs_terse"] / terse, 4
    ) if terse else 0.0
    return totals


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--release", required=True)
    parser.add_argument("--project-root", required=True)
    parser.add_argument("--source-revision", default="unversioned-worktree")
    parser.add_argument(
        "--redact-path",
        action="append",
        default=[],
        metavar="LABEL=PATH",
        help="additional private path to replace with <LABEL> before publication",
    )
    parser.add_argument(
        "--preserve-existing-reproduction",
        action="store_true",
        help="refresh an existing reproduction tree without removing any files",
    )
    args = parser.parse_args()
    release = Path(args.release).resolve()
    project = Path(args.project_root).resolve()
    runs_dir = release / "runs"
    results_dir = release / "results"
    results_dir.mkdir(parents=True, exist_ok=True)

    summaries: list[dict[str, Any]] = []
    caveman: dict[str, Any] | None = None
    run_files = sorted(runs_dir.glob("*.json"))
    corpus_source = project / "benchmarks" / "corpus" / "authored" / "v1" / "cases.json"
    corpus_data = json.loads(corpus_source.read_text(encoding="utf-8"))
    expected_corpus_hash = hashlib.sha256(
        json.dumps(corpus_data, sort_keys=True, ensure_ascii=False).encode("utf-8")
    ).hexdigest()
    expected_case_ids = {case["id"] for case in corpus_data["cases"]}
    for run_file in run_files:
        run = json.loads(run_file.read_text(encoding="utf-8"))
        if run.get("optimizer", {}).get("class") == "generation_policy":
            caveman = caveman_summary(run)
        elif run.get("schema_version") == "kendr.peer-run/v1":
            if run.get("corpus", {}).get("canonical_json_sha256") != expected_corpus_hash:
                raise ValueError(f"corpus fingerprint mismatch in {run_file.name}")
            actual_case_ids = {case.get("case_id") for case in run.get("cases", [])}
            if actual_case_ids != expected_case_ids:
                raise ValueError(f"case-set mismatch in {run_file.name}")
            summaries.extend(regular_summary(run, run_file.name))

    summary_payload = {
        "schema_version": "kendr.peer-summary/v1",
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "claim_level": "payload_reduction_with_preservation_proxies",
        "target_model_executed": False,
        "paired_provider_usage": False,
        "tokenizer": f"tiktoken o200k_base {TOKENIZER_VERSION}",
        "rows": summaries,
        "generation_policy": {"caveman": caveman} if caveman else {},
    }
    (results_dir / "summary.json").write_text(
        json.dumps(summary_payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    with (results_dir / "summary.csv").open("w", newline="", encoding="utf-8") as output:
        if summaries:
            writer = csv.DictWriter(output, fieldnames=list(summaries[0]))
            writer.writeheader()
            writer.writerows(summaries)

    evidence_dir = release / "evidence"
    evidence_dir.mkdir(parents=True, exist_ok=True)
    copies = {
        project / "benchmarks" / "corpus" / "authored" / "v1" / "cases.json": evidence_dir / "corpus.json",
        project / "benchmarks" / "configs" / "peers.lock.json": evidence_dir / "peers.lock.json",
        project / "benchmarks" / "configs" / "scope-ledger.json": evidence_dir / "scope-ledger.json",
        project / "integrations" / "harnesses.lock.json": evidence_dir / "harnesses.lock.json",
        project / "integrations" / "verification.json": evidence_dir / "harness-verification.json",
    }
    for source, destination in copies.items():
        shutil.copy2(source, destination)

    reproduction = release / "reproduction"
    if reproduction.exists() and not args.preserve_existing_reproduction:
        shutil.rmtree(reproduction)
    shutil.copytree(
        project / "benchmarks" / "runners",
        reproduction / "benchmarks" / "runners",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )
    corpus_builder_destination = (
        reproduction / "benchmarks" / "corpus" / "authored" / "v1" / "build_corpus.py"
    )
    corpus_builder_destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(
        project / "benchmarks" / "corpus" / "authored" / "v1" / "build_corpus.py",
        corpus_builder_destination,
    )
    shutil.copy2(
        project / "benchmarks" / "corpus" / "authored" / "v1" / "cases.json",
        corpus_builder_destination.with_name("cases.json"),
    )
    shutil.copytree(
        project / "benchmarks" / "configs",
        reproduction / "benchmarks" / "configs",
        dirs_exist_ok=True,
    )
    shutil.copy2(
        project / "benchmarks" / "competitors.json",
        reproduction / "benchmarks" / "competitors.json",
    )
    harness_lock_destination = reproduction / "integrations" / "harnesses.lock.json"
    harness_lock_destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(
        project / "integrations" / "harnesses.lock.json",
        harness_lock_destination,
    )
    for name in ("Cargo.toml", "Cargo.lock", "rustfmt.toml"):
        destination = reproduction / name
        destination.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(project / name, destination)
    shutil.copytree(
        project / "crates",
        reproduction / "crates",
        dirs_exist_ok=True,
        ignore=shutil.ignore_patterns("__pycache__", "*.pyc"),
    )
    bootstrap_source = project / "benchmarks" / ".cache" / "bootstrap"
    if bootstrap_source.is_dir():
        shutil.copytree(
            bootstrap_source,
            evidence_dir / "bootstrap",
            dirs_exist_ok=True,
            ignore=shutil.ignore_patterns("*.whl", "*.zip", "*.exe"),
        )

    kendr_diagnostic_ids = {
        "kendr-safe-low-threshold",
        "kendr-extractive-tool-output",
    }
    primary_rows = [
        row for row in summaries if row["optimizer_id"] not in kendr_diagnostic_ids
    ]
    diagnostic_rows = [
        row for row in summaries if row["optimizer_id"] in kendr_diagnostic_ids
    ]
    prompt_rows = [
        row for row in primary_rows if row["surface"] == "prompt_context"
    ]
    tool_rows = [row for row in primary_rows if row["surface"] == "tool_output"]
    scope = json.loads((evidence_dir / "scope-ledger.json").read_text(encoding="utf-8"))
    scope_lines = ["| Peer | Release status | Reason |", "| --- | --- | --- |"]
    for item in scope["entries"]:
        scope_lines.append(
            f"| {item['id']} | `{item['status']}` | {item['reason']} |"
        )

    caveman_section = "No Caveman artifact was produced."
    if caveman:
        caveman_section = (
            f"The official committed snapshot contains {caveman['cases']} prompts. Remeasuring with "
            f"o200k_base gives {caveman['terse_output_tokens']:,} terse-control output tokens and "
            f"{caveman['caveman_output_tokens']:,} Caveman output tokens: "
            f"{fmt_percent(caveman['additional_output_reduction_percent_vs_terse'])} additional output reduction. "
            f"The skill adds {caveman['caveman_input_overhead_tokens']:,} input tokens across those single turns, "
            f"so the simple input-plus-output net versus terse is {caveman['net_token_saving_vs_terse_single_turn']:+,} tokens. "
            "Quality is unscored and these are upstream model outputs, not a fresh run on this machine."
        )

    report = f"""# KendrOptimizer peer benchmark report

Generated: {summary_payload['generated_at']}

## What this run establishes

This is a local, provider-neutral **payload reduction** experiment over an authored nine-case corpus. It independently recounts visible text with `tiktoken o200k_base`, retains complete inputs and outputs, and checks exact literals plus applicable JSON structure. It does **not** call a target LLM, observe a provider bill, or establish downstream answer quality. Therefore it does not support a “measured cost saving,” “same quality,” or “best optimizer” claim.

Configured 50% keep-rate arms are labeled `target-50`; their reduction is a requested operating point, not an optimizer discovering the ideal amount. The two primary peer tables contain exactly one Kendr arm: the shipped `default` policy. Development-only Kendr profiles are kept in a separate diagnostic appendix and never raise the headline result.

Headroom's structural-only rows keep its learned model disabled and are labeled accordingly. The separate `kompress-target-50` arm warms the pinned Kompress-v2-base ONNX model from an offline cache and includes Headroom's structural routers. OmniRoute is invoked through its pure RTK-then-Caveman modules; its gateway, routing, and provider features are not started.

## Prompt and context track

{markdown_table(prompt_rows)}

## Command and tool-output track

{markdown_table(tool_rows)}

RTK appears only here because it transforms command output. Prompt compressors are not assigned synthetic RTK scores. The Kendr extractive arm is opt-in and separate from its safe default.

## Kendr development diagnostics (not shipped-default comparisons)

{markdown_table(diagnostic_rows) if diagnostic_rows else "No diagnostic Kendr profiles were run."}

These rows are retained for engineering diagnosis only. `safe-low-threshold` changes application gates, while `extractive-tool-output` enables a higher-risk generic reducer. Neither row represents the optimizer shipped by default.

## Generation-policy track: Caveman

{caveman_section}

Caveman changes future model generation; it does not compress an already-produced string. A valid fresh comparison needs randomized paired target-model runs, usage counters, and task-quality scoring.

## Unsupported and deliberately unranked peers

{chr(10).join(scope_lines)}

Failures and unsupported cases remain in each raw run. PCToolkit is a meta-harness around algorithms rather than another algorithm, so it is catalogued instead of double-counted.

Raw percentages are shown only when every predefined eligible case completed. A proxy-qualified percentage is shown only when coverage is complete **and every preservation proxy passes**. Large raw reductions that lose a required literal or JSON value remain visible but are explicitly not qualified. Incomplete arms display `N/A`; their successful-case deltas remain available in `results/summary.json` for diagnosis, not ranking.

## Reading the preservation column

"Preservation proxy" means every fixture-declared literal and the exact query survived and, for the JSON fixture, the primary payload remained value-equivalent JSON. Number, URL, and path recall are also reported per case for diagnosis; they qualify the aggregate only when declared as fixture requirements. These checks catch obvious corruption, but they are not a substitute for target-model or task-native quality evaluation.

Per-case latency is diagnostic only. Learned-model initialization is outside the LLMLingua case timer, while process startup is included for CLI-based arms, so this release does not compare optimizer latency across implementations.

## Reproduction and raw evidence

- `runs/` contains complete per-arm inputs, outputs, native metrics, timing, errors, and unsupported cases.
- `logs/` contains command stdout/stderr and the execution ledger.
- `evidence/` contains the exact corpus, peer lock, scope decisions, and harness compatibility lock.
- `results/summary.json` and `results/summary.csv` are derived only from the raw runs.
- `reproduction/` mirrors the minimal repository layout needed by the workflow; peer runtimes, cloned upstream repositories, and model weights remain external and are pinned in evidence.
- `SHA256SUMS` authenticates every release artifact except itself.

Run `python benchmarks/runners/execute_release.py --help` from the repository root to reproduce the workflow. Model caches and virtual environments intentionally remain outside the release; resolved versions and failures are recorded in the artifacts.
"""
    (release / "report.md").write_text(report, encoding="utf-8")

    release_readme = """# Benchmark release bundle

Start with [report.md](report.md). This folder contains the complete raw runs, execution logs, exact corpus and locks, derived tables, a runnable workflow/source snapshot, environment metadata, and checksums. Large model weights, peer virtual environments, upstream clones, and executables are excluded; immutable revisions and runtime evidence are retained so they can be fetched separately.

The release is evidence, not a universal leaderboard. Each optimizer is reported only on the content surface it actually transforms.
"""
    (release / "README.md").write_text(release_readme, encoding="utf-8")

    environment_payload = {
        "captured_at": summary_payload["generated_at"],
        "platform": platform.platform(),
        "python": platform.python_version(),
        "processor": platform.processor(),
        "machine": platform.machine(),
        "source_revision": args.source_revision,
        "source_is_git_repository": (project / ".git").exists(),
    }
    (release / "environment.json").write_text(
        json.dumps(environment_payload, indent=2) + "\n", encoding="utf-8"
    )

    aliases = default_aliases(project, release)
    aliases.extend(parse_alias(item) for item in args.redact_path)
    sanitizer = PublicArtifactSanitizer(aliases)
    publication_log = release / "logs" / "publication-sanitization.json"
    previous_publication: dict[str, Any] = {}
    if publication_log.is_file():
        previous_publication = json.loads(publication_log.read_text(encoding="utf-8"))
    sanitization = sanitizer.sanitize_tree(
        release,
        exclude_names={"SHA256SUMS", "manifest.json"},
    )
    publication_log.parent.mkdir(parents=True, exist_ok=True)
    files_changed = sorted(
        set(previous_publication.get("files_changed", []))
        | set(sanitization.files_changed)
    )
    publication_log.write_text(
        json.dumps(
            {
                "schema_version": "kendr.publication-sanitization/v1",
                "policy": "known private roots and account identifiers are replaced by deterministic public tokens",
                "public_tokens": sorted(
                    {alias.public for alias in sanitizer.aliases}
                    | {LOCAL_FILE_URI_TOKEN}
                ),
                "sanitization_passes": int(
                    previous_publication.get("sanitization_passes", 1)
                )
                + 1
                if previous_publication
                else 1,
                "files_changed": files_changed,
                "replacements": int(previous_publication.get("replacements", 0))
                + sanitization.replacements,
                "private_markers_after_sanitization": 0,
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    sanitizer.assert_tree_safe(
        release,
        exclude_names={"SHA256SUMS", "manifest.json"},
    )

    artifacts = []
    for path in sorted(
        item
        for item in release.rglob("*")
        if item.is_file() and item.name not in {"SHA256SUMS", "manifest.json"}
    ):
        artifacts.append(
            {
                "path": path.relative_to(release).as_posix(),
                "sha256": digest(path),
                "bytes": path.stat().st_size,
            }
        )
    manifest = {
        "schema_version": "kendr.benchmark-release/v1",
        "release": release.name,
        "generated_at": summary_payload["generated_at"],
        "source_revision": args.source_revision,
        "claim_level": summary_payload["claim_level"],
        "artifacts": artifacts,
    }
    (release / "manifest.json").write_text(
        json.dumps(manifest, indent=2) + "\n", encoding="utf-8"
    )
    checksum_paths = sorted(
        item for item in release.rglob("*") if item.is_file() and item.name != "SHA256SUMS"
    )
    checksum_text = "\n".join(
        f"{digest(path)}  {path.relative_to(release).as_posix()}" for path in checksum_paths
    )
    (release / "SHA256SUMS").write_text(checksum_text + "\n", encoding="utf-8")
    verify_release(release, sanitizer=sanitizer)


if __name__ == "__main__":
    main()
