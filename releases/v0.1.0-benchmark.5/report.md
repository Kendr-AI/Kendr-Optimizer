# KendrOptimizer peer benchmark report

Generated: 2026-08-07T19:57:36.252960+00:00

## What this run establishes

This is a local, provider-neutral **payload reduction** experiment over an authored nine-case corpus. It independently recounts visible text with `tiktoken o200k_base`, retains complete inputs and outputs, and checks exact literals plus applicable JSON structure. It does **not** call a target LLM, observe a provider bill, or establish downstream answer quality. Therefore it does not support a “measured cost saving,” “same quality,” or “best optimizer” claim.

Configured 50% keep-rate arms are labeled `target-50`; their reduction is a requested operating point, not an optimizer discovering the ideal amount. The two primary peer tables contain exactly one Kendr arm: the shipped `default` policy. Development-only Kendr profiles are kept in a separate diagnostic appendix and never raise the headline result.

Headroom's structural-only rows keep its learned model disabled and are labeled accordingly. The separate `kompress-target-50` arm warms the pinned Kompress-v2-base ONNX model from an offline cache and includes Headroom's structural routers. OmniRoute is invoked through its pure RTK-then-Caveman modules; its gateway, routing, and provider features are not started.

## Prompt and context track

| Optimizer / setting | Completed / eligible | Input tokens | Output tokens | Raw payload reduction | Proxy-qualified reduction | Preservation proxy | No-op cases | Failures |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Headroom (Kompress + structural) — `kompress-target-50` | 5 / 5 | 6,358 | 2,541 | 60.03% | Not qualified | 3 / 5 | 1 / 5 | 0 |
| Headroom (structural routers only) — `structural-default` | 5 / 5 | 6,358 | 6,358 | 0.00% | 0.00% | 5 / 5 | 5 / 5 | 0 |
| Headroom (structural routers only) — `structural-target-50` | 5 / 5 | 6,358 | 3,906 | 38.57% | 38.57% | 5 / 5 | 3 / 5 | 0 |
| KendrOptimizer — `default` | 5 / 5 | 6,358 | 1,803 | 71.64% | 71.64% | 5 / 5 | 1 / 5 | 0 |
| LLMLingua (GPT-2 feasibility profile) — `target-50` | 5 / 5 | 6,358 | 2,261 | 64.44% | 64.44% | 5 / 5 | 1 / 5 | 0 |
| LLMLingua-2 small — `target-50` | 5 / 5 | 6,358 | 3,618 | 43.10% | Not qualified | 0 / 5 | 0 / 5 | 0 |
| LongLLMLingua (GPT-2 feasibility profile) — `target-50` | 1 / 1 | 1,843 | 942 | 48.89% | 48.89% | 1 / 1 | 0 / 1 | 0 |
| OmniRoute deterministic stack — `rtk-standard+caveman-full` | 5 / 5 | 6,358 | 6,260 | 1.54% | 1.54% | 5 / 5 | 2 / 5 | 0 |
| Unoptimized pass-through — `none` | 5 / 5 | 6,358 | 6,358 | 0.00% | 0.00% | 5 / 5 | 5 / 5 | 0 |

## Command and tool-output track

| Optimizer / setting | Completed / eligible | Input tokens | Output tokens | Raw payload reduction | Proxy-qualified reduction | Preservation proxy | No-op cases | Failures |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| Headroom (Kompress + structural) — `kompress-target-50` | 4 / 4 | 12,569 | 9,857 | 21.58% | Not qualified | 2 / 4 | 2 / 4 | 0 |
| Headroom (structural routers only) — `structural-default` | 4 / 4 | 12,569 | 10,400 | 17.26% | Not qualified | 3 / 4 | 3 / 4 | 0 |
| Headroom (structural routers only) — `structural-target-50` | 4 / 4 | 12,569 | 10,400 | 17.26% | Not qualified | 3 / 4 | 3 / 4 | 0 |
| KendrOptimizer — `default` | 4 / 4 | 12,569 | 4,854 | 61.38% | 61.38% | 4 / 4 | 1 / 4 | 0 |
| OmniRoute deterministic stack — `rtk-standard+caveman-full` | 4 / 4 | 12,569 | 3,588 | 71.45% | Not qualified | 1 / 4 | 0 / 4 | 0 |
| Unoptimized pass-through — `none` | 4 / 4 | 12,569 | 12,569 | 0.00% | 0.00% | 4 / 4 | 4 / 4 | 0 |
| RTK — `documented filter per fixture` | 4 / 4 | 12,569 | 343 | 97.27% | Not qualified | 0 / 4 | 0 / 4 | 0 |

RTK appears only here because it transforms command output. Prompt compressors are not assigned synthetic RTK scores. The Kendr extractive arm is opt-in and separate from its safe default.

## Kendr development diagnostics (not shipped-default comparisons)

