# CLI and Provider Integration

Kendr Optimizer runs beside an LLM harness. It does not replace the harness's
provider client.

```text
host CLI -> Kendr adapter -> local Kendr transform -> host's normal provider
```

API keys, provider URLs, model names, routing, retries, streaming, and billing
remain in Claude Code, OpenCode, Pi, OpenClaw, Hermes, or another host. Kendr
receives only normalized content on numeric loopback and never receives the
provider credential.

Do not set `OPENAI_BASE_URL`, an Anthropic base URL, or another inference URL to
Kendr. It does not implement `/chat/completions`, `/responses`, or `/messages`.

## Two-command path

Install from the public `v0.1.3` GitHub Release:

```bash
curl --proto '=https' --tlsv1.2 -LsSf \
  https://github.com/Kendr-AI/Kendr-Optimizer/releases/download/v0.1.3/kendr-opt-installer.sh | sh
```

```powershell
irm https://github.com/Kendr-AI/Kendr-Optimizer/releases/download/v0.1.3/kendr-opt-installer.ps1 | iex
```

Launch a supported, already-installed harness:

```bash
kendr-opt run opencode
kendr-opt run claude-code
kendr-opt run pi
kendr-opt run openclaw
kendr-opt run hermes
```

`run` performs idempotent setup, starts `kendr-opt serve` on
`127.0.0.1:7331`, launches the host, and stops the service it started when the
host exits. If a service is already listening, the launcher leaves it running.

Pass host arguments after `--`:

```bash
kendr-opt run opencode -- --model anthropic/claude-sonnet-4
kendr-opt run claude-code -- --resume
```

## Setup without launch

Configure one host:

```bash
kendr-opt setup opencode
```

Configure every supported host detected on `PATH`:

```bash
kendr-opt setup
```

Show support without changing files:

```bash
kendr-opt setup --list
```

Kendr owns only its same-name adapter file or directory. Setup refuses to
replace an unmanaged same-name path unless `--force` is supplied. Provider
configuration files are not read or modified.

Adapter storage defaults:

| Host | Installed location or mechanism |
| --- | --- |
| OpenCode | `$XDG_CONFIG_HOME/opencode/plugins/kendr-optimizer.js`, or `~/.config/opencode/plugins/kendr-optimizer.js` |
| Pi | `~/.pi/agent/extensions/kendr-optimizer.js` |
| Claude Code | Kendr data directory, loaded with Claude's supported `--plugin-dir` option |
| OpenClaw | Kendr data directory, then OpenClaw's `plugins install` and `config set` commands |
| Hermes | `~/.hermes/plugins/kendr-optimizer/`, then `hermes plugins enable` |

Set `KENDR_HOME` to change Kendr's adapter data directory. On Windows it
otherwise uses `%LOCALAPPDATA%\Kendr`; on macOS and Linux it uses
`$XDG_DATA_HOME/kendr` or `~/.local/share/kendr`.

## CLI updates

### Check and install

The native CLI can check its release channel without modifying the executable:

```bash
kendr-opt update --check
kendr-opt update --check --json
```

The JSON form emits one `kendr.update/v1` object on stdout with the status,
current and latest versions, channel, release identity and immutability state,
target archive and digest, release URL, check time, and updated executable path
when applicable. An available update is a successful check, not a command
failure.

Install the newest published channel release after it passes every gate with:

```bash
kendr-opt update
kendr-opt update --json
```

Official installs retain the channel in their install receipt. The current
installers record `preview`, which considers both published prereleases and full
releases and selects the highest semantic version. That newest release must
itself be immutable and complete; Kendr fails closed instead of silently falling
back to an older release. An unreceipted standalone check also defaults to
`preview`. To ignore prereleases, use:

```bash
kendr-opt update --check --channel stable
kendr-opt update --channel stable
```

