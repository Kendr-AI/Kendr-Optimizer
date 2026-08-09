# NanoClaw adapter

NanoClaw does not expose a stable plugin middleware. This integration therefore
ships an idempotent NanoClaw customization skill at [`skill/`](skill/) that:

1. copies a dependency-free TypeScript prompt adapter and tests;
2. guards and patches the initial `provider.query` prompt seam;
3. guards and patches the normal follow-up `query.push` prompt seam; and
4. fails open to the original prompt on any sidecar or validation failure.

Copy the entire `skill/` directory into a NanoClaw checkout as
`.claude/skills/add-kendr-optimizer/`, then follow its `SKILL.md` or apply its
official `nc:` directives.

The audit pin is official `nanocoai/nanoclaw` commit
[`743e32df4e6c05f3725c17cb2ec11f2b65079eec`](https://github.com/nanocoai/nanoclaw/commit/743e32df4e6c05f3725c17cb2ec11f2b65079eec).
At that commit NanoClaw had no tagged release. The relevant upstream contracts
are [`providers/types.ts`](https://github.com/nanocoai/nanoclaw/blob/743e32df4e6c05f3725c17cb2ec11f2b65079eec/container/agent-runner/src/providers/types.ts),
[`poll-loop.ts`](https://github.com/nanocoai/nanoclaw/blob/743e32df4e6c05f3725c17cb2ec11f2b65079eec/container/agent-runner/src/poll-loop.ts),
and the [`nc:` directive reference](https://github.com/nanocoai/nanoclaw/blob/743e32df4e6c05f3725c17cb2ec11f2b65079eec/docs/skill-directives.md).

Coverage is intentionally partial: only newly formatted inbound strings are
visible at the generic provider boundary. Opaque continuation history and
provider-specific tools/results require separate provider integrations.

