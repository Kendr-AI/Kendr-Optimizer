# Harness compatibility

Last verified: 2026-08-07

KendrOptimizer is a transform service, not a gateway. Each adapter maps only the
surfaces a harness actually exposes into the neutral Kendr envelope, validates
the returned shape, and fails open to the original value. Compatibility means
an audited integration exists for the listed surface; it does not mean every
field of every future harness version can be intercepted.

| Harness | Integration | Applied surfaces | Deliberate boundary | Audited pin |
| --- | --- | --- | --- | --- |
| Claude Code | Hook package | Successful `PostToolUse.updatedToolOutput` | Submitted prompt and assistant output are shadow-only; history, failed tool output, tool catalog, and generation settings cannot be replaced through stable hooks | 2.1.224 / `66edf535` |
| Claude Code Channels | Source-side channel helper | Authorized notification content before `notifications/claude/channel` | Cannot interpose on another channel server; sender authentication remains the channel author's responsibility | research preview in 2.1.224 / `66edf535` |
| Pi coding agent | Extension | System prompt, context messages, and text-bearing tool results | Tool narrowing, generic provider payload rewriting, and generation controls remain disabled | 0.84.1 / `53fa77cc` |
| OpenCode | V1 plugin | Current user message and tool output; opt-in experimental history/system hooks | Tool schema and generation rewrites are disabled; experimental hooks require explicit opt-in | 1.18.15 / `d7b115f6` |
| Hermes Agent | Python plugin | Main-agent `llm_request`, system/instructions text, recognized tools, and string tool results | Auxiliary plugin-owned model calls, opaque multimodal parts, and completed output are not rewritten | 0.20.0, tag v2026.8.3 / `3c27eb62` |
| OpenClaw | Context-engine plugin | Assembled history and supported message/tool-result text | Context-engine ownership is exclusive; initial current prompt and arbitrary first-call schemas are outside the ordinary plugin seam | 2026.7.2 / `60fc2fe6` |
| NanoClaw | Guarded `nc:` source-customization skill | Initial and normal follow-up formatted prompt strings | No stable middleware exists; opaque resumed history, tools, tool results, and provider internals need provider-specific work | unreleased main / `743e32df` |

All adapters default to representation-safe transforms, accept only a
credential-free numeric loopback optimizer endpoint, do not receive provider
credentials, and preserve the original host value on timeout, invalid output,
or sidecar failure.

## Verification

The TypeScript integration packages build before running their tests:

```text
cd integrations/<adapter>
npm test
```

Hermes uses its official middleware dispatcher contract test:

```text
HERMES_AGENT_SOURCE=/path/to/pinned/hermes-agent \
  python -m unittest discover -s integrations/hermes-agent/tests -v
```

NanoClaw's skill includes idempotent install, drift, removal, prompt mapping,
timeout, circuit-breaker, loopback, and structural-seam tests. See each
integration README for installation and runtime smoke instructions.

Continuous integration checks out the exact Hermes Agent and NanoClaw commits
from the pin ledger. The Hermes suite therefore cannot silently skip its
upstream dispatcher contract, and the NanoClaw job applies the guarded patch
twice before running its eight adapter and structural-seam tests.

The immutable compatibility ledger is
[`integrations/harnesses.lock.json`](../integrations/harnesses.lock.json).
The latest contract-test counts are recorded in
[`integrations/verification.json`](../integrations/verification.json).
