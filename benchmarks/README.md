# Kendr Optimizer benchmarks

The benchmark suite exists to test cost reduction and downstream quality together. It is not a collection of hand-picked compression examples.

The normative protocol is [docs/benchmark-methodology.md](../docs/benchmark-methodology.md). The research and licensing rules are in [docs/provenance.md](../docs/provenance.md).

The machine-readable [competitor registry](competitors.json) records scope,
license, official source, and audited revisions without making any competitor a
production dependency. A null revision means that entry was catalogued but not
executed; the release scope ledger states why.

The current local publication bundle is
[`releases/v0.1.0-benchmark.5`](../releases/v0.1.0-benchmark.5/README.md). It
contains complete raw input/output artifacts, command logs, exact locks,
independently recounted summaries, a minimal source/workflow snapshot, and
139 verified checksum entries. It is a payload experiment, not a provider-cost
or downstream-quality leaderboard. Earlier draft runs are intentionally absent
from the public source tree.

The reproduction snapshot preserves the exact source executed by the release.
It is build-tested and checksum-addressed; it is not rewritten after execution
to follow later source-tree formatting changes.

## Reading the preservation-gated ranking

The published ranking uses three deliberately different concepts:

- **Raw token reduction** is the signed aggregate reduction over successfully
  completed cases. With `I` as the sum of completed-case input tokens and `O`
  as the sum of completed-case output tokens, it is
  `100 × (I - O) / I`. Failed and unsupported cases do not enter either token
  sum, so coverage counts must always be read beside this number.
- A **fixture-preservation gate** passes one completed case only when every
  fixture-declared required literal remains exact, the primary payload is
  semantically equal JSON where the fixture requires JSON equivalence, and the
  exact benchmark query marker remains present. URL, path, and number recall
  are reported as diagnostics; they become hard requirements only when the
  fixture declares those values as required literals.
- **Qualified token reduction** is exactly the raw token reduction, unchanged,
  when the public full-surface gate passes. Otherwise it is `null`/`N/A`, and
  the configuration is excluded from the primary rank. It is not a weighted,
  discounted, or separately calculated score.

There are two qualification scopes. The immutable release summary first checks
an optimizer's *declared eligible cases*: all of those cases must complete and
pass their fixture gates. The derived public ranking is stricter. A
configuration must declare eligible for and complete every frozen case on the
surface - currently 5 prompt/context cases or 4 tool-output cases - with zero
failures, and every completed case must pass its fixture gate. This prevents a
configuration from improving its public rank by declaring difficult cases
unsupported.

The test and publication flow is:

1. Freeze the authored corpus, optimizer settings, source revisions, and
   tokenizer.
2. Run one configuration on every case assigned to its surface, retaining raw
   input, output, status, timing, and scoring evidence.
3. Independently recount tokens for successfully completed cases and compute
   the diagnostic raw aggregate. Keep failures and unsupported cases visible in
   the coverage ledger rather than inserting them as zero-token results.
4. Apply the composite fixture-preservation gate to each completed case.
5. Apply the public full-surface, zero-failure, all-fixture-gates-pass rule.
6. Rank only passing configurations. Publish other raw percentages in
   **Diagnostic raw reductions - excluded from primary ranking**, with concrete
   completion, eligibility, failure, and fixture-gate counts.

These fixture checks detect specified corruption; they are not a downstream
target-model or task-native quality evaluation. An excluded row means that the
exact configuration did not satisfy this corpus gate on this surface. It is not
a universal judgment about the optimizer.

## Scope

The suite will support four kinds of run:

- transform microbenchmarks for individual engines;
- complete-envelope benchmarks for planner and receipt behavior;
- paired target-model evaluations;
- paired multi-turn agent or tool workflows.

Every paired evaluation compares optimization with pass-through on the same case. A preflight token delta by itself is not a measured cost saving.

## Layout

