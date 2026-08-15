# KendrOptimizer Architecture

## Status and intent

KendrOptimizer is a pre-alpha, open-source, provider-neutral token optimization engine. It transforms normalized model context and reports what it changed. It does not call, route to, or proxy an LLM.

This document separates the architecture we are committing to from the subset currently implemented. Unless a section is explicitly labeled **Implemented**, it describes a target or design constraint rather than shipped behavior.

The project objective is not maximum compression. It is the lowest defensible token and cost footprint subject to an explicit quality-risk budget, host capabilities, cache behavior, and latency limits. A correct no-op is a successful decision.

## Product boundary

The host owns inference. KendrOptimizer owns only analysis and transformation:

```text
Host or gateway
    |
    | normalized content + capabilities + policy
    v
KendrOptimizer
    | optimized content + recovery capsule + preflight receipt
    v
Host or gateway
    |
    | provider request, credentials, routing, streaming, retries
    v
Model provider
    |
    | provider usage and task outcome, optionally reported later
    v
KendrOptimizer observe
```

There is deliberately no KendrOptimizer-to-provider arrow. The core does not know provider credentials or upstream URLs and does not initiate network requests. See [ADR 0001](decisions/0001-product-boundary.md).

## Design principles

1. **Transform, do not transport.** Optimization is callable as a library, CLI, or local transform service. None of those surfaces forwards inference.
2. **Preserve typed structure.** Roles, tool calls, tool arguments, code, JSON, images, and output contracts are first-class protocol objects rather than undifferentiated prompt text.
3. **Choose a portfolio, not a universal compressor.** Logs, tool schemas, conversation history, documents, and output policy require different algorithms.
4. **No-op by default under uncertainty.** A transformation must clear risk, integrity, cache, and signed net-gain gates; a best-effort shared latency guard limits later dispatch.
5. **Measure honestly.** Local token reduction is not automatically provider bill savings. Verified savings require a positive paired provider delta and a passing quality signal.
6. **Make loss explicit.** Every engine declares a risk level and reversibility. Extractive and learned behavior is opt-in.
7. **Keep originals private.** Recovery data can be more sensitive than the optimized request and must be handled as secret content.
8. **Fail open for optimization.** A valid host request should still reach the host's normal model path if optimization fails.
9. **Stay model-neutral but target-aware.** Algorithms are not tied to a provider; token accounting can still use target tokenizer, context, price, and cache information supplied by the host.
10. **Keep extensions constrained.** Future third-party engines receive bounded data and return declarative candidates with proof obligations; they do not mutate arbitrary host state.

## Repository layers

### Implemented

The current Rust workspace contains three crates:

- `kendr-optimizer-contracts`: serializable `kendr.optimize/v1` request and `kendr.receipt/v1` receipt types.
- `kendr-optimizer-core`: normalization-aware token measurement, nine native engines, sequential policy evaluation, verification, recovery, and usage comparison.
- `kendr-optimizer-cli`: file/stdin commands plus an optional transform-only HTTP service.

The OpenClaw integration is a TypeScript context-engine adapter. It invokes the local service over a strictly validated loopback origin, validates the returned shape, and falls back to original messages on error.

### Planned layering

As the implementation grows, responsibilities should be split without changing the wire contract casually:

```text
contracts     normalized IR, policies, outcomes, receipt schemas
tokenizers    exact profiles, conservative ensembles, host callbacks
engines       native candidate generators grouped by content domain
planner       conflict-aware portfolio selection and net-value policy
verifier      structural, reconstruction, protected-content, cache gates
receipts      preflight/final observations and versioned evidence
state         host-supplied recovery and session-state traits
sdk           Node, Python, C/WASM bindings
adapters      OpenClaw and other host-specific mappings
benchmarks    pinned competitor runners and paired quality/cost evaluation
```

The core networking boundary must remain even if crates are reorganized.

## Normalized content contract

`OptimizeRequest` contains:

- `schema_version` and `phase`;
- request and optional session identity;
- a `ContentEnvelope`;
- target metadata;
- host capabilities; and
- optimization policy.

The envelope currently represents:

- ordered messages with stable IDs, roles, optional parent and turn IDs;
- text, code, JSON, documents, image references, tool calls, and tool results;
- tool definitions including schemas, required flags, tags, and metadata;
- an opaque output contract; and
- host metadata.

