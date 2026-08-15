# KendrOptimizer for OpenCode V1

This package is an OpenCode V1 plugin that sends only transform envelopes to a local KendrOptimizer process. It does not proxy LLM requests, alter provider URLs, or read provider credentials.

The package is type-checked against `@opencode-ai/plugin@1.18.15` and audited at commit `d7b115f623760e68a4749d16508a9eca350f246f`. OpenCode's separate V2 plugin API is currently beta and is not claimed by this adapter.

## Hooks

| Hook | Default | Behavior |
| --- | --- | --- |
| `chat.message` | on | Optimizes text parts in the current user message |
| `tool.execute.after` | on | Optimizes the completed tool output string |
| `experimental.chat.messages.transform` | off | Experimental full-history text transform |
| `experimental.chat.system.transform` | off | Experimental system-string transform |
| `chat.params` | unused | Generic output limits are not assumed quality-neutral |
| `tool.definition` | unused | No generic quality-safe schema rewrite or guaranteed full-tool retry |

OpenCode V1's hook runner propagates plugin exceptions. Each Kendr hook therefore catches internally, makes no host mutation before full response validation, and leaves the supplied output unchanged on every failure.

## Install

The normal user path is:

```powershell
kendr-opt run opencode
```

`kendr-opt` installs the dependency-free `dist/kendr-optimizer.js` bundle in
OpenCode's global local-plugin directory and manages the optimizer process for
the OpenCode session. The GitHub Release also carries
`kendr-optimizer-opencode-0.1.4.tgz` for managed package deployment. No npm
registry publication is required.

## Options

OpenCode V1 accepts plugin option tuples:

```json
{
  "plugin": [
    [
      "@kendr/optimizer-opencode",
      {
        "coreEndpoint": "http://127.0.0.1:7331",
        "timeoutMs": 100,
        "shadow": false,
        "experimentalHistory": false,
        "experimentalSystem": false
      }
    ]
  ]
}
```

Equivalent environment flags are:

- `KENDR_OPTIMIZER_ENDPOINT`
- `KENDR_OPTIMIZER_TIMEOUT_MS`
- `KENDR_OPTIMIZER_SHADOW`
- `KENDR_OPENCODE_EXPERIMENTAL_HISTORY`
- `KENDR_OPENCODE_EXPERIMENTAL_SYSTEM`

Only credential-free numeric loopback endpoints are accepted. An invalid endpoint disables the plugin hooks instead of failing OpenCode initialization.

Experimental hooks are absent from the returned hook object unless explicitly enabled. They remain subject to OpenCode API changes and should be re-audited before upgrading the host beyond the pinned version.

## Safety and measurement

The adapter transforms only explicit `type: "text"` parts and the stable tool-output string. File, image, reasoning, tool-call, message-info, title, and metadata structures are preserved.

A result is committed only when the receipt is structurally valid, has `status = applied`, reports a positive local token delta, and contains fewer optimized tokens. `verified_savings` remains an external paired-usage concept and is not conflated with local application.

The adapter reports no reference restoration, tool narrowing, full-tool retry, generation controls, or provider payload support.

## Development

```powershell
npm ci
npm run typecheck
npm test
npm pack --dry-run
```

References: [OpenCode V1 plugin docs](https://opencode.ai/docs/plugins/), [exact V1 hook interface](https://github.com/anomalyco/opencode/blob/d7b115f623760e68a4749d16508a9eca350f246f/packages/plugin/src/index.ts), [v1.18.15 release](https://github.com/anomalyco/opencode/releases/tag/v1.18.15), and [V2 beta docs](https://opencode.ai/v2/docs/build/plugins).
