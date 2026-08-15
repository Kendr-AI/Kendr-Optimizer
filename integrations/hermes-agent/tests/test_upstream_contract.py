"""Optional contract smoke against a pinned Hermes Agent source checkout."""

from __future__ import annotations

from copy import deepcopy
import os
from pathlib import Path
import sys
import unittest


ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "src"))

import kendr_hermes_plugin as plugin  # noqa: E402
from kendr_hermes_plugin.config import Settings  # noqa: E402


HERMES_SOURCE = os.getenv("HERMES_AGENT_SOURCE")


class FakeClient:
    settings = Settings(
        endpoint="http://127.0.0.1:7331",
        timeout_seconds=0.04,
        backoff_seconds=30,
        risk_ceiling="representation_safe",
        tokenizer_profile="o200k_base",
        min_gain_tokens=8,
        min_gain_percent=1,
        preserve_recent_messages=6,
        max_tool_result_chars=24_000,
        shadow=False,
    )

    def optimize(self, envelope):
        content = deepcopy(envelope["content"])
        part = content["messages"][0]["parts"][0]
        part["content" if part["type"] == "tool_result" else "text"] = "compact"
        return {
            "content": content,
            "receipt": {
                "status": "applied",
                "original": {"tokens": 20},
                "optimized": {"tokens": 10},
                "token_delta": 10,
            },
        }


@unittest.skipUnless(
    HERMES_SOURCE, "set HERMES_AGENT_SOURCE to run upstream contract smoke"
)
class UpstreamContractTests(unittest.TestCase):
    def test_official_middleware_dispatch_contract(self):
        source = Path(HERMES_SOURCE).resolve()
        self.assertTrue((source / "hermes_cli" / "middleware.py").is_file())
        sys.path.insert(0, str(source))
        manager_installed = False
        try:
            import hermes_cli.plugins as host_plugins
            from hermes_cli.middleware import (
                apply_llm_request_middleware,
                run_tool_execution_middleware,
            )

            previous = host_plugins._plugin_manager
            manager = host_plugins.PluginManager()
            host_plugins._plugin_manager = manager
            manager_installed = True
            ctx = host_plugins.PluginContext(
                host_plugins.PluginManifest(name="kendr-optimizer", version="0.1.3"),
                manager,
            )
            plugin.register(ctx)
            plugin._CLIENT = FakeClient()

            request = {
                "messages": [{"role": "user", "content": "redundant prompt"}],
                "temperature": 0.1,
            }
            transformed = apply_llm_request_middleware(
                request, api_request_id="official-contract"
            )
            self.assertTrue(transformed.changed)
            self.assertEqual(transformed.payload["messages"][0]["content"], "compact")
            self.assertEqual(transformed.payload["temperature"], 0.1)
            self.assertEqual(request["messages"][0]["content"], "redundant prompt")

            calls = []
            tool_result = run_tool_execution_middleware(
                "terminal",
                {"command": "x"},
                lambda args: calls.append(args) or "long result",
            )
            self.assertEqual(tool_result, "compact")
            self.assertEqual(len(calls), 1)
        finally:
            plugin._CLIENT = None
            if manager_installed:
                host_plugins._plugin_manager = previous
            sys.path.remove(str(source))


if __name__ == "__main__":
    unittest.main()
