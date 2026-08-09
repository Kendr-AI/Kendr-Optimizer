# OpenCode V1 compatibility audit

Audit date: 2026-08-07

Audited host: `@opencode-ai/plugin@1.18.15`, commit `d7b115f623760e68a4749d16508a9eca350f246f`.

Adapter: `integrations/opencode`.

## Hook classification

| V1 hook | Stability label | Kendr mode |
| --- | --- | --- |
| `chat.message` | Stable V1 interface | Current-user text transform |
| `tool.execute.after` | Stable V1 interface | Tool-output string transform |
| `chat.params` | Stable V1 interface | Disabled; generic limits are not quality-neutral |
| `tool.definition` | Typed but not used | Disabled; no quality-safe generic rewrite/retry contract |
| `experimental.chat.messages.transform` | Experimental | Explicit opt-in history transform |
| `experimental.chat.system.transform` | Experimental | Explicit opt-in system transform |

The experimental hooks are not merely no-ops by default: they are absent from the hook object unless `experimentalHistory` or `experimentalSystem` is true.

## Fail-open design

OpenCode V1's plugin trigger awaits hook functions without a general catch around each hook. A thrown plugin exception can therefore fail the host operation. The adapter guards at two levels:

1. the local HTTP client converts timeout, network, non-2xx, and JSON errors into no result;
2. every OpenCode hook has its own `try/catch` and mutates host output only after full receipt and shape validation.

Invalid endpoint configuration returns an empty hooks object, allowing OpenCode initialization to continue.

## Shape policy

`chat.message` and experimental context hooks bind only explicit `type: "text"` parts. File, image, reasoning, tool-call, tool-state, message-info, and unknown parts remain untouched. `tool.execute.after` changes only `output.output` and retains title and metadata.

The adapter does not use `before` hooks, provider registration, header mutation, or provider payloads. It reports all tool-selection, recovery, and generation capabilities as false.

## V1 versus V2

OpenCode V2 plugins use a separate beta API and a different default-export manifest. This package makes no V2 claim. Keep the V1 package pinned until a dedicated V2 adapter is implemented and tested; do not alias the V1 hooks into a V2 manifest.

## Verification and upgrade gate

The package compiles against `@opencode-ai/plugin@1.18.15`. Tests cover stable hooks, experimental opt-in registration, history/system shape preservation, local receipt gating, network fail-open, shadow behavior, invalid configuration, and loopback enforcement.

Re-audit V1 hook types and the host trigger's exception semantics before upgrading. Also check whether the experimental hooks have stabilized, changed shape, or been removed.

Primary sources: [V1 plugin docs](https://opencode.ai/docs/plugins/), [exact V1 hook interface](https://github.com/anomalyco/opencode/blob/d7b115f623760e68a4749d16508a9eca350f246f/packages/plugin/src/index.ts), [V1 trigger implementation](https://github.com/anomalyco/opencode/blob/d7b115f623760e68a4749d16508a9eca350f246f/packages/opencode/src/plugin/index.ts), [v1.18.15 release](https://github.com/anomalyco/opencode/releases/tag/v1.18.15), and [V2 beta docs](https://opencode.ai/v2/docs/build/plugins).