The published `v0.1.2` executable predates this command. Upgrade from it once by
opening the [GitHub Releases page](https://github.com/Kendr-AI/Kendr-Optimizer/releases)
and running the installer shipped with the first newer release. The installed
updater can handle later releases.

The updater replaces the exact executable that is running. It refuses symbolic
links, reparse points, unwritable locations, and unrecognized install locations.
An official install receipt authorizes normal replacement. If a standalone
binary was deliberately copied without that receipt, `kendr-opt update --force`
authorizes that destination. `--force` does **not** bypass release-channel selection,
immutability, asset, checksum, archive, or candidate-binary checks. A CLI managed
by another package manager should be updated through that package manager.
The adjacent receipt is an ownership marker, not a signature or hash binding;
a same-directory writer or copied/stale matching receipt is outside this guard.

After an executable update, its bundled adapters refresh on the next
`kendr-opt setup` or `kendr-opt run`.

`kendr-opt update --reinstall` re-verifies and reinstalls the same eligible
version. It is intended for repair and release smoke testing; it never permits a
downgrade or bypasses any trust check.

### Passive notices and cache

Before an interactive `kendr-opt setup` or `kendr-opt run`, the CLI may check
for a newer release and print one concise notice to stderr. Passive checks:

- run only when stderr is attached to a terminal;
- never run for `--help`, `--version`, JSON transformation commands,
  `engines`, `serve`, or `setup --list`;
- are disabled when `CI` or `GITHUB_ACTIONS` is present;
- cache a successful result for 24 hours and back off for six hours after a
  failed check; and
- repeat a notice for the same installed/latest pair no more than once per 24
  hours.

Set `KENDR_NO_UPDATE_CHECK=1` to disable passive checks. This does not disable an
explicit `kendr-opt update` or `kendr-opt update --check` command.

The cache is `update.json` under `%LOCALAPPDATA%\Kendr\cache` on Windows,
`$XDG_CACHE_HOME/kendr` when set, or `~/.cache/kendr` otherwise. `KENDR_HOME`
relocates it to `$KENDR_HOME/cache`.

### Release and network boundary

The production updater is compiled for the public
`Kendr-AI/Kendr-Optimizer` GitHub repository. Its only intentional outbound
traffic is repository identity, release metadata, and selected release assets
from GitHub's API and HTTPS asset-delivery path. It does not send normalized
envelopes, prompts, tool output, recovery data, provider credentials, provider
URLs, or model configuration. It does not contact Kendr.org. The Rust core has
no networking dependency; normal optimization and the loopback transform
service do not use the updater's HTTP client.

The updater accepts only a published release that GitHub reports as immutable.
Before replacing the executable it:

1. verifies the compiled repository identity and release channel;
2. requires the platform archive and `SHA256SUMS` to have GitHub-recorded
   SHA-256 digests;
3. requires `SHA256SUMS` to cover the exact release asset set and agree with
   GitHub's digests;
4. checks the downloaded archive, expected member list, paths, permissions, and
   size limits;
5. runs the candidate's `--version` and `engines --compact` smoke tests; and
6. fetches the release again and rejects changes to the security-relevant
   release fingerprint before replacement.

Release immutability prevents accepted assets from being changed afterward.
SHA-256 detects corruption and disagreement between downloaded artifacts and
GitHub's recorded release state. Neither mechanism proves who originally
published the release: the binaries are not yet protected by a maintainer
signature, Sigstore identity, or OS code signature. A compromise of the GitHub
organization, release workflow, or initial immutable upload remains a
supply-chain risk. Backup-backed rollback covers detected replacement and
post-install validation failures, but it is not a power-loss-safe transaction
or journal. Review [the threat model](threat-model.md#supply-chain-and-extensions)
before unattended deployment.

## Provider configuration

Configure the provider exactly as the host documents, before or after Kendr
setup. Kendr does not need to know which provider the host will call.

| Concern | Host | Kendr adapter |
| --- | --- | --- |
| API key and authentication | Yes | No |
| Provider base URL | Yes | No |
| Model selection and routing | Yes | No |
| Streaming and retries | Yes | No |
| Prompt/context optimization | No | Yes, where the host exposes it |
| Tool-output optimization | No | Yes, where the host exposes it |
| Provider-reported usage | Host records it | `kendr-opt observe` compares it |

This makes the adapters provider-neutral: the same OpenCode adapter works when
OpenCode uses Anthropic, OpenAI, Google, a local model, or another provider.

## Compatibility matrix

| Host | Automatic setup | Applied surface | Important limit |
| --- | --- | --- | --- |
| OpenCode | Yes | Current user text and tool output | Experimental history/system hooks are off by default |
| Claude Code | Yes | Successful tool output | Prompt and assistant output are shadow-only; Node.js 22+ is currently required for the bridge |
| Pi | Yes | System prompt, context messages, and text tool results | Tool narrowing and provider payload rewriting are disabled |
| OpenClaw | Yes | Assembled history and supported message/tool-result text | `contextEngine` is an exclusive slot |
| Hermes | Yes | Main-agent request, instructions, recognized tools, and string tool results | Auxiliary plugin model calls and opaque multimodal content are unchanged |
| Claude Code Channels | Library only | Authorized notification content before emission | Must be called by the channel server after sender authentication |
| NanoClaw | Release skill | Initial and follow-up formatted prompt strings | Requires guarded source customization because no stable middleware exists |
| OpenAI coding CLI | No | None | No supported pre-dispatch replacement hook is available |

Every automatic adapter fails open: timeout, unavailable service, malformed
response, or structural mismatch leaves the original host content unchanged.

## OpenCode

```bash
kendr-opt run opencode
```

Setup installs a dependency-free JavaScript bundle in OpenCode's documented
global local-plugin directory. No `opencode.json` edit, Bun install, npm
registry, or OpenCode source checkout is needed.

Set `KENDR_OPTIMIZER_SHADOW=1` to analyze without applying replacements.
`KENDR_OPENCODE_EXPERIMENTAL_HISTORY=1` and
`KENDR_OPENCODE_EXPERIMENTAL_SYSTEM=1` opt into the audited experimental hooks.

## Claude Code

```bash
kendr-opt run claude-code
```

Setup writes the plugin to Kendr's data directory. The launcher starts the
local bridge on `127.0.0.1:7332` and supplies the plugin directory to Claude
Code. It stops that bridge on exit if it started it.

Organizations that restrict HTTP hooks must allow these exact local URLs:

```text
http://127.0.0.1:7332/hooks/claude-code/user-prompt-submit
http://127.0.0.1:7332/hooks/claude-code/post-tool-use
http://127.0.0.1:7332/hooks/claude-code/stop
```

For a persistent Claude marketplace install directly from this public repo:

```bash
claude plugin marketplace add Kendr-AI/Kendr-Optimizer
claude plugin install kendr-optimizer@kendr
```

The bridge still needs to be running; the `kendr-opt run claude-code` path
handles that lifecycle automatically.

## Pi

```bash
kendr-opt run pi
```

Setup writes the compiled extension into Pi's documented global extension
directory. Pi auto-discovers it; no package-manager entry is required.

Optional variables are `KENDR_OPTIMIZER_TIMEOUT_MS`,
`KENDR_OPTIMIZER_SHADOW`, and `KENDR_OPTIMIZER_OBSERVE_OUTPUT`.

## OpenClaw

```bash
kendr-opt run openclaw
```

Setup uses OpenClaw's plugin installer, enables `kendr-optimizer`, and selects
it as `plugins.slots.contextEngine`. When another engine owns that exclusive
slot, setup stops and names the conflict. `--force` is required to replace it:

```bash
kendr-opt run openclaw --force
```

The adapter does not modify OpenClaw's model, provider, channel, or gateway
authentication settings.

## Hermes

```bash
kendr-opt run hermes
```

Setup installs the dependency-free Python source through Hermes's user-plugin
loader and calls `hermes plugins enable kendr-optimizer`. It does not install a
second Hermes environment or change the active provider.

Optional variables are `KENDR_OPTIMIZER_TIMEOUT_MS`,
`KENDR_OPTIMIZER_RISK_CEILING`, and `KENDR_OPTIMIZER_SHADOW`.

## GitHub Release packages

The repository and GitHub Release are the distribution source. No Kendr npm or
PyPI publication is required.

| Asset | Use |
| --- | --- |
| `kendr-optimizer-opencode-0.1.3.tgz` | Manual OpenCode package deployment |
| `kendr-optimizer-claude-code-0.1.3.tgz` | Manual Claude Code plugin/bridge deployment |
| `kendr-optimizer-claude-channels-0.1.3.tgz` | Channel-server library deployment |
| `kendr-optimizer-pi-0.1.3.tgz` | Manual Pi package deployment |
| `kendr-optimizer-openclaw-0.1.3.tgz` | Manual OpenClaw package deployment |
| `kendr_optimizer_hermes-0.1.3-py3-none-any.whl` | Manual Hermes environment deployment |
| `kendr-optimizer-nanoclaw-0.1.3.tar.gz` | Guarded NanoClaw customization skill |

Package managers can install a release URL directly when a managed deployment
needs that form. For example:

```bash
npm install -g https://github.com/Kendr-AI/Kendr-Optimizer/releases/download/v0.1.3/kendr-optimizer-claude-code-0.1.3.tgz
```

```bash
python -m pip install https://github.com/Kendr-AI/Kendr-Optimizer/releases/download/v0.1.3/kendr_optimizer_hermes-0.1.3-py3-none-any.whl
```

## Adding another CLI

A new adapter needs a stable host seam before provider dispatch, after tool
execution, or during context assembly. It must map only supported fields,
validate the complete returned shape, preserve unsupported fields, and fail
open. A provider proxy setting by itself is not an integration seam.

For direct adapter development, run the local service with:

```bash
kendr-opt serve --bind 127.0.0.1:7331
```

Use `POST /v1/optimize` with `kendr.optimize/v1`; health and capabilities are
available at `/healthz` and `/v1/capabilities`.
