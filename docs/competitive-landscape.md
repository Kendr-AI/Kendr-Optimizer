# Competitive landscape

Last reviewed: 2026-08-07

This document maps the prompt and context optimization landscape that informed KendrOptimizer. It is not a leaderboard, and upstream headline numbers are not treated as directly comparable. Each project operates at a different layer, uses different workloads, and defines “saving” differently.

KendrOptimizer is an independently implemented optimization engine. It is not an LLM gateway, a provider router, or a compatibility wrapper around the projects below. Its contract ends after returning an optimized payload, a risk decision, and an auditable receipt. The host remains responsible for model selection, provider authentication, network calls, retries, and billing.

## Comparison at a glance

| Project | Primary layer | Core mechanism | Learned model required | Typical integration | Main strength | Boundary or gap relevant to KendrOptimizer |
| --- | --- | --- | --- | --- | --- | --- |
| [Headroom](https://github.com/headroomlabs-ai/headroom) | Whole agent context | Content routing across JSON, code, prose, history, cache alignment, and reversible retrieval | Some modes do; deterministic content compressors also exist | Library, middleware, proxy, wrapper, MCP | Broad, content-aware coverage with a recovery path | Its product surface extends into proxying, cross-agent memory, and agent wrapping. KendrOptimizer deliberately exposes only transformation and observation contracts |
| [RTK](https://github.com/rtk-ai/rtk) | Developer command output | Command-aware parsers and filters for git, tests, builds, logs, and related CLI output | No | CLI command proxy or coding-agent hook | Fast, deterministic reduction of verbose shell output | It reduces the text emitted by supported commands, not every token in a model request or an end-to-end session |
| [Caveman](https://github.com/JuliusBrussee/caveman) | Model output style | Prompt instructions and local hooks that ask the model to answer tersely while protecting code, commands, and errors | No auxiliary model; the target LLM follows the style instruction | Agent skill, plugin, extension, or rules file | Low operational overhead and direct influence on future output length | It is primarily a generation policy, not an input-context compressor. Its own honest-numbers note shows that short sessions can be net-negative |
| [LLMLingua](https://github.com/microsoft/LLMLingua) | Prompt text | Small language-model perplexity, budget control, coarse context selection, and token-level pruning | Yes | Python library before inference | Aggressive learned prompt compression across long-form tasks | Adds local model cost and latency; lossy token removal requires downstream-quality evaluation and protected-region controls |
| [LLMLingua-2](https://aclanthology.org/2024.findings-acl.57/) | Prompt text | Bidirectional token classifier trained from data distilled by a larger model | Yes | Python library before inference | Faster, task-agnostic token classification compared with the original LLMLingua approach | Still lossy and model-backed; general benchmark quality does not prove safety for code, schemas, credentials, or tool causality |
| [LongLLMLingua](https://aclanthology.org/2024.acl-long.91/) | Long, query-focused context | Question-aware coarse ranking, document reordering, and fine-grained compression | Yes | Python library in long-context or RAG flows | Explicitly addresses long-context relevance and “lost in the middle” behavior | Query-aware RAG assumptions do not transfer automatically to arbitrary agent history or structured tool traffic |
| [Selective Context](https://github.com/liyucheng09/Selective_Context) | Long prompt or conversation text | Self-information estimation followed by phrase- or token-level pruning | Yes | Python library or application preprocessing | Clear information-theoretic framing and controllable reduction | Local importance is not the same as task-criticality; syntax, role boundaries, safety instructions, and latent future needs require separate guards |
| [RECOMP](https://github.com/carriex/recomp) | Retrieved documents | Learned extractive sentence selection or abstractive summarization, including empty augmentation when retrieval is unhelpful | Yes | RAG pipeline between retrieval and prompt assembly | Trains compression against downstream task usefulness rather than length alone | Specialized for retrieved evidence; abstractive compression introduces faithfulness risk and is not a general message-envelope optimizer |
| [PCToolkit](https://github.com/3DAgentWorld/Toolkit-for-Prompt-Compression) | Research and evaluation toolkit | Common interfaces around multiple existing prompt compressors, datasets, and metrics | Depends on selected compressor | Python research toolkit | Useful comparative harness and modular dataset/metric design | It is primarily an aggregator and evaluation surface, not a new universal optimization algorithm |

## What “token optimizer” can mean

The projects above occupy five distinct layers:

1. **Source reduction.** RTK changes what a tool emits before that output enters the conversation.
2. **Generation policy.** Caveman asks the target model to produce a shorter answer. This can lower future billed output only when the instruction actually changes generation.
3. **Prompt compression.** LLMLingua, LLMLingua-2, and Selective Context remove prompt tokens before inference.
4. **Query-focused evidence compression.** LongLLMLingua and RECOMP use the current question to retain more relevant long-context or retrieved evidence.
5. **Context systems.** Headroom spans several content types and includes restoration, cache, and deployment surfaces.

Those layers can complement one another, but their savings cannot be multiplied as if they were independent. A command-output reduction may be a small fraction of a complete session; a terse-output instruction consumes input tokens; a learned compressor adds compute and may cause a retry; cache-prefix churn can make a shorter request more expensive. KendrOptimizer therefore measures the complete serialized envelope and, when observations are available, the complete paired workload.

## Detailed notes

### Headroom

The [official repository](https://github.com/headroomlabs-ai/headroom) describes a local-first, content-aware stack for tool outputs, logs, RAG chunks, files, and conversation history. It offers library, proxy, agent-wrapper, and MCP surfaces. Its current architecture includes content routing, specialized JSON/code/text compression, cache alignment, and a cache-and-retrieve recovery design.

Ideas worth learning from:

- route content to a structure-aware engine instead of applying one prose heuristic everywhere;
- retain a recovery path for transformations that are not self-contained;
- treat prompt-cache behavior as part of cost, not an afterthought;
- publish workload-specific numbers rather than a single universal percentage.

KendrOptimizer’s deliberate difference is product scope. It does not proxy provider traffic, manage cross-agent memory, wrap an LLM client, or expose a chat-completions endpoint. A host may embed the library or call a local transformation-only service, then independently send the result to any model.

### RTK

[RTK](https://github.com/rtk-ai/rtk) is a Rust CLI proxy for common developer commands. It applies command-specific output shaping and can preserve full failed-command output for later inspection. This is a strong pattern for deterministic, parser-first optimization: a test report, git diff, compiler log, and JSON response should not share one generic truncation rule.

RTK’s [savings explanation](https://github.com/rtk-ai/rtk/blob/develop/docs/guide/resources/savings-explained.md) explicitly distinguishes reduced shell-output bytes from the user’s full model bill and describes its token estimate. That distinction is foundational for KendrOptimizer receipts.

Ideas worth learning from:

- build bounded, format-aware reducers for known tool outputs;
- fail open on malformed input;
- preserve errors and a route to full diagnostic output;
- report the denominator used by every percentage.

Gap to cover:

- optimization should also account for message history, tool definitions, serialization overhead, prompt cache effects, retries, and downstream success;
- unsupported content must produce an explicit no-op reason, not an implied saving.

### Caveman

[Caveman](https://github.com/JuliusBrussee/caveman) is a skill/plugin that changes answer style. It asks coding agents to remove ceremony and filler while keeping code, commands, and errors intact. This is best understood as a generation policy: the saving happens only if the target model emits fewer tokens.

The project’s [honest-numbers document](https://github.com/JuliusBrussee/caveman/blob/main/docs/HONEST-NUMBERS.md) is especially relevant. It discusses instruction overhead, short-workload break-even behavior, and paired comparison rather than assuming that shorter-looking prose always lowers the bill.

Ideas worth learning from:

- protect exact technical artifacts;
- make terseness adjustable and yield to clarity or safety;
- measure instruction overhead and break-even;
- distinguish a future-output recommendation from post-processing.

KendrOptimizer must never claim that trimming an already generated answer saved output tokens. Such a transform may reduce future history size, but the provider has already billed the original generation.

### LLMLingua

The [LLMLingua paper](https://aclanthology.org/2023.emnlp-main.825/) uses a smaller language model to identify less essential prompt tokens. The method combines a budget controller, coarse-grained prompt selection, token-level iterative compression, and distribution alignment. Its core contribution is learned salience under a target compression budget.

Ideas worth learning from:

- make compression budget explicit;
- score importance rather than deleting solely by surface repetition;
- evaluate both efficiency and target-model task performance;
- allow regions to be marked as incompressible.

Gaps to cover in a general agent optimizer:

- auxiliary-model latency, memory, and energy belong in the receipt;
- token salience does not prove preservation of syntax, structured schemas, tool-call relations, security constraints, or exact identifiers;
- target-tokenizer mismatch changes the realized saving;
- learned methods need deterministic timeouts and a pass-through fallback.

### LLMLingua-2

[LLMLingua-2](https://aclanthology.org/2024.findings-acl.57/) reframes compression as token classification with a bidirectional encoder and uses data distilled from a larger model. The paper reports substantially faster compression than earlier LLMLingua variants and evaluates task-agnostic transfer.

Ideas worth learning from:

- use a learned component as a candidate ranker or veto input, not as unchecked authority;
- train on preservation-oriented labels;
- benchmark out-of-domain data and multiple target models.

For KendrOptimizer, a future learned engine belongs behind an explicit high-risk policy tier. Deterministic protected spans, structural validators, minimum-gain gates, explicit latency/cancellation behavior, and whole-envelope rollback still apply.

### LongLLMLingua

[LongLLMLingua](https://aclanthology.org/2024.acl-long.91/) extends prompt compression for long-context scenarios. It uses the question to rank context, adjusts budgets across documents, reorders content, and performs fine-grained compression. The design targets both cost and the failure mode where important evidence is poorly attended in the middle of a long prompt.

Ideas worth learning from:

- the current user objective should influence retention;
- allocate budgets by segment rather than applying one global ratio;
- position and cache effects matter alongside raw token count;
- long-context evaluation must include answer quality, not only reconstruction.

The limitation is scope: a conversation contains policies, unresolved tool calls, latent constraints, and future-use information that may not be relevant to the current lexical query. KendrOptimizer therefore treats query relevance as one signal inside a constrained planner.

### Selective Context

The [Selective Context paper](https://arxiv.org/abs/2310.06201) estimates self-information with a language model and removes less informative lexical units. Its [reference implementation](https://github.com/liyucheng09/Selective_Context) supports phrase- and token-level reduction.

Ideas worth learning from:

- information density is a useful compression signal;
- phrase-level units can preserve readability better than isolated-token deletion;
- evaluate summaries, question answering, and conversations separately.

Gaps to cover:

- predictable text can still be mandatory, such as a repeated safety rule or closing delimiter;
- rare text can still be irrelevant noise;
- local information scores need structure-aware and role-aware constraints;
- a general engine needs receipts explaining which regions were changed and why.

### RECOMP

[RECOMP](https://arxiv.org/abs/2310.04408) learns extractive and abstractive compressors for retrieved documents. It trains against end-task utility and can return an empty augmentation when retrieved material does not help. The [official code repository](https://github.com/carriex/recomp) publishes the task-specific compressors and evaluation workflow.

Ideas worth learning from:

- optimize for downstream utility rather than compression ratio alone;
- make “include nothing” a valid decision when evidence is unhelpful;
- compare extractive and abstractive risk separately;
- test transfer across target models.

Gaps to cover:

- RAG evidence is only one region of a model request;
- abstractive summaries may omit or alter details;
- task-trained compressors may not transfer to code, tool output, policy text, or another domain;
- a universal host adapter needs bounded latency and no model call in its deterministic core.

### PCToolkit

[PCToolkit](https://github.com/3DAgentWorld/Toolkit-for-Prompt-Compression) provides common interfaces for prompt compressors, datasets, and evaluation metrics. Its [technical report](https://arxiv.org/abs/2403.17411) is useful as a map of experimental components.

Ideas worth learning from:

- separate compressors, datasets, metrics, and runners;
- make new evaluation fixtures easy to add;
- reproduce a method under a pinned configuration rather than comparing prose claims.

KendrOptimizer is not a PCToolkit-style wrapper. Competitor adapters belong in the benchmark harness only. The production core independently implements its own transform engines and has no runtime dependency on the compared projects.

## Design gaps KendrOptimizer is intended to close

No single reviewed project establishes all of the following in one model-neutral transformation engine:

- a stable envelope that covers messages, tool definitions, tool results, output contracts, tokenizer hints, cache metadata, host capabilities, and policy;
- a planner that compares multiple non-conflicting candidates across the complete serialized request;
- explicit risk tiers ranging from pass-through and representation-safe transforms to extractive or learned transforms;
- protected spans for code, paths, URLs, numbers, identifiers, schemas, errors, security instructions, and tool-call causality;
- exact reconstruction tests for every transform labeled reversible;
- positive-gain gates and explicit latency-budget/cancellation behavior, including compression metadata and cache penalties;
- fail-open behavior with attempted-engine and no-op reasons;
- a preflight receipt separated from observed provider usage;
- paired downstream-quality, total-cost, and total-latency benchmarks;
- a pure library or transformation-only sidecar that never selects or calls an LLM provider.

This list is a design hypothesis, not a superiority claim. It becomes credible only as implementation, adversarial tests, and reproducible benchmarks land.

## What would count as “best”

There is no honest universal best compressor independent of workload, target model, tokenizer, latency budget, and acceptable quality risk. KendrOptimizer should seek a defensible Pareto frontier:

- no regression and low overhead for representation-safe transforms;
- exact recovery for transforms described as reversible;
- statistically bounded quality loss for explicitly enabled lossy modes;
- lower observed end-to-end cost at equal task success;
- transparent no-op behavior where evidence of benefit is absent.

Any public claim should name the corpus, target models, tokenizer, policy, cache state, quality metric, statistical interval, and full denominator. See [benchmark methodology](benchmark-methodology.md).
