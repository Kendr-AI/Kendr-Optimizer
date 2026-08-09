#!/usr/bin/env python3
"""Reproduce Caveman's committed offline token measurement without an LLM call."""

from __future__ import annotations

import argparse
import json
import os
import subprocess
import sys
from pathlib import Path

from common import SCHEMA_VERSION, count_tokens, environment, sha256_text


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--revision", default="fcf7663366c217dc8f334a11028de52ed950ceab")
    args = parser.parse_args()
    repository = Path(args.repo).resolve()
    revision_check = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repository,
        text=True,
        encoding="utf-8",
        capture_output=True,
        check=False,
        timeout=30,
    )
    resolved_revision = revision_check.stdout.strip()
    if revision_check.returncode != 0 or resolved_revision != args.revision:
        raise RuntimeError(
            f"expected Caveman revision {args.revision}, found {resolved_revision or revision_check.stderr.strip()}"
        )
    snapshot_path = repository / "evals" / "snapshots" / "results.json"
    skill_path = repository / "skills" / "caveman" / "SKILL.md"
    snapshot = json.loads(snapshot_path.read_text(encoding="utf-8"))
    skill = skill_path.read_text(encoding="utf-8")
    terse_prefix = str(snapshot.get("metadata", {}).get("terse_prefix", "Answer concisely."))
    caveman_prefix = terse_prefix + "\n\n" + skill
    input_overhead = count_tokens(caveman_prefix) - count_tokens(terse_prefix)

    measurement_environment = os.environ.copy()
    measurement_environment["PYTHONUTF8"] = "1"
    measurement = subprocess.run(
        [sys.executable, str(repository / "evals" / "measure.py")],
        cwd=repository,
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
        timeout=60,
        env=measurement_environment,
    )

    rows = []
    prompts = snapshot["prompts"]
    baseline = snapshot["arms"]["__baseline__"]
    terse = snapshot["arms"]["__terse__"]
    caveman = snapshot["arms"]["caveman"]
    for index, prompt in enumerate(prompts):
        baseline_tokens = count_tokens(baseline[index])
        terse_tokens = count_tokens(terse[index])
        caveman_tokens = count_tokens(caveman[index])
        additional_output_saving = terse_tokens - caveman_tokens
        rows.append(
            {
                "case_id": f"upstream-{index + 1:02d}",
                "status": "upstream_snapshot_remeasured",
                "prompt": prompt,
                "outputs": {
                    "baseline": baseline[index],
                    "terse": terse[index],
                    "caveman": caveman[index],
                },
                "tokens": {
                    "baseline_output": baseline_tokens,
                    "terse_output": terse_tokens,
                    "caveman_output": caveman_tokens,
                    "caveman_input_overhead_vs_terse": input_overhead,
                    "additional_output_saving_vs_terse": additional_output_saving,
                    "net_token_saving_vs_terse_single_turn": additional_output_saving
                    - input_overhead,
                },
                "quality": {
                    "status": "unscored",
                    "reason": "The committed snapshot has no task-native expected answer or paired quality grade.",
                },
            }
        )

    payload = {
        "schema_version": SCHEMA_VERSION,
        "optimizer": {
            "id": "caveman-v1.10.0-upstream-snapshot",
            "name": "Caveman",
            "version": "1.10.0",
            "revision": args.revision,
            "source": "https://github.com/JuliusBrussee/caveman",
            "class": "generation_policy",
            "setting": "upstream committed Claude snapshot",
        },
        "environment": environment(),
        "evidence": {
            "origin": "upstream_committed_snapshot",
            "fresh_model_execution": False,
            "snapshot_path": str(snapshot_path),
            "snapshot_sha256": sha256_text(snapshot_path.read_text(encoding="utf-8")),
            "snapshot_metadata": snapshot.get("metadata", {}),
            "skill_sha256": sha256_text(skill),
            "input_overhead_tokens_vs_terse": input_overhead,
            "official_measure_exit_code": measurement.returncode,
            "official_measure_stdout": measurement.stdout,
            "official_measure_stderr": measurement.stderr,
        },
        "notes": [
            "This reruns the official tokenizer measurement over upstream's committed outputs; it is not a fresh Claude A/B.",
            "Caveman affects future generation and is not ranked beside input or command-output compressors.",
            "o200k_base is an approximation of Claude tokenization, exactly as disclosed by the upstream script.",
        ],
        "cases": rows,
        "raw_upstream_snapshot": snapshot,
    }
    destination = Path(args.output)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )


if __name__ == "__main__":
    main()
