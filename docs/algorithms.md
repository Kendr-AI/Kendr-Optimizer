# Native Algorithms and Planning Model

## Scope

KendrOptimizer is not a dispatcher for Caveman, RTK, LLMLingua, or another optimizer. Production transformations are implemented natively against KendrOptimizer's typed envelope, risk levels, verification gates, cache policy, and receipt format. Existing projects are research and benchmark subjects; their executables belong only in the benchmark harness.

This document describes both the nine pre-alpha engines that exist today and the larger native algorithm system planned for the project. The current implementation is intentionally narrow.

## Risk levels

All engines declare one of the ordered `RiskLevel` values:

| Level | Intended interpretation | Typical example |
| --- | --- | --- |
| `pass_through` | No content mutation | Measurement or classification |
| `representation_safe` | Typed representation changes; no intended information removal | JSON minification |
| `recoverable` | Content is omitted or referenced, with an exact original available | Repeated-line folding with recovery capsule |
| `extractive` | Content selected and omitted based on relevance or heuristics | Oversized log pruning |
| `learned` | Local learned inference affects selection or rewriting | Salience-ranked context extraction |

The risk label is not an assertion that model output will be identical. Even representation-only changes may influence an LLM. It is a policy category and proof obligation.

The default policy ceiling is `recoverable` in the generic contract. Integrations may be stricter. The current OpenClaw adapter permits only `pass_through` and `representation_safe` because it does not install or protect recovery data.

## Implemented engine pipeline

The current core invokes a fixed, ordered list:

| Order | Engine | Risk | Current behavior | Recovery |
| ---: | --- | --- | --- | --- |
| 1 | `json-minify` | `representation_safe` | Parses a complete JSON-looking text/document/tool-result value and reserializes it compactly | No granular record; parsed value is intended to be equivalent |
| 2 | `terminal-clean` | `representation_safe` | Removes matched ANSI escape/control sequences from tool-result strings | No granular record |
| 3 | `text-normalize` | `representation_safe` | Collapses runs above two blank lines outside basic backtick/tilde fences | No granular record |
| 4 | `repeat-lines` | `recoverable` | Replaces the fourth and later identical consecutive non-empty lines with an omission marker containing count and SHA-256 | Whole original envelope capsule |
| 5 | `pytest-result-fold` | `recoverable` | Folds bounded, exactly reconstructable numeric pytest result sequences after validating their summary | Whole original envelope capsule plus typed expansion proof |
| 6 | `context-repetition` | `recoverable` | References exact prefix blocks, paragraphs, adjacent lines, and runs of at least three adjacent sentences in eligible text/document parts | Whole original envelope capsule plus typed expansion proof |
| 7 | `history-dedup` | `recoverable` | Replaces exact old, text-only user/assistant replays with a reference to an earlier message | Whole original envelope capsule |
| 8 | `tool-output-prune` | `extractive` | Keeps first/last boundaries and lines around diagnostic keywords in oversized tool results | Whole original envelope capsule |
| 9 | `tool-selector` | `extractive` | Conservatively removes optional tools using lexical relevance and dependency tags | Whole original envelope capsule |

Each engine currently proposes at most one whole-envelope candidate. The optimizer immediately measures and accepts or rejects it before invoking the next engine. This is a greedy pipeline, not yet a global optimizer.

## Implemented algorithms in detail

### `json-minify`

The engine visits text, document, and tool-result parts. It only considers trimmed values starting with `{` or `[`. It asks `serde_json` to parse the complete value and, if valid, serializes it without pretty-print whitespace.

Properties:

- Typed `ContentPart::Json` values are not touched; those are already represented structurally.
- Partial JSON embedded inside prose is ignored.
- Invalid JSON is ignored.
- JSON string values, numbers, arrays, and object membership are preserved under `serde_json` parsing/serialization semantics.
- Object formatting and lexical number representation may change. JSON object order is retained by the current value representation only to the extent provided by the serializer configuration; raw byte identity is not promised.
- The candidate must still save the configured number and percentage of measured tokens.

This engine is useful but does not prove model behavioral equivalence. If exact source formatting is task-relevant, the host should encode it as code or mark it for preservation rather than as compressible prose.

### `terminal-clean`

The engine applies an ANSI escape-sequence regex only to tool results. It does not execute terminal text or attempt shell interpretation.

Current gaps:

- It does not yet parse carriage-return progress redraws.
- It may remove color/style metadata that is meaningful to a human or model, even though visible text remains.
- It does not normalize backspaces, OSC hyperlinks, terminal widths, or all possible control families.
- There are no per-input regex time or size budgets beyond the global between-engine latency check.