This is intentionally not an OpenAI or Anthropic request object. Host adapters must map provider- or runtime-specific structures to it and back without silently discarding unsupported fields.

### Current contract limitations

- Schema compatibility is checked by exact version string; capability negotiation is not implemented.
- Envelope validation currently checks only non-empty, unique message IDs and tool names. It does not yet fully validate parent links, turn topology, role ordering, duplicate tool-call IDs, call/result pairing, schema dialect, metadata limits, or content-size/depth limits.
- Only `approximate`, `cl100k_base`, and `o200k_base` token profiles exist.
- The contract has cache segments but not full provider serialization or cache-write/read pricing semantics.
- The public result returns a full envelope; an inspection patch and declarative transform plan are not yet implemented.
- A stateless pre-call generation recommendation is implemented. Stateful output-length learning, adapter application feedback, and dedicated post-turn history ingestion remain planned.

Adapters must treat unknown or unsupported content conservatively and return the original request.

## Current optimization flow

The pre-alpha `Optimizer` performs the following steps:

1. Require `kendr.optimize/v1`.
2. Validate basic envelope identifiers.
3. Serialize the complete normalized `ContentEnvelope` with `serde_json` and measure it.
4. Convert an empty request ID into a digest-derived anonymous ID.
5. Reject post-generation rewriting for `output_observation` with an explicit no-op receipt.
6. Evaluate the opt-in generation policy against user intent, host capabilities, and expected break-even margin.
7. Iterate over a fixed ordered list of native engines.
8. Skip disabled engines, reject engines above the risk ceiling, and record `timed_out` for engines reached after the global latency budget.
9. Ask each engine for one candidate against the current working envelope.
10. Measure the candidate, run verification, enforce cache policy, and require signed minimum token and percentage gain.
11. Accept valid candidates into the working envelope; reject or revert the rest while recording every attempt.
12. Re-measure the whole candidate sequence and revert the complete portfolio if it no longer meets the global gain threshold.
13. In shadow mode, return original content while reporting the hypothetical optimized measurement.
14. If an applied portfolio needs recovery, return a recovery capsule containing the complete original envelope.

This is a sequential greedy pipeline. It is not yet the planned conflict-aware constrained planner. The current order is:

```text
json-minify
terminal-clean
text-normalize
repeat-lines
pytest-result-fold
context-repetition
history-dedup
tool-output-prune
tool-selector
```

Sequential ordering can miss a better portfolio, allow one engine to change a later engine's opportunity, and makes latency accounting coarse. The whole-envelope rollback prevents final inflation, but it does not solve portfolio optimality.

## Target candidate and planner architecture

Each native or external engine should eventually produce declarative candidates rather than one directly selected replacement. A candidate should include:

- stable candidate and engine identity;
- exact scope IDs;
- preconditions;
- proposed transformed fragments or patch;
- dependencies and conflicts;
- risk and reversibility level;
- recovery records, if any;
- estimated token delta under each available tokenizer;
- cache segments touched;
- expected latency and memory cost;
- required host capabilities; and
- proof obligations for the verifier.

The planner should solve a bounded constrained-selection problem. Its utility is conceptually:

```text
expected billed-token benefit
  - optimizer compute and latency cost
  - cache invalidation/write penalty
  - quality-risk penalty
```

When price or cache information is absent, the planner must use a documented token objective and lower confidence. It must not invent monetary precision.

The first planner should be deterministic and explainable. A learned planner may later rank candidates, but candidate generation, immutable protocol fields, and final acceptance remain constrained by deterministic gates.

## Risk model

Risk is ordered in the contract:

| Level | Meaning | Default expectation |
| --- | --- | --- |
| `pass_through` | Analysis only; content unchanged | Always allowed |
| `representation_safe` | Changes representation while preserving the engine's declared typed meaning | Default upper bound for early integrations |
| `recoverable` | Omits or references content but supplies an exact original recovery path | Requires host recovery capability when model access to the original matters |
| `extractive` | Removes content judged irrelevant | Explicit opt-in and task-aware evaluation |
| `learned` | A local learned model influences retention or rewriting | Experimental, opt-in, locally executed |

