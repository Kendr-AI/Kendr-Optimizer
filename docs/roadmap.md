# KendrOptimizer Roadmap

## Roadmap policy

This roadmap is capability- and evidence-gated, not date-driven. A milestone is complete only when its acceptance criteria are met. Token reduction alone is not sufficient: safety, task quality, measurement honesty, latency, privacy, and integration behavior are release gates.

KendrOptimizer is currently **pre-alpha**. Version `0.1.2` identifies the scaffold; it does not imply API stability or production readiness.

The initial project is intentionally independent of KendrWeb. We will evaluate the optimizer as an open-source engine and integration component before deciding how KendrWeb should consume it. No milestone below assumes that KendrWeb must be modified.

## Maturity labels

| Label | Meaning |
| --- | --- |
| Scaffold | Interfaces and representative behavior exist; substantial correctness and security gaps remain |
| Experimental | Usable in shadow or controlled tests; algorithms and contracts may change |
| Preview | Default-safe for named workloads and integrations; still not a broad stability promise |
| Stable | Versioned compatibility, production hardening, non-inferiority evidence, and security process are in place |

## Milestone 0: Native pre-alpha scaffold

**Status: implemented in the current repository, with known gaps.**

Current capabilities:

- Apache-2.0 Rust workspace with separate contracts, core, and CLI crates.
- Provider-neutral `kendr.optimize/v1` normalized envelope.
- Versioned preflight receipt types and signed token/byte deltas.
- Approximate, `cl100k_base`, and `o200k_base` envelope measurement.
- Nine native engines: JSON minification, terminal control cleanup, blank-line normalization, repeated-line encoding, exact pytest sequence folding, exact context repetition, exact-history references, oversized diagnostic extraction, and lexical tool selection.
- Ordered risk levels from pass-through through learned.
- Basic immutable-field, protected-artifact, recovery, cache, per-engine gain, and whole-envelope gain gates plus a between-engine latency guard.
- Explicit applied, skipped, shadow, and reverted outcomes.
- Whole-envelope recovery capsule and digest-checked restoration.
- Usage comparison with optional paired baseline.
- Opt-in pre-call generation recommendations with detailed/structured-intent bypass, host-capability checks, instruction-overhead accounting, and expected net-gain gates.
- CLI operations for analyze, optimize, restore, observe, engine listing, and a local transform-only service.
- OpenClaw context-engine adapter with strict loopback destination validation, representation-safe ceiling, structural round-trip checks, timeout, circuit breaker, and fail-open behavior.
- Unit/integration tests covering representative no-op, JSON, cache, recovery, tool selection, generation policy, usage comparison, OpenClaw adapter behavior, typed marker expansion, pytest folding, and exact prefix-block/paragraph/line/sentence repetition.

Important current limitations:

- Fixed crate-private engine list and sequential greedy acceptance.
- Incomplete envelope and causal validation.
- Token counts cover normalized `serde_json`, not exact provider serialization.
- Recovery duplicates the entire original envelope in plaintext.
- No uniform core-wide request size/depth limit, hard per-engine cancellation, or service authentication; pytest, context-repetition, and repeated-line marker expansion have local bounds only.
- Local service can be explicitly bound beyond loopback.
- One narrow exact pytest reducer plus generic diagnostic keyword pruning rather than a broad typed command-parser pack.
- Lexical tool selector is uncalibrated and not used by OpenClaw.
- Generation policy uses an uncalibrated expected-length heuristic and is not yet applied by the OpenClaw adapter; there is no learned engine, stable SDK, or external engine sandbox.
- No broad task-level non-inferiority evidence.

## Milestone 1: Contract and security foundation

**Status: planned; highest priority.**

Deliverables:

- Publish canonical JSON Schemas for requests, outcomes, receipts, observations, and recovery handles.
- Add capability negotiation and compatibility rules for minor contract evolution.
- Validate message parent/turn topology, role constraints, unique call IDs, tool-call/result pairing, schema limits, metadata limits, content count, byte size, and nesting depth.
- Make system/developer mutation disabled by default at the core level.
- Add first-class user/host preserve scopes rather than relying only on text markers.
- Enforce request, candidate, recovery, CPU, memory, wall-clock, and output-size budgets.
- Enforce loopback/local IPC for the service by default; require an explicit unsafe development flag for broader binds or remove them.
- Add an authenticated local IPC option and document multi-user host boundaries.
- Add dependency and route tests that enforce no provider SDK, no upstream configuration, and no inference-relay endpoints.
- Extend the implemented protected-artifact receipt redaction to every
  diagnostic/logging path and add secret-shaped fuzz coverage.