Future versions should use a bounded terminal parser and distinguish display-only control data from meaningful terminal content.

### `text-normalize`

The engine preserves up to two consecutive blank lines and removes additional blank lines in text and document parts. It toggles a simple fenced state for lines beginning, after indentation, with triple backticks or tildes.

It deliberately does not rewrite prose, remove words, shorten sentences, or touch typed code parts.

Current gaps:

- Fence handling is a simple toggle rather than a complete Markdown parser.
- Indented code, nested fences, unmatched fences, HTML preformatted blocks, tables, and whitespace-sensitive non-Markdown formats may be misclassified.
- Whitespace can influence model behavior even when rendered meaning appears unchanged.

### `repeat-lines`

For each tool result, consecutive identical non-empty lines are grouped. Runs shorter than four remain unchanged. A longer run keeps the first line and adds a marker with the omitted count and the SHA-256 of that line.

This is recoverable at the envelope level because the optimizer currently includes the entire original envelope in the recovery capsule. A strict expander also validates the canonical count and SHA-256 against the retained preceding line; the verifier accepts the typed proof only when the changed part expands byte-for-byte to its input. Checked arithmetic rejects any expansion above 1,000,000 lines or 64 MiB before allocating the repeated copies. The marker is not a random, scoped, or authenticated storage handle, and the model does not automatically have access to the recovery capsule.

The engine skips content containing its marker prefix to reduce marker confusion. The full-input comparison prevents an authored or altered marker from proving a different transformation, but scoped random authenticated recovery markers remain planned.

### `pytest-result-fold`

This first typed test-result reducer examines `ToolResult` content rather than trusting a tool name. It accepts a bounded pytest-looking stream only when result lines parse consistently and exactly one summary matches the observed `passed`, `skipped`, and `xfailed` counts. It folds only runs of at least eight sequential numeric node IDs with identical prefix, suffix, width, and status. `PASSED`, `SKIPPED`, and `XFAIL` runs are eligible; failures, errors, unexpected passes, diagnostic blocks, and the summary remain model-visible. Two lines at each edge of a folded run are retained.

The marker records the status, omitted range, numeric width, hexadecimal prefix/suffix bytes, and SHA-256 of the omitted block. Its independent expander preserves LF or CRLF and must reproduce the changed part byte-for-byte before the verifier accepts it. Inputs above 4 MiB, 50,000 lines, or 64 KiB per line, malformed summaries, mixed line endings, marker-shaped input, and non-sequential IDs fail open. This is a narrow exact-sequence fold, not a general pytest or test-runner optimizer.

### `context-repetition`

This engine visits only `Text` and `Document` parts in user and assistant messages. It skips system, developer, and tool roles as well as code, JSON, image, tool-call, and tool-result parts. It considers two deterministic candidates:

- an exclusive repeated-prefix-block candidate whose possible periods are paragraph starts and whose common-prefix lengths are computed in linear time with the Z algorithm; and
- a layered candidate that references exact repeated paragraphs, profitable adjacent line runs, and exact runs of at least three adjacent sentences.

The sentence layer is deliberately conservative: it works line by line, recognizes ASCII `.`, `!`, or `?` followed by horizontal whitespace or the line end, requires at least 32 non-padding bytes, and does not inspect fenced backtick or tilde lines. It is exact repetition encoding, not linguistic sentence understanding, relevance ranking, or summarization.

Every form retains an exact source span and inserts a readable count/byte or source-ordinal marker. Compression is bounded to 2 MiB and 8,192 units per part, marker-shaped input is skipped, and candidates must save at least 16 bytes before the optimizer's token-gain gate. Prefix-block markers are exclusive; layered markers expand in reverse order: sentence, line, then paragraph. The verifier attempts expansion only for parts that actually changed and accepts the proof only when the reconstructed envelope equals the engine input byte-for-byte. The deterministic markers are not authenticated recovery handles, and the risk remains `recoverable` because replacing visible repetition with a count can still influence a model.

### `history-dedup`

The engine runs only when the host declares `can_restore_references`. It excludes the configured recent-message suffix, considers only old user/assistant messages made entirely of text/document parts, serializes the parts, hashes them, and replaces exact role-matched repetitions with a stable reference to the first message ID.

It does not deduplicate similar or semantically equivalent turns. It also does not remove system, developer, tool-call, or tool-result messages.

The capability name is stronger than the current integration. A host that can store a capsule but cannot make the referenced original available when the model needs it may still suffer a quality loss. Future contracts should separate “can restore after the fact,” “can resolve during generation,” and “can retry with original context.”

