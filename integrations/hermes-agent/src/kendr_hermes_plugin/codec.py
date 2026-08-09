"""Lossless mappings between Hermes provider kwargs and the Kendr envelope.

Only provider fields represented by Kendr are transformed. Opaque provider
fields stay on a copy-on-write provider request and are never sent to the
sidecar.
"""

from __future__ import annotations

from copy import deepcopy
from dataclasses import dataclass
import json
from typing import Any, Iterable
from uuid import uuid4

from .config import Settings


Json = dict[str, Any]
Path = tuple[str | int, ...]


@dataclass(frozen=True)
class TextBinding:
    message_id: str
    part_index: int
    path: Path
    value_key: str


@dataclass
class EncodedRequest:
    original: Json
    envelope: Json
    messages: list[Json]
    tools: list[Json]
    bindings: list[TextBinding]

    def decode(self, outcome: Any) -> Json | None:
        """Rebuild provider kwargs only after a strict shape validation."""
        if not isinstance(outcome, dict):
            return None
        receipt = outcome.get("receipt")
        content = outcome.get("content")
        if not isinstance(receipt, dict) or not isinstance(content, dict):
            return None
        if receipt.get("status") != "applied":
            return None
        if not _valid_measurements(receipt):
            return None

        actual_messages = content.get("messages")
        if not isinstance(actual_messages, list) or len(actual_messages) != len(
            self.messages
        ):
            return None
        if content.get("tools") != self.tools:
            return None
        if (
            content.get("output_contract")
            != self.envelope["content"]["output_contract"]
        ):
            return None

        allowed = {(b.message_id, b.part_index): b for b in self.bindings}
        updates: list[tuple[Path, str]] = []
        for expected, actual in zip(self.messages, actual_messages, strict=True):
            if not _same_message_header(expected, actual):
                return None
            expected_parts = expected["parts"]
            actual_parts = actual.get("parts")
            if not isinstance(actual_parts, list) or len(actual_parts) != len(
                expected_parts
            ):
                return None
            for part_index, (before, after) in enumerate(
                zip(expected_parts, actual_parts, strict=True)
            ):
                binding = allowed.get((expected["id"], part_index))
                if binding is None:
                    if after != before:
                        return None
                    continue
                value = _changed_text(before, after, binding.value_key)
                if value is None:
                    return None
                updates.append((binding.path, value))

        try:
            rebuilt = self.original
            for path, value in updates:
                rebuilt = _copy_with_update(rebuilt, path, value)
        except (KeyError, IndexError, TypeError, ValueError):
            return None
        return rebuilt


def encode_provider_request(
    request: Any, context: dict[str, Any], settings: Settings
) -> EncodedRequest | None:
    if not isinstance(request, dict):
        return None
    # Hermes has already copied the middleware payload. Keep this reference
    # immutable and use copy-on-write during decode; provider-only objects can
    # be non-deepcopyable and must never make an otherwise safe request fail.
    original = request

    messages: list[Json] = []
    bindings: list[TextBinding] = []
    cached_ids: list[str] = []

    _encode_top_level_text(
        request,
        "system",
        "system",
        "hermes-system",
        messages,
        bindings,
        cached_ids,
    )
    _encode_top_level_text(
        request,
        "instructions",
        "developer",
        "hermes-instructions",
        messages,
        bindings,
        cached_ids,
    )
    if isinstance(request.get("input"), str):
        _encode_top_level_text(
            request,
            "input",
            "user",
            "hermes-input-string",
            messages,
            bindings,
            cached_ids,
        )

    sequence_name, sequence = _message_sequence(request)
    if sequence_name is not None and sequence is not None:
        for index, item in enumerate(sequence):
            encoded = _encode_sequence_item(sequence_name, index, item)
            if encoded is None:
                continue
            message, item_bindings = encoded
            messages.append(message)
            bindings.extend(item_bindings)
            if _contains_cache_control(item):
                cached_ids.append(message["id"])

    if not messages:
        return None

    tools = _encode_tools(request.get("tools"))
    output_contract = _output_contract(request)
    request_id = _request_id(context)
    session_id = context.get("session_id")
    model = (
        request.get("model")
        if isinstance(request.get("model"), str)
        else context.get("model")
    )
    api_mode = context.get("api_mode")
    envelope: Json = {
        "schema_version": "kendr.optimize/v1",
        "phase": "request",
        "request_id": request_id,
        "session_id": session_id if isinstance(session_id, str) else None,
        "content": {
            "messages": messages,
            "tools": tools,
            "output_contract": output_contract,
            "metadata": {
                "host": "hermes-agent",
                "adapter_version": "0.1.0",
                "api_mode": api_mode if isinstance(api_mode, str) else "unknown",
            },
        },
        "target": {
            "tokenizer_profile": settings.tokenizer_profile,
            "model": model if isinstance(model, str) else None,
            "context_limit": None,
            "pricing": None,
            "cache_segments": (
                [{"id": "hermes-explicit-cache", "message_ids": cached_ids}]
                if cached_ids
                else []
            ),
        },
        "generation": {
            "current_max_output_tokens": _optional_nonnegative_int(
                request.get("max_output_tokens", request.get("max_tokens"))
            ),
            "target_output_tokens": None,
            "expected_output_tokens": None,
            "requested_verbosity": "auto",
            "required_elements": [],
        },
        "host_capabilities": {
            "can_narrow_tools": False,
            "can_restore_references": False,
            "can_retry_with_full_tools": False,
            "streaming_output": bool(request.get("stream", True)),
            "can_set_max_output_tokens": False,
            "can_set_verbosity": False,
            "can_append_generation_policy": False,
        },
        "policy": _policy(settings),
    }
    return EncodedRequest(original, envelope, messages, tools, bindings)


