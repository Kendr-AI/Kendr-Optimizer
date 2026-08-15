# Hermes Agent adapter

This package registers KendrOptimizer at Hermes Agent's behavior-changing
`llm_request` and `tool_execution` middleware seams. It is a transform-only
adapter: it never calls an LLM and never receives provider credentials.

## Compatibility pin

The implementation was audited against the official
[`NousResearch/hermes-agent`](https://github.com/NousResearch/hermes-agent)
repository at commit
[`3c27eb6234bf91b8ceee9e9071591b31e9b148cb`](https://github.com/NousResearch/hermes-agent/commit/3c27eb6234bf91b8ceee9e9071591b31e9b148cb),
released as `v2026.8.3` (Hermes Agent package `0.20.0`). The relevant upstream
contracts are the
[plugin guide](https://github.com/NousResearch/hermes-agent/blob/3c27eb6234bf91b8ceee9e9071591b31e9b148cb/website/docs/developer-guide/plugins/index.md),
[`hermes_cli/middleware.py`](https://github.com/NousResearch/hermes-agent/blob/3c27eb6234bf91b8ceee9e9071591b31e9b148cb/hermes_cli/middleware.py),
and the
[conversation-loop wiring](https://github.com/NousResearch/hermes-agent/blob/3c27eb6234bf91b8ceee9e9071591b31e9b148cb/agent/conversation_loop.py).

Compatibility means these audited APIs are implemented; it is not a promise
that a future Hermes commit cannot change them. CI should re-run this package's
tests when advancing the pin.

## Coverage

| Surface | Coverage | Boundary |
|---|---|---|
| Main-agent provider request | Full for supported text/tool-call/tool-result boundaries in `messages` or Responses `input` | Provider-only kwargs and unsupported multimodal blocks remain byte-for-byte Python objects outside the normalized envelope |
| Top-level Anthropic `system` / Responses `instructions` | Supported for string and text-block forms | Non-text blocks are retained but not optimized |
| Recognized function-tool definitions and output contract | Included in verification | If any tool has an opaque provider-specific shape, normalized tool mapping is disabled for that call; provider tools still remain untouched. Tool narrowing is always disabled |
| Tool execution result | Supported for a returned string or a dictionary whose `content` is a string | Structured return values pass through unchanged |
| Auxiliary/plugin-owned LLM calls | Not claimed | Only calls routed through Hermes's main `llm_request` middleware are visible |
| Completed model output | Not rewritten | Rewriting already-generated output cannot reduce provider-billed output tokens |

The request mapper retains the complete provider kwargs object with
copy-on-write updates, changes only validated text bindings, and fails open on
timeouts, schema drift, opaque part mutation, malformed responses, or sidecar failure. Explicit
`cache_control` message regions are declared as protected cache segments.

Receipt token counts cover the normalized, supported surface—not opaque
provider fields or unsupported multimodal blocks. The signed token delta is the
mapped text change; its percentage must not be presented as whole provider-bill
savings. Use paired provider usage observations for that claim.

## Install

The normal user path is:

```bash
kendr-opt run hermes
```

`kendr-opt` writes the dependency-free source package to Hermes's user plugin
directory, enables it with Hermes's own CLI, and manages the optimizer process.
The GitHub Release also carries
`kendr_optimizer_hermes-0.1.2-py3-none-any.whl` for managed Python environment
deployment. No PyPI publication is required.

## Configuration

All settings are optional:

| Variable | Default | Constraint |
|---|---:|---|
| `KENDR_OPTIMIZER_ENDPOINT` | `http://127.0.0.1:7331` | HTTP origin on literal `127.0.0.1` or `::1`, with an explicit port |
| `KENDR_OPTIMIZER_TIMEOUT_MS` | `40` | 5–250 ms; Hermes middleware is synchronous |
| `KENDR_OPTIMIZER_BACKOFF_MS` | `30000` | Circuit-open interval after a failure |
| `KENDR_OPTIMIZER_RISK_CEILING` | `representation_safe` | Kendr Q0–Q4 wire value |
| `KENDR_OPTIMIZER_TOKENIZER` | `o200k_base` | `approximate`, `cl100k_base`, or `o200k_base` |
| `KENDR_OPTIMIZER_MIN_GAIN_TOKENS` | `8` | Non-negative integer |
| `KENDR_OPTIMIZER_MIN_GAIN_PERCENT` | `1` | 0–100 |
| `KENDR_OPTIMIZER_PRESERVE_RECENT` | `6` | Recent message count |
| `KENDR_OPTIMIZER_MAX_TOOL_RESULT_CHARS` | `24000` | Tool-result policy bound |
| `KENDR_OPTIMIZER_SHADOW` | `0` | Truthy enables receipt-only shadow mode |

The HTTP client disables environment proxies and enforces a literal loopback
origin so prompt data cannot be redirected to a configured proxy. If the
service needs to run on another machine, do not weaken this adapter: use an
authenticated transport implementation with explicit tenant isolation.

## Verify

```bash
python -m unittest discover -s integrations/hermes-agent/tests -v
python -m pip wheel --no-deps --wheel-dir /tmp/kendr-hermes-wheel integrations/hermes-agent
```

To exercise Hermes's real dispatcher rather than the isolated fake context,
point the optional contract smoke at the pinned upstream checkout:

```bash
HERMES_AGENT_SOURCE=/path/to/hermes-agent \
  python -m unittest discover -s integrations/hermes-agent/tests -v
```
