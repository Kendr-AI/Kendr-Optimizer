#!/usr/bin/env python3
"""Run the pinned Headroom library without its provider proxy."""

from __future__ import annotations

import argparse
import importlib.metadata
import json
import os
from pathlib import Path
from typing import Any

from common import load_corpus, run_cases, write_run


KOMPRESS_MODEL = "chopratejas/kompress-v2-base"
KOMPRESS_REVISION = "b1563631b35bfdcee37587ad530147497d820d4c"
MODERNBERT_MODEL = "answerdotai/ModernBERT-base"
MODERNBERT_REVISION = "8949b909ec900327062f0ebf497f51aef5e6f0c8"


def require_pinned_snapshot(model_id: str, revision: str) -> None:
    cache_name = "models--" + model_id.replace("/", "--")
    root = Path(os.environ["HF_HOME"]) / "hub" / cache_name
    snapshot = root / "snapshots" / revision
    if not snapshot.is_dir():
        raise RuntimeError(f"missing pinned model snapshot: {model_id}@{revision}")
    main_ref = root / "refs" / "main"
    if not main_ref.is_file() or main_ref.read_text(encoding="utf-8").strip() != revision:
        raise RuntimeError(f"{model_id} main cache ref is not pinned to {revision}")


def visible_text(content: Any) -> str:
    if isinstance(content, str):
        return content
    if isinstance(content, list):
        chunks: list[str] = []
        for item in content:
            if isinstance(item, str):
                chunks.append(item)
            elif isinstance(item, dict):
                value = item.get("text") or item.get("content")
                if isinstance(value, str):
                    chunks.append(value)
        return "\n".join(chunks)
    return json.dumps(content, ensure_ascii=False, separators=(",", ":"))


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument(
        "--mode",
        choices=[
            "structural-default",
            "structural-target-50",
            "kompress-target-50",
        ],
        required=True,
    )
    parser.add_argument("--revision", default="9fd5ae3d530b5d9eb9e9127e413235a5a3b2faf6")
    args = parser.parse_args()

    from headroom import compress

    package_version = importlib.metadata.version("headroom-ai")
    if package_version != "0.34.0":
        raise RuntimeError(f"expected headroom-ai 0.34.0, found {package_version}")

    kompress_ready = False
    if args.mode == "kompress-target-50":
        require_pinned_snapshot(KOMPRESS_MODEL, KOMPRESS_REVISION)
        require_pinned_snapshot(MODERNBERT_MODEL, MODERNBERT_REVISION)
        from headroom.transforms.kompress_compressor import warm_kompress_model

        kompress_ready = warm_kompress_model(
            KOMPRESS_MODEL, device="cpu", allow_download=False
        )
        if not kompress_ready:
            raise RuntimeError("pinned Headroom Kompress model did not warm successfully")

    corpus = load_corpus(args.corpus)

    def transform(case: dict[str, Any]) -> tuple[str, str, dict[str, Any]]:
        tool_surface = case["surface"] == "tool_output"
        first: dict[str, Any] = {
            "role": "tool" if tool_surface else "user",
            "content": case["text"],
        }
        if tool_surface:
            first["tool_call_id"] = f"benchmark-{case['id']}"
        messages = [
            first,
            {"role": "user", "content": f"Question: {case['query']}"},
        ]
        kwargs: dict[str, Any] = {"kompress_model": "disabled"}
        if args.mode in {"structural-target-50", "kompress-target-50"}:
            kwargs = {
                "compress_user_messages": True,
                "compress_system_messages": True,
                "target_ratio": 0.5,
                "protect_recent": 1,
                "protect_analysis_context": False,
                "min_tokens_to_compress": 0,
                "kompress_model": (
                    KOMPRESS_MODEL
                    if args.mode == "kompress-target-50"
                    else "disabled"
                ),
            }
        result = compress(messages, model="gpt-4o", **kwargs)
        returned = result.messages
        if not returned:
            returned = messages
        primary = visible_text(returned[0].get("content", ""))
        returned_query = "\n".join(
            visible_text(message.get("content", "")) for message in returned[1:]
        )
        expected_query = f"Question: {case['query']}"
        if returned_query != expected_query:
            raise RuntimeError("Headroom changed the protected benchmark query")
        combined = f"{primary}\n\n{expected_query}"
        return combined, primary, {
            "tokens_before": result.tokens_before,
            "tokens_after": result.tokens_after,
            "tokens_saved": result.tokens_saved,
            "compression_ratio": result.compression_ratio,
            "transforms_applied": list(result.transforms_applied),
            "raw_messages": returned,
            "kompress_model_ready": kompress_ready,
        }

    write_run(
        args.output,
        optimizer={
            "id": f"headroom-{args.mode}",
            "name": (
                "Headroom (Kompress + structural)"
                if args.mode == "kompress-target-50"
                else "Headroom (structural routers only)"
            ),
            "version": package_version,
            "revision": args.revision,
            "source": "https://github.com/headroomlabs-ai/headroom",
            "class": "structured_context_optimizer",
            "setting": args.mode,
            **(
                {
                    "model": KOMPRESS_MODEL,
                    "model_revision": KOMPRESS_REVISION,
                    "tokenizer_model": MODERNBERT_MODEL,
                    "tokenizer_model_revision": MODERNBERT_REVISION,
                }
                if args.mode == "kompress-target-50"
                else {}
            ),
        },
        corpus=corpus,
        cases=run_cases(corpus, transform),
        notes=[
            (
                "Headroom was called as a local library with its pinned Kompress-v2-base ONNX model warmed from an offline cache; its provider proxy was not started."
                if args.mode == "kompress-target-50"
                else "Headroom was called as a local library; its provider proxy and learned Kompress model were disabled."
            ),
            (
                "This arm includes Headroom's learned Kompress model plus its structural routers."
                if args.mode == "kompress-target-50"
                else "This arm benchmarks only Headroom's deterministic structural routers and is not labeled as Headroom's full/default optimizer."
            ),
            f"{args.mode} is an explicit keep-ratio setting when applicable and is not an automatically discovered optimum.",
            "The current query is protected and counted unchanged; reported reduction is payload-only.",
        ],
    )


if __name__ == "__main__":
    main()
