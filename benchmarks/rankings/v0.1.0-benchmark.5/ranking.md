# Preservation-gated optimizer ranking: v0.1.0-benchmark.5

This ranking compares optimizer **configurations**, not projects in the abstract. It ranks prompt/context and tool-output workloads separately. Results are specific to this authored corpus and its fixture-preservation checks; they do not establish target-model quality, provider cost savings, or a universal ‘best optimizer’ claim.

## What the measurements mean

**Raw token reduction** is the signed aggregate over successfully completed cases:

```text
100 × (Σ completed-case input tokens − Σ completed-case output tokens)
      / Σ completed-case input tokens
```

Failed and unsupported cases contribute no tokens to that numerator or denominator. The report therefore always shows completion and eligibility counts beside a raw percentage. A partial-run raw percentage is diagnostic, not evidence of full-surface performance.

**Qualified token reduction** is not a discounted or independently recalculated score. It equals the raw token reduction exactly when the public primary-ranking gate passes. Otherwise its value is `null`/`N/A`, and the configuration is excluded from the primary rank even though its diagnostic raw reduction remains visible.

An exclusion says that this exact configuration did not satisfy this corpus gate on this surface. It does not, by itself, claim that the optimizer can never preserve quality on another workload.

## How this release is tested

1. Freeze the corpus, configuration, source revisions, and tokenizer. This release contains 5 prompt/context cases and 4 tool-output cases.
2. Execute one optimizer configuration on every case assigned to its surface and retain the raw input, output, status, timing, and score artifacts.
3. Independently recount input and output tokens for every successfully completed case with the tokenizer recorded under Scope and provenance.
4. Compute the diagnostic raw aggregate from those completed cases only. Preserve failed and unsupported cases in the coverage ledger rather than treating them as zero-token successes.
5. Apply one composite fixture-preservation gate to each completed case. It requires every fixture-declared literal, semantic JSON equality where declared, and the exact benchmark query marker. URL, path, and number recall are diagnostics unless the fixture also declares those values as required literals.
6. Apply the public full-surface gate: every frozen case must be declared eligible and completed, failures must be zero, and every completed case's fixture gate must pass.
7. Copy the raw percentage unchanged into Qualified token reduction only after that gate passes; otherwise emit `null`/`N/A` and show the row only in the diagnostic excluded table.

The release summary has a narrower, source-level qualification field: it checks completion and fixture gates over the optimizer's *declared eligible cases*. The public ranking deliberately strengthens that rule to every frozen corpus case on the surface, preventing an optimizer from improving rank by declaring hard cases unsupported. The fixture gate catches specified corruption; it is not a downstream target-model or task-native quality evaluation.

## Primary ranking rules

A configuration enters a primary table only when it completed every corpus case on that surface with zero failures and every composite fixture-preservation gate passed. Rows are ordered by higher qualified token reduction and then fewer completed cases with zero token delta. Configurations tied on both share a standard competition rank (for example, 1, 2, 2, 4); optimizer ID and setting control display order within that shared rank. Latency is diagnostic only and never affects rank.

Configured keep-rate arms are labeled `configured-target-rate`; their achieved reduction must not be read as automatic rate selection. GPT-2 substitutions for canonical LLMLingua/LongLLMLingua checkpoints are labeled `noncanonical-feasibility-model`. Only Kendr’s shipped `default` arm can enter a primary table; Kendr engineering profiles appear separately.

## Prompt and context surface

Full-surface coverage is 5 cases.

### Primary qualified ranking

| Rank | Optimizer / setting | Qualified token reduction | Coverage | Cases passing fixture gate | Zero-token-delta cases | Median latency (ms) | Labels |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | KendrOptimizer — `default` | 71.64% | 5/5 | 5/5 | 1/5 | 221.194 | — |
| 2 | LLMLingua (GPT-2 feasibility profile) — `target-50` | 64.44% | 5/5 | 5/5 | 1/5 | 4734.272 | `configured-target-rate`, `noncanonical-feasibility-model` |
| 3 | Headroom (structural routers only) — `structural-target-50` | 38.57% | 5/5 | 5/5 | 3/5 | 18.139 | `configured-target-rate`, `structural-only` |
| 4 | OmniRoute deterministic stack — `rtk-standard+caveman-full` | 1.54% | 5/5 | 5/5 | 2/5 | 0.008 | — |
| 5 | Headroom (structural routers only) — `structural-default` | 0.00% | 5/5 | 5/5 | 5/5 | 3.325 | `structural-only` |

### Diagnostic raw reductions - excluded from primary ranking

These percentages remain visible for diagnosis, but are not mixed into the primary ranking. A high raw reduction cannot compensate for missing cases or failed fixture-preservation gates.

| Optimizer / setting | Diagnostic raw token reduction | Full-surface coverage | Cases passing fixture gate | Excluded because | Labels |
| --- | ---: | ---: | ---: | --- | --- |
| Headroom (Kompress + structural) — `kompress-target-50` | 60.03% | 5/5 | 3/5 | fixture-preservation gate failed (3/5 completed cases passed) | `configured-target-rate` |
| LongLLMLingua (GPT-2 feasibility profile) — `target-50` | 48.89% | 1/5 | 1/1 | full-surface coverage failed (1/5 cases completed, 1/5 declared eligible, 0 failed) | `configured-target-rate`, `noncanonical-feasibility-model` |
| LLMLingua-2 small — `target-50` | 43.10% | 5/5 | 0/5 | fixture-preservation gate failed (0/5 completed cases passed) | `configured-target-rate` |

