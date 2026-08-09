# Provenance and independent implementation policy

Last reviewed: 2026-08-07

KendrOptimizer is an independent open-source implementation. It is informed by published research and observable product behavior, but it is not a source-code port, compatibility wrapper, or bundled distribution of another optimizer.

This document records what “independent” means in practice and how contributors must handle upstream ideas, code, datasets, weights, names, and claims.

## Product boundary

The production project may:

- accept a model-request envelope or an individual context region;
- analyze content and propose native transform candidates;
- apply policy, risk, benefit, and cache gates plus a pre-dispatch latency guard;
- return optimized content, optional recovery data, and an audit receipt;
- accept later observations such as actual usage, cost, latency, and outcome.

The production project must not:

- forward a request to an LLM provider;
- choose or route among target models;
- store provider credentials;
- expose an inference-compatible chat endpoint;
- depend at runtime on Headroom, RTK, Caveman, LLMLingua, Selective Context, RECOMP, PCToolkit, or another compared optimizer;
- present an upstream benchmark result as a KendrOptimizer result.

Benchmark-only adapters may invoke separately installed upstream projects under pinned versions. They must be isolated from the shipping optimizer crates and clearly label their own dependencies and licenses.

## Permitted sources of inspiration

Contributors may study:

- peer-reviewed papers, preprints, project documentation, public talks, and public benchmark protocols;
- public input/output behavior needed to understand a problem class;
- standard data structures, file formats, and algorithms that are not copied from a specific implementation;
- upstream source code for landscape analysis and interoperability review, provided no code or distinctive implementation expression is copied into KendrOptimizer.

Reading public source means this project should be called an independent implementation, not a formal clean-room implementation. A clean-room claim has a more specific legal and organizational meaning that this project does not currently assert.

## Implementation rules

Every new production transform should include:

1. A short design note explaining the problem and algorithm in original language.
2. A list of papers or projects that informed the design.
3. A statement of what is materially different in KendrOptimizer.
4. Native tests derived from public specifications or newly authored fixtures, not copied upstream fixtures unless their license and attribution are explicitly approved.
5. A license review for any imported dataset, model weight, grammar, generated table, or nontrivial code fragment.
6. A receipt identifier and version so benchmark results can trace the exact implementation.

Do not copy and lightly rename upstream functions, tests, prompts, regular expressions, tables, documentation, or benchmark outputs. A permissive license may allow reuse, but this project’s stated engineering goal is to understand the logic, identify gaps, and implement a coherent native design. If direct reuse ever becomes the correct choice, it must be explicit, attributed, license-compliant, and recorded in the repository’s notices rather than described as independent code.

## Research inventory

The license column describes the referenced code repository as observed on the review date. Papers remain subject to their publisher or archive terms. Model weights and datasets can have separate licenses and must be reviewed independently.

| Reference | Materials reviewed | Repository license observed | Ideas studied | Production code copied or linked |
| --- | --- | --- | --- | --- |
| [Headroom](https://github.com/headroomlabs-ai/headroom) | README, architecture and product documentation | [Apache-2.0](https://github.com/headroomlabs-ai/headroom/blob/main/LICENSE) | Content routing, structure-aware engines, reversible retrieval, cache awareness, workload-specific reporting | No |
| [RTK](https://github.com/rtk-ai/rtk) | README, architecture, savings explanation, public behavior | [Apache-2.0](https://github.com/rtk-ai/rtk/blob/develop/LICENSE) | Parser-first command-output filtering, failure preservation, honest denominator | No |
| [Caveman](https://github.com/JuliusBrussee/caveman) | README, installation model, honest-numbers documentation | [MIT](https://github.com/JuliusBrussee/caveman/blob/main/LICENSE) | Generation-policy overhead, protected technical artifacts, adjustable terseness, paired tests | No |
| [LLMLingua family](https://github.com/microsoft/LLMLingua) | LLMLingua, LongLLMLingua, and LLMLingua-2 papers and documentation | [MIT](https://github.com/microsoft/LLMLingua/blob/main/LICENSE) for repository code | Budgeted salience, query-aware long-context ranking, token classification, incompressible regions | No |
| [Selective Context](https://github.com/liyucheng09/Selective_Context) | Paper, README, documented interfaces | No repository license file was observed on the review date | Self-information as a candidate salience signal; phrase-level pruning | No; paper-level concepts only |
| [RECOMP](https://github.com/carriex/recomp) | Paper, README, training and evaluation description | [MIT](https://github.com/carriex/recomp/blob/main/LICENSE) | Downstream-trained extractive/abstractive evidence compression and selective augmentation | No |
| [PCToolkit](https://github.com/3DAgentWorld/Toolkit-for-Prompt-Compression) | Technical report, README, toolkit organization | [MIT](https://github.com/3DAgentWorld/Toolkit-for-Prompt-Compression/blob/main/LICENSE) | Separation of compressor, dataset, metric, and evaluation interfaces | No |

The inventory must be updated when a contributor performs additional source review that materially influences a production engine.

## Ideas are not results

An upstream project may report a compression ratio, latency improvement, or quality score under its own test conditions. Those values provide context only. KendrOptimizer documentation must not:

- multiply upstream percentages to predict a combined saving;
- transpose a byte reduction into a token or monetary reduction without measurement;
- apply a result from one model, tokenizer, or dataset to another;
- omit auxiliary-model compute, retries, cache effects, or instruction overhead;
- describe a post-generation text edit as a reduction in output tokens already billed.

KendrOptimizer results originate only from a reproducible KendrOptimizer benchmark artifact. Upstream values should use language such as “the authors report” and link directly to the primary source.

## Dataset, weight, and fixture policy

Code licensing does not automatically grant rights to datasets or model weights linked by a project. Before adding any external asset:

- record its canonical source, version or digest, license, and required attribution;
- verify whether commercial use, redistribution, modification, and generated derivatives are allowed;
- download it during benchmark setup when redistribution is not allowed;
- keep private production traces out of the repository;
- redact secrets and personal data before turning a trace into a fixture;
- prefer small, newly authored fixtures for regression tests.

Learned engines must ship a model card describing training sources, intended use, limitations, evaluation, license, and cryptographic digest. If those records are incomplete, the model cannot be enabled in a release profile.

## Contribution attestation

Pull requests that add or materially change an optimizer should answer:

- Which external papers, repositories, or products did you review?
- Did you copy any code, prompt, fixture, generated output, data, or weights?
- Are all third-party materials listed with their licenses and notices?
- What native tests show that the implementation satisfies its own contract?
- Which claims are measured here, and which are quotations or paraphrases of upstream claims?

Maintainers may request a rewrite when an implementation is too close to an upstream expression even if the upstream license is permissive. This protects the project’s architectural independence and makes provenance easier to audit.

## Trademarks and naming

Project names such as Headroom, RTK, Caveman, LLMLingua, Selective Context, RECOMP, and PCToolkit belong to their respective owners. They are used here only for factual comparison and attribution. KendrOptimizer integrations and benchmark adapters must not imply endorsement, affiliation, or certification.

## Review cadence

Before each public benchmark release:

- pin every compared implementation to a version or commit;
- re-check its license and benchmark instructions;
- update the review date in this file and the competitive landscape;
- archive configuration and raw receipts;
- disclose unsupported or failed competitor runs instead of silently dropping them.

For evaluation and claim rules, see [benchmark methodology](benchmark-methodology.md).