These labels are proof obligations, not guarantees of identical model output. Even whitespace or ordering can affect a model. “Representation safe” means the transformation preserves a specified structural interpretation and passed the available gates; it does not mean an LLM is mathematically invariant to the change.

## Verification boundary

### Implemented gates

Current candidates are checked for:

- basic envelope validity;
- unchanged output contract;
- unchanged tool-call IDs, names, and arguments;
- unchanged typed code, JSON, and image-reference parts;
- unchanged tool definitions, except that `tool-selector` may remove optional definitions while retaining required ones;
- exact in-memory reconstruction when the candidate supplies one;
- multiplicity-aware retention of detected URLs, paths, numbers, identifiers, negations, preserve blocks, and diagnostic lines;
- byte-exact typed-marker expansion for changed parts produced by `repeat-lines`, `pytest-result-fold`, and `context-repetition`, followed by comparison with the complete pre-transform envelope;
- no changes to declared cache segments when prefix preservation is enabled;
- per-candidate minimum signed token and percentage gain; and
- whole-envelope minimum signed gain.

### Important gaps

- Tool call/result causality and message topology are not fully validated.
- Protected-artifact detection remains regex-based and incomplete; literal multiplicity is enforced, but this does not establish semantic equivalence.
- Typed expansion validates the three exact marker formats before candidate acceptance, while the separate recovery capsule still stores a full-envelope copy rather than granular authenticated records. Its stored digest is checked by `restore`.
- `EngineDescriptor.cache_safe` is descriptive and is not independently enforced by a generic policy.
- The latency budget is checked between engines. A single engine has no cancellation deadline or memory allocation budget.
- There is no runtime semantic model, task-success gate, adversarial quality judge, or statistical non-inferiority proof.

Any documentation or UI must describe these as pre-alpha safeguards, not complete quality proof.

## Recovery architecture

### Implemented

If an applied candidate uses reconstruction or has recoverable-or-higher risk, the core returns one `RecoveryCapsule` containing:

- request ID;
- SHA-256 digest of the original normalized envelope; and
- a recovery record whose `original` field is the entire original `ContentEnvelope` as JSON.

`restore` deserializes that envelope, validates it, remeasures it with the approximate profile, and verifies the original SHA-256 digest.

### Privacy consequence

The capsule is a plaintext duplicate of everything optimization may have removed, including private prompts, source code, secrets, tool output, and identifiers. It is not encrypted, authenticated for storage, redacted, TTL-bound, or persisted by the core. It must be handled as secret request content and must not be put into ordinary receipts, logs, analytics, crash reports, URLs, or unencrypted databases.

The host owns capsule lifecycle. Until a secure state interface exists, integrations that cannot protect and restore capsules should cap risk below `recoverable`, as the current OpenClaw adapter does.

### Planned

Recovery should evolve toward scoped records with:

- tenant, session, request, and content-scope binding;
- random collision-resistant markers;
- authenticated digests or encryption at rest supplied by the host;
- explicit TTL and deletion semantics;
- maximum storage budgets;
- one-time or bounded lookup behavior;
- no raw recovery content in the optimization receipt; and
- exact reconstruction tests for every engine that claims recoverability.

The core should expose a `RecoveryStore` trait but should not silently create a global database.

## Measurement and receipts

Preflight measurement is part of the decision path, but it is not provider billing evidence. Current token counts cover the normalized envelope serialized by `serde_json`. Exact BPE tokenization of that serialization still does not include the provider's actual message framing, hidden system content, image accounting, cache behavior, or tokenizer drift.

Every result includes signed token and byte deltas, per-engine attempts, verification checks, cache impact, warnings, and no-op reasons. `verified_savings` remains false during optimization. `observe` can compare host-supplied optimized usage with a paired baseline. See [measurement.md](measurement.md) for the evidence model and current caveats.

## Output lifecycle

There are three separate output concerns:

1. **Generation policy before inference.** The optimizer can return an opt-in recommendation to use a host-native output limit or verbosity control, or a concise instruction when its heuristic benefit exceeds estimated instruction overhead. It declines detailed, exact, and structured-output requests. The host decides whether and how to apply the recommendation.
2. **Output observation after inference.** Provider usage and task outcome can be attached to a receipt. This measures but does not rewrite.
3. **History ingest.** An assistant answer may be optimized before it becomes input to a future turn, under the same risk and recovery rules as other context.

