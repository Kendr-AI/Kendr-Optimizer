# KendrOptimizer for OpenClaw

This package adapts OpenClaw's assembled context to KendrOptimizer's local,
transform-only API. It is not a model provider, gateway, router, or request
relay.

The adapter:

- registers the `kendr-optimizer` context engine;
- converts supported OpenClaw messages to `kendr.optimize/v1`;
- posts that envelope only to `/v1/optimize` on a loopback origin;
- validates the returned message structure before applying any text change;
- returns the original messages on timeout, connection failure, malformed
  output, or an unsupported message shape;
- never reads, stores, forwards, or adds provider credentials; and
- never sends a model request.

## Compatibility status

This scaffold has been audited against the OpenClaw plugin/context-engine API
shipped in `2026.7.2` at commit
`60fc2fe64d8ec2988a555a638dc1074d31e5760b` and declares
`>=2026.7.1-2 <2027` compatibility.
OpenClaw's plugin API is moving quickly, so test each OpenClaw upgrade before
using it in production. The adapter deliberately uses the documented structural
plugin entry contract instead of importing internal OpenClaw modules.

| OpenClaw version | Status |
| --- | --- |
| `2026.7.2` | Audited API surface |
| `2026.7.1-2` | Minimum declared compatibility |
| Older than `2026.7.1-2` | Unsupported |
| `2027.x` or later | Unverified; blocked by the declared peer range |

Node must also satisfy OpenClaw's supported runtime range: Node 22.22.3+,
24.15+, or 25.9+ in the corresponding major-version bands.

## Start the optimizer

From the KendrOptimizer repository root:

```powershell
cargo run -p kendr-optimizer-cli -- serve --bind 127.0.0.1:7331
```

Keep the service bound to loopback. The transform endpoint receives model
context, which may contain private prompts, tool output, source code, or other
sensitive data.

## Build and install the plugin

```powershell
cd integrations/openclaw
npm install
npm test
openclaw plugins install --link .
openclaw plugins inspect kendr-optimizer --runtime --json
```

Published installations must include compiled `dist/` output. The
package metadata already points OpenClaw at `dist/index.js`; it does
not rely on TypeScript execution at runtime.

## Configure OpenClaw

Select the adapter in OpenClaw's exclusive context-engine slot:

```json5
{
  plugins: {
    slots: {
      contextEngine: "kendr-optimizer",
    },
    entries: {
      "kendr-optimizer": {
        enabled: true,
        config: {
          endpoint: "http://127.0.0.1:7331",
          timeoutMs: 100,
          failureBackoffMs: 5000,
          tokenizerProfile: "approximate",
          riskCeiling: "representation_safe",
          minGainTokens: 8,
          minGainPercent: 1,
          preserveRecentMessages: 6,
          maxToolResultChars: 24000,
          shadow: false,
        },
      },
    },
  },
}
```

Restart the active OpenClaw Gateway after changing plugin code or configuration:

```powershell
openclaw gateway restart
openclaw gateway status --deep --require-rpc
openclaw plugins inspect kendr-optimizer --runtime --json
```

Start with `shadow: true` to collect optimizer receipts without
changing the context returned to OpenClaw. This adapter logs status and the
signed estimated input-token delta at debug level; it does not claim observed
provider savings.

## Safety boundary

The adapter accepts only `http` or `https` origins whose
hostname is the numeric loopback address `127.0.0.1` or
`[::1]`.
User information, paths, query strings, fragments, and redirects are rejected.
The only outbound request constructed by the package is:

```text
POST <configured-loopback-origin>/v1/optimize
Content-Type: application/json
Accept: application/json
```

There is no `Authorization` header, provider base URL, upstream URL,
model routing setting, or chat-completions endpoint in this package.

Optimization is accepted only when the response preserves:

- message count, order, identifiers, and roles;
- content-part count and type;
- tool-call IDs, names, and arguments;
- opaque image, thinking, and unknown content blocks; and
- OpenClaw-owned top-level fields such as timestamps and tool metadata.

If one of those checks fails, the adapter returns the original OpenClaw message
array. The default risk ceiling is `representation_safe`. Recoverable,
extractive, learned, and lossy transforms are intentionally unavailable because
this first adapter does not yet expose a recovery path to OpenClaw.

The context engine keeps only SHA-256 hashes of OpenClaw advancement keys in:

```text
<agent-or-workspace-dir>/.kendr-optimizer/openclaw-commits-v1.json
```

That small registry implements OpenClaw's atomic/idempotent
`commitTurn` contract. It contains no messages, tool results,
credentials, or provider responses. A registry I/O failure is allowed to reach
OpenClaw so OpenClaw can activate its documented legacy-engine fallback rather
than pretending the durable contract succeeded.

## Failure behavior

The reply path is fail-open for optimization:

```text
OpenClaw messages
  -> encode supported content
  -> loopback /v1/optimize
  -> verify every structural invariant
  -> optimized messages

Any encode, transport, timeout, decode, or verification failure
  -> original OpenClaw messages
```

After a service error, a short circuit breaker avoids paying the timeout on
every turn. The adapter retries after `failureBackoffMs` and announces
recovery through the plugin logger.

## Current OpenClaw limitations

The `contextEngine` slot is exclusive. Selecting
`kendr-optimizer` replaces `legacy` or any other custom
context engine for that slot; OpenClaw does not compose two context engines.

This first adapter also has deliberate limits:

- `ownsCompaction` is false. OpenClaw may retain its built-in
  in-attempt automatic compaction, but this plugin's direct
  `compact()` operation returns a documented no-op. Switch to
  `legacy` when a manual compaction workflow is required.
- OpenClaw's context-engine assembly surface supplies available tool names, not
  the complete schemas required by KendrOptimizer's schema-aware selector.
  Therefore `can_narrow_tools` and tool selection are disabled.
- Recovery capsules are not installed into OpenClaw context, so recoverable and
  lossy policies are disabled.
- The adapter optimizes context before inference. It does not rewrite streamed
  final answers and cannot retroactively save already billed output tokens.
- `estimatedTokens` comes from KendrOptimizer's canonical envelope
  measurement. It is a planning estimate, not provider-reported usage or proof
  of cost savings.
- Generic OpenClaw backends that do not invoke context-engine
  `assemble()` cannot benefit from this adapter.

These limits are intentional. Future work should use documented OpenClaw hooks
for schema-aware tool narrowing, tool-result persistence, and provider-usage
observation only after those contracts can be implemented without weakening
the fail-open and no-provider-relay boundary.

## Configuration reference

| Setting | Default | Meaning |
| --- | --- | --- |
| `endpoint` | `http://127.0.0.1:7331` | Credential-free loopback optimizer origin |
| `timeoutMs` | `100` | HTTP optimization deadline |
| `failureBackoffMs` | `5000` | Retry delay after a service error |
| `tokenizerProfile` | `approximate` | KendrOptimizer tokenizer profile |
| `riskCeiling` | `representation_safe` | `pass_through` or `representation_safe` only |
| `minGainTokens` | `8` | Minimum positive whole-envelope token delta |
| `minGainPercent` | `1` | Minimum positive percentage delta |
| `preserveRecentMessages` | `6` | Recent-message protection passed to the core |
| `maxToolResultChars` | `24000` | Tool-result policy bound; lossy truncation remains off |
| `shadow` | `false` | Analyze without changing returned context |

## Development checks

```powershell
npm run typecheck
npm test
npm pack --dry-run
```

The unit tests cover loopback enforcement, lossless message round-tripping,
text-only application, tool-call tamper rejection, and conservative defaults.
