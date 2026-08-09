# Benchmark methodology

Last reviewed: 2026-08-07

This is the normative methodology for evaluating KendrOptimizer and making public claims. The primary question is not “did the text become shorter?” It is:

> Did the optimizer lower total observed cost or resource use for the same workload while preserving downstream success within an explicit risk bound?

A benchmark that measures only characters, bytes, or preflight token estimates can test a transform, but it cannot establish end-to-end savings or quality preservation.

## Principles

1. **Pass-through is the primary baseline.** Every optimized run is paired with the same workload using no optimization.
2. **Quality and task completion are co-primary outcomes.** A lower bill with a failed task is not a saving.
3. **The whole workload is the denominator.** Count every model request, retry, tool round, cache write/read, optimizer invocation, and generated token through terminal success or failure.
4. **Estimated and observed measurements are separate.** Preflight receipts are useful for gating; provider usage or local-inference counters establish what was billed or computed.
5. **Output savings require changed generation.** Editing an answer after the provider generated it cannot lower output tokens already billed.
6. **No-op, timeout, rejection, and rollback are results.** They remain in the denominator and in published artifacts.
7. **Claims are workload-specific.** Results must identify corpus, target model, tokenizer, policy, cache state, optimizer build, and date.
8. **Competitors run under their documented conditions.** Failures or unsupported cases are reported, not silently removed.

## Evaluation layers

KendrOptimizer should be tested at four layers. Results from one layer must not be relabeled as another.

| Layer | Purpose | Required measurements | Permitted conclusion |
| --- | --- | --- | --- |
| Transform microbenchmark | Validate one native engine on a known content region | Exact bytes, tokenizer-specific tokens, latency, verification checks, reconstruction where applicable | This engine transformed this fixture by this amount |
| Envelope benchmark | Test planning across the complete serialized request | Original/optimized tokens and bytes, cache impact, all attempts, optimizer latency | This policy reduced or skipped this request before inference |
| Paired model benchmark | Test downstream response quality and actual usage | Provider/local usage, output, latency, quality, price schedule, optimizer overhead | This policy changed cost and quality for this target model and task set |
| Paired agent/workflow benchmark | Test multi-turn consequences | All requests, tools, retries, turns, cache events, wall time, terminal success, total cost | This policy changed end-to-end workload outcomes |

## Workload matrix

A release benchmark should cover representative strata rather than averaging all inputs into one flattering number.

### Input length

- short requests where optimization overhead may exceed benefit;
- medium contexts typical of ordinary chat or agent turns;
- long contexts near target-model limits;
- repeated multi-turn histories.

### Content type

- natural-language instructions and documents;
- source code and diffs;
- test, build, compiler, and application logs;
- JSON objects and arrays, including malformed JSON;
- tool definitions and structured schemas;
- tool calls and corresponding results;
- retrieved evidence for single-hop and multi-hop questions;
- mixed content with URLs, paths, numbers, identifiers, Unicode, code fences, and quoted instructions.

### Task family

- exact-answer question answering;
- retrieval-grounded question answering;
- summarization with factuality checks;
- code generation and repair executed against tests;
- schema-constrained generation;
- tool selection and multi-step tool use;
- debugging workflows with failures and retries;
- safety- or policy-sensitive instruction following.

### Risk profile

- pass-through;
- representation-safe;
- recoverable;
- extractive;
- learned.

Results must be reported separately for each risk profile. An opt-in lossy profile must not raise the headline number for the default quality-preserving profile without a clear split.

## Corpus construction

Each benchmark case needs:

- a stable case identifier;
- source and license metadata;
- content-type and task-family labels;
- the unoptimized envelope;
- target model and tokenizer configuration;
- cache condition;
- expected protected artifacts and structural invariants;
- task-specific scorer and success criteria;
- maximum turns or termination rule;
- optional expected output for exact-match tasks.

Prefer public, versioned datasets for broad task coverage and newly authored adversarial fixtures for engine correctness. Do not commit private customer prompts, secrets, credentials, or personal data. Externally sourced datasets, test suites, or model outputs need explicit redistribution and attribution review under [the provenance policy](provenance.md).

Avoid contamination between engine development and held-out evaluation. Record which cases were used to tune rules. Maintain at least one frozen holdout and one periodically refreshed challenge set.

## Compared systems

Every comparison includes:

- unoptimized pass-through;
- the current KendrOptimizer default policy;
- each KendrOptimizer risk profile being evaluated;
- selected upstream baselines appropriate to the workload.