```text
benchmarks/
  competitors.json             Peer registry and provenance
  configs/                     Peer locks and scope ledger
  corpus/authored/v1/          Authored fixtures and corpus builder
  runners/                     Execution, sanitization, verification, ranking
  rankings/<release>/          Derived Markdown, JSON, and CSV ranking
releases/<release>/            Immutable raw evidence bundle
```

Future licensed corpora, task-native scorers, and schemas should receive their
own directories when implemented; this README does not imply that unshipped
components already exist. Generated environments, cloned peers, model weights,
and caches live under ignored `benchmarks/.cache/` and never belong in a release
or source archive.

Generated or sensitive results should remain ignored by default until their redistribution and privacy status is reviewed.

## Case manifest requirements

Each case must declare:

- stable identifier and version;
- source and license;
- workload family and content type;
- optimization phase;
- unoptimized envelope or a reproducible generator;
- target and tokenizer profile;
- cache condition;
- protected artifacts and structural invariants;
- scorer and success threshold;
- maximum turns, retries, and timeout;
- whether the case was used for development or held out.

Do not add customer traces, API keys, private source code, personal data, or secrets.

## Run manifest requirements

Each run must pin:

- Kendr Optimizer source revision and engine versions;
- build profile and feature flags;
- policy;
- host adapter;
- target model or local checkpoint;
- tokenizer;
- generation parameters and seeds;
- hardware and operating system;
- cache state;
- provider price sheet and currency date;
- randomized pair order;
- competitor versions and configurations;
- scorer and judge versions.

If the worktree is dirty, record it. If a hosted model revision cannot be pinned, record the deployment name, provider, region when relevant, and run timestamp.

## Minimum outputs

For every case and arm, retain:

- the original envelope digest;
- the transformed envelope digest;
- complete kendr.receipt/v1 preflight receipt;
- optimizer latency and resource measurements;
- provider or local-model usage;
- generated output or a permitted digest;
- task score and terminal success;
- cache events;
- retries and recovery operations;
- error, timeout, no-op, rejection, and rollback records.

For a paired case, retain the baseline-to-optimized pairing key and order. Aggregate reports must include negative savings and failures.

## Competitor policy

Competitor runners are benchmark dependencies, never production dependencies. They live outside the optimizer core and must:

- install a pinned upstream release or commit;
- preserve upstream notices and comply with its license;
- use documented settings;
- disclose any patch or conversion;
- retain unsupported and failed cases;
- report the content layer each competitor actually optimizes.

Do not combine percentages quoted by Headroom, RTK, Caveman, LLMLingua, LongLLMLingua, Selective Context, RECOMP, or PCToolkit. Run the same corpus and measure the full paired workload.

## Contribution checklist

Before submitting a new benchmark case or report:

- verify source and redistribution rights;
- remove secrets and personal data;
- add or update the case manifest;
- use a task-native quality scorer;
- test pass-through first;
- preserve signed deltas and failures;
- separate exact token counts from estimates;
- include optimizer overhead and retries;
- document whether prompt caching was cold or warm;
- regenerate reports from raw artifacts;
- avoid universal “same quality” or “best” language.

## Result interpretation

Use precise labels:

- payload reduction for byte or preflight-token changes;
- estimated input-cost reduction when pricing is applied without paired provider usage;
- observed usage when only the optimized arm was observed;
- paired observed delta when both comparable arms exist;
- paired observed saving only when that signed net delta is positive;
- quality-bounded paired saving only when the positive delta also passes the registered non-inferiority gate;
- end-to-end workflow saving only when the full multi-turn task is included.

“No measured saving” is appropriate when a measured local or paired delta is
zero or negative. When the transform applied locally but no paired baseline
exists, say “provider saving not yet verified” or “no paired saving
measurement.” The report should state whether a transform was a no-op,
rejected, rolled back, applied but unobserved, or observed without a baseline
instead of hiding the case.
