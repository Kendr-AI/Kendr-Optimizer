import assert from "node:assert/strict";
import test from "node:test";
import type { ExtensionAPI, ExtensionContext } from "@earendil-works/pi-coding-agent";

import {
  createPiOptimizerExtension,
  type FetchLike,
  validateNumericLoopbackEndpoint,
} from "./index.js";

type Handler = (event: never, context: ExtensionContext) => Promise<unknown> | unknown;

interface FetchCall {
  url: URL;
  request: Record<string, unknown>;
}

function extensionHarness(options: Parameters<typeof createPiOptimizerExtension>[0]): {
  handlers: Map<string, Handler>;
} {
  const handlers = new Map<string, Handler>();
  const pi = {
    on(name: string, handler: Handler) {
      handlers.set(name, handler);
    },
  } as unknown as ExtensionAPI;
  createPiOptimizerExtension(options)(pi);
  return { handlers };
}

function context(): ExtensionContext {
  return {
    model: { id: "test-model", contextWindow: 128_000 },
    sessionManager: { getSessionId: () => "pi-session-1" },
  } as unknown as ExtensionContext;
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
          original: { tokens: 100 },
          optimized: { tokens: status === "applied" ? 90 : 100 },
          token_delta: status === "applied" ? 10 : 0,
          verified_savings: false,
        },
      }),
      { status: 200 },
    );
  };
}

test("context hook transforms supported text and preserves Pi-specific blocks", async () => {
  const calls: FetchCall[] = [];
  const { handlers } = extensionHarness({
    fetch: outcomeFetch(calls, (content) => {
      for (const message of content.messages) {
        for (const part of message.parts) {
          if (part.type === "text" && typeof part.text === "string") {
            part.text = part.text.replace("\n\n\n\n", "\n\n\n");
          }
          if (part.type === "tool_result" && typeof part.content === "string") {
            part.content = part.content.replace("\u001b[31m", "").replace("\u001b[0m", "");
          }
        }
      }
    }),
  });
  const originalMessages = [
    { role: "user", content: "first\n\n\n\nsecond", timestamp: 1 },
    {
      role: "assistant",
      content: [
        { type: "text", text: "answer\n\n\n\ncontinued" },
        { type: "toolCall", id: "call-1", name: "bash", arguments: { command: "pwd" } },
      ],
      usage: { input: 1, output: 2 },
    },
    {
      role: "toolResult",
      toolCallId: "call-1",
      toolName: "bash",
      content: [
        { type: "text", text: "\u001b[31merror\u001b[0m" },
        { type: "image", mimeType: "image/png", data: "AA==" },
      ],
      details: { exitCode: 1 },
      isError: true,
      timestamp: 2,
    },
  ];
  const handler = handlers.get("context");
  assert.ok(handler);
  const result = (await handler(
    { type: "context", messages: originalMessages } as never,
    context(),
  )) as { messages: typeof originalMessages };

  assert.equal(calls.length, 1);
  assert.equal(calls[0]?.url.pathname, "/v1/optimize");
  const capabilities = calls[0]?.request.host_capabilities as Record<string, unknown>;
  assert.equal(capabilities.can_narrow_tools, false);
  assert.equal(capabilities.can_retry_with_full_tools, false);
  assert.equal(result.messages[0]?.content, "first\n\n\nsecond");
  const assistantContent = result.messages[1]?.content as Array<Record<string, unknown>>;
  assert.equal(assistantContent[0]?.text, "answer\n\n\ncontinued");
  assert.deepEqual(assistantContent[1], originalMessages[1]?.content[1]);
  const toolContent = result.messages[2]?.content as Array<Record<string, unknown>>;
  assert.equal(toolContent[0]?.text, "error");
  assert.deepEqual(toolContent[1], originalMessages[2]?.content[1]);
  assert.deepEqual(result.messages[2]?.details, { exitCode: 1 });
  assert.equal(originalMessages[0]?.content, "first\n\n\n\nsecond");
});

