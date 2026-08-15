# Threat Model

## Status

KendrOptimizer is pre-alpha. The current code demonstrates conservative transformation and structural gates; it has not completed a production security review. This document records both implemented controls and known gaps so early adopters do not infer protections that are not present.

## Security objectives

KendrOptimizer should:

- never become an LLM credential, routing, or provider-egress boundary;
- never execute content from prompts, tool results, documents, or recovery data;
- preserve immutable protocol and safety-critical fields;
- fail open to original content when optimization cannot be trusted;
- prevent cross-request, cross-session, and cross-tenant recovery disclosure;
- avoid leaking raw content through receipts, logs, telemetry, or crash reports;
- remain bounded under maliciously large, nested, repetitive, or adversarial input;
- make extractive and learned behavior explicit and opt-in;
- keep third-party engines less trusted than the core planner/verifier; and
- make every accepted transformation and failed gate auditable without requiring raw content retention.

It cannot guarantee that two semantically similar prompts produce identical model output. Runtime safeguards reduce risk; task-level non-inferiority must be established through evaluation.

## Assets

Normalized envelopes may contain:

- system and developer instructions;
- private user conversations;
- source code, local paths, documents, and credentials accidentally printed by tools;
- tool names, schemas, arguments, results, and error traces;
- image URIs and metadata;
- business rules, personal data, and regulated data;
- provider/model and pricing metadata; and
- output format or safety constraints.

Recovery capsules are at least as sensitive as original envelopes because they deliberately retain content removed from the optimized form. Provider usage and task-success observations may also be commercially sensitive.

## Trust boundaries

```text
Untrusted prompt/document/tool output
             |
             v
Host adapter and normalized envelope
             |
             v
Core classifier, native engines, planner, verifier
             |
             +----> receipt metadata
             |
             +----> recovery capsule (raw sensitive content today)
             |
             v
Host-controlled inference path

Optional local service boundary:
Host process --loopback/local transport--> kendr-opt serve

Optional CLI update boundary:
kendr-opt update --release metadata and assets only--> GitHub Releases

Future extension boundary:
Core --bounded scoped data--> sandboxed WASI engine
```

The model provider is outside KendrOptimizer's architecture. Provider credentials must never cross into the normalized contract or optimizer service.

## Adversaries and failure sources

- A user intentionally submits adversarial content.
- A remote document or tool result contains prompt injection or crafted parser input.
- A compromised tool emits secrets, fake optimizer markers, or token-amplifying output.
- A malicious local process connects to an exposed optimizer service.
- An integration accidentally sends credentials or persists raw receipts/capsules.
- A buggy or malicious future engine removes critical content or consumes excessive resources.
- A dependency or release artifact is compromised.
- A learned model is evaded, poisoned, or replaced.
- An operator misconfigures risk, cache, network binding, or recovery retention.
- An honest algorithm loses task-critical context that its heuristic did not recognize.

## Current controls and gaps

### No provider or content egress

**Implemented:** The core crate has no networking dependency. The CLI service
implements inbound transform endpoints only and contains no upstream/provider
configuration. The OpenClaw adapter constructs one credential-free request to
`/v1/optimize` on a validated loopback origin, rejects redirects, and omits
credentials.

The CLI updater is a narrow, separate exception for release distribution. In
production it is compiled for the public `Kendr-AI/Kendr-Optimizer` repository
and requests only repository identity, release metadata, and selected release
assets from GitHub's API and HTTPS asset-delivery path. It never includes an
envelope, prompt, tool result, recovery capsule, provider credential, provider
URL, or model setting, and it does not contact Kendr.org. Passive checks run
only before interactive setup or run commands, cache successful checks for 24
hours, and can be disabled with `KENDR_NO_UPDATE_CHECK=1`.

**Gaps:** An update request discloses the operator's IP address, request timing,
and CLI version in its user agent to GitHub and its asset delivery network.
Official release binaries have no update-authority override; a separately
compiled CI test feature accepts only an explicitly enabled numeric-loopback
fixture. The CLI
permits an operator to bind the inbound transform service to a non-loopback
address. Architectural intent is not yet enforced by a complete automated
dependency-policy test or runtime egress sandbox.

**Required:** CI must reject provider SDKs and outbound HTTP clients in core
crates, prove that adapters and transform routes cannot relay provider traffic,
and constrain updater tests to repository metadata and release assets without
sensitive headers or bodies. Distribution guidance should prefer an embedded
SDK, stdio, Unix socket/named pipe, or enforced loopback binding.

### Prompt injection and inert data

**Implemented:** Engines operate on strings and structured values; they do not run shell commands, evaluate code, follow URLs, or ask a model to interpret content. Tool-call arguments are immutable under verification.

**Gaps:** Injection text can manipulate lexical selectors or cause diagnostic keyword retention. The optimizer does not currently distinguish trusted host instructions from hostile statements embedded in tool output beyond typed roles. A future learned engine could be influenced by adversarial phrasing.

**Required:** Content must remain inert at every stage. Feature extractors should be role- and origin-aware. Tool output must never authorize tools or override policy. Learned models need adversarial evaluation and deterministic vetoes.

