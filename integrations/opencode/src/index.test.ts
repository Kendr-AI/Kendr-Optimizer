import assert from "node:assert/strict";
import test from "node:test";
import type { Hooks, PluginInput } from "@opencode-ai/plugin";

import {
  createOpenCodePlugin,
  type FetchLike,
  validateNumericLoopbackEndpoint,
} from "./index.js";

type Hook = (input: any, output: any) => Promise<void>;

function requireHook(value: unknown): Hook {
  assert.equal(typeof value, "function");
  return value as Hook;
}

interface FetchCall {
  url: URL;
  request: Record<string, unknown>;
}

async function hooksFor(options: Parameters<typeof createOpenCodePlugin>[0]): Promise<Hooks> {
  const plugin = createOpenCodePlugin(options);
  return plugin({} as PluginInput);
}

function outcomeFetch(
  calls: FetchCall[],
  mutate: (content: { messages: Array<{ parts: Array<Record<string, unknown>> }> }) => void,
  status = "applied",
): FetchLike {
  return async (input, init) => {
    const request = JSON.parse(String(init?.body)) as Record<string, unknown>;
    calls.push({ url: new URL(String(input)), request });
    const content = structuredClone(request.content) as {
      messages: Array<{ parts: Array<Record<string, unknown>> }>;
    };
    mutate(content);
    return new Response(
      JSON.stringify({
        content,
        receipt: {
          schema_version: "kendr.receipt/v1",
          request_id: request.request_id,
          status,
          original: { tokens: 120 },
          optimized: { tokens: status === "applied" ? 100 : 120 },
          token_delta: status === "applied" ? 20 : 0,
          verified_savings: false,
        },
      }),
      { status: 200 },
    );
  };
}

test("stable chat.message hook optimizes text parts and preserves non-text parts", async () => {
  const calls: FetchCall[] = [];
  const hooks = await hooksFor({
    fetch: outcomeFetch(calls, (content) => {
      const part = content.messages[0]?.parts[0];
      if (part) part.text = "first\n\n\nsecond";
    }),
  });
  const hook = requireHook(hooks["chat.message"]);
  const filePart = { type: "file", mime: "image/png", url: "data:image/png;base64,AA==" };
  const output = {
    message: { id: "message-1", role: "user" },
    parts: [
      { type: "text", text: "first\n\n\n\nsecond", id: "part-1" },
      filePart,
    ],
  };
  await hook(
    {
      sessionID: "session-1",
      messageID: "message-1",
      model: { providerID: "anthropic", modelID: "claude-test" },
    },
    output,
  );

  assert.equal(calls.length, 1);
  assert.equal(calls[0]?.url.pathname, "/v1/optimize");
  const capabilities = calls[0]?.request.host_capabilities as Record<string, unknown>;
  assert.equal(capabilities.can_narrow_tools, false);
  assert.equal(capabilities.can_retry_with_full_tools, false);
  assert.equal((output.parts[0] as { text?: string } | undefined)?.text, "first\n\n\nsecond");
  assert.deepEqual(output.parts[1], filePart);
});

test("stable tool.execute.after hook changes output only", async () => {
  const hooks = await hooksFor({
    fetch: outcomeFetch([], (content) => {
      const part = content.messages[0]?.parts[0];
      if (part) part.content = "plain output";
    }),
  });
  const hook = requireHook(hooks["tool.execute.after"]);
  const metadata = { exitCode: 0, duration: 20 };
  const output = {
    title: "Build",
    output: "\u001b[32mplain output\u001b[0m",
    metadata,
  };
  await hook(
    { tool: "bash", sessionID: "session-1", callID: "call-1", args: {} },
    output,
  );
  assert.equal(output.output, "plain output");
  assert.equal(output.title, "Build");
  assert.equal(output.metadata, metadata);
});

test("experimental hooks are absent by default and explicit when opted in", async () => {
  const stable = await hooksFor({ fetch: outcomeFetch([], () => undefined) });
  assert.equal(stable["experimental.chat.messages.transform"], undefined);
  assert.equal(stable["experimental.chat.system.transform"], undefined);

  const experimental = await hooksFor({
    experimentalHistory: true,
    experimentalSystem: true,
    fetch: outcomeFetch([], () => undefined),
  });
  assert.equal(typeof experimental["experimental.chat.messages.transform"], "function");
  assert.equal(typeof experimental["experimental.chat.system.transform"], "function");
});