test("tool_result hook replaces text only and leaves details, errors, usage, and images to Pi", async () => {
  const { handlers } = extensionHarness({
    fetch: outcomeFetch([], (content) => {
      const part = content.messages[0]?.parts[0];
      if (part) part.content = "clean output";
    }),
  });
  const handler = handlers.get("tool_result");
  assert.ok(handler);
  const result = (await handler(
    {
      type: "tool_result",
      toolCallId: "tool-1",
      toolName: "bash",
      input: { command: "build" },
      content: [
        { type: "text", text: "\u001b[32mclean output\u001b[0m" },
        { type: "image", mimeType: "image/png", data: "AA==" },
      ],
      details: { exitCode: 0 },
      isError: false,
      usage: { input: 10, output: 20 },
    } as never,
    context(),
  )) as { content: Array<Record<string, unknown>> };
  assert.deepEqual(result, {
    content: [
      { type: "text", text: "clean output" },
      { type: "image", mimeType: "image/png", data: "AA==" },
    ],
  });
  assert.equal("details" in result, false);
  assert.equal("isError" in result, false);
  assert.equal("usage" in result, false);
});

test("before_agent_start optimizes the system prompt through the supported seam", async () => {
  const { handlers } = extensionHarness({
    fetch: outcomeFetch([], (content) => {
      const part = content.messages[0]?.parts[0];
      if (part) part.text = "System\n\n\nRules";
    }),
  });
  const handler = handlers.get("before_agent_start");
  assert.ok(handler);
  const result = await handler(
    {
      type: "before_agent_start",
      prompt: "hello",
      systemPrompt: "System\n\n\n\nRules",
      systemPromptOptions: {},
    } as never,
    context(),
  );
  assert.deepEqual(result, { systemPrompt: "System\n\n\nRules" });
});

test("receipt status and local token delta gate application; provider verification is separate", async () => {
  const { handlers } = extensionHarness({
    fetch: outcomeFetch(
      [],
      (content) => {
        const part = content.messages[0]?.parts[0];
        if (part) part.text = "changed";
      },
      "skipped",
    ),
  });
  const result = await handlers.get("context")?.(
    { type: "context", messages: [{ role: "user", content: "original" }] } as never,
    context(),
  );
  assert.equal(result, undefined);
});

test("optimizer failures are fail-open for every mutating handler", async () => {
  const fetch: FetchLike = async () => {
    throw new Error("offline");
  };
  const { handlers } = extensionHarness({ fetch });
  const contextResult = await handlers.get("context")?.(
    { type: "context", messages: [{ role: "user", content: "unchanged" }] } as never,
    context(),
  );
  const toolResult = await handlers.get("tool_result")?.(
    {
      type: "tool_result",
      toolCallId: "c",
      toolName: "bash",
      input: {},
      content: [{ type: "text", text: "unchanged" }],
      isError: false,
    } as never,
    context(),
  );
  assert.equal(contextResult, undefined);
  assert.equal(toolResult, undefined);
});

test("shadow mode analyzes without returning replacement messages", async () => {
  const calls: FetchCall[] = [];
  const { handlers } = extensionHarness({
    shadow: true,
    fetch: outcomeFetch(calls, () => undefined),
  });
  const result = await handlers.get("context")?.(
    { type: "context", messages: [{ role: "user", content: "inspect me" }] } as never,
    context(),
  );
  assert.equal(result, undefined);
  assert.equal(calls[0]?.url.pathname, "/v1/analyze");
});

test("assistant output observation is opt-in and never registers a rewrite", async () => {
  const calls: FetchCall[] = [];
  const { handlers } = extensionHarness({
    observeOutput: true,
    fetch: outcomeFetch(calls, () => undefined),
  });
  const handler = handlers.get("message_end");
  assert.ok(handler);
  const result = await handler(
    { type: "message_end", message: { role: "assistant", content: "final answer" } } as never,
    context(),
  );
  assert.equal(result, undefined);
  assert.equal(calls[0]?.url.pathname, "/v1/analyze");
});

test("only credential-free numeric loopback endpoints are accepted", () => {
  assert.equal(validateNumericLoopbackEndpoint("http://127.0.0.1:7331").port, "7331");
  assert.throws(() => validateNumericLoopbackEndpoint("http://localhost:7331"));
  assert.throws(() => validateNumericLoopbackEndpoint("https://127.0.0.1:7331"));
});
