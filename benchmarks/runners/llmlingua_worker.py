#!/usr/bin/env python3
"""Run LLMLingua-family algorithms in a pinned, CPU-only benchmark process."""

from __future__ import annotations

import argparse
import importlib.metadata
import re
from typing import Any

from common import load_corpus, run_cases, write_run


LLMLINGUA2_SMALL = (
    "microsoft/llmlingua-2-bert-base-multilingual-cased-meetingbank"
)
LLMLINGUA2_REVISION = "5f0c82792b7ea14c6484e015b6a072009496b7f2"
GPT2_REVISION = "607a30d783dfa663caf39e06633721c8d4cfcd7e"


def split_retrieved_documents(text: str) -> list[str]:
    """Keep authored retrieval documents separate for LongLLMLingua ranking."""
    documents = re.split(r"\n\n(?=Document (?:[A-Z]|\d+) \u2014 )", text.strip())
    return [document.strip() for document in documents if document.strip()]


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument(
        "--mode", choices=["llmlingua-gpt2", "llmlingua2-small", "longllmlingua-gpt2"], required=True
    )
    parser.add_argument("--revision", default="a411a3fa61df74411157b2512b592d5357bd8f17")
    args = parser.parse_args()

    from llmlingua import PromptCompressor

    package_version = importlib.metadata.version("llmlingua")
    if package_version != "0.2.2":
        raise RuntimeError(f"expected llmlingua 0.2.2, found {package_version}")

    model_name = LLMLINGUA2_SMALL if args.mode == "llmlingua2-small" else "gpt2"
    model_revision = LLMLINGUA2_REVISION if args.mode == "llmlingua2-small" else GPT2_REVISION
    compressor = PromptCompressor(
        model_name=model_name,
        device_map="cpu",
        use_llmlingua2=args.mode == "llmlingua2-small",
        model_config={"revision": model_revision, "trust_remote_code": False},
    )
    corpus = load_corpus(args.corpus)

    def transform(case: dict[str, Any]) -> tuple[str, str, dict[str, Any]]:
        if args.mode == "longllmlingua-gpt2" and case["content_type"] != "retrieved_documents":
            raise ValueError("LongLLMLingua is only applicable to the retrieved-document case")
        if args.mode == "llmlingua2-small":
            result = compressor.compress_prompt(
                str(case["text"]),
                rate=0.5,
                force_tokens=["\n", "?"],
                force_reserve_digit=True,
            )
            primary = str(result["compressed_prompt"])
        else:
            contexts = [str(case["text"])]
            if args.mode == "longllmlingua-gpt2":
                contexts = split_retrieved_documents(str(case["text"]))
                if len(contexts) < 2:
                    raise ValueError("LongLLMLingua requires separately ranked documents")
            kwargs: dict[str, Any] = {
                "context": contexts,
                "question": str(case["query"]),
                "rate": 0.5,
                "force_tokens": ["\n", "?"],
                "force_reserve_digit": True,
            }
            if args.mode == "longllmlingua-gpt2":
                kwargs.update(
                    {
                        "condition_in_question": "after_condition",
                        "reorder_context": "sort",
                        "dynamic_context_compression_ratio": 0.3,
                        "condition_compare": True,
                        "context_budget": "+100",
                        "rank_method": "longllmlingua",
                    }
                )
            result = compressor.compress_prompt(**kwargs)
            output = str(result["compressed_prompt"])
            suffix = f"\n\n{case['query']}"
            primary = output[: -len(suffix)] if output.endswith(suffix) else output
        raw_compressed_prompt = str(result["compressed_prompt"])
        output = f"{primary}\n\nQuestion: {case['query']}"
        native = {key: value for key, value in result.items() if key != "compressed_prompt"}
        native["raw_compressed_prompt"] = raw_compressed_prompt
        native["model_name"] = model_name
        native["model_revision"] = model_revision
        native["device_map"] = "cpu"
        native["target_keep_ratio"] = 0.5
        native["force_tokens"] = ["\n", "?"]
        native["force_reserve_digit"] = True
        native["trust_remote_code"] = False
        if args.mode == "longllmlingua-gpt2":
            native["retrieved_document_count"] = len(
                split_retrieved_documents(str(case["text"]))
            )
        return output, primary, native

    raw_cases = run_cases(
        corpus,
        transform,
        surfaces={"prompt_context"},
    )
    if args.mode == "longllmlingua-gpt2":
        for result in raw_cases:
            if result.get("status") == "failed" and "only applicable" in result.get("error", ""):
                result["status"] = "unsupported"
                result["reason"] = result.pop("error")
                result.pop("error_type", None)

    display_name = {
        "llmlingua-gpt2": "LLMLingua (GPT-2 feasibility profile)",
        "llmlingua2-small": "LLMLingua-2 small",
        "longllmlingua-gpt2": "LongLLMLingua (GPT-2 feasibility profile)",
    }[args.mode]
    notes = [
        "The 0.50 keep rate is configured, not an automatically discovered optimum.",
        "All inference ran locally on CPU; no target LLM or provider was called.",
        "The query is supplied for original/LongLLMLingua ranking where supported, then protected and counted unchanged in every arm.",
    ]
    if "gpt2" in args.mode:
        notes.append(
            "GPT-2 is a deliberately small feasibility model. It is not the canonical 7B default, so do not generalize these numbers to the paper configuration."
        )
    write_run(
        args.output,
        optimizer={
            "id": args.mode,
            "name": display_name,
            "version": package_version,
            "revision": args.revision,
            "source": "https://github.com/microsoft/LLMLingua",
            "class": "prompt_compressor",
            "setting": "target-50",
            "model": model_name,
            "model_revision": model_revision,
        },
        corpus=corpus,
        cases=raw_cases,
        notes=notes,
    )


if __name__ == "__main__":
    main()
