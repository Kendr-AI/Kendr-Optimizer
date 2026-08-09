import assert from "node:assert/strict";
import test from "node:test";

import {
  handleClaudeCodeHook,
  type FetchLike,
  validateNumericLoopbackEndpoint,
} from "./index.js";

interface CapturedCall {
  url: URL;
  body: Record<string, unknown>;
}

function respondingFetch(
  calls: CapturedCall[],
  transform: (request: Record<string, unknown>) => Record<string, unknown>,
): FetchLike {
  return async (input, init) => {
    const body = JSON.parse(String(init?.body)) as Record<string, unknown>;
    calls.push({ url: new URL(String(input)), body });
    return new Response(JSON.stringify(transform(body)), {
      status: 200,
      headers: { "content-type": "application/json" },
    });
  };
}

function outcome(
  request: Record<string, unknown>,
  replacement: string,
  status = "applied",
): Record<string, unknown> {
  const content = structuredClone(request.content) as {
    messages: Array<{ parts: Array<Record<string, unknown>> }>;
  };
  const firstMessage = content.messages[0];
  const firstPart = firstMessage?.parts[0];
  if (firstPart) firstPart.content = replacement;
  return {
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
  };
}

test("PostToolUse replaces only text-bearing leaves and preserves output shape", async () => {
  const calls: CapturedCall[] = [];
  const original = {
    stdout: "line\nline\nline\nline\n",
    stderr: "",
    interrupted: false,
    isImage: false,
    status: "ok",
  };
  const result = await handleClaudeCodeHook(
    "/hooks/claude-code/post-tool-use",
    {
      hook_event_name: "PostToolUse",
      session_id: "session-1",
      tool_use_id: "call-1",
      tool_name: "Bash",
      tool_response: original,
    },
    {
      fetch: respondingFetch(calls, (request) => outcome(request, "line\n")),
    },
  );

  assert.equal(calls.length, 1);
  assert.equal(calls[0]?.url.pathname, "/v1/optimize");
  assert.deepEqual(result, {
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      updatedToolOutput: {
        ...original,
        stdout: "line\n",
      },
    },
  });
});

test("PostToolUse supports a plain string tool response", async () => {
  const result = await handleClaudeCodeHook(
    "/hooks/claude-code/post-tool-use",
    {
      hook_event_name: "PostToolUse",
      tool_use_id: "call-2",
      tool_response: "alpha\nalpha\nalpha\nalpha\n",
    },
    {
      fetch: respondingFetch([], (request) => outcome(request, "alpha\n")),
    },
  );
  assert.deepEqual(result, {
    hookSpecificOutput: {
      hookEventName: "PostToolUse",
      updatedToolOutput: "alpha\n",
    },
  });
});

test("skipped optimizer output is never applied", async () => {
  const result = await handleClaudeCodeHook(
    "/hooks/claude-code/post-tool-use",
    {
      hook_event_name: "PostToolUse",
      tool_use_id: "call-3",
      tool_response: "repeat\nrepeat\nrepeat\nrepeat\n",
    },
    {
      fetch: respondingFetch([], (request) => outcome(request, "repeat\n", "skipped")),
    },
  );
  assert.deepEqual(result, {});
});

test("prompt and assistant output hooks are shadow-only", async () => {
  const calls: CapturedCall[] = [];
  const fetch = respondingFetch(calls, (request) => outcome(request, "ignored"));

  const promptResult = await handleClaudeCodeHook(
    "/hooks/claude-code/user-prompt-submit",
    { hook_event_name: "UserPromptSubmit", prompt: "Please inspect the build" },
    { fetch },
  );
  const stopResult = await handleClaudeCodeHook(
    "/hooks/claude-code/stop",
    { hook_event_name: "Stop", last_assistant_message: "The build failed." },
    { fetch },
  );

  assert.deepEqual(promptResult, {});
  assert.deepEqual(stopResult, {});
  assert.equal(calls.length, 2);
  assert.equal(calls[0]?.url.pathname, "/v1/analyze");
  assert.equal(calls[1]?.url.pathname, "/v1/analyze");
  const firstPolicy = calls[0]?.body.policy as Record<string, unknown>;
  assert.equal(firstPolicy.shadow, true);
});

test("optimizer failure is fail-open", async () => {
  const fetch: FetchLike = async () => {
    throw new Error("optimizer unavailable");
  };
  const result = await handleClaudeCodeHook(
    "/hooks/claude-code/post-tool-use",
    {
      hook_event_name: "PostToolUse",
      tool_response: "keep this exact output",
    },
    { fetch },
  );
  assert.deepEqual(result, {});
});

test("only credential-free numeric loopback endpoints are accepted", () => {
  assert.equal(validateNumericLoopbackEndpoint("http://127.0.0.1:7331").port, "7331");
  assert.equal(validateNumericLoopbackEndpoint("http://[::1]:7331").port, "7331");
  assert.throws(() => validateNumericLoopbackEndpoint("http://localhost:7331"));
  assert.throws(() => validateNumericLoopbackEndpoint("https://127.0.0.1:7331"));
  assert.throws(() => validateNumericLoopbackEndpoint("http://user:secret@127.0.0.1:7331"));
  assert.throws(() => validateNumericLoopbackEndpoint("http://127.0.0.1:7331/v1"));
});
