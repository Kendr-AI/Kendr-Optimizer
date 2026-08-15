# Pi compatibility audit

Audit date: 2026-08-07

Audited host: `@earendil-works/pi-coding-agent@0.84.1`, commit `53fa77ccd8a279eb87e92294ef3687b03ff80112`.

Adapter: `integrations/pi-agent`.

## Supported extension events

| Event/API | Host capability | Kendr decision |
| --- | --- | --- |
| `before_agent_start` | Can replace the assembled system prompt | Apply representation-safe text transform |
| `context` | Can replace `AgentMessage[]` before each LLM call | Apply text-only context transform |
| `tool_result` | Can replace content/details/error/usage | Return content text only; preserve all other host fields |
| `message_end` | Can replace finalized same-role message | Optional observation only |
| `input` | Can transform raw user input | Unused because `context` sees the actual dispatched message |
| `before_provider_request` | Can replace provider payload | Excluded as provider-specific |
| `getAllTools/getActiveTools/setActiveTools` | Can change active tool set | Disabled without guaranteed automatic full-tool retry |
| Tool definition descriptions/schemas | No direct generic rewrite API | Unsupported |

The context mapper retains Pi's complete messages and changes only string content or explicit `type: "text"` blocks. Thinking, images, tool calls, usage, timestamps, custom fields, and message ordering stay host-owned.

## Capability honesty

Although Pi can change the active tool set, this adapter reports:

```json
{
  "can_narrow_tools": false,
  "can_restore_references": false,
  "can_retry_with_full_tools": false,
  "can_set_max_output_tokens": false,
  "can_set_verbosity": false,
  "can_append_generation_policy": false
}
```

This prevents the core from selecting tools, emitting recovery references, or recommending generic output controls that the adapter cannot safely complete.

Pi catches ordinary extension errors, but the adapter does not rely on host behavior: every registered handler catches locally and returns no replacement on error. It deliberately does not register a `tool_call` handler, where extension errors can have fail-safe blocking implications.

## Installation contract

The package declares:

- `pi.extensions = ["./dist/index.js"]`;
- the `pi-package` discovery keyword;
- a `*` peer range for Pi's host-provided coding-agent package, as Pi's package guide requires;
- exact `0.84.1` only as a development/type-check dependency;
- Node `>=22.19`.

The normal user path is `kendr-opt run pi`, which writes the compiled extension
to Pi's global extension directory. The GitHub Release also includes
`kendr-optimizer-pi-0.1.4.tgz` for managed package installation; no npm
registry publication is required.

## Verification and upgrade gate

Package tests compile against the exact audited host types and cover system, context, tool result, skipped receipts, shadow mode, output observation, fail-open behavior, and endpoint security.

Before widening the peer-support claim, run the tests against the candidate Pi version and re-check event return types, `AgentMessage` unions, package-manifest rules, host error semantics, and the Node requirement.

Primary sources: [extension types](https://github.com/earendil-works/pi/blob/53fa77ccd8a279eb87e92294ef3687b03ff80112/packages/coding-agent/src/core/extensions/types.ts), [extensions guide](https://github.com/earendil-works/pi/blob/53fa77ccd8a279eb87e92294ef3687b03ff80112/packages/coding-agent/docs/extensions.md), [packages guide](https://github.com/earendil-works/pi/blob/53fa77ccd8a279eb87e92294ef3687b03ff80112/packages/coding-agent/docs/packages.md), and [v0.84.1 release](https://github.com/earendil-works/pi/releases/tag/v0.84.1).