### Instruction and protocol mutation

**Implemented:** The verifier checks output-contract equality, tool-call IDs/names/arguments, typed code/JSON/image parts, and tool definitions. The OpenClaw adapter separately preserves message count, order, IDs, roles, part types, tool calls, opaque blocks, and host-owned fields.

**Gaps:** The core's envelope validator does not yet enforce full message topology, role ordering, parent links, unique tool-call IDs, result pairing, or complete metadata invariants. Text normalization can currently touch text/document parts regardless of message role if it finds a net gain. Protected regexes are incomplete.

**Required:** System and developer content should be immutable by default. Full causal validation and adapter round-trip properties are release blockers. User-declared preserve regions must become first-class typed constraints.

### Tool-surface reduction

**Implemented:** The native selector is disabled by default, requires extractive risk, host authorization to narrow, and host ability to retry with all tools. It only removes optional tools and preserves required/`always` tools plus declared dependencies.

**Gaps:** Retry capability is a claim, not observed. Lexical false negatives can make a needed tool unavailable. Dependency tags can be incomplete. Selection is not a security allow/deny mechanism.

**Required:** The host's security policy must run independently and take precedence. The optimizer must never add or authorize a tool. Serious deployments should keep selection in shadow mode until calibrated false-negative and retry rates are known.

### Marker spoofing and reference confusion

**Implemented:** Repeated-line compaction avoids content already containing its current marker prefix. History references include message IDs and SHA-256 digests. Full-envelope recovery can restore the original.

**Gaps:** Markers are recognizable, deterministic text and are not random, scoped, or authenticated. A malicious tool result can imitate them. A model has no automatic access to the recovery capsule. SHA-256 digests provide integrity identifiers, not authorization or authenticity.

**Required:** Use request-scoped random markers with authenticated metadata, escape literal collisions, bind every record to tenant/session/request/scope, and require explicit host capability for model-time resolution. Marker text must never be interpreted as trusted instruction.

### Cache churn and cost amplification

**Implemented:** Hosts may declare cache segments. Candidates touching protected segments are rejected when prefix preservation is enabled; tool-surface changes count as a touch when cache segments exist. Whole-envelope inflation is rejected.

**Gaps:** Provider-level prefix serialization and cache-write pricing are not modeled. Repeated request-specific markers or unstable transformations could defeat caching. An attacker may shape inputs to trigger expensive optimization with no bill benefit.

**Required:** Prefer deterministic output, stable prefixes, explicit cache-boundary data, minimum gain margins, resource budgets, and cache-aware paired evaluation. Rate-limit or bypass pathological repeated failures.

### Denial of service

**Implemented:** The optimizer has a configured global latency budget checked between engines. Rust's regex engine avoids classical catastrophic backtracking. The OpenClaw adapter uses an abort timeout and a short failure circuit breaker.

**Gaps:** One engine cannot currently be interrupted by the global budget. The HTTP service has no explicit request-body, concurrency, per-tenant, nesting-depth, or memory limit and no authentication. JSON deserialization and envelope cloning can amplify memory. Large recovery capsules duplicate the entire envelope. Hashing, tokenization, and regex scanning remain proportional to content size.

**Required:** Add byte, message, part, schema-depth, JSON-depth, output-size, CPU, wall-clock, memory, and concurrency limits. Enforce them before expensive cloning/tokenization. Use cancellation-aware workers or process isolation for learned/external engines. Keep the service loopback-only until authenticated local IPC and limits exist.

### Supply chain and extensions

**Implemented:** Production engines are native repository code rather than
runtime wrappers around external optimizers. Workspace dependencies are pinned
in `Cargo.lock` for this checkout. The updater requires a published GitHub
Release reported as immutable, validates GitHub's SHA-256 asset digests against
the release `SHA256SUMS`, requires exact archive membership, smoke-tests the
candidate binary, and rechecks release metadata before replacement.

**Gaps:** GitHub's digest and `SHA256SUMS` are integrity evidence within the same
GitHub release trust boundary; they are not a maintainer signature. Immutability
prevents later mutation but cannot make a malicious or compromised initial
upload trustworthy. Native binaries are not yet protected by Sigstore or OS
code signing. The updater's backup-backed rollback handles detected failures,
but replacement is not journaled or guaranteed crash-consistent across power
loss. There is no extension sandbox, signed plugin registry, release
signing policy, SBOM gate, dependency audit gate, or stable engine ABI. A future
in-process dynamic plugin could access all memory and process privileges.

**Required:** Releases need independent provenance and maintainer signatures in
addition to checksums, plus OS code signing where practical, SBOMs, dependency
review, and reproducible-build direction. External engines should be WASI
components with no network by default and bounded scoped input. Do not define an
unsafe native dynamic-library ABI as the public extension surface.

### Learned engines

**Implemented:** None. There is currently no local or external model inference in the optimizer.

**Risks:** Model artifact substitution, prompt/data exfiltration, adversarial salience errors, training-data poisoning, unbounded inference, architecture-specific nondeterminism, and silent quality drift.