### `tool-output-prune`

This engine is disabled unless `enable_lossy_tool_output` is true and the risk ceiling permits `extractive`. It activates only above `max_tool_result_chars`.

For sufficiently long line-oriented content, it retains:

- the first 24 lines;
- the last 24 lines;
- lines containing case-insensitive diagnostic terms such as error, fatal, panic, exception, failed, warning, assert, or caused by; and
- one adjacent line before and after each diagnostic line.

It inserts omission markers for gaps and a final count summary. The complete original envelope is placed in recovery data.

This is a generic bootstrap heuristic, not an RTK-equivalent typed command system. It can omit relevant non-diagnostic data, identifiers, successful results, context needed to understand an error, or domain-specific signals. It requires workload evaluation and must remain opt-in.

### `tool-selector`

The selector is disabled unless all of the following hold:

- `enable_tool_selection` is true;
- risk ceiling permits `extractive`;
- the host can narrow tools;
- the host can retry with the full tool surface; and
- more than three tools are present.

It extracts terms of at least three alphanumeric characters from the latest user text and scores each tool:

- name-term match: 6 points;
- tag-term match: 3 points;
- description-term match: 2 points; and
- serialized schema-term match: 1 point.

Required tools and tools tagged `always` are retained. Tools scoring at least two, the best-scoring tools, and declared `depends:<tool-name>` dependencies are retained. The selector only removes complete optional definitions; it does not rename or rewrite a tool.

Current gaps:

- No stemming, synonyms, phrase/entity model, BM25 normalization, calibrated confidence, negative intent, conversation-level intent, or tool-family grouping.
- A best score above zero is enough to produce a candidate, even if the relevance margin is weak.
- Dependency tags are host-provided and are not validated as a complete graph.
- Retry capability is declared, not executed or observed by the core.
- Tool safety authorization remains entirely with the host.

The selector should stay shadow-only on serious workloads until false-negative rates are measured.

## Protected artifacts

The verifier currently extracts these artifacts from text-like parts:

- HTTP(S) URLs;
- Windows and Unix-looking paths;
- numbers with a small unit suffix set;
- long hexadecimal or uppercase identifiers;
- common English negations;
- `KENDR_PRESERVE_BEGIN`/`KENDR_PRESERVE_END` blocks; and
- full lines containing selected error terms.

Typed code, JSON, image URIs, and tool calls receive separate exact checks.

Protected-artifact checking is a veto layer, not an optimizer. Counts are multiplicity-aware: ordinary candidates must retain at least the original occurrence count, and an exact recovery copy alone does not excuse content missing from the model-visible request. The three typed encodings (`repeat-lines`, `pytest-result-fold`, and `context-repetition`) may represent omitted multiplicity only when an independent marker expansion reconstructs every changed part and the complete pre-transform envelope byte-for-byte. Unchanged authored marker-shaped text is never treated as transform proof.

The current regexes remain incomplete, language-specific in places, and can be evaded or over-triggered. Exact expansion proves representation recovery, not semantic equivalence or identical downstream behavior. Planned work includes typed entity extraction, quoted spans, dates, units, citations, checksums, source anchors, user-declared spans, and a documented Unicode normalization policy.

## Net-gain algorithm

For each candidate, the current optimizer computes:

```text
token_delta = tokens_before - tokens_after
gain_percent = 100 * token_delta / tokens_before
```

A candidate passes only if both configured thresholds are met. Deltas are signed: an inflation is negative, never clamped to zero. After sequential acceptance, the whole envelope is remeasured and all changes are reverted if the aggregate portfolio falls below the same thresholds.

This correctly rejects obvious inflation but is not yet a net-cost planner. It does not model:

- exact provider message framing;
- cache-write and cache-read prices;
- output-token consequences;
- optimizer compute cost;
- retry probability;
- loss-driven correction turns; or
- alternative, conflicting candidate combinations.

## Native algorithms planned next

### Typed tool-result reducers

The narrow exact pytest sequence fold above is the first implemented member of this family. Broader format detection, parsers, and renderers remain planned for terminal redraws, compiler output, other test-runner forms, stack traces, logs, Git status/log/diff, build output, homogeneous JSON records, tabular data, and plan/change output.

Each reducer should publish:

- accepted grammar and limits;
- retained invariants;
- malformed-input fallback;
- recovery semantics;
- adversarial fixtures; and
- a benchmark card by task type.

Errors, warnings, exit status, counts, boundaries, source locations, and user-requested fields should be preserved by contract, not incidental keyword matching.

