# KendrOptimizer for Claude Code

This package is a real Claude Code V1 plugin plus a small local HTTP-hook bridge. It never calls an LLM provider, reads provider credentials, or acts as a gateway.

The adapter was audited against Claude Code `2.1.224` at commit `66edf5358349356774812264b75b8ea792f0d0a3`. Claude Code's documented hooks impose an important limit: `PostToolUse.updatedToolOutput` can replace a successful tool result, but `UserPromptSubmit` cannot replace the prompt. There is no documented generic history, tool-schema, or generation-request rewrite hook.

## What is active

| Surface | Behavior |
| --- | --- |
| Successful tool result | Applies a validated, representation-safe transform through `PostToolUse.updatedToolOutput` |
| Current user prompt | Sends a local shadow analysis only; returns no context or replacement |
| Assistant output | Sends a local shadow analysis from `Stop`; never rewrites finalized output |
| Failed tool result | Unsupported by Claude Code's replacement API |
| History, tool schemas, generation controls | Unsupported |

The full machine-readable declaration is in `capabilities.json`.

## Local install

Start KendrOptimizer on numeric loopback:

```powershell
cargo run -p kendr-optimizer-cli -- serve --bind 127.0.0.1:7331
```

Build this package and start its hook bridge in a second terminal:

```powershell
cd integrations/claude-code
npm ci
npm run build
node dist/server.js
```

Load the plugin from its package root:

```powershell
claude --plugin-dir "D:\path\to\KendrOptimizer\integrations\claude-code"
```

For a published package, `npm install -g @kendr/optimizer-claude-code` exposes the `kendr-claude-code-bridge` executable. The plugin root still needs to be installed through a Claude Code marketplace or supplied with `--plugin-dir`.

If an organization configures `allowedHttpHookUrls`, its policy must allow the three exact `http://127.0.0.1:7332/hooks/claude-code/...` URLs in `hooks/hooks.json`.

## Configuration

| Environment variable | Default | Meaning |
| --- | --- | --- |
| `KENDR_OPTIMIZER_ENDPOINT` | `http://127.0.0.1:7331` | Transform-only core endpoint; numeric loopback only |
| `KENDR_CLAUDE_BRIDGE_PORT` | `7332` | Local Claude HTTP-hook bridge port |
| `KENDR_OPTIMIZER_SHADOW` | false | Analyze successful tool results without replacing them |

The bridge binds only `127.0.0.1`, caps hook bodies at 1 MiB by default, uses a 100 ms optimizer timeout, and always returns an empty successful hook response on optimizer failure. If the bridge itself is stopped, Claude Code documents HTTP connection failures and timeouts as non-blocking.

## Structured tool results

Claude Code requires `updatedToolOutput` to retain the original tool output shape. The bridge therefore:

1. extracts only plain-string results or known text-bearing fields such as `stdout`, `stderr`, `content`, `text`, `output`, and `diff`;
2. preserves all other keys and values;
3. validates message IDs, part counts, part types, and tool call IDs in the optimizer response;
4. commits the replacement only when the receipt status is `applied` and has a positive local token delta.

`verified_savings` is deliberately not the application gate. Kendr uses that field for paired provider-usage verification, which is separate from a locally applied and measured transform.

## Development

```powershell
npm ci
npm run typecheck
npm test
npm pack --dry-run
```

References: [Claude Code hooks](https://code.claude.com/docs/en/hooks), [plugins](https://code.claude.com/docs/en/plugins), and [v2.1.224 release](https://github.com/anthropics/claude-code/releases/tag/v2.1.224).
