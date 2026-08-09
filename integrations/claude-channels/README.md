# KendrOptimizer for Claude Code Channels

This package is a source-side helper for Claude Code Channels. It optimizes `notifications/claude/channel.params.content` immediately before a channel MCP server emits the notification. It is not a gateway and it cannot interpose on notifications emitted by some other installed channel plugin.

The adapter was audited against the Channels research preview shipped with Claude Code `2.1.224` at commit `66edf5358349356774812264b75b8ea792f0d0a3`.

## Required order

Authentication and sender authorization must happen before optimization:

```text
transport input
  -> authenticate sender
  -> apply channel allowlist / sender policy
  -> Kendr prepareNotification(...)
  -> notifications/claude/channel
```

The helper requires `senderAuthorized: true`. Any other value returns the original notification object without contacting KendrOptimizer.

## Install and use

Start the local transform-only core:

```powershell
cargo run -p kendr-optimizer-cli -- serve --bind 127.0.0.1:7331
npm install @kendr/optimizer-claude-channels
```

Use the helper inside the channel MCP server, after its existing sender gate:

```typescript
import { createClaudeChannelOptimizer } from "@kendr/optimizer-claude-channels"

const optimizer = createClaudeChannelOptimizer()

const incoming = {
  content: normalizedMessageText,
  meta: existingChannelMetadata,
}

const result = await optimizer.prepareNotification(incoming, {
  senderAuthorized: true,
  sessionId,
  channelName: "telegram",
  senderClass: "allowlisted",
})

await mcpServer.notification({
  method: "notifications/claude/channel",
  params: result.notification,
})
```

`meta` and every unknown notification property are retained. On timeouts, connection failures, non-2xx responses, invalid receipts, shadow mode, or an unauthorized sender, the original object is returned unchanged.

## Configuration

```typescript
const optimizer = createClaudeChannelOptimizer({
  coreEndpoint: "http://127.0.0.1:7331",
  timeoutMs: 100,
  shadow: false,
})
```

Only credential-free numeric loopback endpoints are accepted. The default policy is representation-safe, with tool selection, lossy tool-output pruning, reference recovery, and generation policy disabled.

The result contains:

- `notification`: original or optimized notification;
- `applied`: whether content was replaced;
- `reason`: a stable reason such as `optimized`, `sender_not_authorized`, `shadow_only`, or `optimizer_unavailable`;
- `requestId` when an optimizer request was attempted.

Kendr applies only receipts with status `applied`, a positive local token delta, and a lower optimized token measurement. The separate `verified_savings` receipt field remains reserved for paired provider-usage evidence.

## Development

```powershell
npm ci
npm run typecheck
npm test
npm pack --dry-run
```

References: [Claude Code Channels reference](https://code.claude.com/docs/en/channels-reference) and [Claude Code v2.1.224](https://github.com/anthropics/claude-code/releases/tag/v2.1.224).