An upstream optimizer should only be included where its scope applies. For example, RTK is meaningful on supported command output, Caveman on future model-output style, and RECOMP on retrieved evidence. PCToolkit may run prompt-compression methods but is not itself a single compression algorithm.

For every competitor record:

- repository URL;
- immutable commit or released package version;
- installation lockfile or environment image digest;
- license;
- exact configuration and model weights;
- hardware and accelerator;
- tokenizer and target model;
- any changes needed to run;
- unsupported, errored, timed-out, or no-op cases.

Do not tune KendrOptimizer on the test set while leaving a competitor at defaults, or vice versa. Publish both default-policy and equally budgeted comparisons when tuning is material.

## Pairing and run protocol

### Unit of pairing

The unit is a complete benchmark case from initial input through the defined terminal state. For a single-turn task this is one request and answer. For an agent task it includes every follow-up request, tool call, tool result, retry, compaction, and final answer.

Baseline and optimized arms must use:

- the same source case;
- the same target-model revision or deployment;
- the same system/developer instructions except for the optimizer’s intentional generation policy;
- the same tool set before any measured narrowing;
- the same temperature, top-p, seed when supported, output limit, and stop conditions;
- the same cache condition;
- the same maximum-turn and retry rules.

### Order effects

Randomize whether baseline or optimized runs first. Use blocked randomization by model, workload stratum, and cache condition. When provider-side nondeterminism or load is material, interleave paired arms closely in time.

For deterministic local models, run fixed seeds plus enough additional seeds to test sensitivity. For nondeterministic APIs, use repeated paired runs. Never treat one favorable sample as a quality guarantee.

### Cache conditions

Report cold and warm cache separately when the target supports prompt caching. Preserve:

- cache creation tokens;
- cache read tokens;
- cache TTL assumptions;
- prefix identity or digest;
- whether optimization invalidated a previously reusable prefix;
- provider-specific cached-token price.

Shorter raw input can cost more when it destroys an inexpensive cache hit. The benchmark must retain that outcome.

### Failure handling

Predefine timeout, retry, and terminal-failure rules. Attribute optimizer-caused recovery requests and full-context retries to the optimized arm. Infrastructure failures unrelated to either arm may invalidate a pair, but the exclusion and reason must be logged before outcome inspection where possible.

## Token and byte accounting

### Preflight

For every envelope, retain the KendrOptimizer receipt fields, including:

- receipt schema version;
- original and optimized serialized digests;
- bytes and tokens;
- tokenizer identifier and confidence;
- signed token and byte deltas;
- estimated input reduction;
- cache impact;
- total optimizer latency;
- every engine attempt, status, risk, reason, and verification result;
- warnings and no-op reason;
- recovery-capsule metadata where present.

Approximate token counts must be labeled conservative estimates and cannot be mixed with exact-tokenizer counts in an aggregate without stratification. Byte reduction is never reported as token reduction.

Define preflight input-token reduction for a case as:

    original_input_tokens - optimized_input_tokens

and preflight reduction rate as:

    (original_input_tokens - optimized_input_tokens) / original_input_tokens

The signed delta remains negative when the optimizer inflates the request. Do not clamp it to zero.

### Preservation-qualified public ranking

The release's payload comparison publishes a diagnostic raw aggregate and a
stricter qualified value for each optimizer configuration and surface. These
fields must not be conflated.

For a configuration on one surface, let `C` be the set of successfully
completed cases (`status == ok` with a score), `I_c` the independently
recounted input tokens for case `c`, and `O_c` its independently recounted
output tokens. Raw token reduction is:

    100 × (Σ[c in C] I_c - Σ[c in C] O_c) / Σ[c in C] I_c

Only successfully completed cases enter those sums. Failed and unsupported
cases contribute no tokens to the numerator or denominator. The raw value is
therefore always published with completed, eligible, failed, and expected-case
counts. A raw percentage from partial coverage is a diagnostic of the cases
that completed; it is not a full-surface result. The value remains signed, so
inflation remains negative.

Each completed case receives one composite fixture-preservation result. It
passes only when all applicable hard requirements pass:

- every fixture-declared required literal is retained exactly;
- where the fixture requests JSON equivalence, the primary output parses as
  JSON and is semantically equal to the original JSON value;
- the benchmark's exact query marker is retained.

