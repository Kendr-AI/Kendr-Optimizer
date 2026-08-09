import assert from "node:assert/strict";
import test from "node:test";

import {
  createClaudeChannelOptimizer,
  type FetchLike,
  validateNumericLoopbackEndpoint,
} from "./index.js";

function fakeFetch(
  inspect: (request: Record<string, unknown>, url: URL) => void,
  replacement: string,
  status = "applied",
): FetchLike {
  return async (input, init) => {
    const request = JSON.parse(String(init?.body)) as Record<string, unknown>;
    inspect(request, new URL(String(input)));
    const content = structuredClone(request.content) as {
      messages: Array<{ parts: Array<Record<string, unknown>> }>;
    };
    const firstPart = content.messages[0]?.parts[0];
    if (firstPart) firstPart.text = replacement;
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

test("authorized source content is optimized while meta and unknown fields are preserved", async () => {
  let called = 0;
  const meta = { sender: "trusted-user", messageId: "m-1" };
  const notification = {
    content: "hello    world",
    meta,
    deliveryClass: "realtime",
  };
  const optimizer = createClaudeChannelOptimizer({
    fetch: fakeFetch((request, url) => {
      called += 1;
      assert.equal(url.pathname, "/v1/optimize");
      assert.equal(request.phase, "request");
    }, "hello world"),
  });

  const result = await optimizer.prepareNotification(notification, {
    senderAuthorized: true,
    channelName: "telegram",
  });

  assert.equal(called, 1);
  assert.equal(result.applied, true);
  assert.equal(result.reason, "optimized");
  assert.notEqual(result.notification, notification);
  assert.equal(result.notification.content, "hello world");
  assert.equal(result.notification.meta, meta);
  assert.equal(result.notification.deliveryClass, "realtime");
});

test("sender authorization gates optimization before any optimizer call", async () => {
  let called = false;
  const notification = { content: "do not inspect", meta: { sender: "unknown" } };
  const optimizer = createClaudeChannelOptimizer({
    fetch: fakeFetch(() => {
      called = true;
    }, "changed"),
  });

  const result = await optimizer.prepareNotification(notification, {
    senderAuthorized: false,
  });
  assert.equal(called, false);
  assert.equal(result.notification, notification);
  assert.equal(result.reason, "sender_not_authorized");
});

test("network and HTTP failures return the original object unchanged", async () => {
  const fetch: FetchLike = async () => {
    throw new Error("offline");
  };
  const notification = { content: "preserve exact payload", meta: { id: 1 } };
  const result = await createClaudeChannelOptimizer({ fetch }).prepareNotification(notification, {
    senderAuthorized: true,
  });
  assert.equal(result.notification, notification);
  assert.equal(result.applied, false);
  assert.equal(result.reason, "optimizer_unavailable");
});

test("shadow mode analyzes but never mutates channel content", async () => {
  let endpoint = "";
  const notification = { content: "hello    world", meta: { id: 2 } };
  const optimizer = createClaudeChannelOptimizer({
    shadow: true,
    fetch: fakeFetch((request, url) => {
      endpoint = url.pathname;
      const policy = request.policy as Record<string, unknown>;
      assert.equal(policy.shadow, true);
    }, "hello world"),
  });
  const result = await optimizer.prepareNotification(notification, { senderAuthorized: true });
  assert.equal(endpoint, "/v1/analyze");
  assert.equal(result.notification, notification);
  assert.equal(result.reason, "shadow_only");
});

test("skipped output is not applied", async () => {
  const notification = { content: "hello    world" };
  const result = await createClaudeChannelOptimizer({
    fetch: fakeFetch(() => undefined, "hello world", "skipped"),
  }).prepareNotification(notification, { senderAuthorized: true });
  assert.equal(result.notification, notification);
  assert.equal(result.reason, "not_applied_or_invalid");
});

test("only numeric loopback is accepted", () => {
  assert.equal(validateNumericLoopbackEndpoint("http://127.0.0.1:7331").hostname, "127.0.0.1");
  assert.throws(() => validateNumericLoopbackEndpoint("http://localhost:7331"));
  assert.throws(() => validateNumericLoopbackEndpoint("http://192.168.1.10:7331"));
});
