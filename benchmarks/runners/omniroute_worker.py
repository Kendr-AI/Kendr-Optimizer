#!/usr/bin/env python3
"""Run OmniRoute's deterministic RTK -> Caveman stack without its gateway."""

from __future__ import annotations

import argparse
import json
import subprocess
from pathlib import Path
from typing import Any

from common import load_corpus, run_cases, write_run


EXPECTED_COMMIT = "1e15583f294d9d137320fe544288ca34c9435351"


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--node", required=True)
    parser.add_argument("--repo", required=True)
    parser.add_argument("--revision", default=EXPECTED_COMMIT)
    args = parser.parse_args()

    repository = Path(args.repo).resolve()
    if not repository.is_dir():
        raise RuntimeError(f"OmniRoute repository not found: {repository}")
    head = subprocess.run(
        ["git", "-C", str(repository), "rev-parse", "HEAD"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    ).stdout.strip()
    if head != args.revision:
        raise RuntimeError(f"expected OmniRoute {args.revision}, found {head}")

    node_version = subprocess.run(
        [args.node, "--version"],
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
    ).stdout.strip()
    corpus = load_corpus(args.corpus)
    bridge = Path(__file__).with_name("omniroute_bridge.mjs").resolve()
    completed = subprocess.run(
        [
            args.node,
            "--experimental-strip-types",
            str(bridge),
            "--repo",
            str(repository),
        ],
        cwd=repository,
        input=json.dumps({"cases": corpus["cases"]}, ensure_ascii=False),
        check=True,
        capture_output=True,
        text=True,
        encoding="utf-8",
        timeout=300,
    )
    upstream = json.loads(completed.stdout)
    by_id = {item["case_id"]: item for item in upstream["results"]}

    def transform(case: dict[str, Any]) -> tuple[str, str, dict[str, Any]]:
        item = by_id[case["id"]]
        primary = str(item["primary_output"])
        output = f"{primary}\n\nQuestion: {case['query']}"
        return output, primary, dict(item["native_metrics"])

    write_run(
        args.output,
        optimizer={
            "id": "omniroute-deterministic-stack",
            "name": "OmniRoute deterministic stack",
            "version": "3.8.50",
            "revision": head,
            "source": "https://github.com/diegosouzapw/OmniRoute",
            "class": "composite_payload_optimizer",
            "setting": "rtk-standard+caveman-full",
        },
        corpus=corpus,
        cases=run_cases(corpus, transform),
        notes=[
            "The gateway/router was not started and no provider was called.",
            "This invokes OmniRoute's pinned pure applyRtkCompression and cavemanCompress modules in its documented default RTK-then-Caveman order.",
            "The current query is protected outside the transform and counted unchanged, matching the cross-peer benchmark rule.",
            f"Node {node_version} executed upstream TypeScript using built-in type stripping.",
        ],
    )


if __name__ == "__main__":
    main()
