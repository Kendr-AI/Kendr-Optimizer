#!/usr/bin/env python3
"""Emit the pass-through arm using the shared accounting path."""

from __future__ import annotations

import argparse

from common import load_corpus, request_text, run_cases, write_run


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", required=True)
    parser.add_argument("--output", required=True)
    args = parser.parse_args()
    corpus = load_corpus(args.corpus)

    def passthrough(case: dict[str, object]) -> tuple[str, str, dict[str, object]]:
        return request_text(case), str(case["text"]), {"mode": "pass_through"}

    write_run(
        args.output,
        optimizer={
            "id": "pass-through",
            "name": "Unoptimized pass-through",
            "version": "1",
            "class": "baseline",
            "setting": "none",
        },
        corpus=corpus,
        cases=run_cases(corpus, passthrough),
    )


if __name__ == "__main__":
    main()
