from __future__ import annotations

from copy import deepcopy
import io
import json
import os
from pathlib import Path
import sys
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

import kendr_hermes_plugin as plugin  # noqa: E402
from kendr_hermes_plugin.client import SidecarClient  # noqa: E402
from kendr_hermes_plugin.config import Settings  # noqa: E402


def settings() -> Settings:
    return Settings(
        endpoint="http://127.0.0.1:7331",
        timeout_seconds=0.04,
        backoff_seconds=30.0,
        risk_ceiling="representation_safe",
        tokenizer_profile="o200k_base",
        min_gain_tokens=8,
        min_gain_percent=1.0,
        preserve_recent_messages=6,
        max_tool_result_chars=24_000,
        shadow=False,
    )


def applied(envelope: dict, *, replacement: str | None = None) -> dict:
    content = deepcopy(envelope["content"])
    if replacement is not None:
        for message in content["messages"]:
            for part in message["parts"]:
                if part["type"] == "text" and "redundant" in part["text"]:
                    part["text"] = replacement
                    break
    return {
        "content": content,
        "receipt": {
            "status": "applied",
            "original": {"tokens": 100},
            "optimized": {"tokens": 80},
            "token_delta": 20,
        },
    }


class FakeClient:
    def __init__(self, transform=None):
        self.settings = settings()
        self.transform = transform or (lambda envelope: applied(envelope))
        self.envelopes = []

    def optimize(self, envelope):
        self.envelopes.append(deepcopy(envelope))
        return self.transform(envelope)


class FakeContext:
    def __init__(self):
        self.middleware = {}

    def register_middleware(self, kind, callback):
        self.middleware[kind] = callback


class PluginTests(unittest.TestCase):
    def tearDown(self):
        plugin._CLIENT = None

    def test_register_uses_official_middleware_names(self):
        ctx = FakeContext()
        fake = FakeClient()
        with patch.object(plugin, "SidecarClient", return_value=fake):
            plugin.register(ctx)
        self.assertEqual(set(ctx.middleware), {"llm_request", "tool_execution"})

    def test_invalid_reload_configuration_clears_previous_client(self):
        plugin._CLIENT = FakeClient()
        ctx = FakeContext()
        with patch.dict(
            os.environ,
            {"KENDR_OPTIMIZER_ENDPOINT": "https://example.com:7331"},
            clear=False,
        ):
            plugin.register(ctx)
        self.assertIsNone(plugin._CLIENT)
        self.assertEqual(ctx.middleware, {})

    def test_request_rebuild_changes_only_bound_text(self):
        request = {
            "model": "test-model",
            "system": [
                {
                    "type": "text",
                    "text": "Never change this cached system prompt.",
                    "cache_control": {"type": "ephemeral"},
                }
            ],
            "messages": [
                {
                    "role": "assistant",
                    "content": "",
                    "tool_calls": [
                        {
                            "id": "call-1",
                            "type": "function",
                            "function": {
                                "name": "read_file",
                                "arguments": '{"path":"/tmp/a"}',
                            },
                        }
                    ],
                },
                {
                    "role": "user",
                    "content": "redundant\n\n\n\ntext",
                    "vendor_extension": {"must": "survive"},
                },
            ],
            "tools": [
                {
                    "type": "function",
                    "function": {
                        "name": "read_file",
                        "description": "Read one file",
                        "parameters": {
                            "type": "object",
                            "properties": {"path": {"type": "string"}},
                        },
                    },
                }
            ],
            "response_format": {"type": "json_object"},
            "provider_only": object(),
        }
        original_user = request["messages"][1]["content"]
        fake = FakeClient(
            lambda envelope: applied(envelope, replacement="compact text")
        )
        plugin._CLIENT = fake

        result = plugin.optimize_llm_request(
            request=request,
            api_request_id="req-1",
            session_id="session-1",
            api_mode="chat_completions",
            model="test-model",
        )

        self.assertIsNotNone(result)
        rebuilt = result["request"]
        self.assertEqual(rebuilt["messages"][1]["content"], "compact text")
        self.assertEqual(request["messages"][1]["content"], original_user)
        self.assertIs(rebuilt["provider_only"], request["provider_only"])
        self.assertEqual(
            rebuilt["messages"][0]["tool_calls"], request["messages"][0]["tool_calls"]
        )
        self.assertEqual(rebuilt["response_format"], request["response_format"])
        envelope = fake.envelopes[0]
        self.assertFalse(envelope["host_capabilities"]["can_narrow_tools"])
        self.assertFalse(envelope["policy"]["enable_generation_policy"])
        self.assertEqual(
            envelope["target"]["cache_segments"][0]["message_ids"],
            ["hermes-system"],
        )

    def test_malformed_or_mutated_opaque_part_fails_open(self):
        request = {
            "messages": [
                {
                    "role": "assistant",
                    "content": "redundant text",
                    "tool_calls": [
                        {
                            "id": "call-1",
                            "function": {"name": "shell", "arguments": "{}"},
                        }
                    ],
                }
            ]
        }

        def mutate_call(envelope):
            outcome = applied(envelope, replacement="compact")
            outcome["content"]["messages"][0]["parts"][-1]["name"] = "evil"
            return outcome

        plugin._CLIENT = FakeClient(mutate_call)
        self.assertIsNone(plugin.optimize_llm_request(request=request))
        self.assertEqual(
            request["messages"][0]["tool_calls"][0]["function"]["name"], "shell"
        )

    def test_responses_scalar_input_is_supported(self):
        plugin._CLIENT = FakeClient(
            lambda envelope: applied(envelope, replacement="compact input")
        )
        request = {"input": "redundant scalar input", "store": False}
        result = plugin.optimize_llm_request(
            request=request, api_mode="responses", api_request_id="response-1"
        )
        self.assertEqual(result["request"], {"input": "compact input", "store": False})

    def test_tool_result_calls_downstream_once_and_preserves_container(self):
        def transform(envelope):
            outcome = applied(envelope)
            outcome["content"]["messages"][0]["parts"][0]["content"] = "short"
            return outcome

        plugin._CLIENT = FakeClient(transform)
        calls = []

        def next_call(args):
            calls.append(args)
            return {"content": "long\n\n\nresult", "mime": "text/plain"}

        result = plugin.optimize_tool_execution(
            tool_name="terminal", args={"command": "status"}, next_call=next_call
        )
        self.assertEqual(len(calls), 1)
        self.assertEqual(result, {"content": "short", "mime": "text/plain"})

    def test_downstream_exception_is_not_swallowed(self):
        plugin._CLIENT = FakeClient()

        def fail(_args):
            raise RuntimeError("tool failed")

        with self.assertRaisesRegex(RuntimeError, "tool failed"):
            plugin.optimize_tool_execution(tool_name="x", args={}, next_call=fail)

    def test_non_string_tool_result_passes_through_without_sidecar(self):
        fake = FakeClient()
        plugin._CLIENT = fake
        value = {"structured": [1, 2, 3]}
        self.assertIs(
            plugin.optimize_tool_execution(
                tool_name="x", args={}, next_call=lambda _args: value
            ),
            value,
        )
        self.assertEqual(fake.envelopes, [])