| Optimizer / setting | Completed / eligible | Input tokens | Output tokens | Raw payload reduction | Proxy-qualified reduction | Preservation proxy | No-op cases | Failures |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| KendrOptimizer — `extractive-tool-output` | 4 / 4 | 12,569 | 4,854 | 61.38% | 61.38% | 4 / 4 | 1 / 4 | 0 |
| KendrOptimizer — `safe-low-threshold` | 5 / 5 | 6,358 | 1,803 | 71.64% | 71.64% | 5 / 5 | 1 / 5 | 0 |
| KendrOptimizer — `safe-low-threshold` | 4 / 4 | 12,569 | 4,854 | 61.38% | 61.38% | 4 / 4 | 1 / 4 | 0 |

These rows are retained for engineering diagnosis only. `safe-low-threshold` changes application gates, while `extractive-tool-output` enables a higher-risk generic reducer. Neither row represents the optimizer shipped by default.

## Generation-policy track: Caveman

The official committed snapshot contains 10 prompts. Remeasuring with o200k_base gives 2,045 terse-control output tokens and 1,026 Caveman output tokens: 49.83% additional output reduction. The skill adds 14,600 input tokens across those single turns, so the simple input-plus-output net versus terse is -13,581 tokens. Quality is unscored and these are upstream model outputs, not a fresh run on this machine.

Caveman changes future model generation; it does not compress an already-produced string. A valid fresh comparison needs randomized paired target-model runs, usage counters, and task-quality scoring.

## Unsupported and deliberately unranked peers

| Peer | Release status | Reason |
| --- | --- | --- |
| selective-context | `not_run_environment_incompatible` | The official 0.1.4 package hard-pins spaCy 3.2.0 and Click 8.0.4 and requires a Python 3.9-era isolated stack; this Windows host exposes only Python 3.11. No source patch was applied. |
| recomp | `not_run_environment_incompatible` | The official extractive runner hardcodes CUDA, pins an older Linux-oriented torch stack, and the published abstractive weights lack model-card license metadata. This Windows CPU run did not silently patch it. |
| pctoolkit | `catalogued_not_scored` | PCToolkit wraps other compression algorithms; ranking it as another optimizer would double-count those methods. |
| omniroute | `pure_optimizer_stack_executed` | Only the pinned RTK-then-Caveman transformation modules are executed. OmniRoute's gateway, routing, authentication, and provider paths remain out of scope. |
| caveman-compression | `catalogued_not_scored` | This separately named project is not the Caveman coding-agent skill requested for the generation-policy track and was not silently substituted for it. |
| caveman | `upstream_snapshot_remeasured_no_fresh_model_run` | A fresh Caveman A/B requires a target model and provider/local inference. No paid API call was authorized; the release remeasures the complete upstream committed snapshot and includes every output. |
| llmlingua-original-canonical | `substituted_feasibility_profile` | The canonical default is a 6.7B Llama checkpoint and is impractical on this CPU-only host. The algorithm is run with GPT-2 and labeled noncanonical. |
| longllmlingua-original-canonical | `substituted_feasibility_profile` | The same GPT-2 feasibility substitution is used and only the authored retrieved-document case is eligible. |

Failures and unsupported cases remain in each raw run. PCToolkit is a meta-harness around algorithms rather than another algorithm, so it is catalogued instead of double-counted.

Raw percentages are shown only when every predefined eligible case completed. A proxy-qualified percentage is shown only when coverage is complete **and every preservation proxy passes**. Large raw reductions that lose a required literal or JSON value remain visible but are explicitly not qualified. Incomplete arms display `N/A`; their successful-case deltas remain available in `results/summary.json` for diagnosis, not ranking.

## Reading the preservation column

"Preservation proxy" means every fixture-declared literal and the exact query survived and, for the JSON fixture, the primary payload remained value-equivalent JSON. Number, URL, and path recall are also reported per case for diagnosis; they qualify the aggregate only when declared as fixture requirements. These checks catch obvious corruption, but they are not a substitute for target-model or task-native quality evaluation.

Per-case latency is diagnostic only. Learned-model initialization is outside the LLMLingua case timer, while process startup is included for CLI-based arms, so this release does not compare optimizer latency across implementations.

## Reproduction and raw evidence

- `runs/` contains complete per-arm inputs, outputs, native metrics, timing, errors, and unsupported cases.
- `logs/` contains command stdout/stderr and the execution ledger.
- `evidence/` contains the exact corpus, peer lock, scope decisions, and harness compatibility lock.
- `results/summary.json` and `results/summary.csv` are derived only from the raw runs.
- `reproduction/` mirrors the minimal repository layout needed by the workflow; peer runtimes, cloned upstream repositories, and model weights remain external and are pinned in evidence.
- `SHA256SUMS` authenticates every release artifact except itself.

Run `python benchmarks/runners/execute_release.py --help` from the repository root to reproduce the workflow. Model caches and virtual environments intentionally remain outside the release; resolved versions and failures are recorded in the artifacts.
