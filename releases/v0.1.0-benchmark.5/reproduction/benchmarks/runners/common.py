"""Shared benchmark result schema, accounting, and preservation proxies."""

from __future__ import annotations

import hashlib
import json
import platform
import re
import sys
import time
from pathlib import Path
from typing import Any, Callable

import tiktoken


SCHEMA_VERSION = "kendr.peer-run/v1"
TOKENIZER = "tiktoken:o200k_base"
ENCODING = tiktoken.get_encoding("o200k_base")
URL_RE = re.compile(r"https?://[^\s\]\[<>()\"']+")
WINDOWS_PATH_RE = re.compile(r"[A-Za-z]:/[A-Za-z0-9._/\-]+")
NUMBER_RE = re.compile(r"(?<![\w.-])-?\d+(?:\.\d+)?(?![\w.-])")


def load_corpus(path: str | Path) -> dict[str, Any]:
    data = json.loads(Path(path).read_text(encoding="utf-8"))
    if data.get("schema_version") != "kendr.benchmark-corpus/v1":
        raise ValueError("unsupported corpus schema")
    return data


def request_text(case: dict[str, Any]) -> str:
    query = str(case.get("query", "")).strip()
    text = str(case["text"])
    return text if not query else f"{text}\n\nQuestion: {query}"


def count_tokens(value: str) -> int:
    return len(ENCODING.encode(value))


def sha256_text(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def _recall(expected: set[str], output: str) -> tuple[int, int, float | None]:
    if not expected:
        return 0, 0, None
    retained = sum(1 for item in expected if item in output)
    return retained, len(expected), retained / len(expected)


def _json_equivalent(original: str, candidate: str) -> bool | None:
    try:
        original_value = json.loads(original)
    except json.JSONDecodeError:
        return None
    try:
        candidate_value = json.loads(candidate)
    except json.JSONDecodeError:
        return False
    return candidate_value == original_value


def score_case(
    case: dict[str, Any],
    input_text: str,
    output_text: str,
    *,
    primary_output: str | None = None,
) -> dict[str, Any]:
    primary = output_text if primary_output is None else primary_output
    required = list(dict.fromkeys(str(item) for item in case.get("required_literals", [])))
    required_kept, required_total, required_recall = _recall(set(required), primary)
    urls_kept, urls_total, url_recall = _recall(set(URL_RE.findall(input_text)), output_text)
    paths_kept, paths_total, path_recall = _recall(
        set(WINDOWS_PATH_RE.findall(input_text)), output_text
    )
    numbers_kept, numbers_total, number_recall = _recall(
        set(NUMBER_RE.findall(input_text)), output_text
    )
    input_tokens = count_tokens(input_text)
    output_tokens = count_tokens(output_text)
    token_delta = input_tokens - output_tokens
    json_equivalent = None
    if case.get("json_equivalence"):
        json_equivalent = _json_equivalent(str(case["text"]), primary)
    query = str(case.get("query", "")).strip()
    query_marker = f"Question: {query}" if query else ""
    query_preserved = not query_marker or query_marker in output_text
    hard_gate = (
        required_recall == 1.0
        and json_equivalent is not False
        and query_preserved
    )
    return {
        "input": {
            "bytes": len(input_text.encode("utf-8")),
            "tokens": input_tokens,
            "sha256": sha256_text(input_text),
        },
        "output": {
            "bytes": len(output_text.encode("utf-8")),
            "tokens": output_tokens,
            "sha256": sha256_text(output_text),
        },
        "token_delta": token_delta,
        "token_reduction_percent": round(100 * token_delta / input_tokens, 4)
        if input_tokens
        else 0.0,
        "byte_delta": len(input_text.encode("utf-8")) - len(output_text.encode("utf-8")),
        "required_literal_recall": required_recall,
        "required_literals_retained": required_kept,
        "required_literals_total": required_total,
        "url_recall": url_recall,
        "urls_retained": urls_kept,
        "urls_total": urls_total,
        "path_recall": path_recall,
        "paths_retained": paths_kept,
        "paths_total": paths_total,
        "number_recall": number_recall,
        "numbers_retained": numbers_kept,
        "numbers_total": numbers_total,
        "json_equivalent": json_equivalent,
        "query_preserved_exactly": query_preserved,
        "preservation_proxy_pass": hard_gate,
        "proxy_notice": (
            "Literal, exact-query, and structural preservation only; this is not downstream model quality."
        ),
    }


def run_cases(
    corpus: dict[str, Any],
    transform: Callable[[dict[str, Any]], tuple[str, str, dict[str, Any]]],
    *,
    surfaces: set[str] | None = None,
) -> list[dict[str, Any]]:
    results: list[dict[str, Any]] = []
    for case in corpus["cases"]:
        if surfaces is not None and case["surface"] not in surfaces:
            results.append(
                {
                    "case_id": case["id"],
                    "surface": case["surface"],
                    "status": "unsupported",
                    "reason": "surface is outside this optimizer's declared scope",
                }
            )
            continue
        started = time.perf_counter()
        try:
            output_text, primary_output, native = transform(case)
            elapsed_ms = (time.perf_counter() - started) * 1000
            source = request_text(case)
            results.append(
                {
                    "case_id": case["id"],
                    "surface": case["surface"],
                    "content_type": case["content_type"],
                    "status": "ok",
                    "elapsed_ms": round(elapsed_ms, 3),
                    "input_text": source,
                    "output_text": output_text,
                    "primary_output": primary_output,
                    "native_metrics": native,
                    "score": score_case(
                        case, source, output_text, primary_output=primary_output
                    ),
                }
            )
        except Exception as error:  # benchmark failures must remain in the run
            results.append(
                {
                    "case_id": case["id"],
                    "surface": case["surface"],
                    "content_type": case["content_type"],
                    "status": "failed",
                    "elapsed_ms": round((time.perf_counter() - started) * 1000, 3),
                    "error_type": type(error).__name__,
                    "error": str(error),
                }
            )
    return results


def environment() -> dict[str, Any]:
    return {
        "python": sys.version,
        "platform": platform.platform(),
        "machine": platform.machine(),
        "processor": platform.processor(),
        "tokenizer": TOKENIZER,
        "tiktoken_version": getattr(tiktoken, "__version__", "unknown"),
    }


def write_run(
    destination: str | Path,
    *,
    optimizer: dict[str, Any],
    corpus: dict[str, Any],
    cases: list[dict[str, Any]],
    notes: list[str] | None = None,
) -> None:
    payload = {
        "schema_version": SCHEMA_VERSION,
        "optimizer": optimizer,
        "corpus": {
            "id": corpus["corpus_id"],
            "canonical_json_sha256": sha256_text(
                json.dumps(corpus, sort_keys=True, ensure_ascii=False)
            ),
            "case_count": len(corpus["cases"]),
        },
        "environment": environment(),
        "notes": notes or [],
        "cases": cases,
    }
    path = Path(destination)
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