def encode_tool_result(
    tool_name: str,
    result: str,
    context: dict[str, Any],
    settings: Settings,
) -> tuple[Json, str, Json]:
    request_id = _request_id(context)
    call_id = context.get("tool_call_id")
    if not isinstance(call_id, str) or not call_id:
        call_id = request_id
    message_id = f"hermes-tool-{request_id}"
    expected_part: Json = {
        "type": "tool_result",
        "call_id": call_id,
        "name": tool_name or None,
        "content": result,
        "is_error": False,
    }
    envelope: Json = {
        "schema_version": "kendr.optimize/v1",
        "phase": "tool_result",
        "request_id": request_id,
        "session_id": context.get("session_id")
        if isinstance(context.get("session_id"), str)
        else None,
        "content": {
            "messages": [
                {
                    "id": message_id,
                    "role": "tool",
                    "parent_id": None,
                    "turn_id": context.get("turn_id")
                    if isinstance(context.get("turn_id"), str)
                    else None,
                    "parts": [expected_part],
                    "metadata": {},
                }
            ],
            "tools": [],
            "output_contract": None,
            "metadata": {"host": "hermes-agent", "adapter_version": "0.1.0"},
        },
        "target": {
            "tokenizer_profile": settings.tokenizer_profile,
            "model": context.get("model")
            if isinstance(context.get("model"), str)
            else None,
            "context_limit": None,
            "pricing": None,
            "cache_segments": [],
        },
        "generation": {},
        "host_capabilities": {
            "can_narrow_tools": False,
            "can_restore_references": False,
            "can_retry_with_full_tools": False,
            "streaming_output": False,
            "can_set_max_output_tokens": False,
            "can_set_verbosity": False,
            "can_append_generation_policy": False,
        },
        "policy": _policy(settings),
    }
    return envelope, message_id, expected_part


def decode_tool_result(
    outcome: Any, message_id: str, expected_part: Json, original: str
) -> str | None:
    if not isinstance(outcome, dict) or not isinstance(outcome.get("receipt"), dict):
        return None
    if outcome["receipt"].get("status") != "applied" or not _valid_measurements(
        outcome["receipt"]
    ):
        return None
    content = outcome.get("content")
    if not isinstance(content, dict) or content.get("tools") != []:
        return None
    messages = content.get("messages")
    if not isinstance(messages, list) or len(messages) != 1:
        return None
    message = messages[0]
    if (
        not isinstance(message, dict)
        or message.get("id") != message_id
        or message.get("role") != "tool"
    ):
        return None
    parts = message.get("parts")
    if not isinstance(parts, list) or len(parts) != 1 or not isinstance(parts[0], dict):
        return None
    part = parts[0]
    if _changed_text(expected_part, part, "content") is None:
        return None
    # Returning the exact original is still a valid applied no-delta outcome,
    # but the caller avoids manufacturing a changed middleware trace.
    return part["content"] if part["content"] != original else None


def _encode_top_level_text(
    request: Json,
    field: str,
    role: str,
    message_id: str,
    messages: list[Json],
    bindings: list[TextBinding],
    cached_ids: list[str],
) -> None:
    value = request.get(field)
    parts: list[Json] = []
    local: list[TextBinding] = []
    if isinstance(value, str):
        parts.append({"type": "text", "text": value})
        local.append(TextBinding(message_id, 0, (field,), "text"))
    elif isinstance(value, list):
        for block_index, block in enumerate(value):
            if not isinstance(block, dict) or not isinstance(block.get("text"), str):
                continue
            if block.get("type") not in {None, "text", "input_text", "output_text"}:
                continue
            part_index = len(parts)
            parts.append({"type": "text", "text": block["text"]})
            local.append(
                TextBinding(
                    message_id, part_index, (field, block_index, "text"), "text"
                )
            )
    if not parts:
        return
    messages.append(_message(message_id, role, parts))
    bindings.extend(local)
    if _contains_cache_control(value):
        cached_ids.append(message_id)


