# Claude Code compatibility audit

Audit date: 2026-08-07

Audited host: Claude Code `2.1.224`, commit `66edf5358349356774812264b75b8ea792f0d0a3`.

Adapter: `integrations/claude-code`.

## Host boundary

Claude Code offers command, HTTP, prompt, agent, and MCP-tool hooks. The integration uses HTTP hooks because the transform core is already a credential-free loopback service and HTTP connection failures are documented as non-blocking.

| Lifecycle point | Host capability | Kendr mode |
| --- | --- | --- |
| `UserPromptSubmit` | May add context or block; cannot replace submitted prompt | Shadow analysis |
| `PostToolUse` | `updatedToolOutput` replaces a successful result with the same shape | Representation-safe apply |
| `PostToolUseFailure` | Logging/feedback; no failed-output replacement | Unsupported |
| `Stop` | Observation/control after response | Shadow output analysis |
| `MessageDisplay` | Can replace displayed text only, not transcript/model context | Unused |
| Conversation history | No documented generic mutation hook | Unsupported |
| Tool definitions | No documented generic schema mutation hook | Unsupported |
| Generation request | No documented provider-neutral mutation hook | Unsupported |

The adapter does not inject an “optimized” copy beside the current prompt because that would increase input tokens while leaving the original prompt intact.

## Architecture

```text
Claude Code HTTP hook
  -> 127.0.0.1:7332 bridge
  -> 127.0.0.1:7331 KendrOptimizer
  -> validated same-shape PostToolUse response
```

Both hops are local. The bridge has no provider client and accepts no provider credential. Active transforms use `risk_ceiling=representation_safe` and explicitly disable lossy tool-output pruning, tool selection, recovery references, and generation policy.

For structured results, only plain-string outputs and known text-bearing keys are bound into the Kendr envelope. Every non-text field is copied from the original. Claude Code itself performs an additional built-in schema check for built-in tool output.

## Fail-open cases

The bridge returns an empty JSON object and makes no Claude decision when:

- the optimizer times out or refuses the request;
- a response is not valid `kendr.receipt/v1`;
- receipt status is not `applied` or the local token delta is not positive;
- message IDs, roles, part counts, part types, or tool call IDs differ;
- no safe text-bearing leaf exists;
- transformed content is unchanged.

If the bridge process is unavailable, Claude Code treats HTTP connection failure or timeout as non-blocking.

## Verification

From `integrations/claude-code`:

```powershell
npm ci
npm run typecheck
npm test
npm pack --dry-run
```

Tests cover string and structured tool results, shape retention, applied-versus-skipped receipts, prompt/output shadow behavior, network fail-open behavior, and numeric-loopback enforcement.

## Upgrade gate

Re-audit before changing the pinned host version. In particular, confirm:

1. HTTP hook error semantics remain non-blocking;
2. `updatedToolOutput` retains its same-shape contract;
3. `UserPromptSubmit` still cannot replace the prompt;
4. hook configuration and plugin manifest schemas remain compatible.

Primary sources: [hooks reference](https://code.claude.com/docs/en/hooks), [plugins reference](https://code.claude.com/docs/en/plugins), [usage monitoring](https://code.claude.com/docs/en/monitoring-usage), and [v2.1.224 release](https://github.com/anthropics/claude-code/releases/tag/v2.1.224).
