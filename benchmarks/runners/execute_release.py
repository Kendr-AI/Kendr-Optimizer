#!/usr/bin/env python3
"""Execute all feasible peer arms and preserve every command artifact."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import platform
import shutil
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat()


def file_sha256(path: Path) -> str:
    value = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def capture_model_cache(hf_cache: Path) -> dict[str, Any]:
    specifications = [
        (
            "openai-community/gpt2",
            "models--gpt2",
            "607a30d783dfa663caf39e06633721c8d4cfcd7e",
        ),
        (
            "microsoft/llmlingua-2-bert-base-multilingual-cased-meetingbank",
            "models--microsoft--llmlingua-2-bert-base-multilingual-cased-meetingbank",
            "5f0c82792b7ea14c6484e015b6a072009496b7f2",
        ),
        (
            "chopratejas/kompress-v2-base",
            "models--chopratejas--kompress-v2-base",
            "b1563631b35bfdcee37587ad530147497d820d4c",
        ),
        (
            "answerdotai/ModernBERT-base",
            "models--answerdotai--ModernBERT-base",
            "8949b909ec900327062f0ebf497f51aef5e6f0c8",
        ),
    ]
    models = []
    for model_id, cache_name, revision in specifications:
        snapshot = hf_cache / "hub" / cache_name / "snapshots" / revision
        record: dict[str, Any] = {
            "model_id": model_id,
            "revision": revision,
            "status": "present" if snapshot.is_dir() else "missing",
            "files": [],
        }
        if snapshot.is_dir():
            for logical in sorted(item for item in snapshot.rglob("*") if item.is_file()):
                resolved = logical.resolve(strict=True)
                record["files"].append(
                    {
                        "name": logical.relative_to(snapshot).as_posix(),
                        "bytes": resolved.stat().st_size,
                        "sha256": file_sha256(resolved),
                        "content_address": resolved.name,
                    }
                )
        models.append(record)
    return {
        "schema_version": "kendr.model-cache-manifest/v1",
        "captured_at": utc_now(),
        "models": models,
    }


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "--release",
        required=True,
        help="new release directory; use a fresh identifier for every publication",
    )
    parser.add_argument("--project-root", default=".")
    parser.add_argument("--headroom-python")
    parser.add_argument("--llmlingua-python")
    parser.add_argument("--rtk")
    parser.add_argument("--caveman-repo", default="benchmarks/.cache/peers/caveman")
    parser.add_argument("--omniroute-node")
    parser.add_argument(
        "--omniroute-repo", default="benchmarks/.cache/peers/omniroute"
    )
    parser.add_argument("--resume", action="store_true")
    parser.add_argument("--model-timeout-seconds", type=int, default=7200)
    parser.add_argument(
        "--require-all-success",
        action="store_true",
        help="refuse to publish when any configured command attempt is incomplete",
    )
    args = parser.parse_args()

    project = Path(args.project_root).resolve()
    release = (project / args.release).resolve() if not Path(args.release).is_absolute() else Path(args.release)
    if release.exists() and any(release.iterdir()) and not args.resume:
        raise SystemExit(f"release already exists and is non-empty: {release}; use --resume")
    runs = release / "runs"
    logs = release / "logs"
    runs.mkdir(parents=True, exist_ok=True)
    logs.mkdir(parents=True, exist_ok=True)
    planned_runs = {
        "pass-through.json",
        "kendr-default.json",
        "kendr-safe-low-threshold.json",
        "kendr-extractive-tool-output.json",
        "caveman-upstream-snapshot.json",
    }
    if args.headroom_python:
        planned_runs.update(
            {
                "headroom-structural-default.json",
                "headroom-structural-target-50.json",
                "headroom-kompress-target-50.json",
            }
        )
    if args.llmlingua_python:
        planned_runs.update(
            {
                "llmlingua-gpt2.json",
                "llmlingua2-small.json",
                "longllmlingua-gpt2.json",
            }
        )
    if args.rtk:
        planned_runs.add("rtk-0.45.0.json")
    if args.omniroute_node:
        planned_runs.add("omniroute-deterministic-stack.json")
    stale_runs = sorted(path.name for path in runs.glob("*.json") if path.name not in planned_runs)
    if stale_runs:
        raise SystemExit(
            "release contains unplanned stale runs; use a fresh release folder or explicitly rerun those peers: "
            + ", ".join(stale_runs)
        )
    cache = project / "benchmarks" / ".cache"
    hf_cache = cache / "huggingface"
    tiktoken_cache = cache / "tiktoken-cache"
    hf_cache.mkdir(parents=True, exist_ok=True)
    tiktoken_cache.mkdir(parents=True, exist_ok=True)
    corpus_builder = project / "benchmarks" / "corpus" / "authored" / "v1" / "build_corpus.py"
    corpus = corpus_builder.parent / "cases.json"
    runners = project / "benchmarks" / "runners"
    attempts: list[dict[str, Any]] = []

    def log_path(run_id: str, stream: str) -> Path:
        base = logs / f"{run_id}.{stream}.log"
        if not base.exists():
            return base
        index = 2
        while True:
            candidate = logs / f"{run_id}.attempt-{index}.{stream}.log"
            if not candidate.exists():
                return candidate
            index += 1

    def command(
        run_id: str,
        argv: list[str],
        *,
        timeout: int = 900,
        env_updates: dict[str, str] | None = None,
        failure_meta: dict[str, str] | None = None,
        output_path: Path | None = None,
    ) -> bool:
        started_at = utc_now()
        started = time.perf_counter()
        inherited_keys = (
            "SystemRoot",
            "WINDIR",
            "PATH",
            "PATHEXT",
            "COMSPEC",
            "TEMP",
            "TMP",
            "USERPROFILE",
            "APPDATA",
            "LOCALAPPDATA",
            "NUMBER_OF_PROCESSORS",
            "PROCESSOR_ARCHITECTURE",
            "INCLUDE",
            "LIB",
            "LIBPATH",
            "VCINSTALLDIR",
            "VCToolsInstallDir",
            "VCToolsVersion",
            "VisualStudioVersion",
            "WindowsSdkDir",
            "WindowsSDKVersion",
            "UniversalCRTSdkDir",
            "UCRTVersion",
            "VSCMD_ARG_HOST_ARCH",
            "VSCMD_ARG_TGT_ARCH",
        )
        environment = {
            key: os.environ[key] for key in inherited_keys if key in os.environ
        }
        safe_env = {
            "PYTHONUTF8": "1",
            "TOKENIZERS_PARALLELISM": "false",
            "HF_HOME": str(hf_cache),
            "TIKTOKEN_CACHE_DIR": str(tiktoken_cache),
            "HF_HUB_DISABLE_TELEMETRY": "1",
            "HF_HUB_DISABLE_XET": "1",
            "DO_NOT_TRACK": "1",
        }
        if env_updates:
            safe_env.update(env_updates)
        environment.update(safe_env)
        try:
            completed = subprocess.run(
                argv,
                cwd=project,
                text=True,
                encoding="utf-8",
                errors="replace",
                capture_output=True,
                check=False,
                timeout=timeout,
                env=environment,
            )
            exit_code = completed.returncode
            stdout = completed.stdout
            stderr = completed.stderr
            timed_out = False
        except subprocess.TimeoutExpired as error:
            exit_code = None
            stdout = error.stdout if isinstance(error.stdout, str) else ""
            stderr = error.stderr if isinstance(error.stderr, str) else ""
            stderr += f"\nTimed out after {timeout} seconds."
            timed_out = True
        except OSError as error:
            exit_code = None
            stdout = ""
            stderr = f"{type(error).__name__}: {error}"
            timed_out = False
        elapsed = time.perf_counter() - started
        stdout_path = log_path(run_id, "stdout")
        stderr_path = log_path(run_id, "stderr")
        stdout_path.write_text(stdout, encoding="utf-8")
        stderr_path.write_text(stderr, encoding="utf-8")
        record = {
            "id": run_id,
            "argv": argv,
            "cwd": str(project),
            "started_at": started_at,
            "finished_at": utc_now(),
            "elapsed_seconds": round(elapsed, 3),
            "exit_code": exit_code,
            "timed_out": timed_out,
            "inherited_env_keys": sorted(environment.keys() - safe_env.keys()),
            "env_overrides": safe_env,
            "stdout_log": stdout_path.name,
            "stderr_log": stderr_path.name,
        }
        attempts.append(record)
        succeeded = exit_code == 0
        if not succeeded and output_path is not None and failure_meta is not None:
            corpus_data = json.loads(corpus.read_text(encoding="utf-8")) if corpus.exists() else {"cases": []}
            surfaces = set(failure_meta["surfaces"].split(","))
            failure = {
                "schema_version": "kendr.peer-run/v1",
                "optimizer": {
                    "id": failure_meta["id"],
                    "name": failure_meta["name"],
                    "version": failure_meta.get("version", "unknown"),
                    "class": failure_meta["class"],
                    "setting": failure_meta["setting"],
                },
                "corpus": {
                    "id": corpus_data.get("corpus_id", "unknown"),
                    "canonical_json_sha256": hashlib.sha256(
                        json.dumps(
                            corpus_data, sort_keys=True, ensure_ascii=False
                        ).encode("utf-8")
                    ).hexdigest(),
                    "case_count": len(corpus_data.get("cases", [])),
                },
                "environment": {"platform": platform.platform()},
                "notes": ["Worker failed before producing case results; see the command logs."],
                "cases": [
                    {
                        "case_id": item["id"],
                        "surface": item["surface"],
                        "content_type": item["content_type"],
                        "status": "failed" if item["surface"] in surfaces else "unsupported",
                        "reason": "worker command failed" if item["surface"] in surfaces else "surface outside peer scope",
                        "command_attempt": run_id,
                    }
                    for item in corpus_data.get("cases", [])
                ],
            }
            output_path.write_text(json.dumps(failure, indent=2) + "\n", encoding="utf-8")
        return succeeded

    if not command("build-corpus", [sys.executable, str(corpus_builder)]):
        raise SystemExit("corpus build failed; refusing to use a stale corpus")
    if not command(
        "cargo-build-release",
        ["cargo", "build", "--locked", "--release", "-p", "kendr-optimizer-cli"],
        timeout=1800,
    ):
        raise SystemExit("release build failed; refusing to use a stale binary")
    executable = project / "target" / "release" / ("kendr-opt.exe" if os.name == "nt" else "kendr-opt")
    if not executable.is_file():
        raise SystemExit(f"release build did not produce {executable}")
    binary_sha256 = hashlib.sha256(executable.read_bytes()).hexdigest()

    if args.headroom_python:
        command(
            "headroom-environment",
            [args.headroom_python, "-m", "pip", "freeze", "--all"],
            timeout=300,
        )
    if args.llmlingua_python:
        command(
            "llmlingua-environment",
            [args.llmlingua_python, "-m", "pip", "freeze", "--all"],
            timeout=300,
        )

    command(
        "pass-through",
        [sys.executable, str(runners / "baseline_worker.py"), "--corpus", str(corpus), "--output", str(runs / "pass-through.json")],
        output_path=runs / "pass-through.json",
        failure_meta={"id": "pass-through", "name": "Pass-through", "class": "baseline", "setting": "none", "surfaces": "prompt_context,tool_output"},
    )
    for mode in ("default", "safe-low-threshold", "extractive-tool-output"):
        output = runs / f"kendr-{mode}.json"
        command(
            f"kendr-{mode}",
            [sys.executable, str(runners / "kendr_worker.py"), "--corpus", str(corpus), "--output", str(output), "--binary", str(executable), "--mode", mode],
            output_path=output,
            failure_meta={"id": f"kendr-{mode}", "name": "KendrOptimizer", "version": "0.1.0-dev", "class": "structured_payload_optimizer", "setting": mode, "surfaces": "tool_output" if mode.startswith("extractive") else "prompt_context,tool_output"},
        )

    if args.headroom_python:
        for mode in (
            "structural-default",
            "structural-target-50",
            "kompress-target-50",
        ):
            output = runs / f"headroom-{mode}.json"
            learned = mode == "kompress-target-50"
            command(
                f"headroom-{mode}",
                [args.headroom_python, str(runners / "headroom_worker.py"), "--corpus", str(corpus), "--output", str(output), "--mode", mode],
                timeout=args.model_timeout_seconds,
                env_updates=(
                    {
                        "HF_HUB_OFFLINE": "1",
                        "TRANSFORMERS_OFFLINE": "1",
                        "HEADROOM_KOMPRESS_BACKEND": "onnx",
                        "HEADROOM_KOMPRESS_ONNX_FILENAME": "onnx/kompress-int8-wo.onnx",
                        "HEADROOM_KOMPRESS_CANARY_SECONDS": "0",
                    }
                    if learned
                    else None
                ),
                output_path=output,
                failure_meta={"id": f"headroom-{mode}", "name": "Headroom (Kompress + structural)" if learned else "Headroom (structural routers only)", "version": "0.34.0", "class": "structured_context_optimizer", "setting": mode, "surfaces": "prompt_context,tool_output"},
            )
    else:
        attempts.append({"id": "headroom", "status": "not_attempted", "reason": "--headroom-python not supplied"})

    if args.llmlingua_python:
        for mode in ("llmlingua-gpt2", "llmlingua2-small", "longllmlingua-gpt2"):
            output = runs / f"{mode}.json"
            command(
                mode,
                [args.llmlingua_python, str(runners / "llmlingua_worker.py"), "--corpus", str(corpus), "--output", str(output), "--mode", mode],
                timeout=args.model_timeout_seconds,
                env_updates={"HF_HUB_OFFLINE": "1", "TRANSFORMERS_OFFLINE": "1"},
                output_path=output,
                failure_meta={"id": mode, "name": mode, "version": "0.2.2", "class": "prompt_compressor", "setting": "target-50", "surfaces": "prompt_context"},
            )
    else:
        attempts.append({"id": "llmlingua-family", "status": "not_attempted", "reason": "--llmlingua-python not supplied"})

    if args.rtk:
        rtk_path = Path(args.rtk).resolve()
        rtk_sha256 = (
            hashlib.sha256(rtk_path.read_bytes()).hexdigest()
            if rtk_path.is_file()
            else "missing"
        )
        expected_rtk_sha256 = "888ecfcc7ca6ceaf9170cf95027d196d6010c7d1a1892b3662b4bb61f18a3618"
        if rtk_sha256 != expected_rtk_sha256:
            raise SystemExit(
                f"RTK executable hash mismatch: expected {expected_rtk_sha256}, found {rtk_sha256}"
            )
        output = runs / "rtk-0.45.0.json"
        command(
            "rtk-0.45.0",
            [sys.executable, str(runners / "rtk_worker.py"), "--corpus", str(corpus), "--output", str(output), "--binary", args.rtk],
            output_path=output,
            failure_meta={"id": "rtk-0.45.0", "name": "RTK", "version": "0.45.0", "class": "command_output_optimizer", "setting": "documented filters", "surfaces": "tool_output"},
        )
    else:
        attempts.append({"id": "rtk", "status": "not_attempted", "reason": "--rtk not supplied"})

    omniroute_repo = (
        (project / args.omniroute_repo).resolve()
        if not Path(args.omniroute_repo).is_absolute()
        else Path(args.omniroute_repo)
    )
    if args.omniroute_node and omniroute_repo.exists():
        output = runs / "omniroute-deterministic-stack.json"
        command(
            "omniroute-deterministic-stack",
            [
                sys.executable,
                str(runners / "omniroute_worker.py"),
                "--corpus",
                str(corpus),
                "--output",
                str(output),
                "--node",
                args.omniroute_node,
                "--repo",
                str(omniroute_repo),
            ],
            timeout=600,
            output_path=output,
            failure_meta={
                "id": "omniroute-deterministic-stack",
                "name": "OmniRoute deterministic stack",
                "version": "3.8.50",
                "class": "composite_payload_optimizer",
                "setting": "rtk-standard+caveman-full",
                "surfaces": "prompt_context,tool_output",
            },
        )
    else:
        attempts.append(
            {
                "id": "omniroute",
                "status": "not_attempted",
                "reason": (
                    "--omniroute-node not supplied"
                    if not args.omniroute_node
                    else f"repository not found: {omniroute_repo}"
                ),
            }
        )

    caveman_repo = (project / args.caveman_repo).resolve() if not Path(args.caveman_repo).is_absolute() else Path(args.caveman_repo)
    caveman_output = runs / "caveman-upstream-snapshot.json"
    if caveman_output.exists():
        caveman_output.unlink()
    if caveman_repo.exists():
        command(
            "caveman-upstream-snapshot",
            [sys.executable, str(runners / "caveman_snapshot_worker.py"), "--repo", str(caveman_repo), "--output", str(caveman_output)],
            timeout=300,
        )
    else:
        attempts.append({"id": "caveman", "status": "not_attempted", "reason": f"repository not found: {caveman_repo}"})

    (logs / "model-cache-manifest.json").write_text(
        json.dumps(capture_model_cache(hf_cache), ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )

    def resolved_private_path(value: str | Path) -> str | None:
        candidate = Path(value)
        if candidate.is_absolute():
            return str(candidate.resolve())
        if "/" in str(value) or "\\" in str(value):
            return str((project / candidate).resolve())
        discovered = shutil.which(str(value))
        return str(Path(discovered).resolve()) if discovered else None

    redaction_candidates: list[tuple[str, str | Path | None]] = [
        ("KENDR_BINARY", executable),
        ("HF_CACHE", hf_cache),
        ("TIKTOKEN_CACHE", tiktoken_cache),
        ("HEADROOM_PYTHON", args.headroom_python),
        ("LLMLINGUA_PYTHON", args.llmlingua_python),
        ("RTK_BINARY", args.rtk),
        ("OMNIROUTE_NODE", args.omniroute_node),
        ("OMNIROUTE_REPO", omniroute_repo),
        ("CAVEMAN_REPO", caveman_repo),
    ]
    redaction_specs: list[str] = []
    for label, value in redaction_candidates:
        if value is None:
            continue
        resolved = resolved_private_path(value)
        if resolved is not None:
            redaction_specs.append(f"{label}={resolved}")

    finalizer_argv = [
        sys.executable,
        str(runners / "assemble_release.py"),
        "--release",
        str(release),
        "--project-root",
        str(project),
    ]
    verifier_argv = [
        sys.executable,
        str(runners / "verify_release.py"),
        "--release",
        str(release),
        "--project-root",
        str(project),
    ]
    if args.require_all_success:
        verifier_argv.append("--require-complete-attempts")
    for specification in redaction_specs:
        finalizer_argv.extend(["--redact-path", specification])
        verifier_argv.extend(["--redact-path", specification])

    execution = {
        "schema_version": "kendr.benchmark-execution/v1",
        "started_by": "execute_release.py",
        "completed_at": utc_now(),
        "release": release.name,
        "kendr_binary": {
            "path": str(executable),
            "sha256": binary_sha256,
            "bytes": executable.stat().st_size,
        },
        "finalizer": {
            "argv": finalizer_argv,
            "note": "Recorded before execution so one final assembly can checksum this ledger.",
        },
        "verifier": {
            "argv": verifier_argv,
            "require_complete_attempts": args.require_all_success,
            "note": "Runs after assembly without mutating the release bundle.",
        },
        "attempts": attempts,
    }
    (logs / "execution.json").write_text(
        json.dumps(execution, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    subprocess.run(
        finalizer_argv,
        cwd=project,
        check=True,
        timeout=300,
    )
    subprocess.run(
        verifier_argv,
        cwd=project,
        check=True,
        timeout=300,
    )
    print(release)


if __name__ == "__main__":
    main()