def _message_sequence(request: Json) -> tuple[str | None, list[Any] | None]:
    if isinstance(request.get("messages"), list):
        return "messages", request["messages"]
    if isinstance(request.get("input"), list):
        return "input", request["input"]
    return None, None


def _encode_sequence_item(
    sequence_name: str, index: int, item: Any
) -> tuple[Json, list[TextBinding]] | None:
    if not isinstance(item, dict):
        return None
    message_id = f"hermes-{sequence_name}-{index}"
    role = _role(item.get("role"))
    item_type = item.get("type")
    if role is None and item_type == "function_call":
        part = _function_call_part(item)
        return (_message(message_id, "assistant", [part]), []) if part else None
    if role is None and item_type == "function_call_output":
        output = item.get("output")
        if not isinstance(output, str):
            return None
        part = {
            "type": "tool_result",
            "call_id": _first_string(item, "call_id", "id") or message_id,
            "name": None,
            "content": output,
            "is_error": False,
        }
        return _message(message_id, "tool", [part]), [
            TextBinding(message_id, 0, (sequence_name, index, "output"), "content")
        ]
    if role is None:
        return None

    parts: list[Json] = []
    bindings: list[TextBinding] = []
    content = item.get("content")
    if isinstance(content, str):
        if role == "tool":
            parts.append(
                {
                    "type": "tool_result",
                    "call_id": _first_string(item, "tool_call_id", "call_id", "id")
                    or message_id,
                    "name": _first_string(item, "name"),
                    "content": content,
                    "is_error": bool(item.get("is_error", False)),
                }
            )
            key = "content"
        else:
            parts.append({"type": "text", "text": content})
            key = "text"
        bindings.append(
            TextBinding(message_id, 0, (sequence_name, index, "content"), key)
        )
    elif isinstance(content, list):
        for block_index, block in enumerate(content):
            encoded = _encode_content_block(
                block, role, message_id, (sequence_name, index, "content", block_index)
            )
            if encoded is None:
                continue
            part, relative_binding = encoded
            part_index = len(parts)
            parts.append(part)
            if relative_binding is not None:
                path, value_key = relative_binding
                bindings.append(TextBinding(message_id, part_index, path, value_key))

    tool_calls = item.get("tool_calls")
    if isinstance(tool_calls, list):
        for call in tool_calls:
            part = _function_call_part(call)
            if part is not None:
                parts.append(part)
    if not parts:
        return None
    return _message(message_id, role, parts), bindings


def _encode_content_block(
    block: Any, role: str, message_id: str, base_path: Path
) -> tuple[Json, tuple[Path, str] | None] | None:
    if not isinstance(block, dict):
        return None
    block_type = block.get("type")
    if block_type in {None, "text", "input_text", "output_text"} and isinstance(
        block.get("text"), str
    ):
        return {"type": "text", "text": block["text"]}, (base_path + ("text",), "text")
    if block_type in {"tool_use", "function_call"}:
        part = _function_call_part(block)
        return (part, None) if part is not None else None
    if block_type in {"tool_result", "function_call_output"}:
        field = "output" if block_type == "function_call_output" else "content"
        value = block.get(field)
        if not isinstance(value, str):
            return None
        return (
            {
                "type": "tool_result",
                "call_id": _first_string(block, "tool_use_id", "call_id", "id")
                or message_id,
                "name": _first_string(block, "name"),
                "content": value,
                "is_error": bool(block.get("is_error", False)),
            },
            (base_path + (field,), "content"),
        )
    return None


def _function_call_part(value: Any) -> Json | None:
    if not isinstance(value, dict):
        return None
    inner = value.get("function") if isinstance(value.get("function"), dict) else value
    name = _first_string(inner, "name")
    if name is None:
        return None
    arguments = inner.get("arguments", inner.get("input", {}))
    if isinstance(arguments, str):
        try:
            arguments = json.loads(arguments)
        except (json.JSONDecodeError, TypeError):
            # The contract accepts JSON values, so retain malformed provider
            # argument strings exactly rather than parsing or repairing them.
            pass
    return {
        "type": "tool_call",
        "id": _first_string(value, "id", "call_id", "tool_use_id") or name,
        "name": name,
        "arguments": arguments,
    }


