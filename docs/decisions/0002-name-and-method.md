# ADR 0002: Project and method name

- Status: Accepted
- Date: 2026-08-08
- Decision owners: Kendr Optimizer maintainers

## Context

The project needs a public name that fits the Kendr family, maps cleanly to the
existing Rust crates and CLI, and distinguishes this implementation from a
gateway, generic prompt rewriter, or wrapper around another optimizer.

The name must also describe the strongest current technical property without
implying formal proof, universal losslessness, or guaranteed downstream answer
equivalence.

## Decision

The public project name is **Kendr Optimizer**.

The executable and concise command name are **`kendr-opt`**.

The technical method is **Verification-Gated Typed Token Reduction**.

The publication title is:

> Kendr Optimizer: Verification-Gated Typed Token Reduction for
> Provider-Neutral LLM Contexts

The project tagline is:

> Reduce only what passes structure, integrity, cache, risk, and net-gain
> gates.

"Verification-gated" means that candidate application depends on executable
invariant checks and, for eligible typed encodings, independent byte-exact
expansion. It does not mean mechanized formal verification or a cryptographic
proof.

## Rationale

- `Kendr Optimizer` preserves the existing product, repository, schema, crate,
  and integration identity.
- `typed` names the contract boundary that separates prose from code, JSON,
  tool calls, tool results, documents, and output contracts.
- `verification-gated` describes the controller's acceptance mechanism.
- `token reduction` states the measured objective without promising that every
  input can or should be compressed.
- `provider-neutral` is accurate: the core neither routes nor calls an LLM.

## Rejected alternatives

### Proof-Carrying Token Reduction

Rejected because "proof-carrying" usually implies a stronger formal proof
system than the current runtime reconstruction and invariant checks provide.

### Lossless Context Optimization

Rejected because representation-safe and recoverable engines have narrow exact
properties, while opt-in extractive engines can intentionally remove
model-visible information.

### Verification-Gated Typed Compaction

Technically acceptable, but "token reduction" is clearer about the measured
objective. The acronym `VGTC` also collides with a well-known visualization and
graphics organization.

### A new standalone brand

Rejected for the current release because a second brand would add migration and
discovery cost without clarifying the architecture better than the selected
technical descriptor.

## Collision check

Exact-phrase web and GitHub repository searches on 2026-08-08 found no existing
use of "Verification-Gated Typed Token Reduction." Repository searches and
registry requests also found no `kendr-optimizer` package on crates.io, npm, or
PyPI at that time.

This was a practical collision check, not trademark or legal clearance.
Package names should be reserved before public announcement.

## Consequences

- Human-facing titles and prose use `Kendr Optimizer`. Machine identities use
  conventional compact forms: `kendr-opt`, `kendr-optimizer-*`, the `@kendr`
  npm scope, and the historical `KendrOptimizer` label preserved inside
  immutable benchmark evidence.
- The CLI remains `kendr-opt`.
- Marketing copy must not shorten the method to "verified savings" because
  provider verification is a separate evidence level.
- A future formal proof system may adopt a stronger name only through another
  recorded decision.
