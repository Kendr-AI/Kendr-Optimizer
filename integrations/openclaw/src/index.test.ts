import assert from "node:assert/strict";
import { mkdtemp, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import test from "node:test";

import register, { __testing } from "./index.js";

interface TestEngine {
  assemble(params: {
    sessionId: string;
    messages: Record<string, unknown>[];
  }): Promise<{
    messages: Record<string, unknown>[];
    estimatedTokens: number;
  }>;
  commitTurn(params: {
    advancementKey: string;
  }): Promise<{ status: "committed" | "duplicate" }>;
}

function createTestEngine(
  config: Record<string, unknown>,
  workspaceDir: string,
): { engine: TestEngine; warnings: string[] } {
  let capturedFactory: unknown;
  const warnings: string[] = [];
  register({
    logger: {
      debug() {},
      info() {},
      warn(message) {
        warnings.push(message);
      },
      error(message) {
        warnings.push(message);
      },
    },
    registerContextEngine(id, factory) {
      assert.equal(id, "kendr-optimizer");
      capturedFactory = factory;
    },
  });
  assert.equal(typeof capturedFactory, "function");
  const factory = capturedFactory as (context: {
    config: Record<string, unknown>;
    workspaceDir: string;
  }) => TestEngine;
  return {
    engine: factory({ config, workspaceDir }),
    warnings,
  };
}

test("accepts only credential-free loopback origins", () => {
  assert.equal(
    __testing.normalizeLoopbackOrigin("http://127.0.0.1:7331"),
    "http://127.0.0.1:7331",
  );
  assert.equal(
    __testing.normalizeLoopbackOrigin("https://[::1]:7331/"),
    "https://[::1]:7331",
  );

  assert.throws(
    () => __testing.normalizeLoopbackOrigin("https://optimizer.example.com"),
    /loopback/,
  );
  assert.throws(
    () => __testing.normalizeLoopbackOrigin("http://localhost:7331"),
    /numeric loopback/,
  );
  assert.throws(
    () => __testing.normalizeLoopbackOrigin("http://user:secret@127.0.0.1:7331"),
    /credential-free/,
  );
  assert.throws(
    () => __testing.normalizeLoopbackOrigin("http://127.0.0.1:7331/proxy"),
    /credential-free/,
  );
});

test("round-trips supported OpenClaw content without semantic reshaping", () => {
  const original = [
    {
      role: "user",
      content: [{ type: "text", text: "Please inspect this." }],
      timestamp: 123,
    },
    {
      role: "assistant",
      content: [
        {
          type: "toolCall",
          id: "call-1",
          name: "shell",
          arguments: { command: "git status" },
        },
      ],
    },
    {
      role: "toolResult",
      toolCallId: "call-1",
      toolName: "shell",
      content: [{ type: "text", text: "\u001b[32mclean\u001b[0m" }],
      isError: false,
    },
  ];

  const encoded = __testing.encodeMessages(original);
  const decoded = __testing.decodeMessages(encoded.messages, encoded.bindings);
  assert.deepEqual(decoded, original);
});

test("applies returned text only while preserving OpenClaw envelope fields", () => {
  const original = [
    {
      role: "user",
      content: [{ type: "text", text: "alpha   beta" }],
      timestamp: 456,
    },
  ];
  const encoded = __testing.encodeMessages(original);
  const firstPart = encoded.messages[0]?.parts[0];
  assert.ok(firstPart);
  assert.equal(firstPart.type, "text");
  if (firstPart.type !== "text") {
    throw new Error("expected a text part");
  }
  firstPart.text = "alpha beta";

  const decoded = __testing.decodeMessages(encoded.messages, encoded.bindings);
  assert.deepEqual(decoded, [
    {
      role: "user",
      content: [{ type: "text", text: "alpha beta" }],
      timestamp: 456,
    },
  ]);
});

test("rejects any optimizer mutation to a tool call", () => {
  const original = [
    {
      role: "assistant",
      content: [
        {
          type: "toolCall",
          id: "call-2",
          name: "read_file",
          arguments: { path: "README.md" },
        },
      ],
    },
  ];
  const encoded = __testing.encodeMessages(original);
  const firstPart = encoded.messages[0]?.parts[0];
  assert.ok(firstPart);
  assert.equal(firstPart.type, "tool_call");
  if (firstPart.type !== "tool_call") {
    throw new Error("expected a tool call");
  }
  firstPart.name = "delete_file";

  assert.throws(
    () => __testing.decodeMessages(encoded.messages, encoded.bindings),
    /tool call changed/,
  );
});

test("defaults to representation-safe optimization", () => {
  const config = __testing.parseConfig({});
  assert.equal(config.endpoint, "http://127.0.0.1:7331");
  assert.equal(config.riskCeiling, "representation_safe");
  assert.equal(config.shadow, false);
});

test("fails open when the local transform service is unavailable", async () => {
  const previousFetch = globalThis.fetch;
  globalThis.fetch = (async () => {
    throw new Error("connection refused");
  }) as typeof globalThis.fetch;

  try {
    const { engine, warnings } = createTestEngine(
      {
        endpoint: "http://127.0.0.1:17331",
        failureBackoffMs: 0,
      },
      process.cwd(),
    );
    const original = [
      {
        role: "user",
        content: [{ type: "text", text: "leave this untouched" }],
      },
    ];
    const result = await engine.assemble({
      sessionId: "fail-open",
      messages: original,
    });

    assert.strictEqual(result.messages, original);
    assert.match(warnings[0] ?? "", /using the original OpenClaw context/);
  } finally {
    globalThis.fetch = previousFetch;
  }
});

test("persists idempotent OpenClaw advancement keys without message data", async () => {
  const temporary = await mkdtemp(join(tmpdir(), "kendr-openclaw-test-"));
  try {
    const { engine } = createTestEngine({}, temporary);
    assert.deepEqual(await engine.commitTurn({ advancementKey: "turn-a" }), {
      status: "committed",
    });
    assert.deepEqual(await engine.commitTurn({ advancementKey: "turn-b" }), {
      status: "committed",
    });
    assert.deepEqual(await engine.commitTurn({ advancementKey: "turn-a" }), {
      status: "duplicate",
    });
  } finally {
    await rm(temporary, { recursive: true, force: true });
  }
});