The current recommendation is stateless and heuristic: it needs a caller-supplied expected output length or target limit, is not calibrated to a model, and does not itself modify the provider request. It always labels expected output reduction unverified. Rewriting a completed answer cannot lower the output tokens already billed for that call. The current core makes `output_observation` an explicit no-op and instructs callers to use `observe`. Streamed final-answer rewriting is not implemented and is not planned as a default.

## Local service

`kendr-opt serve` is an optional RPC convenience layer around the same core. It currently exposes:

```text
GET  /healthz
GET  /v1/capabilities
GET  /v1/engines
POST /v1/analyze
POST /v1/optimize
POST /v1/restore
POST /v1/observe
```

It has no provider routes or egress implementation. It defaults to loopback, but the CLI currently permits other bind addresses. There is no authentication, explicit body limit, concurrency limit, or tenant isolation. Production use should remain on loopback behind operating-system protections until those gaps are closed. An in-process SDK or local IPC transport is preferred for high-trust integrations.

## OpenClaw adapter

### Implemented

The current TypeScript adapter:

- registers `kendr-optimizer` in OpenClaw's exclusive context-engine slot;
- encodes supported assembled messages into the normalized envelope;
- calls only `<loopback-origin>/v1/optimize`;
- omits credentials, blocks redirects, and rejects user information, paths, queries, fragments, and non-loopback hosts;
- preserves original OpenClaw message objects and applies only validated text changes;
- rejects mutations to tool calls and opaque content;
- returns original messages on encoding, network, timeout, decoding, or verification failure;
- uses only `pass_through` or `representation_safe` risk;
- does not install recovery, narrow tools, ingest state, or own compaction; and
- persists only SHA-256 advancement-key hashes for OpenClaw's idempotent commit contract.

### Limitations

- Available tool names are not enough to run schema-aware tool selection, so `tools` is empty in the normalized request.
- The adapter does not report provider usage to `observe`.
- It does not optimize persisted tool results through a dedicated hook.
- It cannot compose with a second selected OpenClaw context engine.
- Generic OpenClaw backends that do not invoke `assemble()` receive no benefit.
- It relies on a loopback HTTP process rather than an embedded Node/WASM/native package.

Future integration work must not weaken structural validation or the no-provider-relay boundary merely to expose more algorithms.

## Open extension direction

The current `Engine` trait is crate-private and the native engine list is fixed. There is no stable third-party engine ABI today.

The intended extension model has two layers:

- A stable language-neutral candidate protocol, versioned separately from internal Rust traits.
- A sandboxed WASI component interface for untrusted or third-party engines, with declared content access, time, memory, risk, and recovery capabilities.

External engines should receive the minimum relevant scopes, have no network access by default, and return candidates rather than final authority. The core planner and verifier remain in control. Native built-ins may use an internal Rust trait, but a Rust dynamic-library ABI should not be treated as stable.

Node, Python, C, and WASM SDKs should all call the same core behavior. They must not become separate optimizer implementations that drift from Rust.

## Failure semantics

- Invalid normalized input returns an error to the adapter; the adapter decides whether to reject the host request or use its untouched original.
- Engine proposal failure records a reverted attempt and continues.
- Candidate verification failure rejects or reverts that candidate.
- Whole-portfolio inflation or insufficient gain restores the original envelope.
- Shadow mode returns original content and a hypothetical measurement.
- Sidecar timeout or transport failure causes supported adapters to use original context.
- Recovery-store failure must not be reported as successful recoverability.

Security policy enforcement is not the optimizer's job. The optimizer may remove optional tools only when authorized by host capabilities; it must never add tools, grant permissions, or override the host's deny policy.

## Maturity warning

Version `0.1.2` is a repository package version, not a production-readiness declaration. The project currently demonstrates the contracts, conservative native transformations, gates, receipts, local service, a detailed OpenClaw sidecar adapter, and additional audited harness mappings. It has not yet established broad model non-inferiority, production resource isolation, secure recovery storage, stable external APIs, or comprehensive provider accounting.

Use shadow mode first. Treat applied pre-alpha transformations as experiments until workload-specific evaluation supports them.
