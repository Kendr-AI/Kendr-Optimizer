#!/usr/bin/env python3
"""Build the deterministic, redistributable KendrOptimizer peer corpus."""

from __future__ import annotations

import json
from pathlib import Path


ROOT = Path(__file__).resolve().parent


def case(
    case_id: str,
    surface: str,
    content_type: str,
    text: str,
    query: str,
    required: list[str],
    *,
    rtk_filter: str | None = None,
    json_equivalence: bool = False,
) -> dict[str, object]:
    return {
        "id": case_id,
        "surface": surface,
        "content_type": content_type,
        "text": text,
        "query": query,
        "required_literals": required,
        "rtk_filter": rtk_filter,
        "json_equivalence": json_equivalence,
        "source": "KendrOptimizer authored fixture",
        "license": "Apache-2.0",
        "development_split": "public-challenge",
    }


def build() -> dict[str, object]:
    pretty_json = json.dumps(
        {
            "service": "payments-api",
            "endpoint": "https://api.example.test/v2/charges",
            "retry_policy": {"attempts": 17, "backoff_ms": [50, 100, 250]},
            "constraints": {
                "tls_required": True,
                "instruction": "must not disable TLS",
                "certificate": "C:/certs/prod-chain.pem",
            },
            "records": [
                {
                    "id": f"charge-{index:04d}",
                    "currency": "USD",
                    "amount_cents": 1299 + index,
                    "status": "captured" if index % 5 else "review",
                }
                for index in range(80)
            ],
        },
        indent=2,
        ensure_ascii=False,
    )

    rag_paragraph = (
        "The Atlas edge service listens on port 8443. During incident INC-2048, "
        "the client retried exactly 17 times. Operators must not disable TLS. "
        "The approved runbook is https://docs.example.test/runbooks/INC-2048 and "
        "the certificate path is C:/certs/prod-chain.pem. "
    )
    rag_text = "\n\n".join(
        [
            "Document A — authoritative incident record\n" + rag_paragraph,
            *[
                f"Document {index} — unrelated capacity note\n"
                + "The staging workers were healthy and no production action was approved. " * 8
                for index in range(2, 18)
            ],
            "Document Z — authoritative remediation\n" + rag_paragraph,
        ]
    )

    repetitive_log_lines = [
        "\x1b[32mINFO\x1b[0m heartbeat shard=blue state=healthy latency_ms=12"
        for _ in range(180)
    ]
    repetitive_log_lines[73] = (
        "ERROR request_id=req-7f9a failed certificate validation at C:/certs/prod-chain.pem"
    )
    repetitive_log_lines[74] = "Caused by: TLS chain expired; code=E_CERT_42; retry_count=17"
    repetitive_log_lines.extend(
        ["INFO draining worker shard=blue state=healthy" for _ in range(80)]
    )
    repetitive_log = "\n".join(repetitive_log_lines)

    pytest_lines = [
        f"tests/test_worker.py::test_case_{index:03d} PASSED [ 42%]" for index in range(140)
    ]
    pytest_lines += [
        "tests/test_payment.py::test_tls_chain FAILED [ 97%]",
        "E   AssertionError: expected status=200, actual status=526",
        "E   request_id=req-7f9a endpoint=https://api.example.test/v2/charges",
        "1 failed, 140 passed in 12.84s",
    ]
    pytest_output = "\n".join(pytest_lines)

    git_log = "\n".join(
        [
            f"{index:07x} author-{index % 5} 2026-08-{(index % 7) + 1:02d} "
            f"refactor worker batch {index}"
            for index in range(1, 121)
        ]
        + ["7f9a204 release-bot 2026-08-07 fix TLS certificate rotation for INC-2048"]
    )

    code_block = """// src/transport/retry.ts
export const MAX_RETRIES = 17;
export const ENDPOINT = "https://api.example.test/v2/charges";
export function connect(path = "C:/certs/prod-chain.pem") {
  if (!path) throw new Error("E_CERT_42: must not disable TLS");
  return { port: 8443, path };
}
"""
    code_context = "\n".join(
        [code_block]
        + [f"// generated commentary block {index}\n" + ("// ordinary implementation note\n" * 9) for index in range(30)]
        + [code_block]
    )

    multilingual = (
        "EN: Keep request_id=req-7f9a and do not disable TLS. "
        "HI: पोर्ट 8443 को न बदलें। "
        "ES: Conserve exactamente 17 reintentos. "
        "JA: 証明書 C:/certs/prod-chain.pem を保持してください。 "
        "URL: https://api.example.test/v2/charges\n"
    ) * 22

    redundant_prose = (
        "Deployment policy\n\n\n\n"
        "The production change requires two reviewers. The emergency path is not authorized.\n\n\n\n"
        "Preserve request_id=req-7f9a and ticket INC-2048.\n\n\n\n"
    ) * 35

    exact_short = (
        "Return request_id=req-7f9a unchanged. Use port 8443. "
        "Do not disable TLS."
    )

    cases = [
        case(
            "short-exact-noop",
            "prompt_context",
            "plain_text",
            exact_short,
            "What are the constraints?",
            ["request_id=req-7f9a", "8443", "Do not disable TLS"],
        ),
        case(
            "redundant-prose",
            "prompt_context",
            "plain_text",
            redundant_prose,
            "State the production approval rule and identifiers.",
            ["two reviewers", "not authorized", "request_id=req-7f9a", "INC-2048"],
        ),
        case(
            "rag-incident",
            "prompt_context",
            "retrieved_documents",
            rag_text,
            "Which port, retry count, TLS rule, runbook, and certificate path are authoritative?",
            [
                "8443",
                "17 times",
                "must not disable TLS",
                "https://docs.example.test/runbooks/INC-2048",
                "C:/certs/prod-chain.pem",
            ],
        ),
        case(
            "pretty-json",
            "tool_output",
            "json",
            pretty_json,
            "Find the TLS rule, endpoint, retries, and certificate path.",
            [
                "must not disable TLS",
                "https://api.example.test/v2/charges",
                "17",
                "C:/certs/prod-chain.pem",
            ],
            rtk_filter="json",
            json_equivalence=True,
        ),
        case(
            "repetitive-terminal-log",
            "tool_output",
            "terminal_log",
            repetitive_log,
            "Diagnose the certificate failure and report its identifiers.",
            ["req-7f9a", "C:/certs/prod-chain.pem", "E_CERT_42", "retry_count=17"],
            rtk_filter="log",
        ),
        case(
            "pytest-output",
            "tool_output",
            "pytest",
            pytest_output,
            "Which test failed and what endpoint/status caused it?",
            [
                "test_tls_chain",
                "status=526",
                "request_id=req-7f9a",
                "https://api.example.test/v2/charges",
                "1 failed",
            ],
            rtk_filter="pytest",
        ),
        case(
            "git-log",
            "tool_output",
            "git_log",
            git_log,
            "Find the commit that fixed INC-2048.",
            ["7f9a204", "INC-2048", "TLS certificate rotation"],
            rtk_filter="git-log",
        ),
        case(
            "code-context",
            "prompt_context",
            "source_code",
            code_context,
            "Report MAX_RETRIES, endpoint, port, and the TLS guard.",
            [
                "MAX_RETRIES = 17",
                "https://api.example.test/v2/charges",
                "port: 8443",
                "E_CERT_42",
            ],
        ),
        case(
            "multilingual-constraints",
            "prompt_context",
            "multilingual",
            multilingual,
            "Return every language-specific constraint and exact artifact.",
            [
                "request_id=req-7f9a",
                "8443",
                "17",
                "C:/certs/prod-chain.pem",
                "https://api.example.test/v2/charges",
                "पोर्ट 8443 को न बदलें",
                "Conserve exactamente 17 reintentos",
                "証明書 C:/certs/prod-chain.pem を保持してください",
            ],
        ),
    ]
    return {
        "schema_version": "kendr.benchmark-corpus/v1",
        "corpus_id": "kendr-authored-peer-v1",
        "description": "Deterministic synthetic corpus for preflight payload and artifact-preservation comparisons.",
        "license": "Apache-2.0",
        "claim_limit": "No target-model inference is included; preservation proxies are not downstream quality.",
        "cases": cases,
    }


def main() -> None:
    destination = ROOT / "cases.json"
    destination.write_text(
        json.dumps(build(), ensure_ascii=False, indent=2) + "\n",
        encoding="utf-8",
    )
    print(destination)


if __name__ == "__main__":
    main()