### Context dependency graph

Construct a graph over turns, tool calls/results, explicit references, constraints, named artifacts, and unresolved work. Preserve system/developer instructions, the recent window, open tool sequences, and active user constraints. Rank only older, closed material for deduplication, extraction, or recovery-backed aging.

This should replace position-only history policies. Summarization is not an early requirement; deterministic exact and extractive context assembly should be proven first.

### Conservative tool relevance

Upgrade lexical scoring with native BM25/trigram features, entity/action extraction, calibrated margins, capability-list intent detection, required groups, dependency closure, and host retry observations. An optional local ONNX model may rank tools later, but deterministic safeguards decide whether any tool can be removed.

### Query-relevant document extraction

Select sentence or line windows with stable source anchors, preserving exact quoted passages, numbers, headings, and neighboring context. Recovery must remain possible when the host supports it. Legal, medical, policy, security, cryptographic, and exact-transformation workloads should default to bypass or stricter risk ceilings.

### Generation policy — basic controller implemented

The current core returns an opt-in semantic pre-generation recommendation rather than post-processing an answer. It can recommend a caller-supplied maximum output limit, a concise host verbosity control, or a short brevity instruction. It bypasses detailed, exact, and structured-output requests, counts instruction overhead, requires an expected net-gain margin, and marks every output estimate unverified.

The current expected-output reduction is a transparent heuristic, not a trained predictor. It does not apply the recommendation to a provider request; that remains the host adapter's responsibility.

The target contract will extend this with:

- expected answer length class;
- optional maximum answer budget;
- required elements such as citations, code, steps, or tables;
- explicit user verbosity preference; and
- confidence and estimated instruction overhead.

The host maps this to supported model controls. If the only mechanism is an injected brevity instruction, its added input/cache tokens are part of the candidate cost. No output saving is verified without paired provider usage.

### Local learned components

Potential locally executed models include sentence/window salience, tool relevance, risk prediction, expected output length, and candidate-success prediction. They must use pinned artifacts, no network access, explicit model cards, bounded inference, and deterministic fallback.

Learned components should rank or veto constrained candidates. They should not receive unrestricted authority to rewrite system instructions, tool calls, output schemas, or arbitrary request fields.

## Target planner

The planned planner treats candidates as a conflict graph. Each candidate supplies scopes, dependencies, costs, risk, and proof obligations. For a bounded candidate set, the planner can use branch-and-bound or dynamic programming; for larger sets it can use a deterministic heuristic with a whole-envelope validation pass.

The target score is multi-objective:

```text
utility(candidate set) =
    estimated billed-input benefit
  + estimated future-context benefit
  + estimated output benefit, if a pre-generation policy exists
  - cache rewrite penalty
  - optimizer latency/compute penalty
  - expected retry/correction penalty
  - risk penalty
```

Hard constraints always dominate utility: immutable protocol structure, risk ceiling, host capabilities, context budget, cache policy, time/memory budget, and all proof obligations.

When provider pricing is absent, use robust token reduction. When the tokenizer is unknown, evaluate a conservative ensemble and require the minimum gain across supported profiles or fall back to a clearly labeled approximation.

## Open engine extension direction

There is no public engine SDK in pre-alpha. The internal Rust trait is crate-private and may change without compatibility guarantees.

A stable extension API should expose:

```text
describe() -> engine capabilities, risk, schemas, version
propose(read-only scoped envelope, policy, budget) -> candidates
verify_optional(candidate evidence) -> engine-specific evidence
```

Extensions must not decide final acceptance. The core remeasures and verifies every candidate. Untrusted extensions should run as WASI components with:

- no network by default;
- read access only to declared content scopes;
- bounded CPU, memory, output size, and wall time;
- versioned WIT contracts;
- deterministic randomness seeded by request digest when needed; and
- signed/pinned package metadata and an SBOM.

Native built-ins may remain compiled Rust for speed. Node, Python, and host adapters must not reimplement algorithms independently.

## Algorithm release requirements

An engine is not stable merely because it saves tokens on a fixture. Promotion requires:

- unit and property tests;
- fuzzing and malformed-input tests;
- exact reconstruction tests when claimed;
- cache and protected-span tests;
- task-level paired quality evaluation;
- negative/no-op cases;
- latency and memory measurements;
- benchmark comparison against the untouched baseline and relevant competitors; and
- a public limitations section.

No single aggregate “compression percentage” is sufficient evidence. Report results by input class, risk policy, model, tokenizer, cache condition, and task-success outcome.