def _encode_tools(value: Any) -> list[Json]:
    if not isinstance(value, list):
        return []
    result: list[Json] = []
    names: set[str] = set()
    for tool in value:
        if not isinstance(tool, dict):
            return []
        inner = tool.get("function") if isinstance(tool.get("function"), dict) else tool
        name = _first_string(inner, "name")
        if name is None or name in names:
            return []
        schema = inner.get("parameters", inner.get("input_schema", {}))
        if (
            not isinstance(schema, (dict, list, str, int, float, bool))
            and schema is not None
        ):
            return []
        result.append(
            {
                "name": name,
                "description": inner.get("description")
                if isinstance(inner.get("description"), str)
                else "",
                "input_schema": schema,
                "required": False,
                "tags": [],
                "metadata": {},
            }
        )
        names.add(name)
    return result


def _output_contract(request: Json) -> Any:
    if "response_format" in request:
        return deepcopy(request["response_format"])
    text = request.get("text")
    if isinstance(text, dict) and "format" in text:
        return deepcopy(text["format"])
    return None


def _message(message_id: str, role: str, parts: list[Json]) -> Json:
    return {
        "id": message_id,
        "role": role,
        "parent_id": None,
        "turn_id": None,
        "parts": parts,
        "metadata": {},
    }


def _role(value: Any) -> str | None:
    if not isinstance(value, str):
        return None
    return {
        "system": "system",
        "developer": "developer",
        "user": "user",
        "assistant": "assistant",
        "tool": "tool",
        "function": "tool",
    }.get(value)


def _policy(settings: Settings) -> Json:
    return {
        "risk_ceiling": settings.risk_ceiling,
        "min_gain_tokens": settings.min_gain_tokens,
        "min_gain_percent": settings.min_gain_percent,
        "latency_budget_ms": max(1, int(settings.timeout_seconds * 1000) - 5),
        "preserve_cache_prefix": True,
        "shadow": settings.shadow,
        "preserve_recent_messages": settings.preserve_recent_messages,
        "max_tool_result_chars": settings.max_tool_result_chars,
        "enable_tool_selection": False,
        "enable_lossy_tool_output": False,
        "enable_generation_policy": False,
        "min_expected_output_saving_tokens": 32,
        "enabled_engines": [],
    }


def _request_id(context: dict[str, Any]) -> str:
    candidate = context.get("api_request_id") or context.get("tool_call_id")
    suffix = candidate if isinstance(candidate, str) and candidate else str(uuid4())
    return f"hermes-{suffix}"


def _same_message_header(expected: Json, actual: Any) -> bool:
    if not isinstance(actual, dict):
        return False
    parent = actual.get("parent_id")
    turn = actual.get("turn_id")
    return (
        actual.get("id") == expected["id"]
        and actual.get("role") == expected["role"]
        and (parent is None or parent == expected["parent_id"])
        and (turn is None or turn == expected["turn_id"])
    )


def _changed_text(before: Any, after: Any, key: str) -> str | None:
    if not isinstance(before, dict) or not isinstance(after, dict):
        return None
    if before.get("type") != after.get("type") or not isinstance(after.get(key), str):
        return None
    before_rest = {k: v for k, v in before.items() if k != key}
    after_rest = {k: v for k, v in after.items() if k != key}
    return after[key] if before_rest == after_rest else None


def _valid_measurements(receipt: Json) -> bool:
    original = receipt.get("original")
    optimized = receipt.get("optimized")
    return (
        isinstance(original, dict)
        and isinstance(optimized, dict)
        and _nonnegative_number(original.get("tokens"))
        and _nonnegative_number(optimized.get("tokens"))
        and _finite_number(receipt.get("token_delta"))
    )


def _finite_number(value: Any) -> bool:
    return (
        isinstance(value, (int, float))
        and not isinstance(value, bool)
        and float("-inf") < value < float("inf")
    )


def _nonnegative_number(value: Any) -> bool:
    return _finite_number(value) and value >= 0


def _optional_nonnegative_int(value: Any) -> int | None:
    return (
        value
        if isinstance(value, int) and not isinstance(value, bool) and value >= 0
        else None
    )


def _first_string(value: Any, *keys: str) -> str | None:
    if not isinstance(value, dict):
        return None
    for key in keys:
        candidate = value.get(key)
        if isinstance(candidate, str) and candidate:
            return candidate
    return None


def _contains_cache_control(value: Any) -> bool:
    if isinstance(value, dict):
        if "cache_control" in value:
            return True
        return any(_contains_cache_control(item) for item in value.values())
    if isinstance(value, list):
        return any(_contains_cache_control(item) for item in value)
    return False


def _copy_with_update(root: Json, path: Iterable[str | int], value: str) -> Json:
    parts = list(path)
    if not parts:
        raise ValueError("empty path")
    rebuilt: Any = dict(root)
    cursor = rebuilt
    for part in parts[:-1]:
        child = cursor[part]
        if isinstance(child, dict):
            child = dict(child)
        elif isinstance(child, list):
            child = list(child)
        else:
            raise TypeError("binding path crosses a scalar")
        cursor[part] = child
        cursor = child
    cursor[parts[-1]] = value
    return rebuilt
