"""Configuration with deliberately narrow network and deadline boundaries."""

from __future__ import annotations

from dataclasses import dataclass
import os
from urllib.parse import urlsplit


_RISKS = {
    "pass_through",
    "representation_safe",
    "recoverable",
    "extractive",
    "learned",
}
_TOKENIZERS = {"approximate", "cl100k_base", "o200k_base"}


@dataclass(frozen=True)
class Settings:
    endpoint: str
    timeout_seconds: float
    backoff_seconds: float
    risk_ceiling: str
    tokenizer_profile: str
    min_gain_tokens: int
    min_gain_percent: float
    preserve_recent_messages: int
    max_tool_result_chars: int
    shadow: bool

    @classmethod
    def from_env(cls) -> "Settings":
        endpoint = _loopback_endpoint(
            os.getenv("KENDR_OPTIMIZER_ENDPOINT", "http://127.0.0.1:7331")
        )
        risk = os.getenv("KENDR_OPTIMIZER_RISK_CEILING", "representation_safe")
        if risk not in _RISKS:
            raise ValueError("KENDR_OPTIMIZER_RISK_CEILING is invalid")
        tokenizer = os.getenv("KENDR_OPTIMIZER_TOKENIZER", "o200k_base")
        if tokenizer not in _TOKENIZERS:
            raise ValueError("KENDR_OPTIMIZER_TOKENIZER is invalid")
        return cls(
            endpoint=endpoint,
            timeout_seconds=_bounded_int("KENDR_OPTIMIZER_TIMEOUT_MS", 40, 5, 250)
            / 1000,
            backoff_seconds=_bounded_int(
                "KENDR_OPTIMIZER_BACKOFF_MS", 30_000, 100, 300_000
            )
            / 1000,
            risk_ceiling=risk,
            tokenizer_profile=tokenizer,
            min_gain_tokens=_bounded_int(
                "KENDR_OPTIMIZER_MIN_GAIN_TOKENS", 8, 0, 100_000
            ),
            min_gain_percent=_bounded_float(
                "KENDR_OPTIMIZER_MIN_GAIN_PERCENT", 1.0, 0.0, 100.0
            ),
            preserve_recent_messages=_bounded_int(
                "KENDR_OPTIMIZER_PRESERVE_RECENT", 6, 0, 10_000
            ),
            max_tool_result_chars=_bounded_int(
                "KENDR_OPTIMIZER_MAX_TOOL_RESULT_CHARS", 24_000, 1, 10_000_000
            ),
            shadow=_truthy(os.getenv("KENDR_OPTIMIZER_SHADOW", "0")),
        )


def _loopback_endpoint(raw: str) -> str:
    parsed = urlsplit(raw.strip())
    if (
        parsed.scheme != "http"
        or parsed.hostname not in {"127.0.0.1", "::1"}
        or parsed.username is not None
        or parsed.password is not None
        or parsed.query
        or parsed.fragment
        or parsed.path not in {"", "/"}
    ):
        raise ValueError(
            "KENDR_OPTIMIZER_ENDPOINT must be an HTTP origin on literal "
            "127.0.0.1 or ::1"
        )
    try:
        port = parsed.port
    except ValueError as exc:
        raise ValueError("KENDR_OPTIMIZER_ENDPOINT has an invalid port") from exc
    if port is None or not (1 <= port <= 65_535):
        raise ValueError("KENDR_OPTIMIZER_ENDPOINT must include a valid port")
    host = "[::1]" if parsed.hostname == "::1" else "127.0.0.1"
    return f"http://{host}:{port}"


def _bounded_int(name: str, default: int, minimum: int, maximum: int) -> int:
    raw = os.getenv(name)
    try:
        value = default if raw is None else int(raw)
    except ValueError as exc:
        raise ValueError(f"{name} must be an integer") from exc
    if not minimum <= value <= maximum:
        raise ValueError(f"{name} must be between {minimum} and {maximum}")
    return value


def _bounded_float(name: str, default: float, minimum: float, maximum: float) -> float:
    raw = os.getenv(name)
    try:
        value = default if raw is None else float(raw)
    except ValueError as exc:
        raise ValueError(f"{name} must be a number") from exc
    if not minimum <= value <= maximum:
        raise ValueError(f"{name} must be between {minimum} and {maximum}")
    return value


def _truthy(value: str) -> bool:
    return value.strip().lower() in {"1", "true", "yes", "on"}