- Add a public security policy, private reporting channel, dependency audit, SBOM, provenance, and release-signing workflow.

Acceptance criteria:

- Invalid causal envelopes are rejected deterministically before engine execution.
- Fuzzing finds no crash, uncontrolled allocation, or cross-scope recovery behavior within published limits.
- Core crates have no network-capable dependency and CI enforces that policy.
- Default service deployment cannot be exposed off-host accidentally.
- Receipts and default logs contain no raw prompt/tool/recovery content in secret-shaped tests.

## Milestone 2: Honest measurement and experiment system

**Status: planned.**

Deliverables:

- Separate canonical-envelope measurement from adapter/provider-serialization measurement.
- Add region-level accounting for messages, tool definitions, tool results, output-policy overhead, and recovery legends.
- Add host-supplied serializer/token-counter callbacks or adapter-specific measurement reports.
- Represent uncached input, cache reads, cache writes, output, reasoning, retries, and pricing revisions separately.
- Replace the binary `verified` observation with explicit evidence states: estimated, observed-unpaired, paired-data, comparable-pair, and quality-adjusted verified.
- Bind observations to request/content/config/model/provider digests.
- Validate paired-run comparability and incorporate task success, retries, and corrections.
- Add randomized repeated-run statistics and confidence intervals to the benchmark harness.
- Publish machine-readable raw result records and negative/no-op cases.

Acceptance criteria:

- No UI or API labels a local estimate “verified savings.”
- Cache-aware examples correctly show cases where fewer raw tokens cost more.
- Paired results are rejected as incomparable when required experiment metadata differs.
- Output savings are never claimed for post-generation rewriting.

## Milestone 3: Typed deterministic tool-result engine pack

**Status: in progress. One narrow exact pytest sequence fold is implemented; the milestone acceptance criteria remain unmet.**

Deliverables:

- Bounded terminal parser for ANSI, carriage-return redraws, progress output, and control sequences.
- Native parsers/renderers for compiler diagnostics, broader test-runner forms, logs, stack traces, Git status/log/diff, build output, homogeneous JSON records, tables, and plan/change output. The existing pytest fold covers only bounded sequential numeric result lines with a matching summary.
- Format detection with confidence and malformed-input fallback.
- Explicit retained invariants for errors, warnings, exit status, source positions, numeric values, counts, and requested fields.
- Minimal granular recovery records rather than whole-envelope copies.
- Engine benchmark cards by format and downstream task.

Acceptance criteria:

- Every representation-safe renderer passes typed round-trip or equivalent structural proof.
- Every recoverable renderer reconstructs exact source content.
- Malformed and adversarial inputs return original content without panics or unbounded work.
- Workload-specific task success meets predeclared non-inferiority thresholds.
- Whole-request savings, not isolated tool-output percentages, are reported.

## Milestone 4: Recovery and state architecture

**Status: planned.**

Deliverables:

- Host-supplied `RecoveryStore` and session-state traits.
- Scoped tenant/session/request/content identifiers.
- Random authenticated markers and collision escaping.
- Minimal encrypted-or-host-protected recovery payloads.
- TTL, deletion, maximum storage, and retrieval-count policies.
- Distinct host capabilities for storage, retry, model-time lookup, and output restoration.
- Atomic failure behavior: no transform claims recoverability until its records are durably available.

Acceptance criteria:

- No applied recoverable transform requires the receipt to carry raw original content.
- Cross-tenant/session/request access tests fail closed.
- Expiry and deletion are observable and deterministic.
- Store outage reverts the affected transform without breaking the host's original model path.

## Milestone 5: Context and tool-surface intelligence

**Status: planned; shadow-first. Exact recoverable context repetition is implemented as a deterministic foundation, but dependency graphs, query relevance, and semantic aging are not.**

Deliverables:

- Turn/tool dependency graph preserving active constraints, unresolved work, recent context, exact quotes, citations, named artifacts, and call/result chains.
- Deterministic context aging and query-relevant document windows with source anchors.
- BM25/trigram/entity/action tool relevance.
- Required groups, dependency closure, capability-list intent, calibrated confidence, and false-negative telemetry.
- Retry-with-full-tools observation and correction accounting.
- Conservative unknown-model/tokenizer objective.

Acceptance criteria:

- Tool selection remains shadow-only until recall and end-task success meet workload thresholds.
- Hosts lacking dynamic retry or restoration receive a safe bypass under uncertainty.
- Context algorithms preserve declared invariants and exact source access where policy requires it.
- Selection reduces full serialized tool/context input after cache effects, not just description characters.

## Milestone 6: Generation policy and output lifecycle

**Status: basic stateless controller implemented; adapter feedback and evidence work planned.**

Deliverables:

