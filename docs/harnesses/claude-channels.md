# Claude Code Channels compatibility audit

Audit date: 2026-08-07

Audited host: Channels research preview in Claude Code `2.1.224`, commit `66edf5358349356774812264b75b8ea792f0d0a3`.

Adapter: `integrations/claude-channels`.

## Host boundary

A channel is an MCP server that emits `notifications/claude/channel` with `params.content` and optional `params.meta`. Claude Code turns the content into a channel event in the conversation.

There is no generic Claude Code plugin seam that lets one package rewrite another installed channel server's notification. Kendr therefore integrates at the producer:

```text
external transport
  -> transport authentication
  -> sender/channel authorization
  -> normalize message
  -> Kendr source-side helper
  -> notifications/claude/channel
```

This placement saves input tokens before Claude sees the event and keeps the existing channel plugin responsible for identity, replay protection, rate limits, and routing.

## Security contract

`prepareNotification` requires `senderAuthorized: true`. Unauthorized calls return the same object reference without an optimizer request. This makes the ordering requirement executable rather than advisory.

The helper:

- accepts only `http://127.0.0.1` or `http://[::1]` core endpoints;
- rejects endpoint credentials, paths, queries, and fragments;
- preserves `meta` and every unknown notification field;
- runs only representation-safe engines;
- returns the original object on every failure.

Channel producers should not place secrets in `content` or `meta` that they would not otherwise deliver to Claude. KendrOptimizer is local, but it still receives the text being transformed.

## Integration assertion

The channel implementation should test these properties at its own boundary:

1. unauthorized senders never reach `prepareNotification` with `senderAuthorized=true`;
2. `params.meta` is byte-for-byte equivalent before and after preparation;
3. the exact original notification is emitted if the core is stopped;
4. shadow mode never changes `params.content`;
5. the emitted MCP method remains `notifications/claude/channel`.

Package tests separately cover metadata retention, authorization-before-fetch, timeout fail-open, shadow mode, skipped receipts, and loopback enforcement.

## Research-preview warning

Channels is explicitly a research preview. Re-audit its notification schema, plugin packaging, and sender-gating guidance on every Claude Code upgrade. Do not silently widen accepted content shapes if the host introduces attachments or richer channel parts; bind new fields only after their semantics are understood.

Primary sources: [Channels reference](https://code.claude.com/docs/en/channels-reference) and [Claude Code v2.1.224 release](https://github.com/anthropics/claude-code/releases/tag/v2.1.224).
