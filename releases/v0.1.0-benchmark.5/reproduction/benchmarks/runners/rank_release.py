#!/usr/bin/env python3
"""Build preservation-gated rankings from an immutable benchmark release.

The source release is read-only input. Generated artifacts are written outside
that release and contain enough provenance to reproduce the exact ordering.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import io
import json
import math
import sys
from pathlib import Path
from typing import Any


SCHEMA_VERSION = "kendr.optimizer-ranking/v1"
SURFACES = ("prompt_context", "tool_output")
SURFACE_TITLES = {
    "prompt_context": "Prompt and context",
    "tool_output": "Command and tool output",
}
DEVELOPMENT_KENDR_IDS = {
    "kendr-extractive-tool-output",
    "kendr-safe-low-threshold",
}
BASELINE_IDS = {"pass-through"}
NONCANONICAL_IDS = {
    "llmlingua-gpt2",
    "longllmlingua-gpt2",
}


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_release_checksums(
    release: Path, expected: dict[str, str]
) -> str:
    checksum_path = release / "SHA256SUMS"
    if not checksum_path.is_file():
        raise FileNotFoundError("release must contain SHA256SUMS")
    recorded: dict[str, str] = {}
    for line in checksum_path.read_text(encoding="utf-8").splitlines():
        if not line.strip():
            continue
        parts = line.split(maxsplit=1)
        if len(parts) != 2:
            raise ValueError("malformed SHA256SUMS entry")
        digest, relative = parts
        recorded[relative.lstrip("*").replace("\\", "/")] = digest.lower()
    for relative, actual in expected.items():
        listed = recorded.get(relative)
        if listed != actual:
            raise ValueError(
                f"release checksum mismatch for {relative}: listed={listed}, actual={actual}"
            )
    return sha256_file(checksum_path)


def labels_for(row: dict[str, Any]) -> list[str]:
    labels: list[str] = []
    optimizer_id = str(row["optimizer_id"])
    setting = str(row.get("setting", ""))
    name = str(row.get("optimizer_name", ""))
    if "target-" in setting:
        labels.append("configured-target-rate")
    if optimizer_id in NONCANONICAL_IDS:
        labels.append("noncanonical-feasibility-model")
    if "structural routers only" in name.lower():
        labels.append("structural-only")
    if optimizer_id in DEVELOPMENT_KENDR_IDS:
        labels.append("kendr-development-diagnostic")
    return labels


def as_number(value: Any, field: str) -> float:
    if isinstance(value, bool) or not isinstance(value, (int, float)):
        raise ValueError(f"{field} must be numeric")
    number = float(value)
    if not math.isfinite(number):
        raise ValueError(f"{field} must be finite")
    return number


def normalize_row(row: dict[str, Any], expected_cases: int) -> dict[str, Any]:
    required = {
        "optimizer_id",
        "optimizer_name",
        "version",
        "setting",
        "class",
        "surface",
        "eligible_cases",
        "completed_cases",
        "failed_cases",
        "input_tokens",
        "output_tokens",
        "raw_payload_reduction_percent",
        "preservation_proxy_passed",
        "zero_reduction_cases",
        "coverage_complete",
        "preservation_complete",
        "qualified_payload_reduction_percent",
        "median_latency_ms",
        "source_run",
    }
    missing = sorted(required - row.keys())
    if missing:
        raise ValueError(
            f"summary row {row.get('optimizer_id', '<unknown>')} is missing {missing}"
        )

    surface = str(row["surface"])
    if surface not in SURFACES:
        raise ValueError(f"unexpected ranking surface: {surface}")

    eligible = int(row["eligible_cases"])
    completed = int(row["completed_cases"])
    failed = int(row["failed_cases"])
    proxy_passed = int(row["preservation_proxy_passed"])
    no_ops = int(row["zero_reduction_cases"])
    raw_reduction = as_number(
        row["raw_payload_reduction_percent"], "raw_payload_reduction_percent"
    )
    latency_value = row["median_latency_ms"]
    latency = (
        as_number(latency_value, "median_latency_ms")
        if latency_value is not None
        else None
    )

    full_surface_coverage = (
        bool(row["coverage_complete"])
        and eligible == expected_cases
        and completed == expected_cases
        and failed == 0
    )
    all_preservation_proxies_pass = (
        bool(row["preservation_complete"])
        and completed > 0
        and proxy_passed == completed
    )
    primary_eligible = full_surface_coverage and all_preservation_proxies_pass
    qualified_value = row["qualified_payload_reduction_percent"]
    if primary_eligible:
        if qualified_value is None:
            raise ValueError(
                f"{row['optimizer_id']} is preservation-complete but has no qualified reduction"
            )
        qualified_reduction: float | None = as_number(
            qualified_value, "qualified_payload_reduction_percent"
        )
        if not math.isclose(qualified_reduction, raw_reduction, abs_tol=0.00005):
            raise ValueError(
                f"{row['optimizer_id']} raw and qualified reductions disagree"
            )
    else:
        qualified_reduction = None

    return {
        "optimizer_id": str(row["optimizer_id"]),
        "optimizer_name": str(row["optimizer_name"]),
        "version": str(row["version"]),
        "setting": str(row["setting"]),
        "class": str(row["class"]),
        "surface": surface,
        "expected_surface_cases": expected_cases,
        "eligible_cases": eligible,
        "completed_cases": completed,
        "failed_cases": failed,
        "input_tokens": int(row["input_tokens"]),
        "output_tokens": int(row["output_tokens"]),
        "raw_payload_reduction_percent": raw_reduction,
        "proxy_qualified_reduction_percent": qualified_reduction,
        "preservation_proxy_passed": proxy_passed,
        "zero_reduction_cases": no_ops,
        "no_op_case_percent": row.get("no_op_case_percent"),
        "median_latency_ms": latency,
        "full_surface_coverage": full_surface_coverage,
        "all_preservation_proxies_pass": all_preservation_proxies_pass,
        "primary_eligible": primary_eligible,
        "source_run": str(row["source_run"]),
        "labels": labels_for(row),
    }


def primary_rank_key(row: dict[str, Any]) -> tuple[float, int]:
    reduction = row["proxy_qualified_reduction_percent"]
    if reduction is None:
        raise ValueError("an unqualified row reached the primary ranking")
    return (
        -float(reduction),
        int(row["zero_reduction_cases"]),
    )


def primary_display_key(row: dict[str, Any]) -> tuple[Any, ...]:
    """Order rank groups, then provide stable display order within a shared rank."""
    return (
        *primary_rank_key(row),
        str(row["optimizer_id"]),
        str(row["setting"]),
    )


def assign_shared_ranks(rows: list[dict[str, Any]]) -> list[dict[str, Any]]:
    """Apply standard competition ranking using reduction and no-op count only."""
    ordered = sorted(rows, key=primary_display_key)
    ranked: list[dict[str, Any]] = []
    previous_key: tuple[float, int] | None = None
    shared_rank = 0
    for position, row in enumerate(ordered, start=1):
        current_key = primary_rank_key(row)
        if current_key != previous_key:
            shared_rank = position
            previous_key = current_key
        ranked.append({**row, "rank": shared_rank})
    return ranked


def unqualified_reason(row: dict[str, Any]) -> str:
    reasons: list[str] = []
    if not row["full_surface_coverage"]:
        reasons.append("incomplete full-surface coverage")
    if not row["all_preservation_proxies_pass"]:
        if row["full_surface_coverage"]:
            reasons.append("one or more preservation proxies failed")
        elif row["preservation_proxy_passed"] < row["completed_cases"]:
            reasons.append("one or more preservation proxies failed")
    return "; ".join(reasons) or "not primary-eligible"


def build_ranking(
    summary: dict[str, Any],
    corpus: dict[str, Any],
    release_id: str,
    summary_sha256: str,
    corpus_sha256: str,
    sha256sums_sha256: str,
) -> dict[str, Any]:
    if summary.get("schema_version") != "kendr.peer-summary/v1":
        raise ValueError("unsupported summary schema")
    if corpus.get("schema_version") != "kendr.benchmark-corpus/v1":
        raise ValueError("unsupported corpus schema")

    expected = {
        surface: sum(case.get("surface") == surface for case in corpus.get("cases", []))
        for surface in SURFACES
    }
    if any(expected[surface] <= 0 for surface in SURFACES):
        raise ValueError("corpus must contain cases for both ranking surfaces")

    normalized: list[dict[str, Any]] = []
    seen: set[tuple[str, str, str]] = set()
    for source_row in summary.get("rows", []):
        surface = source_row.get("surface")
        if surface not in SURFACES:
            continue
        row = normalize_row(source_row, expected[str(surface)])
        key = (row["optimizer_id"], row["setting"], row["surface"])
        if key in seen:
            raise ValueError(f"duplicate summary row: {key}")
        seen.add(key)
        normalized.append(row)

    tracks: dict[str, Any] = {}
    for surface in SURFACES:
        rows = [row for row in normalized if row["surface"] == surface]
        diagnostics = [
            row for row in rows if row["optimizer_id"] in DEVELOPMENT_KENDR_IDS
        ]
        baselines = [row for row in rows if row["optimizer_id"] in BASELINE_IDS]
        contestants = [
            row
            for row in rows
            if row["optimizer_id"] not in DEVELOPMENT_KENDR_IDS | BASELINE_IDS
        ]

        primary = [
            row
            for row in contestants
            if row["primary_eligible"]
        ]
        ranked = assign_shared_ranks(primary)

        unqualified = [row for row in contestants if row not in primary]
        unqualified.sort(
            key=lambda row: (
                -float(row["raw_payload_reduction_percent"]),
                str(row["optimizer_id"]),
                str(row["setting"]),
            )
        )
        unqualified_rows = [
            {**row, "non_ranked_reason": unqualified_reason(row)}
            for row in unqualified
        ]

        tracks[surface] = {
            "expected_cases": expected[surface],
            "primary_ranked": ranked,
            "unqualified_raw_reductions": unqualified_rows,
            "baseline_references": sorted(
                baselines, key=lambda row: (row["optimizer_id"], row["setting"])
            ),
            "kendr_development_diagnostics": sorted(
                diagnostics,
                key=lambda row: (row["optimizer_id"], row["setting"]),
            ),
        }

    payload = {
        "schema_version": SCHEMA_VERSION,
        "release_id": release_id,
        "source": {
            "summary": "results/summary.json",
            "summary_sha256": summary_sha256,
            "corpus": "evidence/corpus.json",
            "corpus_sha256": corpus_sha256,
            "sha256sums": "SHA256SUMS",
            "sha256sums_sha256": sha256sums_sha256,
            "release_input_checksums_verified": True,
            "source_generated_at": summary.get("generated_at"),
            "claim_level": summary.get("claim_level"),
            "target_model_executed": summary.get("target_model_executed"),
            "paired_provider_usage": summary.get("paired_provider_usage"),
            "tokenizer": summary.get("tokenizer"),
        },
        "methodology": {
            "unit_ranked": "optimizer configuration within one surface",
            "surfaces_ranked_separately": list(SURFACES),
            "primary_eligibility": [
                "completed every corpus case assigned to the surface",
                "zero failed cases",
                "every preservation proxy passed",
                "KendrOptimizer is represented only by its default shipped configuration",
            ],
            "ordering": [
                "higher proxy-qualified payload reduction",
                "fewer no-op cases",
                "configurations tied on both criteria share a standard competition rank",
                "optimizer ID and setting lexicographically control display order within a shared rank only",
            ],
            "rank_style": "standard competition ranking (example: 1, 2, 2, 4)",
            "excluded_from_score": [
                "unqualified raw reduction",
                "baseline pass-through",
                "Kendr development diagnostics",
                "generation-policy experiments",
            ],
            "latency_notice": (
                "Latency is not a cross-implementation performance claim because runner "
                "boundaries differ; it is reported for diagnosis and never affects rank."
            ),
        },
        "tracks": tracks,
    }
    validate_ranking(payload, len(normalized))
    return payload


def self_check_shared_rank_semantics() -> None:
    """Exercise a synthetic tie where latency conflicts with display ordering."""
    rows = [
        {
            "optimizer_id": "zeta",
            "setting": "default",
            "proxy_qualified_reduction_percent": 50.0,
            "zero_reduction_cases": 1,
            "median_latency_ms": 0.001,
        },
        {
            "optimizer_id": "alpha",
            "setting": "default",
            "proxy_qualified_reduction_percent": 50.0,
            "zero_reduction_cases": 1,
            "median_latency_ms": 999999.0,
        },
        {
            "optimizer_id": "beta",
            "setting": "default",
            "proxy_qualified_reduction_percent": 50.0,
            "zero_reduction_cases": 2,
            "median_latency_ms": 0.0,
        },
        {
            "optimizer_id": "gamma",
            "setting": "default",
            "proxy_qualified_reduction_percent": 49.0,
            "zero_reduction_cases": 0,
            "median_latency_ms": 0.0,
        },
    ]
    ranked = assign_shared_ranks(rows)
    if [row["optimizer_id"] for row in ranked] != ["alpha", "zeta", "beta", "gamma"]:
        raise ValueError("latency affected rank/display order or lexical display order failed")
    if [row["rank"] for row in ranked] != [1, 1, 3, 4]:
        raise ValueError("standard competition shared-rank semantics failed")


def validate_ranking(payload: dict[str, Any], expected_row_count: int | None = None) -> None:
    self_check_shared_rank_semantics()
    if payload.get("schema_version") != SCHEMA_VERSION:
        raise ValueError("ranking schema mismatch")
    tracks = payload.get("tracks")
    if not isinstance(tracks, dict) or tuple(tracks) != SURFACES:
        raise ValueError("ranking must contain prompt_context and tool_output only")

    categorized = 0
    for surface in SURFACES:
        track = tracks[surface]
        ranked = track["primary_ranked"]
        categories = (
            ranked,
            track["unqualified_raw_reductions"],
            track["baseline_references"],
            track["kendr_development_diagnostics"],
        )
        keys: set[tuple[str, str, str]] = set()
        for category in categories:
            for row in category:
                categorized += 1
                if row["surface"] != surface:
                    raise ValueError("a row crossed ranking surfaces")
                key = (row["optimizer_id"], row["setting"], row["surface"])
                if key in keys:
                    raise ValueError(f"row categorized more than once: {key}")
                keys.add(key)

        if ranked != assign_shared_ranks(ranked):
            raise ValueError(
                f"{surface} primary ranking violates shared-rank or display-order rules"
            )
        for row in ranked:
            if not row["primary_eligible"]:
                raise ValueError("ineligible row entered primary ranking")
            if row["optimizer_id"].startswith("kendr-") and row["optimizer_id"] != "kendr-default":
                raise ValueError("non-default Kendr arm entered primary ranking")
        for row in track["unqualified_raw_reductions"]:
            if "non_ranked_reason" not in row:
                raise ValueError("unqualified row has no reason")
        for row in track["kendr_development_diagnostics"]:
            if row["optimizer_id"] not in DEVELOPMENT_KENDR_IDS:
                raise ValueError("non-diagnostic row entered Kendr diagnostics")

    if expected_row_count is not None and categorized != expected_row_count:
        raise ValueError(
            f"categorized {categorized} rows, expected {expected_row_count}"
        )


def pct(value: float | None) -> str:
    return "N/A" if value is None else f"{value:.2f}%"


def latency(value: float | None) -> str:
    return "N/A" if value is None else f"{value:.3f}"


def label_text(row: dict[str, Any]) -> str:
    labels = row.get("labels", [])
    return ", ".join(f"`{label}`" for label in labels) if labels else "—"


def optimizer_text(row: dict[str, Any]) -> str:
    return f"{row['optimizer_name']} — `{row['setting']}`"


def primary_table(rows: list[dict[str, Any]]) -> str:
    lines = [
        "| Rank | Optimizer / setting | Proxy-qualified reduction | Coverage | Preservation proxies | No-op cases | Median latency (ms) | Labels |",
        "| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |",
    ]
    if not rows:
        lines.append("| — | No eligible optimizer configuration | — | — | — | — | — | — |")
    for row in rows:
        lines.append(
            "| {rank} | {optimizer} | {reduction} | {done}/{expected} | "
            "{proxies}/{expected} | {no_ops}/{expected} | {latency} | {labels} |".format(
                rank=row["rank"],
                optimizer=optimizer_text(row),
                reduction=pct(row["proxy_qualified_reduction_percent"]),
                done=row["completed_cases"],
                expected=row["expected_surface_cases"],
                proxies=row["preservation_proxy_passed"],
                no_ops=row["zero_reduction_cases"],
                latency=latency(row["median_latency_ms"]),
                labels=label_text(row),
            )
        )
    return "\n".join(lines)


def unqualified_table(rows: list[dict[str, Any]]) -> str:
    lines = [
        "| Optimizer / setting | Raw reduction | Full-track coverage | Preservation proxies | Why it is not ranked | Labels |",
        "| --- | ---: | ---: | ---: | --- | --- |",
    ]
    if not rows:
        lines.append("| None | — | — | — | — | — |")
    for row in rows:
        lines.append(
            "| {optimizer} | {reduction} | {done}/{expected} | {proxies}/{done} | {reason} | {labels} |".format(
                optimizer=optimizer_text(row),
                reduction=pct(row["raw_payload_reduction_percent"]),
                done=row["completed_cases"],
                expected=row["expected_surface_cases"],
                proxies=row["preservation_proxy_passed"],
                reason=row["non_ranked_reason"],
                labels=label_text(row),
            )
        )
    return "\n".join(lines)


def diagnostic_table(rows: list[dict[str, Any]]) -> str:
    lines = [
        "| Optimizer / setting | Raw reduction | Coverage | Preservation proxies | No-op cases | Labels |",
        "| --- | ---: | ---: | ---: | ---: | --- |",
    ]
    if not rows:
        lines.append("| None | — | — | — | — | — |")
    for row in rows:
        lines.append(
            "| {optimizer} | {reduction} | {done}/{expected} | {proxies}/{done} | {no_ops}/{expected} | {labels} |".format(
                optimizer=optimizer_text(row),
                reduction=pct(row["raw_payload_reduction_percent"]),
                done=row["completed_cases"],
                expected=row["expected_surface_cases"],
                proxies=row["preservation_proxy_passed"],
                no_ops=row["zero_reduction_cases"],
                labels=label_text(row),
            )
        )
    return "\n".join(lines)


def render_markdown(payload: dict[str, Any]) -> str:
    source = payload["source"]
    lines = [
        f"# Preservation-gated optimizer ranking: {payload['release_id']}",
        "",
        "This ranking compares optimizer **configurations**, not projects in the abstract. "
        "It ranks prompt/context and tool-output workloads separately. Results are specific "
        "to this authored corpus and its preservation proxies; they do not establish target-"
        "model quality, provider cost savings, or a universal ‘best optimizer’ claim.",
        "",
        "## Ranking rules",
        "",
        "A configuration enters a primary table only when it completed every corpus case "
        "on that surface with zero failures and every declared preservation proxy passed. "
        "Rows are ordered by higher proxy-qualified payload reduction and then fewer no-op "
        "cases. Configurations tied on both share a standard competition rank (for example, "
        "1, 2, 2, 4); optimizer ID and setting control display order within that shared rank. "
        "Latency is diagnostic only and never affects rank.",
        "",
        "Configured keep-rate arms are labeled `configured-target-rate`; their achieved "
        "reduction must not be read as automatic rate selection. GPT-2 substitutions for "
        "canonical LLMLingua/LongLLMLingua checkpoints are labeled "
        "`noncanonical-feasibility-model`. Only Kendr’s shipped `default` arm can enter a "
        "primary table; Kendr engineering profiles appear separately.",
        "",
    ]

    for surface in SURFACES:
        track = payload["tracks"][surface]
        lines.extend(
            [
                f"## {SURFACE_TITLES[surface]} surface",
                "",
                f"Full-track coverage is {track['expected_cases']} cases.",
                "",
                "### Primary proxy-qualified ranking",
                "",
                primary_table(track["primary_ranked"]),
                "",
                "### Unqualified raw reductions — not ranked",
                "",
                "These percentages remain visible for diagnosis, but are not mixed into the "
                "primary ranking. A high raw reduction cannot compensate for missing cases or "
                "failed preservation proxies.",
                "",
                unqualified_table(track["unqualified_raw_reductions"]),
                "",
                "### Pass-through baseline reference",
                "",
                diagnostic_table(track["baseline_references"]),
                "",
                "### Kendr development diagnostics — excluded from ranking",
                "",
                "These profiles are retained for engineering comparison and cannot increase "
                "Kendr’s primary position.",
                "",
                diagnostic_table(track["kendr_development_diagnostics"]),
                "",
            ]
        )

    lines.extend(
        [
            "## Scope and provenance",
            "",
            f"- Source release: [`releases/{payload['release_id']}`](../../../releases/{payload['release_id']}/README.md)",
            f"- Summary SHA-256: `{source['summary_sha256']}`",
            f"- Corpus SHA-256: `{source['corpus_sha256']}`",
            f"- Release SHA256SUMS SHA-256: `{source['sha256sums_sha256']}`",
            "- Summary and corpus hashes verified against the release checksum manifest: `true`",
            f"- Tokenizer: `{source['tokenizer']}`",
            f"- Target model executed: `{str(source['target_model_executed']).lower()}`",
            f"- Paired provider usage observed: `{str(source['paired_provider_usage']).lower()}`",
            "- Caveman’s generation-policy snapshot is a different evaluation surface and is not inserted into either optimizer ranking.",
            "",
            "Reproduce the files from the repository root:",
            "",
            "```powershell",
            f"python benchmarks/runners/rank_release.py --release releases/{payload['release_id']} --output benchmarks/rankings/{payload['release_id']}",
            f"python benchmarks/runners/rank_release.py --release releases/{payload['release_id']} --output benchmarks/rankings/{payload['release_id']} --check",
            "```",
            "",
        ]
    )
    return "\n".join(lines)


CSV_FIELDS = [
    "surface",
    "category",
    "rank",
    "optimizer_id",
    "optimizer_name",
    "version",
    "setting",
    "class",
    "raw_payload_reduction_percent",
    "proxy_qualified_reduction_percent",
    "expected_surface_cases",
    "eligible_cases",
    "completed_cases",
    "failed_cases",
    "preservation_proxy_passed",
    "zero_reduction_cases",
    "median_latency_ms",
    "full_surface_coverage",
    "all_preservation_proxies_pass",
    "primary_eligible",
    "labels",
    "non_ranked_reason",
    "source_run",
]


def render_csv(payload: dict[str, Any]) -> str:
    output = io.StringIO(newline="")
    writer = csv.DictWriter(output, fieldnames=CSV_FIELDS, lineterminator="\n")
    writer.writeheader()
    category_keys = (
        ("primary_ranked", "primary_ranked"),
        ("unqualified_raw_reductions", "unqualified_raw"),
        ("baseline_references", "baseline_reference"),
        ("kendr_development_diagnostics", "kendr_development_diagnostic"),
    )
    for surface in SURFACES:
        track = payload["tracks"][surface]
        for key, category in category_keys:
            for row in track[key]:
                record = {field: row.get(field, "") for field in CSV_FIELDS}
                record["category"] = category
                record["labels"] = ";".join(row.get("labels", []))
                writer.writerow(record)
    return output.getvalue()


def artifact_bytes(payload: dict[str, Any]) -> dict[str, bytes]:
    return {
        "ranking.json": (
            json.dumps(payload, ensure_ascii=False, indent=2) + "\n"
        ).encode("utf-8"),
        "ranking.csv": render_csv(payload).encode("utf-8"),
        "ranking.md": render_markdown(payload).encode("utf-8"),
    }


def is_within(path: Path, parent: Path) -> bool:
    try:
        path.relative_to(parent)
        return True
    except ValueError:
        return False


def load_and_build(release: Path) -> dict[str, Any]:
    summary_path = release / "results" / "summary.json"
    corpus_path = release / "evidence" / "corpus.json"
    if not summary_path.is_file() or not corpus_path.is_file():
        raise FileNotFoundError("release must contain results/summary.json and evidence/corpus.json")
    summary = json.loads(summary_path.read_text(encoding="utf-8"))
    corpus = json.loads(corpus_path.read_text(encoding="utf-8"))
    summary_sha256 = sha256_file(summary_path)
    corpus_sha256 = sha256_file(corpus_path)
    sha256sums_sha256 = verify_release_checksums(
        release,
        {
            "results/summary.json": summary_sha256,
            "evidence/corpus.json": corpus_sha256,
        },
    )
    return build_ranking(
        summary,
        corpus,
        release.name,
        summary_sha256,
        corpus_sha256,
        sha256sums_sha256,
    )


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Generate preservation-gated rankings outside an immutable release."
    )
    parser.add_argument("--release", required=True, type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument(
        "--check",
        action="store_true",
        help="validate invariants and verify generated files byte-for-byte without writing",
    )
    args = parser.parse_args()
    release = args.release.resolve()
    output = args.output.resolve()
    if not release.is_dir():
        parser.error(f"release does not exist: {release}")
    if output == release or is_within(output, release):
        parser.error("output must be outside the immutable source release")

    payload = load_and_build(release)
    artifacts = artifact_bytes(payload)
    if args.check:
        mismatches: list[str] = []
        for name, expected in artifacts.items():
            path = output / name
            if not path.is_file():
                mismatches.append(f"missing {path}")
            elif path.read_bytes() != expected:
                mismatches.append(f"content differs: {path}")
        if mismatches:
            print("ranking check failed:", file=sys.stderr)
            for mismatch in mismatches:
                print(f"- {mismatch}", file=sys.stderr)
            return 1
        print(
            f"ranking check passed: {len(artifacts)} artifacts, "
            f"{sum(len(track['primary_ranked']) for track in payload['tracks'].values())} "
            "primary rows; shared-rank semantics verified"
        )
        return 0

    output.mkdir(parents=True, exist_ok=True)
    for name, content in artifacts.items():
        (output / name).write_bytes(content)
    print(f"wrote {len(artifacts)} ranking artifacts to {output}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
