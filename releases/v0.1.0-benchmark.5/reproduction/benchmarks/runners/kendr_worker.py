#!/usr/bin/env python3
"""Run the native KendrOptimizer CLI on the common peer corpus."""

from __future__ import annotations

import argparse
import json
import subprocess
from typing import Any

from common import load_corpus, run_cases, write_run


def make_request(case: dict[str, Any], mode: str) -> dict[str, Any]:
    tool_surface = case["surface"] == "tool_output"
    primary_part: dict[str, Any]
    if tool_surface:
        primary_part = {
            "type": "tool_result",
            "call_id": f"benchmark-{case['id']}",
            "name": case["content_type"],
            "content": case["text"],
            "is_error": "error" in str(case["text"]).lower(),
        }
    else:
        primary_part = {"type": "text", "text": case["text"]}

    extractive = mode == "extractive-tool-output"
    request = {
        "schema_version": "kendr.optimize/v1",
        "phase": "tool_result" if tool_surface else "request",
        "request_id": f"peer-{mode}-{case['id']}",
        "session_id": "peer-release-v1",
        "content": {
            "messages": [
                {
                    "id": f"{case['id']}-payload",
                    "role": "tool" if tool_surface else "user",
                    "parts": [primary_part],
                },
                {
                    "id": f"{case['id']}-query",
                    "role": "user",
                    "parts": [{"type": "text", "text": f"Question: {case['query']}"}],
                },
            ]
        },
        "target": {"tokenizer_profile": "o200k_base"},
    }
    if mode == "default":
        return request
    request["host_capabilities"] = {
        "can_restore_references": True,
        "can_retry_with_full_tools": True,
        "streaming_output": True,
    }
    request["policy"] = {
            "risk_ceiling": "extractive" if extractive else "recoverable",
            "min_gain_tokens": 1,
            "min_gain_percent": 0.0,
            "latency_budget_ms": 5000,
            "preserve_cache_prefix": False,
            "preserve_recent_messages": 0,
            "max_tool_result_chars": 2000,
            "enable_lossy_tool_output": extractive,
            "enable_tool_selection": False,
            "enable_generation_policy": False,
    }
    return request


def returned_text(part: dict[str, Any]) -> str:
    if part["type"] in {"text", "code", "document"}:
        return str(part["text"])
    if part["type"] == "tool_result":
        return str(part["content"])
    if part["type"] == "json":
        return json.dumps(part["value"], ensure_ascii=False, separators=(",", ":"))
    return json.dumps(part, ensure_ascii=False, separators=(",", ":"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument(
        "--mode",
        choices=["default", "safe-low-threshold", "extractive-tool-output"],
        required=True,
    )
    parser.add_argument("--revision", default="unversioned-worktree")
    args = parser.parse_args()
    corpus = load_corpus(args.corpus)

    def transform(case: dict[str, Any]) -> tuple[str, str, dict[str, Any]]:
        request = make_request(case, args.mode)
        completed = subprocess.run(
            [args.binary, "optimize", "--compact"],
            input=json.dumps(request, ensure_ascii=False),
            text=True,
            encoding="utf-8",
            capture_output=True,
            check=False,
            timeout=30,
        )
        if completed.returncode != 0:
            raise RuntimeError(
                f"kendr-opt exit={completed.returncode}: {completed.stderr.strip()}"
            )
        outcome = json.loads(completed.stdout)
        messages = outcome["content"]["messages"]
        primary = "\n".join(returned_text(part) for part in messages[0]["parts"])
        query = str(case["query"])
        combined = primary if not query else f"{primary}\n\nQuestion: {query}"
        return combined, primary, {
            "receipt": outcome["receipt"],
            "generation_recommendation": outcome.get("generation_recommendation"),
            "recovery": outcome.get("recovery"),
            "stderr": completed.stderr,
            "raw_outcome": outcome,
        }

    write_run(
        args.output,
        optimizer={
            "id": f"kendr-{args.mode}",
            "name": "KendrOptimizer",
            "version": "0.1.0-dev",
            "revision": args.revision,
            "class": "structured_payload_optimizer",
            "setting": args.mode,
        },
        corpus=corpus,
        cases=run_cases(
            corpus,
            transform,
            surfaces={"tool_output"}
            if args.mode == "extractive-tool-output"
            else None,
        ),
        notes=[
            "default uses the shipped OptimizationPolicy defaults; safe-low-threshold changes gates for diagnostic benchmarking; extractive-tool-output is opt-in Q3.",
            "Independent o200k_base counts are computed over visible text; the native receipt is retained verbatim.",
        ],
    )


if __name__ == "__main__":
    main()
