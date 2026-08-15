# KendrOptimizer for Pi

This is an installable Pi package. It performs provider-neutral, transform-only optimization through Pi's documented extension events and never registers a model provider, rewrites provider URLs, or handles API keys.

The package is type-checked against `@earendil-works/pi-coding-agent@0.84.1` and audited at commit `53fa77ccd8a279eb87e92294ef3687b03ff80112`. Pi `0.84.1` requires Node.js `22.19` or newer.

## Supported seams

| Pi event | Behavior |
| --- | --- |
| `before_agent_start` | Representation-safe system-prompt transform |
| `context` | Representation-safe text transforms immediately before each LLM call |
| `tool_result` | Text-only tool-result transform; images and Pi-specific result fields are preserved |
| `message_end` | Optional assistant-output shadow analysis; no rewrite |
| `before_provider_request` | Intentionally unused because its payload is provider-specific |
| Tool activation | Intentionally unused because this seam does not guarantee automatic full-tool retry |

The adapter reports `can_narrow_tools=false`, `can_restore_references=false`, and `can_retry_with_full_tools=false`. That keeps Kendr's tool selector and recoverable history deduplication disabled even though Pi exposes adjacent APIs.

## Install

The normal user path is:

```powershell
kendr-opt run pi
```

`kendr-opt` writes the compiled extension to Pi's documented global extension
directory and manages the optimizer process for the Pi session. The GitHub
Release also carries `kendr-optimizer-pi-0.1.2.tgz` for managed package
deployment. No npm registry publication is required.

## Configuration

| Environment variable | Default | Meaning |
| --- | --- | --- |
| `KENDR_OPTIMIZER_ENDPOINT` | `http://127.0.0.1:7331` | Core endpoint; numeric loopback only |
| `KENDR_OPTIMIZER_TIMEOUT_MS` | `100` | Per-request timeout, capped internally |
| `KENDR_OPTIMIZER_SHADOW` | false | Analyze all supported seams without returning replacements |
| `KENDR_OPTIMIZER_OBSERVE_OUTPUT` | false | Analyze finalized assistant messages for local diagnostics only |

Every handler catches its own errors and returns no replacement if KendrOptimizer is unavailable or its response fails structural checks. Host arrays are cloned and committed only after the whole response validates, so a partial transform cannot leak into Pi.

For tool results, only `content` text blocks are returned. Pi therefore retains `details`, `isError`, `usage`, image blocks, tool-call identity, and all other host-owned fields.

## Measurement semantics

An applied receipt must have:

- `schema_version = kendr.receipt/v1`;
- the exact request ID;
- `status = applied`;
- a positive `token_delta`;
- fewer locally measured optimized tokens than original tokens;
- unchanged message/part identity and shape at the adapter boundary.

`verified_savings=false` does not mean the local transform was skipped. It means no paired provider-usage baseline has yet proved a billing reduction. The adapter keeps those concepts separate.

## Development

```powershell
npm ci
npm run typecheck
npm test
npm pack --dry-run
```

References: [Pi extensions](https://github.com/earendil-works/pi/blob/53fa77ccd8a279eb87e92294ef3687b03ff80112/packages/coding-agent/docs/extensions.md), [Pi packages](https://github.com/earendil-works/pi/blob/53fa77ccd8a279eb87e92294ef3687b03ff80112/packages/coding-agent/docs/packages.md), and [v0.84.1 release](https://github.com/earendil-works/pi/releases/tag/v0.84.1).
