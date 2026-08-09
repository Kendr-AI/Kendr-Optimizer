"""Hermes Agent plugin entry point for KendrOptimizer."""

from __future__ import annotations

from copy import deepcopy
import logging
from typing import Any, Callable

from .client import SidecarClient
from .codec import (
    decode_tool_result,
    encode_provider_request,
    encode_tool_result,
)
from .config import Settings


logger = logging.getLogger(__name__)
_CLIENT: SidecarClient | None = None


def register(ctx: Any) -> None:
    """Register the two behavior-changing middleware seams Hermes exposes."""
    global _CLIENT
    _CLIENT = None
    try:
        _CLIENT = SidecarClient(Settings.from_env())
    except ValueError as exc:
        # Invalid operator configuration is explicit, but it must not prevent
        # Hermes itself from starting.
        logger.error("KendrOptimizer plugin disabled: %s", exc)
        return
    ctx.register_middleware("llm_request", optimize_llm_request)
    ctx.register_middleware("tool_execution", optimize_tool_execution)


def optimize_llm_request(*, request: Any, **context: Any) -> dict[str, Any] | None:
    """Optimize supported message fields and retain every provider-only field."""
    client = _CLIENT
    if client is None:
        return None
    try:
        encoded = encode_provider_request(request, context, client.settings)
        if encoded is None:
            return None
        outcome = client.optimize(encoded.envelope)
        if outcome is None:
            return None
        rebuilt = encoded.decode(outcome)
        if rebuilt is None or rebuilt == request:
            return None
        return {
            "request": rebuilt,
            "source": "kendr-optimizer",
            "reason": "validated local preflight transformation",
        }
    except Exception as exc:
        logger.debug(
            "KendrOptimizer request mapping failed open (%s)", type(exc).__name__
        )
        return None


def optimize_tool_execution(
    *,
    tool_name: str,
    args: dict[str, Any],
    next_call: Callable[[dict[str, Any]], Any],
    **context: Any,
) -> Any:
    """Run a tool exactly once, then optimize string-shaped returned content."""
    result = next_call(args)
    client = _CLIENT
    if client is None:
        return result

    original: str | None = None
    container: dict[str, Any] | None = None
    if isinstance(result, str):
        original = result
    elif isinstance(result, dict) and isinstance(result.get("content"), str):
        original = result["content"]
        try:
            container = deepcopy(result)
        except Exception:
            return result
    if original is None:
        return result

    try:
        envelope, message_id, expected_part = encode_tool_result(
            tool_name, original, context, client.settings
        )
        optimized = decode_tool_result(
            client.optimize(envelope), message_id, expected_part, original
        )
    except Exception as exc:
        logger.debug(
            "KendrOptimizer tool-result mapping failed open (%s)", type(exc).__name__
        )
        return result
    if optimized is None:
        return result
    if container is None:
        return optimized
    container["content"] = optimized
    return container


__all__ = ["register", "optimize_llm_request", "optimize_tool_execution"]