- Stabilize the existing provider-neutral generation contract for answer budget, verbosity, required elements, and confidence.
- Host adapters that map policy only to controls they support.
- Extend current instruction-token accounting with exact provider serialization and cache overhead.
- Output observation linked to preflight decisions.
- History-ingest optimization for completed assistant answers under normal risk/recovery gates.
- Streaming behavior that observes but does not rewrite final output by default.

Acceptance criteria:

- No generated answer is post-processed and reported as current-call output savings.
- Generation policies are applied only when expected net benefit clears a configured margin.
- Completeness, truncation, follow-up, and correction metrics pass non-inferiority thresholds.
- Paired provider usage supports every public output-savings claim.

## Milestone 7: Planner and native SDKs

**Status: planned.**

Deliverables:

- Declarative candidate format with scopes, conflicts, dependencies, proof obligations, cost, risk, cache effects, and host capability requirements.
- Conflict-aware bounded portfolio planner replacing the fixed greedy sequence.
- Deterministic explainable policy and stable seeding.
- Node native package with WASM fallback, Python wheel, C ABI, and CLI/stdio surface backed by the same Rust implementation.
- Pure provider-shape converters for common host formats, without inference forwarding.
- In-process OpenClaw integration where its plugin runtime permits it, retaining loopback as an optional fallback.

Acceptance criteria:

- SDK conformance fixtures produce the same normalized outcomes and receipts across languages.
- Candidate selection never bypasses core verification.
- Planner results are deterministic for fixed inputs, versions, and policy.
- No SDK grows a provider credential, upstream URL, routing, or chat-relay surface.

## Milestone 8: Open extension SDK

**Status: planned after core contracts stabilize.**

Deliverables:

- Versioned engine descriptor and candidate schemas.
- WIT/WASI component contract.
- Capability-scoped content access.
- No-network default sandbox with CPU, memory, wall-time, and output limits.
- Engine signing, allowlists, provenance, SBOM, and compatibility metadata.
- Public conformance suite and engine benchmark-card template.

Acceptance criteria:

- A malicious test engine cannot access provider credentials, arbitrary filesystem content, other request scopes, network, or unbounded host resources.
- Core independently measures and verifies every external candidate.
- Disabling or timing out an extension returns original content or continues with safe built-ins.
- Extensions cannot grant themselves a lower risk level than policy allows.

## Milestone 9: Optional local learned engines

**Status: research only; not required for a useful stable deterministic product.**

Candidate uses:

- sentence/window salience;
- tool relevance;
- expected output length;
- transformation-risk prediction; and
- candidate-success ranking.

Constraints:

- local execution only;
- pinned artifact digests and model cards;
- documented dataset and training provenance;
- bounded inference and deterministic fallback;
- offline promotion with adversarial and cross-model evaluation;
- no unrestricted rewriting authority; and
- no uncontrolled online learning from production traffic.

Acceptance criteria:

- Learned ranking beats the deterministic policy on quality-adjusted cost with statistical confidence.
- Failures cannot mutate protected protocol fields or bypass deterministic gates.
- Model absence, timeout, or unsupported platform produces a safe deterministic fallback.
- The open-source deterministic engine remains fully useful without learned artifacts.

## Milestone 10: Stable 1.0

**Status: future.**

Required before `1.0`:

- stable normalized and receipt contracts with compatibility policy;
- production resource isolation and recovery privacy;
- at least one supported in-process SDK and one validated host integration;
- broad deterministic engine coverage for declared workloads;
- published reproducible competitor and baseline benchmarks;
- paired quality-adjusted cost evidence across multiple model families;
- security review, vulnerability-response process, signed releases, and SBOMs;
- operational documentation for upgrades, rollback, observability, and data deletion; and
- no unresolved critical issue against the no-gateway/no-egress boundary.

## Work explicitly deferred

- KendrWeb integration or migration.
- Provider/model routing, fallbacks, load balancing, or credential management.
- OpenAI-compatible inference proxy endpoints.
- Cloud-hosted optimization as a default.
- Post-generation prose rewriting presented as output-token savings.
- Automatic abstractive summarization through an external LLM.
- Marketing claims based on multiplying unrelated component compression ratios.

## Contribution priorities

Contributions should be prioritized in this order:

1. Correct contracts, validation, privacy, and resource bounds.
2. Honest full-payload measurement and reproducible evaluation.
3. Typed deterministic algorithms with clear proof obligations.
4. Safe host adapters and shared-language SDKs.
5. Context/tool intelligence in shadow mode.
6. Learned policy only where it demonstrably improves quality-adjusted cost.

The project should prefer a trustworthy no-op over an impressive but unverifiable compression number.