### Pass-through baseline reference

| Optimizer / setting | Raw token reduction | Coverage | Cases passing fixture gate | Zero-token-delta cases | Labels |
| --- | ---: | ---: | ---: | ---: | --- |
| Unoptimized pass-through — `none` | 0.00% | 5/5 | 5/5 | 5/5 | — |

### Kendr development diagnostics — excluded from ranking

These profiles are retained for engineering comparison and cannot increase Kendr’s primary position.

| Optimizer / setting | Raw token reduction | Coverage | Cases passing fixture gate | Zero-token-delta cases | Labels |
| --- | ---: | ---: | ---: | ---: | --- |
| KendrOptimizer — `safe-low-threshold` | 71.64% | 5/5 | 5/5 | 1/5 | `kendr-development-diagnostic` |

## Command and tool output surface

Full-surface coverage is 4 cases.

### Primary qualified ranking

| Rank | Optimizer / setting | Qualified token reduction | Coverage | Cases passing fixture gate | Zero-token-delta cases | Median latency (ms) | Labels |
| ---: | --- | ---: | ---: | ---: | ---: | ---: | --- |
| 1 | KendrOptimizer — `default` | 61.38% | 4/4 | 4/4 | 1/4 | 205.132 | — |

### Diagnostic raw reductions - excluded from primary ranking

These percentages remain visible for diagnosis, but are not mixed into the primary ranking. A high raw reduction cannot compensate for missing cases or failed fixture-preservation gates.

| Optimizer / setting | Diagnostic raw token reduction | Full-surface coverage | Cases passing fixture gate | Excluded because | Labels |
| --- | ---: | ---: | ---: | --- | --- |
| RTK — `documented filter per fixture` | 97.27% | 4/4 | 0/4 | fixture-preservation gate failed (0/4 completed cases passed) | — |
| OmniRoute deterministic stack — `rtk-standard+caveman-full` | 71.45% | 4/4 | 1/4 | fixture-preservation gate failed (1/4 completed cases passed) | — |
| Headroom (Kompress + structural) — `kompress-target-50` | 21.58% | 4/4 | 2/4 | fixture-preservation gate failed (2/4 completed cases passed) | `configured-target-rate` |
| Headroom (structural routers only) — `structural-default` | 17.26% | 4/4 | 3/4 | fixture-preservation gate failed (3/4 completed cases passed) | `structural-only` |
| Headroom (structural routers only) — `structural-target-50` | 17.26% | 4/4 | 3/4 | fixture-preservation gate failed (3/4 completed cases passed) | `configured-target-rate`, `structural-only` |

### Pass-through baseline reference

| Optimizer / setting | Raw token reduction | Coverage | Cases passing fixture gate | Zero-token-delta cases | Labels |
| --- | ---: | ---: | ---: | ---: | --- |
| Unoptimized pass-through — `none` | 0.00% | 4/4 | 4/4 | 4/4 | — |

### Kendr development diagnostics — excluded from ranking

These profiles are retained for engineering comparison and cannot increase Kendr’s primary position.

| Optimizer / setting | Raw token reduction | Coverage | Cases passing fixture gate | Zero-token-delta cases | Labels |
| --- | ---: | ---: | ---: | ---: | --- |
| KendrOptimizer — `extractive-tool-output` | 61.38% | 4/4 | 4/4 | 1/4 | `kendr-development-diagnostic` |
| KendrOptimizer — `safe-low-threshold` | 61.38% | 4/4 | 4/4 | 1/4 | `kendr-development-diagnostic` |

## Scope and provenance

- Source release: [`releases/v0.1.0-benchmark.5`](../../../releases/v0.1.0-benchmark.5/README.md)
- Summary SHA-256: `7b11acd6a887a0f4baa7a0846567942f7054c00ee52f7c6c45aa75c8788b8004`
- Corpus SHA-256: `629e42b1d8a3246f0e427dc280cad70c4bc677c3a282982a17704349052e435f`
- Release SHA256SUMS SHA-256: `8efc6078a959611dec2122654f76b8b0dd3da88da6e2e6c1f4a79bba7a27a73c`
- Summary and corpus hashes verified against the release checksum manifest: `true`
- Tokenizer: `tiktoken o200k_base 0.12.0`
- Target model executed: `false`
- Paired provider usage observed: `false`
- Caveman’s generation-policy snapshot is a different evaluation surface and is not inserted into either optimizer ranking.

Reproduce the files from the repository root:

```powershell
python benchmarks/runners/rank_release.py --release releases/v0.1.0-benchmark.5 --output benchmarks/rankings/v0.1.0-benchmark.5
python benchmarks/runners/rank_release.py --release releases/v0.1.0-benchmark.5 --output benchmarks/rankings/v0.1.0-benchmark.5 --check
```
