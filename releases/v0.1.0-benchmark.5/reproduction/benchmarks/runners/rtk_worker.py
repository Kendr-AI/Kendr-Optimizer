#!/usr/bin/env python3
"""Run RTK only on its declared command/tool-output surface."""

from __future__ import annotations

import argparse
import os
import subprocess
import tempfile
from pathlib import Path
from typing import Any

from common import load_corpus, run_cases, write_run


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--corpus", required=True)
    parser.add_argument("--output", required=True)
    parser.add_argument("--binary", required=True)
    parser.add_argument("--revision", default="b34be37caf3796b69a50952a28e60e32b5daad43")
    args = parser.parse_args()
    corpus = load_corpus(args.corpus)
    environment = os.environ.copy()
    environment.update({"NO_COLOR": "1", "TERM": "dumb"})

    version = subprocess.run(
        [args.binary, "--version"],
        text=True,
        encoding="utf-8",
        errors="replace",
        capture_output=True,
        check=False,
        env=environment,
        timeout=10,
    )
    version_text = (version.stdout or version.stderr).strip()
    if version.returncode != 0 or "0.45.0" not in version_text:
        raise RuntimeError(f"expected RTK 0.45.0, found: {version_text!r}")

    def transform(case: dict[str, Any]) -> tuple[str, str, dict[str, Any]]:
        filter_name = case.get("rtk_filter")
        if not filter_name:
            raise ValueError("no documented RTK filter for this fixture")
        text = str(case["text"])
        with tempfile.TemporaryDirectory(prefix="kendr-rtk-") as temporary:
            if filter_name == "json":
                source = Path(temporary) / "fixture.json"
                source.write_text(text, encoding="utf-8")
                command = [args.binary, "json", str(source)]
                standard_input = None
            elif filter_name == "log":
                source = Path(temporary) / "fixture.log"
                source.write_text(text, encoding="utf-8")
                command = [args.binary, "log", str(source)]
                standard_input = None
            else:
                command = [args.binary, "pipe", "--filter", str(filter_name)]
                standard_input = text
            completed = subprocess.run(
                command,
                input=standard_input,
                text=True,
                encoding="utf-8",
                errors="replace",
                capture_output=True,
                check=False,
                env=environment,
                timeout=60,
            )
        if completed.returncode != 0:
            raise RuntimeError(
                f"RTK exit={completed.returncode}: {completed.stderr.strip()}"
            )
        primary = completed.stdout.rstrip("\r\n")
        query = str(case["query"])
        combined = primary if not query else f"{primary}\n\nQuestion: {query}"
        return combined, primary, {
            "command": command,
            "exit_code": completed.returncode,
            "stderr": completed.stderr,
            "rtk_version_output": version_text,
            "token_method": "independent tiktoken o200k_base (RTK native estimates bytes/4)",
        }

    raw_cases = run_cases(corpus, transform, surfaces={"tool_output"})
    for result in raw_cases:
        if result.get("status") == "failed" and "no documented RTK filter" in result.get("error", ""):
            result["status"] = "unsupported"
            result["reason"] = result.pop("error")
            result.pop("error_type", None)

    write_run(
        args.output,
        optimizer={
            "id": "rtk-0.45.0",
            "name": "RTK",
            "version": "0.45.0",
            "revision": args.revision,
            "source": "https://github.com/rtk-ai/rtk",
            "class": "command_output_optimizer",
            "setting": "documented filter per fixture",
            "version_output": version_text,
        },
        corpus=corpus,
        cases=raw_cases,
        notes=[
            "RTK is scored only on command/tool output; prompt-context cases are retained as unsupported.",
            "The user query is preserved outside RTK and included unchanged in independent request-text accounting.",
        ],
    )


if __name__ == "__main__":
    main()