**Required:** Learned engines must be optional, local, pinned by digest, offline-promoted, bounded, and unable to bypass deterministic gates. Model cards and training/evaluation provenance are mandatory. No uncontrolled online learning from production traffic.

## Recovery-capsule privacy

### Current behavior

When an applied portfolio is considered recoverable, the current core returns a `RecoveryCapsule` whose record contains the **entire original normalized envelope in plaintext JSON**. The digest is SHA-256, not an encryption or authentication mechanism. The core does not persist the capsule, impose a TTL, redact it, or encrypt it.

This design proves simple exact restoration in pre-alpha tests, but it is not an acceptable default durable-storage format.

### Operator rules for pre-alpha

- Treat a capsule exactly like the original unredacted provider request.
- Do not log it or attach it to tracing spans, analytics, support tickets, URLs, or ordinary receipts.
- Do not persist it unless storage encryption, strict access control, retention, deletion, and tenant separation are already provided by the host.
- Never send it to an LLM merely to explain an optimization.
- Do not enable `recoverable`, `extractive`, or `learned` transforms in an integration that cannot secure and restore it.
- Delete it as soon as the host's recovery/retry window ends.

The current OpenClaw adapter correctly caps risk at `representation_safe` and does not accept recovery capsules into its context lifecycle.

### Target design

Replace whole-envelope copies with minimal scoped records and a host-supplied `RecoveryStore`. Each record should contain:

- tenant/session/request/scope binding;
- random marker and authenticated associated data;
- original-content digest;
- encrypted or host-protected payload;
- creation, expiry, and deletion metadata;
- maximum retrieval count or explicit reusable policy; and
- engine/ruleset version needed for validation.

Receipts should contain only a safe recovery handle and metadata, never the payload. If a store write or later validation fails, the transform must not claim recoverability and must be rejected or downgraded.

## Receipt and telemetry privacy

Current receipts contain request IDs, digests, counts, engine IDs, reasons, and
verification details. Protected-artifact failures report only aggregate missing
value and occurrence counts; a regression test prevents the raw URL/path/ID
values from being copied into that detail. Receipts remain sensitive: request
IDs may carry tenant data, full-envelope digests can enable dictionary tests on
low-entropy inputs, and future engine reasons could accidentally widen the
telemetry surface.

Before production:

- every verification and engine-reason field must remain covered by
  secret-shaped redaction tests;
- raw artifact samples must require an explicit local debug mode;
- request IDs must not encode user or tenant data;
- digests exposed across trust boundaries should be keyed where dictionary attacks are plausible;
- traces must exclude request/response bodies and recovery payloads;
- content-free telemetry must be opt-in; and
- retention and deletion must be documented.

The OpenClaw adapter's advancement registry stores SHA-256 hashes rather than raw advancement keys, but those hashes are not signatures and should still be stored with restrictive permissions as currently attempted.

## Local service deployment

The service defaults to `127.0.0.1:7331`. Pre-alpha safe deployment requires:

- keep the bind address on loopback;
- do not expose it through a reverse proxy, public container port, LAN bind, or tunnel;
- run as a non-privileged OS user;
- restrict filesystem access, especially if file input/output commands are used;
- avoid debug/body logging;
- set operating-system and container CPU/memory limits; and
- isolate it from untrusted local users when envelopes contain secrets.

The current service has no authentication. Loopback is not a complete boundary on a multi-user or compromised host. Local IPC with peer credentials or an embedded library is the preferred production direction.

## Fail-open and fail-closed decisions

Optimization failures should fail open to original content so a cost feature does not break the host's model request. Examples include engine exceptions, insufficient gain, timeout, malformed optimizer response, and verification failure.

Security authorization must not fail open through the optimizer. The optimizer is not the security policy engine. A host must apply tool, data, and provider authorization independently before and after optimization as appropriate.

Invalid normalized input should return an error rather than guessing. The host adapter, which still has the untouched host request, may then bypass optimization. Recovery-store failure for a transform requiring recovery must fail that transform, not silently proceed.

## Security test requirements

Before a stable release, CI should include:

- fuzzing of envelope deserialization, parsers, markers, and restoration;
- property tests for immutable fields and adapter round trips;
- adversarial prompt/tool-result fixtures;
- cross-tenant recovery isolation tests;
- request-size, nesting, timeout, memory, and concurrency tests;
- route and dependency tests proving no provider relay or content egress, with
  updater network access restricted to repository metadata and release assets;
- secret-shaped fixtures proving no content appears in logs/receipts by default;
- malicious extension sandbox tests;
- cache-churn and repeated-failure tests;
- dependency auditing, license checks, SBOM generation, and signed artifacts; and
- an external security review before recommending production deployment.

## Reporting vulnerabilities

A public security policy and private reporting channel are not yet present in this pre-alpha scaffold. They are required before publishing broadly or accepting third-party extensions. Until then, avoid processing production secrets and report issues privately to the repository maintainers rather than publishing exploit payloads in an issue.
