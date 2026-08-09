# Measurement, Savings, and Receipts

## Why this distinction matters

Token optimization projects often report a percentage obtained by comparing characters or tokenizer counts before and after a transformation. That number can be useful, but it is not automatically the customer's bill reduction and says nothing by itself about task quality.

KendrOptimizer uses an evidence ladder. Documentation, APIs, dashboards, and benchmark reports should use the strongest term supported by the available evidence and no stronger.

## Vocabulary

### Byte reduction

The signed difference between the serialized normalized envelope before and after optimization:

```text
byte_delta = original_bytes - optimized_bytes
```

This is reproducible locally. It is not a token or cost measurement.

### Local token reduction

The signed difference measured by a named local tokenizer or approximation over KendrOptimizer's serialized normalized envelope:

```text
token_delta = original_local_tokens - optimized_local_tokens
```

Positive means the transformed normalized envelope is smaller under that measurement. Negative means it inflated. Deltas must never be saturated to zero.

### Estimated input cost reduction

The local token delta multiplied by host-supplied input pricing:

```text
estimated_input_cost_reduction =
    token_delta * input_price_per_million / 1,000,000
```

This is a planning estimate. It does not include provider framing, cache categories, output, reasoning, retries, corrections, or optimizer compute unless explicitly modeled.

### Observed optimized usage

Usage returned by the provider or gateway for the optimized run. It proves what that run consumed, but not what the same run would have consumed without optimization.

### Paired observed delta

The signed difference between comparable provider-reported baseline and optimized runs. A pair can show a reduction, no change, or an increase. Only a positive net delta can be called a saving.

### Quality-adjusted savings

Paired cost or token reduction conditioned on the optimized run remaining non-inferior on task-success and safety metrics. This is the real release objective. The pre-alpha `observe` contract applies a single binary task-success gate; it does not perform a statistical non-inferiority evaluation.

## Current preflight measurement

The core serializes the complete `ContentEnvelope` with `serde_json` and measures that UTF-8 JSON. It also records:

- serialized byte length;
- tokenizer identity;
- measurement-confidence enum; and
- SHA-256 of the serialized envelope.

Current tokenizer profiles are:

| Profile | Current implementation | Receipt confidence |
| --- | --- | --- |
| `approximate` | Maximum of one token per three Unicode scalar values and whitespace-delimited lexical floor | `conservative_estimate` |
| `cl100k_base` | `tiktoken-rs` encoding of normalized-envelope JSON | `exact_tokenizer` |
| `o200k_base` | `tiktoken-rs` encoding of normalized-envelope JSON | `exact_tokenizer` |

`exact_tokenizer` means exact execution of that tokenizer over KendrOptimizer's serialization. It does **not** mean exact provider-billed input. A provider may serialize roles, tools, images, schemas, cache controls, and metadata differently, add hidden framing, or use a tokenizer revision not represented here.

The approximate profile is intentionally labeled, but “conservative” is not a formal upper bound for every language or tokenizer. It is a heuristic and must not be presented as exact.

## Current acceptance gates

For each engine candidate:

```text
candidate_delta = before_tokens - after_tokens
candidate_gain_percent = 100 * candidate_delta / before_tokens
```

Both `min_gain_tokens` and `min_gain_percent` must pass. After all sequentially accepted candidates, the same test is applied to the whole envelope. If the portfolio falls below the threshold, all changes are reverted.

This has two important consequences:

- A correct “no measured saving” result is expected for short or already compact content.
- Engine execution does not imply saving. Each attempted engine can report `no_candidate`, `rejected`, `reverted`, `timed_out`, `shadow`, or `applied`.

Current timing is wall-clock microseconds measured around the pipeline and each proposal. The global latency budget is checked before each engine. It is not a hard interrupt for a currently running engine.

## Receipt interpretation

`OptimizationReceipt` currently includes:

- request and receipt schema identity;
- outcome status;
- original and optimized token measurements;
- signed token and byte deltas;
- estimated input reduction percentage;
- optional local input-cost estimate;
- cache impact;
- total optimization latency;
- every dispatched engine attempt and its verification checks;
- warnings; and
- explicit no-op reason.