test("published V1 entrypoint exposes exactly one plugin factory", async () => {
  const entrypoint = await import("./plugin.js");
  assert.deepEqual(Object.keys(entrypoint), ["KendrOptimizerPlugin"]);
  assert.equal(typeof entrypoint.KendrOptimizerPlugin, "function");
});

test("experimental history hook preserves message info and tool parts", async () => {
  const hooks = await hooksFor({
    experimentalHistory: true,
    fetch: outcomeFetch([], (content) => {
      const part = content.messages[0]?.parts[0];
      if (part) part.text = "condensed\n\n\nhistory";
    }),
  });
  const hook = requireHook(hooks["experimental.chat.messages.transform"]);
  const info = { id: "u-1", role: "user", time: { created: 1 } };
  const toolPart = { type: "tool", callID: "c-1", tool: "bash", state: { status: "completed" } };
  const output = {
    messages: [
      {
        info,
        parts: [
          { type: "text", text: "condensed\n\n\n\nhistory" },
          toolPart,
        ],
      },
    ],
  };
  await hook({}, output);
  assert.deepEqual(output.messages[0]?.info, info);
  assert.equal(
    (output.messages[0]?.parts[0] as { text?: string } | undefined)?.text,
    "condensed\n\n\nhistory",
  );
  assert.deepEqual(output.messages[0]?.parts[1], toolPart);
});

test("experimental system hook preserves array shape", async () => {
  const hooks = await hooksFor({
    experimentalSystem: true,
    fetch: outcomeFetch([], (content) => {
      const part = content.messages[0]?.parts[0];
      if (part) part.text = "System\n\n\nRules";
    }),
  });
  const hook = requireHook(hooks["experimental.chat.system.transform"]);
  const output = { system: ["System\n\n\n\nRules", "Second unchanged"] };
  await hook({ sessionID: "s", model: { id: "m" } }, output);
  assert.deepEqual(output.system, ["System\n\n\nRules", "Second unchanged"]);
});

test("OpenCode hook exceptions and optimizer outages are fail-open", async () => {
  const fetch: FetchLike = async () => {
    throw new Error("optimizer offline");
  };
  const hooks = await hooksFor({ fetch });
  const chat = requireHook(hooks["chat.message"]);
  const tool = requireHook(hooks["tool.execute.after"]);
  const chatOutput = {
    message: { role: "user" },
    parts: [{ type: "text", text: "keep exact" }],
  };
  const toolOutput = { title: "t", output: "keep exact", metadata: { x: 1 } };
  await chat({ sessionID: "s" }, chatOutput);
  await tool({ tool: "bash", sessionID: "s", callID: "c", args: {} }, toolOutput);
  assert.equal(chatOutput.parts[0]?.text, "keep exact");
  assert.equal(toolOutput.output, "keep exact");
});

test("skipped receipts are not applied even if content is different", async () => {
  const hooks = await hooksFor({
    fetch: outcomeFetch(
      [],
      (content) => {
        const part = content.messages[0]?.parts[0];
        if (part) part.text = "malicious replacement";
      },
      "skipped",
    ),
  });
  const hook = requireHook(hooks["chat.message"]);
  const output = { message: { role: "user" }, parts: [{ type: "text", text: "original" }] };
  await hook({ sessionID: "s" }, output);
  assert.equal(output.parts[0]?.text, "original");
});

test("shadow mode calls analyze and does not mutate host output", async () => {
  const calls: FetchCall[] = [];
  const hooks = await hooksFor({
    shadow: true,
    fetch: outcomeFetch(calls, (content) => {
      const part = content.messages[0]?.parts[0];
      if (part) part.text = "shadow replacement";
    }),
  });
  const hook = requireHook(hooks["chat.message"]);
  const output = { message: { role: "user" }, parts: [{ type: "text", text: "original" }] };
  await hook({ sessionID: "s" }, output);
  assert.equal(calls[0]?.url.pathname, "/v1/analyze");
  assert.equal(output.parts[0]?.text, "original");
});

test("invalid endpoint disables hooks rather than failing plugin initialization", async () => {
  const hooks = await hooksFor({ coreEndpoint: "http://localhost:7331" });
  assert.deepEqual(hooks, {});
});

test("only credential-free numeric loopback endpoints are accepted", () => {
  assert.equal(validateNumericLoopbackEndpoint("http://127.0.0.1:7331").port, "7331");
  assert.throws(() => validateNumericLoopbackEndpoint("http://localhost:7331"));
  assert.throws(() => validateNumericLoopbackEndpoint("http://token@127.0.0.1:7331"));
});
