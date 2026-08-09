# ADR 0001: KendrOptimizer Is a Transform Engine, Not an LLM Gateway

- Status: Accepted
- Date: 2026-08-07
- Scope: Repository-wide

## Context

KendrOptimizer is intended to reduce the token footprint of model and agent workloads while remaining independent of the model provider. It must be usable inside products such as OpenClaw, an existing AI gateway, an agent runtime, a command-line workflow, or Kendr itself.

Calling the project a proxy can be misleading. A conventional LLM proxy accepts a provider-shaped inference request, holds or forwards credentials, selects an upstream, performs the model call, streams the response, and may retry or route the request. That would duplicate the host gateway, couple optimization to provider protocols, and make the optimizer a privileged bearer of prompts and API keys.

The optimization problem has a cleaner boundary: accept a normalized content envelope, return a transformed envelope and an auditable receipt, then let the host perform its own provider call. Actual usage can be reported back separately. This also makes it possible to embed the same implementation in a process, call it over loopback, or invoke it through a CLI without changing its semantics.

## Decision

KendrOptimizer is a provider-neutral transformation engine. It is not, and will not expose itself as, an LLM gateway.

The core accepts normalized messages, tool definitions, tool results, output constraints, host capabilities, target/tokenizer metadata, cache metadata, and an optimization policy. It may analyze or transform that content and may compare usage observations supplied by the host. It does not perform inference.

The following are architectural invariants:

1. The core library has no networking or provider-SDK dependency.
2. The core never accepts, stores, logs, or forwards provider credentials.
3. The core never selects a model, provider, region, endpoint, or routing fallback.
4. The core never initiates an LLM call, including for summarization or quality judging.
5. No executable in this repository implements an OpenAI-, Anthropic-, or other provider-compatible inference relay such as `/chat/completions`, `/responses`, or `/messages`.
6. The optional HTTP process is a transform-only local service. Its endpoints are limited to operations such as analyze, optimize, restore, observe, health, and engine discovery.
7. Provider-shaped adapters are pure conversion and integration layers. They convert between host structures and `kendr.optimize/v1`; they do not forward inference traffic.
8. The host remains responsible for serialization, authentication, provider calls, streaming, retries, routing, billing records, and delivery.
9. Provider usage is evidence supplied after a call. KendrOptimizer must not fabricate provider-reported usage from a local estimate.
10. Optimization must fail open for an otherwise valid host request: a transform error returns or causes the adapter to use the original content.

The word *proxy* may be used only in the loose deployment sense of “a component placed before a model call.” Documentation and command names must make clear that the host invokes KendrOptimizer and then invokes the model; KendrOptimizer does not transparently relay the model request.

## Current implementation

The pre-alpha workspace follows the central boundary:

- `kendr-optimizer-contracts` defines the normalized request, outcome, receipt, recovery, and usage-observation types.
- `kendr-optimizer-core` contains native transformations and has no network dependency.
- `kendr-opt` supports analyze, optimize, restore, observe, engine discovery, and an optional transform-only HTTP service.
- The HTTP service exposes `/v1/analyze`, `/v1/optimize`, `/v1/restore`, `/v1/observe`, `/v1/engines`, and `/healthz`. It contains no upstream URL or provider route.
- The OpenClaw adapter posts normalized context to a credential-free loopback origin and returns the original context on failures.

This is pre-alpha, not proof that the boundary is permanently enforced. In particular, `kendr-opt serve` defaults to `127.0.0.1:7331` but currently accepts an operator-supplied non-loopback bind address. It has no authentication or explicit request-size limit. Until those controls are implemented, operators must keep it on loopback or another separately protected local transport.

## Consequences

### Benefits

- One optimization core can be embedded in many gateways and runtimes.
- The optimizer does not become a credential or routing trust boundary.
- Benchmarking can isolate transformation quality from provider routing behavior.
- Hosts retain their existing retry, streaming, policy, and observability semantics.
- The project can support models not known when the optimizer was released, provided the host can normalize the content.

### Costs

- Every host needs an adapter or must use the normalized contract directly.
- The optimizer cannot know exact provider framing, cache billing, hidden tokens, or actual cost unless the host supplies that data.
- Reducing billed output tokens requires a pre-generation policy that the host applies. Rewriting an already generated answer cannot reduce the bill for that call.
- Recovery and state must be explicitly integrated; the core cannot assume it can retrieve an omitted block during generation.
- A transform-only sidecar requires one extra local call unless the core is embedded in-process.

## Alternatives rejected

### OpenAI-compatible reverse proxy

Rejected because it would own inference transport, credentials, streaming compatibility, retries, and upstream behavior. It would turn a portable optimization engine into another gateway.

### Wrapper around existing optimizers

Rejected as the product architecture. Competitor tools may be invoked by the benchmark harness at pinned versions, but production engines are implemented natively against KendrOptimizer's contracts, risk model, verification gates, and receipts.

### Remote optimization as the default

Rejected because normalized envelopes can contain private conversations, source code, secrets, and tool output. Local in-process or loopback operation is the default. A future hosted transform service, if ever offered, would require a separate security decision and still would not become an inference gateway.

### LLM-backed compression in the core

Rejected because it introduces provider dependency, credentials, non-determinism, latency, new token cost, and data egress. Optional learned engines must execute locally from pinned model artifacts and remain subject to the same gates as deterministic engines.

## Enforcement direction

The boundary should be protected with automated checks:

- dependency-policy tests that reject provider SDKs and outbound HTTP clients in core crates;
- route tests that reject provider-compatible inference paths;
- secret-shaped fixture tests confirming credentials are neither required nor logged;
- loopback or local-transport enforcement for the sidecar;
- adapter tests that block redirects, user information, authorization headers, and non-local endpoints; and
- release review of any new network-capable dependency.

Changing this decision requires a new ADR. A convenience request to forward model traffic is not sufficient reason to weaken the boundary.