class ConfigTests(unittest.TestCase):
    def test_remote_and_credentialed_origins_are_rejected(self):
        for endpoint in (
            "https://127.0.0.1:7331",
            "http://example.com:7331",
            "http://user:pass@127.0.0.1:7331",
            "http://127.0.0.1:7331/path",
        ):
            with self.subTest(endpoint=endpoint), patch.dict(
                os.environ, {"KENDR_OPTIMIZER_ENDPOINT": endpoint}, clear=False
            ):
                with self.assertRaises(ValueError):
                    Settings.from_env()

    def test_literal_ipv6_loopback_is_normalized(self):
        with patch.dict(
            os.environ,
            {"KENDR_OPTIMIZER_ENDPOINT": "http://[::1]:7331/"},
            clear=False,
        ):
            self.assertEqual(Settings.from_env().endpoint, "http://[::1]:7331")


class FakeResponse(io.BytesIO):
    status = 200

    def __enter__(self):
        return self

    def __exit__(self, *_args):
        self.close()


class FakeOpener:
    def __init__(self, responses):
        self.responses = list(responses)
        self.calls = []

    def open(self, request, timeout):
        self.calls.append((request, timeout))
        value = self.responses.pop(0)
        if isinstance(value, Exception):
            raise value
        return FakeResponse(json.dumps(value).encode())


class ClientTests(unittest.TestCase):
    def test_timeout_is_forwarded_and_failure_opens_circuit(self):
        opener = FakeOpener([TimeoutError("late")])
        client = SidecarClient(settings(), opener=opener)
        self.assertIsNone(client.optimize({"x": 1}))
        self.assertIsNone(client.optimize({"x": 2}))
        self.assertEqual(len(opener.calls), 1)
        self.assertEqual(opener.calls[0][1], 0.04)

    def test_valid_object_response_is_returned(self):
        opener = FakeOpener([{"receipt": {}, "content": {}}])
        client = SidecarClient(settings(), opener=opener)
        self.assertEqual(client.optimize({"x": 1}), {"receipt": {}, "content": {}})
        self.assertEqual(
            opener.calls[0][0].full_url, "http://127.0.0.1:7331/v1/optimize"
        )


if __name__ == "__main__":
    unittest.main()
