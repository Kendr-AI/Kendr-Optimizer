---
name: add-kendr-optimizer
description: Add fail-open KendrOptimizer handling to NanoClaw's initial and follow-up inbound prompt-string seams.
---

# Add KendrOptimizer

Install a conservative KendrOptimizer adapter in NanoClaw's container-side
agent runner. NanoClaw deliberately has no stable plugin middleware; this skill
uses its supported source-customization model and guards the exact source seams
it changes.

This adapter sees only newly formatted inbound prompt strings. It does **not**
see the provider's opaque resumed history, system-prompt implementation, tool
schemas, tool calls, tool results, or generated output. Provider-specific
adapters are required for those surfaces.

Kendr receipts from this adapter measure that new prompt string's normalized
surface. They are not whole-session or provider-bill percentages because the
opaque resumed context is absent. Publish paired provider usage separately.

## Compatibility pin

This skill was audited against official `nanocoai/nanoclaw` commit
`743e32df4e6c05f3725c17cb2ec11f2b65079eec`. NanoClaw had no tagged release at
the audit date, so the commit—not a fabricated version—is the compatibility
pin. The guarded patch refuses to edit a changed poll loop ambiguously.

## Prerequisite

Make a `kendr-opt serve --bind 127.0.0.1:7331` process available **inside each
agent container's network namespace**. `127.0.0.1` on the host is not the
container's loopback. How the binary is copied, mounted, and supervised is an
operator deployment decision and is intentionally not fabricated by this
source-only skill.

Without the sidecar, NanoClaw continues normally: the adapter times out after
40 ms, opens a 30-second circuit, and passes every original prompt through.

## Install

### 1. Verify the target seam

Confirm `container/agent-runner/src/poll-loop.ts` still contains one initial
`provider.query({ prompt, ... })` call and one follow-up `query.push(prompt)`
call. The helper performs this check again before writing.

```nc:run effect:check
node .claude/skills/add-kendr-optimizer/assets/apply-patch.mjs --check-source .
```

### 2. Copy the adapter and guard test

Copy the helper and its test into the mounted agent-runner source. The official
copy directive is idempotent: it skips when both destinations already exist and
copies the complete pair when either destination is missing.

```nc:copy
.claude/skills/add-kendr-optimizer/assets/kendr-optimizer.ts -> container/agent-runner/src/kendr-optimizer.ts
.claude/skills/add-kendr-optimizer/assets/kendr-optimizer.test.ts -> container/agent-runner/src/kendr-optimizer.test.ts
```

### 3. Patch the two prompt seams

Run the guarded patcher. It adds one import, awaits optimization for the
initial `provider.query` prompt, and awaits optimization for normal follow-up
`query.push` prompts. The follow-up path rechecks stream completion after that
await before pushing. Re-running detects the already-installed form and is a
no-op. Internal delivery-retry nudges remain untouched.

```nc:run effect:external
node .claude/skills/add-kendr-optimizer/assets/apply-patch.mjs .
```

### 4. Build and validate

The typecheck validates the awaited helper at both call sites. The Bun test
checks loopback enforcement, strict outcome reconstruction, timeout/circuit
fail-open behavior, slash-command preservation, and both structural seams.

```nc:run effect:build
pnpm exec tsc -p container/agent-runner/tsconfig.json --noEmit
./container/build.sh
```

```nc:run effect:test
cd container/agent-runner && bun test src/kendr-optimizer.test.ts && cd -
node .claude/skills/add-kendr-optimizer/assets/apply-patch.mjs --check .
```

### 5. Propagate existing overlays

Existing group overlays can shadow the rebuilt image's source. Copy the helper
and patched poll loop into each existing overlay, preserving the same relative
paths, then restart those groups. Inspect each overlay before copying; do not
blindly overwrite unrelated group-specific poll-loop changes.

## Configuration

The agent-runner reads these optional environment variables:

- `KENDR_OPTIMIZER_ENDPOINT` — default `http://127.0.0.1:7331`; only literal
  loopback HTTP origins are accepted.
- `KENDR_OPTIMIZER_TIMEOUT_MS` — default 40, allowed 5–250.
- `KENDR_OPTIMIZER_BACKOFF_MS` — default 30000, allowed 100–300000.
- `KENDR_OPTIMIZER_TOKENIZER` — `o200k_base` (default), `cl100k_base`, or
  `approximate`.

The adapter fixes risk at `representation_safe`, disables tool selection and
generation policy, and skips raw slash commands so provider-native commands
retain their exact spelling. Its dependency-free `node:http` transport connects
directly to the validated literal loopback socket; it does not consult proxy
environment variables or follow redirects.

## Verify at runtime

After the image and existing overlays are updated:

1. Restart a test agent group.
2. Confirm `/healthz` from inside its container reaches `127.0.0.1:7331`.
3. Warm that long-running sidecar with one `/v1/analyze` request before testing
   the 40 ms deadline; cold tokenizer initialization can exceed the deadline.
4. Send a redundant, non-command prompt.
5. Inspect Kendr receipts at the sidecar; do not log raw prompt bodies.
6. Stop the sidecar and confirm the next prompt still succeeds unchanged after
   the bounded fail-open delay.

## Troubleshooting

### Every request passes through unchanged

The common cause is namespace placement: a host-side service is not reachable
at container loopback. Run or supervise the sidecar in the same container
network namespace and verify it from inside that container.

### Patch helper reports source drift

Do not weaken the occurrence checks. Re-audit the new NanoClaw poll loop,
update the before/after fixtures and structural test, then advance the pinned
commit in this skill.

### Prompt latency spikes

Keep the local timeout below the host's responsiveness budget. The adapter
opens a circuit on timeout, but a repeatedly restarted process will reset that
circuit.