URL, path, and number recall are separately reported diagnostics. They are not
additional hard gates unless a fixture declares those exact values as required
literals. This composite gate is intended to expose specified corruption. It
does not establish downstream target-model quality, task success, or semantic
equivalence for arbitrary text.

Qualification occurs at two scopes:

1. **Source release-summary qualification.** For an optimizer's declared
   eligible cases on a surface, every eligible case must complete and every
   completed case must pass the composite fixture gate. If that condition
   holds, the source summary copies raw reduction unchanged into its qualified
   field; otherwise that field is `null`.
2. **Public primary-ranking qualification.** The derived public ranking checks
   the frozen corpus rather than accepting an optimizer's eligibility boundary.
   The configuration must declare eligible for and complete every frozen case
   assigned to the surface, report zero failures, and pass the composite fixture
   gate on every completed case. The current authored corpus assigns 5 cases to
   prompt/context and 4 cases to tool output.

The second gate deliberately prevents an optimizer from improving its public
rank by marking difficult cases unsupported. When it passes, qualified token
reduction equals raw token reduction exactly; it is not discounted or
recomputed. When it fails, qualified reduction is `null`/`N/A` and the row is
excluded from the primary rank. Its raw completed-case result remains visible
in a diagnostic table with concrete coverage and fixture-gate counts.

An exclusion means only that this exact configuration failed the stated public
gate for this corpus and surface. It is not a claim that the optimizer is
universally unsafe or ineffective. Primary rows are ordered by higher qualified
reduction and then fewer completed cases with zero token delta; latency is
diagnostic and never affects rank.

### Observed provider or local-model usage

Capture authoritative usage fields when available:

- uncached input tokens;
- cached input tokens;
- cache creation tokens when separately billed;
- output tokens;
- reasoning or other billed token classes;
- request count;
- reported provider cost, or a calculation using a versioned price sheet;
- time to first token, generation time, and end-to-end latency.

If a provider omits a class, report it as unavailable rather than zero. If an API returns only total tokens, do not fabricate the split.

For local inference, record tokenizer counts plus target compute metrics appropriate to the deployment, such as accelerator time, energy, memory peak, and throughput. A dollar estimate for local inference needs a disclosed costing model.

## Cost accounting

For one workload arm, total observed cost is:

    sum of all provider request charges
    + separately metered optimizer services
    + recovery or retry charges

Local optimizer CPU time may be shown as latency and compute even when it is not monetized. If it is converted to currency, publish the hardware, utilization, energy, and unit-price assumptions.

Paired cost saving is:

    baseline_total_cost - optimized_total_cost

Paired cost-saving rate is:

    (baseline_total_cost - optimized_total_cost) / baseline_total_cost

Report both the signed absolute difference and rate. A negative saving is a regression and remains in the aggregate.

Never calculate total-session savings by multiplying:

- the percentage of traffic thought to contain tool output;
- an upstream tool-output compression percentage;
- an input-token price assumption;
- a separate output-style percentage.

Those factors are dependent and their denominators differ. Replay or run the whole session and measure it.

## Quality and utility

Compression quality is downstream behavior, not textual resemblance alone. Use task-native metrics first.

### Deterministic tasks

- exact match or normalized exact match;
- token-level F1 where partial answer overlap is meaningful;
- executable unit, integration, or hidden tests for code;
- JSON Schema validation and exact required-field checks;
- tool-call name, argument, ordering, and call/result-causality checks;
- expected error diagnosis or required artifact retention.

### Retrieval and factual tasks

- answer exact match or F1;
- citation precision and recall;
- evidence attribution;
- entity, number, date, negation, and qualifier preservation;
- faithfulness or entailment against source evidence.

### Open-ended tasks

Use a combination of:

- blinded human preference or rubric scoring;
- randomized A/B presentation order;
- a disclosed model-judge prompt and judge version;
- multiple judges or human calibration on a sample;
- length-normalized rubrics so terse output is not rewarded merely for being terse;
- explicit factuality, instruction-following, completeness, safety, and clarity dimensions.

The target model should not judge its own output as the only quality measure.

### Agent workflows

Primary utility is terminal task success. Also report:

- number of turns and model calls;
- tool-call correctness;
- unnecessary or repeated commands;
- retries with full context or full tool catalog;
- files changed and test status where applicable;
- wall-clock completion time;
- human intervention rate.

A run that appears cheaper per turn but needs extra turns may be more expensive overall.

## Structural and safety checks

Every case should run applicable invariant checks before downstream inference:

- system and developer roles remain ordered and present;
- tool-call identifiers still match tool-result identifiers;
- required tool definitions and schema signatures remain valid;
- output contract remains byte-identical unless the policy explicitly permits a representation-safe normalization;
- protected code, paths, URLs, numbers, identifiers, error lines, safety rules, and user constraints remain present;
- reversible transforms reconstruct the original exactly;
- recovery data is scoped to the correct request and session;
- prompt-cache prefixes claimed as preserved have matching serialized digests;
- markers cannot be spoofed by untrusted tool output;
- malformed, deeply nested, or adversarial input respects time and memory limits.

Any failed hard invariant forces pass-through or rollback. The event stays visible in the receipt.

## Statistics

### Paired estimates

Compute per-case optimized-minus-baseline differences for cost, input tokens, output tokens, latency, and quality. Prefer paired bootstrap confidence intervals stratified by workload family. Publish sample count, mean, median, dispersion, and interval; token and latency distributions are often skewed.

### Non-inferiority

Pre-register an acceptable quality margin for each metric and risk profile before running the test. Test whether the lower bound of the optimized-minus-baseline quality interval stays above the negative margin. For exact task success, report paired win/loss/tie counts and an appropriate paired proportion analysis.

Representation-safe and recoverable profiles should ordinarily target a zero or near-zero quality margin plus structural proof. Extractive and learned profiles may use a nonzero margin only when explicitly enabled and disclosed.

Do not infer “same quality” merely because a small sample produced no statistically significant difference. Absence of evidence is not evidence of equivalence.

### Multiple comparisons

When testing many policies, engines, workloads, or target models, distinguish exploratory results from confirmatory claims and control false discoveries where appropriate. Select a release policy on development data, then evaluate it once on the frozen holdout.

## Latency

Measure:

- optimizer wall time and CPU time;
- any auxiliary model loading separately from steady-state calls;
- peak memory;
- serialization and restoration overhead;
- time to first token;
- target-model inference time;
- complete workload wall time.

Report cold-start and warm steady-state distributions separately. A future policy with a hard, cancellable deadline should time out and pass through; the timeout overhead remains part of the optimized arm. The current synchronous implementation only stops dispatching later engines after the shared budget is exhausted, so an in-flight overrun must be reported rather than relabeled as deadline-compliant.

## Claim levels

Use the strongest level supported by the evidence, no stronger:

1. **Payload reduction:** exact bytes or tokenizer-specific preflight tokens decreased.
2. **Estimated input-cost reduction:** pricing metadata was applied to preflight input tokens; no paired provider baseline exists.
3. **Observed usage:** optimized provider usage was recorded, but there is no paired baseline.
4. **Paired observed delta:** baseline and optimized usage exist for the same case under the paired protocol; the signed result may be positive, zero, or negative.
5. **Quality-bounded paired saving:** the paired delta is positive and the pre-registered quality non-inferiority criterion passed.
6. **End-to-end workflow saving:** quality-bounded paired saving holds over complete multi-turn workflows, including retries, tools, cache, and optimizer overhead.

For levels 1–3, a UI may report the supported local reduction or observed usage,
but it must say “provider saving not yet verified” or “no paired saving
measurement.” It must not replace a real local reduction with “no measured
saving” merely because a paired observation is absent. The receipt should also
explain whether optimization was skipped, applied but unobserved, or observed
without a baseline.

## Release gate

A default policy is release-ready only when:

- golden and property tests pass;
- all reversible engines pass exact reconstruction tests;
- malformed and adversarial corpora pass bounded-resource tests;
- adapter round trips preserve unknown fields;
- cache-prefix claims are digest-verified;
- no hard structural or protected-artifact regression exists;
- no-op and rollback reasons are covered;
- the frozen paired benchmark meets its pre-registered quality margin;
- full raw results, configuration, and environment manifests can be reproduced.

A high compression ratio cannot waive these gates.

## Required result artifacts

Each published run should contain:

- run manifest and timestamp;
- source revision and dirty-state marker;
- target model/deployment revision;
- tokenizer and price-sheet versions;
- hardware and operating system;
- policy and engine versions;
- randomized pair order and seeds;
- per-case preflight receipts;
- provider usage observations;
- raw model outputs or secure hashes where redistribution is restricted;
- quality scores and scorer versions;
- exclusions and failure ledger;
- aggregate tables and the script that generated them.

See [benchmarks/README.md](../benchmarks/README.md) for the repository layout.