Outcome statuses mean:

| Status | Returned content | Measurement meaning |
| --- | --- | --- |
| `applied` | Optimized content | Preflight delta describes returned normalized content |
| `skipped` | Original content | No candidate cleared all gates |
| `shadow` | Original content | `optimized` measurement and delta are hypothetical |
| `reverted` | Original content | A candidate sequence existed but whole-envelope acceptance failed |

`verified_savings` is always false on the preflight optimization receipt. This is deliberate.

That flag must not erase the preflight result in a UI. When `status=applied` and
`token_delta=13`, show **"13 input tokens reduced (preflight, o200k_base)"** and
then a separate evidence label such as **"Provider saving not yet verified."**
Use **"No preflight reduction"** only when the signed local delta is zero or
negative. Use **"No paired saving measurement"** when the transform applied but
no comparable baseline/provider observation exists.

The current input-cost estimate uses only `input_per_million`. Although the contract can carry cached-input and output prices, preflight cost estimation does not yet use them. It also cannot know whether a changed prefix will trigger a new cache write. `CacheImpact` is currently limited to declared message/tool segment touches and should be treated as coarse.

## Current `observe` behavior

The host can submit `UsageObservation` containing optimized provider usage and, optionally, a paired baseline. Without a baseline, the result is unverified and reports no deltas.

When a baseline is supplied, current code:

- marks `paired_baseline_supplied` true;
- subtracts optimized input tokens from baseline input tokens;
- subtracts optimized output tokens from baseline output tokens;
- subtracts total monetary cost only when both runs supply the same currency;
- compares the optional binary `task_success` signals;
- requires the optimized task to report success; and
- marks `verified` true only when that quality gate passes and the pair shows a
  positive comparable-cost reduction, or otherwise a positive combined
  input-plus-output token reduction.

This is an intentionally minimal primitive, not a complete experiment validator. It currently does **not** verify:

- that both runs used the same request, model, provider, parameters, tools, cache state, or pricing period;
- that baseline and optimized runs were randomized or repeated;
- task-native quality or statistical non-inferiority beyond the binary `task_success` signal;
- that cached input was categorized consistently;
- reasoning-token deltas;
- retry/correction turns;
- statistical uncertainty; or
- receipt/request digest linkage beyond the caller-supplied request ID.

Until these checks are added, callers bear responsibility for supplying a legitimate pair. A future contract should distinguish `paired_data_supplied` from `experiment_verified` and `quality_adjusted_non_inferior`.

## Output billing truth

Provider output tokens are billed while the model generates them. Once the response exists, deleting words or rewriting it cannot retroactively lower the output-token charge for that call.

There are only three honest output-related savings mechanisms:

1. **Pre-generation control.** Before the provider call, the host applies a semantic generation policy, such as an explicit answer budget, supported verbosity setting, stop condition, or concise-answer instruction. Any instruction overhead and cache impact must be counted.
2. **Avoided future input.** The host compacts a completed response before including it in later context. This may lower future input usage, not the completed call's output bill.
3. **Avoided generation through task design.** The host does not request unnecessary variants or retries. This is orchestration behavior outside the core optimizer unless supplied as an explicit policy decision.

The current optimizer treats phase `output_observation` as a no-op and directs the caller to `observe`. Its opt-in pre-call generation controller can recommend host-native limits/verbosity or a short instruction after intent and break-even gates. The estimate is heuristic, is returned separately from measured input savings, and always has `verified_savings: false`. The core does not rewrite streamed output.

Any estimate of output savings from a pre-generation policy must remain unverified unless an equivalent baseline generation was actually observed. The output delta in a paired run can also reflect normal model randomness, so repeated trials are required for strong claims.

## Cache accounting

Caching can dominate economics. Reducing raw input while changing a stable prefix can cost more if it converts cheap cache reads into expensive writes or uncached input.

Current behavior:

- The host may declare cache segments by message ID.
- With `preserve_cache_prefix` enabled, a candidate touching one of those messages is rejected.
- Any tool-surface change is treated as touching the protected prefix when cache segments exist.
- Receipts report `prefix_preserved`, `invalidated`, `none`, or `unknown` using coarse logic.

Current limitations:

- Cache ordering and prefix boundaries are not modeled at provider-serialization level.
- Cache-write tokens and prices are not represented separately from cached input.
- Provider TTL, minimum cache size, ephemeral cache controls, and partial prefix matches are not modeled.
- A host that declares no segments receives `none`, not proof that caching was irrelevant.

Planned measurement should accept actual serialized cache regions and prices, then compute a counterfactual cost by category. When those inputs are absent, cache savings remain unknown.

## Recommended paired evaluation protocol

For credible optimizer comparisons:

1. Freeze the source request, tool surface, model, provider, parameters, and evaluator version.
2. Record the canonical normalized envelope and adapter/provider serialization digests.
3. Randomize baseline and optimized ordering when provider cache state permits.
4. Use fixed seeds where supported, but still run repetitions because determinism is not guaranteed.
5. Record provider-reported input, cache-read, cache-write, output, reasoning, total cost, latency, retries, and errors.
6. Evaluate task success with deterministic checks first: exact answers, JSON/schema validation, tool-call correctness, citations, code tests, and explicit constraints.
7. Use blinded human or model judging only where deterministic evaluation is insufficient, and report judge identity and uncertainty.
8. Treat retries and user corrections as part of the optimized workload's cost.
9. Report distributions and confidence intervals, not only averages.
10. Publish negative, inflationary, and no-op cases.

A release should pass a workload-specific non-inferiority threshold, not merely save tokens on average. A large saving on logs must not conceal tool-selection failures or loss of exact facts on other workloads.

## Metrics by optimization surface

### Tool definitions

- submitted tool-schema tokens;
- tool-selection recall and precision;
- correct first tool-call rate;
- retry-with-full-tools rate;
- dangerous or impossible tool-call rate; and
- end-task success.

### Tool results

- source and optimized tokens;
- diagnostic, numeric, identifier, and source-location recall;
- exact recovery success;
- downstream diagnosis/task success; and
- added latency.

### Conversation history

- retained dependency and constraint recall;
- exact quote/citation recall;
- contradiction and correction rate;
- recovery lookups; and
- later-turn task success.

### Generation policy

- instruction/control overhead;
- provider-reported output delta;
- completeness and requested-format adherence;
- truncation and follow-up rate; and
- total cost across the complete task, not one response.

## Planned evidence model

Receipts should become two-stage records:

### Preflight receipt

- canonical input and output digests;
- tokenizer/serializer versions;
- signed per-region deltas;
- candidate and portfolio decisions;
- estimated cache and monetary effects with confidence;
- verification evidence;
- risk/recovery declarations; and
- optimization compute/latency.

### Final observation

- provider-reported optimized usage;
- paired baseline metadata where present;
- comparable-run validation;
- quality and safety outcomes;
- retries/corrections;
- actual total-cost delta; and
- evidence state: estimated, observed-unpaired, paired, or quality-adjusted verified.

Reasoning tokens, cache-write tokens, multiple currencies/pricing revisions, and host-level serialization should be explicit rather than hidden in a generic total.

## Reporting rules

- Say “local token reduction” for preflight counts.
- Say “estimated input cost reduction” only when a price was supplied.
- Say “observed usage” for one provider run.
- Say “paired observed delta” for any comparable baseline/optimized pair, and “paired observed saving” only when that net delta is positive.
- Say “quality-adjusted savings” only after non-inferiority checks.
- Never combine percentages from different optimization surfaces by multiplication unless the same end-to-end workload measurement mathematically supports it.
- Never report “up to” without the distribution, workload, policy, model, and version.
- Never hide negative or zero-saving decisions.

The project's credibility depends more on these distinctions than on a headline compression ratio.
