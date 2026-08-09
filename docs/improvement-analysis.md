# Optimizer improvement analysis

Date: 2026-08-07

This note compares the pre-improvement default measured during development
(figures retained below) with the canonical, publication-sanitized peer run in
[`v0.1.0-benchmark.5`](../releases/v0.1.0-benchmark.5/report.md).

## Outcome

The strategy change worked on the authored nine-case corpus:

| Surface | Old output / input | Old reduction | New output / input | New reduction | Preservation proxy | Positive cases |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Prompt/context | 6,358 / 6,358 | 0.00% | 1,803 / 6,358 | 71.64% | 5 / 5 | 4 / 5 |
| Tool output | 6,915 / 12,569 | 44.98% | 4,854 / 12,569 | 61.38% | 4 / 4 | 3 / 4 |

The remaining no-ops are a 29-token unique prompt and a unique 120-entry git
history. Keeping those unchanged is preferable to inventing a favorable score
by deleting model-visible information.

## Why the earlier strategy stalled

The original pipeline was strongest on JSON and terminal cleanup. Most prompt
fixtures did not match an engine, repeated short paragraphs could cost more in
marker tokens than they saved, pytest output had no typed parser, and recovery
metadata was not an independent proof that omitted artifacts remained
represented to the model. The optimizer therefore skipped or rejected many
candidates correctly.

There was also a measurement-language problem. `verified_savings=false` means
that no paired provider-usage baseline with a non-regressing task result has
verified billing savings. It does not mean an applied transform has zero local
token reduction. A UI should report the signed preflight token delta and the
provider-verification state separately.

## Strategy implemented

The new approach is native, deterministic, and typed. Candidate acceptance is
fail-closed; a valid host request fails open to its original content:

- Exact repeated prefix blocks use one readable count marker instead of a
  marker per short paragraph. Candidate periods are paragraph boundaries and a
  linear-time Z algorithm avoids adversarial rescanning.
- Exact repeated paragraphs, profitable adjacent line runs, and runs of at
  least three adjacent sentences are represented by typed markers. Sentence
  runs never cross lines or fenced Markdown regions.
- System, developer, and tool roles plus code, JSON, images, tool calls, and
  tool results are excluded from the context-repetition engine.
- A narrow pytest reducer folds only validated sequential `PASSED`, `SKIPPED`,
  and `XFAIL` ranges. Failures, errors, summaries, reasons, and edge lines stay
  visible.
- The verifier independently expands every changed typed part and requires the
  complete pre-transform envelope byte-for-byte. Protected-artifact checks are
  multiplicity-aware; a stored recovery copy alone cannot excuse a missing
  model-visible artifact.
- The optimizer measures the input only after an engine proposes a candidate,
  avoiding tokenizer work for engines that immediately decline a content type.

## Fresh peer comparison

The primary tables contain only Kendr's shipped default. Development profiles
are separated from headline comparisons.

| Prompt/context arm | Raw reduction | Qualified reduction | Cases passing composite fixture gate |
| --- | ---: | ---: | ---: |
| Kendr default | 71.64% | 71.64% | 5 / 5 |
| LLMLingua GPT-2 feasibility, target 50 | 64.44% | 64.44% | 5 / 5 |
| Headroom Kompress + structural, target 50 | 60.03% | N/A - excluded; gate passed on 3 / 5 cases | 3 / 5 |
| LLMLingua-2 small, target 50 | 43.10% | N/A - excluded; gate passed on 0 / 5 cases | 0 / 5 |
| Headroom structural, target 50 | 38.57% | 38.57% | 5 / 5 |
| OmniRoute RTK + Caveman | 1.54% | 1.54% | 5 / 5 |

| Tool-output arm | Raw reduction | Qualified reduction | Cases passing composite fixture gate |
| --- | ---: | ---: | ---: |
| RTK documented fixture filters | 97.27% | N/A - excluded; gate passed on 0 / 4 cases | 0 / 4 |
| OmniRoute RTK + Caveman | 71.45% | N/A - excluded; gate passed on 1 / 4 cases | 1 / 4 |
| Kendr default | 61.38% | 61.38% | 4 / 4 |
| Headroom structural | 17.26% | N/A - excluded; gate passed on 3 / 4 cases | 3 / 4 |

This makes Kendr the highest public-ranking-qualified arm in both tracks in this release.
It does not establish a universal best optimizer: the corpus is small, several
peer arms use configured target rates or noncanonical small models, and the
preservation checks are proxies rather than downstream task evaluation.

## What remains before a product claim

1. Run randomized optimizer-on/off trials through target models and score task
   success, exact constraints, retries, correction turns, latency, and total
   provider usage.
2. Keep unique git history as a no-op under the safe default. Any query-aware
   git/build/diff extraction should be a separately evaluated higher-risk
   engine, not a benchmark-specific lossless claim.
3. Add broader parser-first engines for test runners, build output, diagnostics,
   tables, and diffs using the same typed-expansion discipline where possible.
4. Give harness adapters a protected recovery lifecycle before enabling
   `recoverable` engines. Existing adapters that cap risk at
   `representation_safe` remain compatible with the contract but will
   intentionally bypass these new reducers.
5. In KendrWeb, show local preflight reduction separately from paired provider
   verification. Do not turn an independently recounted token delta into a
   verified cost-saving claim.

## Evidence integrity

The `.5` release contains 13 raw run files. All 17 executable attempts in its
ledger exited successfully, and all 139 entries in `SHA256SUMS` were verified
after assembly. Earlier working releases and a failed linker preflight are
retained outside the public source tree as local, recoverable development
evidence; they are not publication releases.
