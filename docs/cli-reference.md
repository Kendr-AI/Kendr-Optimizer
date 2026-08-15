# Kendr Optimizer CLI reference

This page is the command-line contract for the native `kendr-opt` executable in
Kendr Optimizer `0.1.4`. For adapter behavior and provider ownership, also read
the [CLI and provider integration guide](cli-provider-integration.md).

Kendr is a local transformer. It is not an inference gateway, and none of the
optimization commands need a provider credential.

## Command map

| Command | Purpose | Changes local state? |
| --- | --- | --- |
| `kendr-opt` | Open the terminal dashboard when interactive; otherwise print long help | No |
| `kendr-opt dashboard` | Open the read-only terminal dashboard | No |
| `kendr-opt tui` | Visible alias for `dashboard` | No |
| `kendr-opt analyze` | Evaluate an optimization request in forced shadow mode | No, unless output is directed to a file |
| `kendr-opt optimize` | Apply candidates that pass the configured gates | No, unless output is directed to a file |
| `kendr-opt restore` | Reconstruct an original envelope from a recovery capsule | No, unless output is directed to a file |
| `kendr-opt observe` | Compare provider usage with an optional paired baseline | No, unless output is directed to a file |
| `kendr-opt engines` | List native engines and their declared properties | No, unless output is directed to a file |
| `kendr-opt serve` | Run the transform-only HTTP service | Opens a listening socket |
| `kendr-opt setup` | Inspect or install bundled harness adapters | `--list` is read-only; setup writes adapter files |
| `kendr-opt run` | Set up and launch a supported harness beside Kendr | May refresh adapter files and starts child processes |
| `kendr-opt update` | Check for or install a verified GitHub release | A check only updates its cache; installation replaces the executable |

Use `kendr-opt --help` for root help, `kendr-opt <command> --help` for one
command, and `kendr-opt --version` for the installed version. The short forms
are `-h` and `-V`.

## Process and stream conventions

- Exit status `0` means the requested operation completed. An available update
  reported by `update --check` is a successful result and also exits `0`.
- Kendr runtime failures exit `1`. Command-line syntax errors reported by the
  argument parser exit `2`.
- The JSON transformation commands read a complete JSON value. Their input and
  output both default to `-`, meaning stdin and stdout.
- Human-readable results go to stdout. Errors and passive update notices go to
  stderr.
- `--compact` changes only JSON whitespace. It does not select more aggressive
  optimization.
- `--json` on `update` produces one compact, versioned JSON object on stdout.
  JSON and other machine-output paths never contain terminal color codes.
- Supplying an output path replaces that file; it does not append.

## Interactive dashboard contract

<!-- DASHBOARD-CONTRACT: keep synchronized with the CLI dashboard implementation. -->

The dashboard is an explicitly read-only terminal interface:

```bash
kendr-opt dashboard
kendr-opt tui
```

It has no command-specific options beyond `--help`. Running `kendr-opt` with no
subcommand opens the same dashboard only when both stdin and stdout are
terminals. When either stream is redirected, no-argument invocation prints the
root long help to stdout and exits successfully instead. An explicit
`dashboard` or `tui` request requires usable terminal input and output; it
returns a clear runtime error rather than silently changing behavior when used
in a pipe.

The dashboard contains five panels:

| Panel | Contents |
| --- | --- |
| Quick start | Inspect support, configure a harness, launch it, and check updates |
| Commands | Native command summary and common examples |
| Harnesses | Supported automatic integrations and boundaries |
| Updates | Check, install, channel, and passive-notice behavior |
| Trust | Local-processing and release-verification boundaries |

Keyboard controls:

| Keys | Action |
| --- | --- |
| Left / Right or Tab / BackTab | Switch panels |
| Up / Down or `j` / `k` | Scroll the active panel |
| `?` | Toggle dashboard help |
| `q` or Escape | Exit |
| Ctrl-C | Exit |

The standalone dashboard uses the terminal alternate screen and raw mode, and
restores the terminal on normal and error exits. `kendr-opt run` never wraps a
child harness in that screen or mode. Layout becomes compact on small
terminals; below 28 columns or seven rows it shows a resize prompt instead of
pretending that hidden panel content is available.

Interactive accents use Kendr saffron (`#E2712A`) on Kendr ink (`#2B2925`) for
readable contrast, with deep saffron (`#B8551A`) for decorative borders where
the terminal supports color. Status is always communicated with text as well
as color. Set `NO_COLOR` to keep the dashboard while disabling adaptive line
color. `TERM=dumb` is treated as unsuitable for alternate-screen
operation: no-argument invocation prints long help, while an explicit
`dashboard` or `tui` request returns a runtime error. Redirected setup, support,
error, JSON, and version output remains plain.

## Harness setup

```text
kendr-opt setup [HARNESS] [--list] [--force]
```

Supported harness values and accepted aliases are:

| Canonical value | Aliases | Executable Kendr expects on `PATH` |
| --- | --- | --- |
| `opencode` | — | `opencode` |
| `claude-code` | `claude` | `claude` plus Node.js 22 or newer |
| `pi` | `pi-agent` | `pi` |
| `openclaw` | — | `openclaw` |
| `hermes` | `hermes-agent` | `hermes` |

With no harness, setup configures every supported harness it detects on
`PATH`. With a harness, it configures only that integration.

| Option | Meaning |
| --- | --- |
| `--list` | Print automatic and manual support without changing files. This is a full support list, not a per-harness filter. |
| `--force` | Authorize replacement of a same-name unmanaged adapter or a conflicting exclusive OpenClaw context-engine slot. |

Examples:

```bash
kendr-opt setup --list
kendr-opt setup
kendr-opt setup claude-code
kendr-opt setup openclaw --force
```

Setup owns only Kendr's same-name adapter paths. It does not read or replace
provider keys, provider URLs, models, or billing settings. See the
[installation-location table](cli-provider-integration.md#setup-without-launch)
for each harness.

## Launching a harness

```text
kendr-opt run <HARNESS> [--force] [-- <HARNESS_ARGUMENT>...]
```

`run` performs idempotent setup, starts the native service on
`127.0.0.1:7331` if needed, starts the Claude hook bridge on
`127.0.0.1:7332` when applicable, launches the host CLI, and stops only the
processes it started when the host exits. It leaves a previously running local
service or bridge in place.

Pass every host-specific argument after `--` so Kendr does not parse it:

```bash
kendr-opt run opencode -- --model anthropic/claude-sonnet-4
kendr-opt run claude-code -- --resume
kendr-opt run hermes -- --help
```

When no host arguments are supplied, OpenClaw is launched with its `tui`
subcommand; other harnesses receive no default argument. `--force` has the same
adapter-ownership meaning as it does for `setup`. A nonzero host exit becomes a
Kendr runtime failure after locally started helper processes are stopped.

## JSON transformation commands

The following common options apply to `analyze`, `optimize`, `restore`, and
`observe`:

| Option | Default | Meaning |
| --- | --- | --- |
| `-i, --input <PATH>` | `-` | Read one JSON value from a file, or from stdin with `-` |
| `-o, --output <PATH>` | `-` | Write one JSON value to a file, or to stdout with `-` |
| `--compact` | off | Emit compact JSON rather than pretty-printed JSON |

The [request example](../examples/request.json),
[paired observation example](../examples/observation-paired.json), and
[unpaired observation example](../examples/observation-unpaired.json) are
ready-to-run inputs. The language-neutral request and receipt schemas are under
[`spec/`](../spec/README.md).

### `analyze`

```text
kendr-opt analyze [-i <PATH>] [-o <PATH>] [--compact]
```

`analyze` accepts a `kendr.optimize/v1` request and forces
`policy.shadow = true` in its in-memory copy. Engines and preservation gates
run, but the returned `content` remains the original envelope. The receipt uses
`shadow` when a hypothetical candidate portfolio qualifies, and its optimized
measurement describes that hypothetical result.

```bash
kendr-opt analyze --input examples/request.json --output analysis.json
```

Use this command before applying replacements to a new workload.

### `optimize`

```text
kendr-opt optimize [-i <PATH>] [-o <PATH>] [--compact]
```

`optimize` accepts a `kendr.optimize/v1` request and returns an
`OptimizeOutcome` containing:

- the complete authoritative `content` envelope;
- a `kendr.receipt/v1` receipt with measurements, attempts, verification
  checks, warnings, and the applied/skipped/reverted status;
- an optional generation recommendation; and
- an optional recovery capsule.

The original content is returned when no candidate passes, shadow mode is set
in the request, or the whole portfolio misses its gain threshold.

```bash
kendr-opt optimize --input examples/request.json --output outcome.json
cat examples/request.json | kendr-opt optimize --compact > outcome.json
```

The request controls phase, typed messages and tools, tokenizer profile,
optional pricing hints, host capabilities, risk ceiling, gain thresholds,
latency budget, cache policy, recent-message preservation, and enabled engines.
Use the checked-in schema rather than treating an adapter's provider-native
payload as a Kendr request.

### `restore`

```text
kendr-opt restore [-i <PATH>] [-o <PATH>] [--compact]
```

`restore` accepts the `RecoveryCapsule` returned in an optimization outcome and
emits the reconstructed original `ContentEnvelope`. It validates the recorded
original digest. A null or missing capsule is not restorable.

With `jq`, an apply-and-restore flow is:

```bash
kendr-opt optimize -i examples/request.json -o outcome.json
jq '.recovery' outcome.json > recovery.json
kendr-opt restore -i recovery.json -o original-envelope.json
```

Recovery data may contain original model-visible values. Protect it with the
same care as the input payload.

### `observe`

```text
kendr-opt observe [-i <PATH>] [-o <PATH>] [--compact]
```

`observe` compares provider-reported optimized usage with an optional paired
baseline. It can report token and comparable-currency cost deltas. A result is
`verified: true` only when a paired baseline is supplied, the optimized task is
reported successful, and the pair shows a positive net usage or cost saving.
Without a paired baseline, usage is recorded but savings remain unverified.

```bash
kendr-opt observe -i examples/observation-paired.json
kendr-opt observe -i examples/observation-unpaired.json --compact
```

Provider usage is supplied by the caller; Kendr does not query provider billing
APIs.

## Engine inventory

```text
kendr-opt engines [-o <PATH>] [--compact]
```

`engines` emits a JSON array of native engine descriptors. Each descriptor
contains its ID, version, summary, risk level, reversibility declaration, and
cache-safety declaration.

```bash
kendr-opt engines
kendr-opt engines --compact > engines.json
```

`--output` defaults to `-`, as with the transformation commands.

## Transform-only service

```text
kendr-opt serve [--bind <IP:PORT>]
```

The default bind address is `127.0.0.1:7331`.

| Method | Path | Operation |
| --- | --- | --- |
| `GET` | `/healthz` | Boundary and health status |
| `GET` | `/v1/capabilities` | Contracts, tokenizers, engines, and boundary flags |
| `GET` | `/v1/engines` | Native engine list |
| `POST` | `/v1/analyze` | Shadow analysis of a `kendr.optimize/v1` request |
| `POST` | `/v1/optimize` | Optimization of a `kendr.optimize/v1` request |
| `POST` | `/v1/restore` | Recovery-capsule restoration |
| `POST` | `/v1/observe` | Paired or unpaired usage observation |

```bash
kendr-opt serve
kendr-opt serve --bind 127.0.0.1:7331
curl http://127.0.0.1:7331/healthz
```

Set `RUST_LOG` to a compatible tracing filter when changing service log
verbosity, for example `RUST_LOG=kendr_optimizer_cli=debug`. Stop the service
with Ctrl-C.

The service has no provider egress, inference endpoints, or credential store.
It also has no authentication and currently permits an operator-supplied
non-loopback bind. Keep it on loopback or behind a separately protected local
transport.

## Verified updates

```text
kendr-opt update [--check] [--json] [--channel <stable|preview>]
                 [--force] [--reinstall]
```

| Option | Meaning |
| --- | --- |
| `--check` | Check metadata without downloading an archive or replacing the executable |
| `--json` | Emit one compact `kendr.update/v1` result instead of human text |
| `--channel stable` | Consider only full releases |
| `--channel preview` | Consider prereleases and full releases |
| `--force` | Authorize replacement of a deliberately unreceipted standalone executable; it bypasses no release-verification gate |
| `--reinstall` | Re-verify and reinstall the same eligible version; never downgrade |

Examples:

```bash
kendr-opt update --check
kendr-opt update --check --json
kendr-opt update --check --channel stable
kendr-opt update
kendr-opt update --reinstall
```

Without `--channel`, an official install follows the channel in its adjacent
install receipt. Current prerelease installers record `preview`; an
unreceipted check also defaults to `preview`. The highest published release in
the selected channel must itself be complete and immutable. Kendr does not
silently fall back past an ineligible newer release and does not downgrade.

The JSON status is one of `up_to_date`, `update_available`, or `updated`. The
object also carries current and latest versions, channel, prerelease and
immutability flags, GitHub release identity and URL, target, archive name and
SHA-256, check time, and the installed executable path when applicable.

Normal replacement requires the matching `.kendr-opt-install.json` ownership
receipt installed beside the executable. `--force` is for a standalone binary,
not for overriding immutability, digest, archive, candidate-smoke, or channel
checks. Use a package manager to update an executable owned by that package
manager.

Interactive `setup` and `run` may print a cached update notice to stderr before
continuing. Passive checks require terminal stderr, do not run in CI, cache a
successful result for 24 hours, back off for six hours after failure, and show
the same installed/latest pair no more than once per 24 hours. They never
install an update.

For the complete update trust and rollback contract, see
[CLI updates](cli-provider-integration.md#cli-updates) and the
[threat model](threat-model.md#supply-chain-and-extensions).

## Environment variables

### Native CLI

| Variable | Scope | Effect |
| --- | --- | --- |
| `KENDR_HOME` | `setup`, `run`, update cache | Relocates Kendr's adapter data directory and places the update cache under `$KENDR_HOME/cache` |
| `KENDR_NO_UPDATE_CHECK` | Passive notices | Truthy values `1`, `true`, `yes`, or `on` disable passive checks; explicit `update` commands still work |
| `KENDR_UPDATE_CACHE_DIR` | Update cache | Advanced override for the directory containing `update.json`; prefer an absolute path |
| `RUST_LOG` | `serve` | Sets the tracing filter; defaults to `info` when absent or invalid |
| `NO_COLOR` | Human terminal UI | Presence disables adaptive terminal color |
| `TERM=dumb` | Dashboard | Disables full-screen dashboard operation; no-argument invocation prints long help and an explicit dashboard request errors |
| `CI` | Human color and passive notices | Presence disables adaptive color and suppresses passive update checks |
| `GITHUB_ACTIONS` | Human color and passive notices | Presence disables adaptive color and suppresses passive update checks |

Location discovery also follows standard platform variables:

- `PATH` (and Windows `PATHEXT`/`COMSPEC`) for harness command discovery;
- `HOME` or Windows `USERPROFILE` for the user home directory;
- `XDG_CONFIG_HOME` for OpenCode configuration;
- `XDG_DATA_HOME` or Windows `LOCALAPPDATA` for Kendr adapter data; and
- `XDG_CACHE_HOME` or Windows `LOCALAPPDATA` for the update cache.

### Release installers

| Variable | Effect |
| --- | --- |
| `KENDR_VERSION` | Selects a release tag such as `v0.1.4` instead of the installer's default |
| `KENDR_INSTALL_DIR` | Selects the directory that receives the executable and install receipt |
| `KENDR_NO_MODIFY_PATH` | On Windows, truthy `1` or `true` prevents the installer from adding the install directory to the user `PATH` |

`KENDR_DOWNLOAD_BASE_URL`, `KENDR_INSTALLER_TEST_MODE`, and
`KENDR_ALLOW_INSECURE` are restricted failure-injection controls for the
repository's numeric-loopback installer tests. They are not alternate
production distribution settings.

### Bundled adapter controls

These variables are consumed by adapters, not by the native optimizer command
parser. Managed `run` is designed around the documented loopback defaults.

| Variable | Consumers | Purpose or default |
| --- | --- | --- |
| `KENDR_OPTIMIZER_ENDPOINT` | Claude Code, OpenCode, Pi, Hermes, NanoClaw | Transform service endpoint; normally `http://127.0.0.1:7331` and restricted to literal loopback by guarded adapters |
| `KENDR_OPTIMIZER_TIMEOUT_MS` | OpenCode, Pi, Hermes, NanoClaw | Per-transform timeout; adapter-specific bounds apply |
| `KENDR_OPTIMIZER_SHADOW` | Claude Code, OpenCode, Pi, Hermes | Analyze supported seams without returning replacements |
| `KENDR_CLAUDE_BRIDGE_PORT` | Claude Code bridge | Bridge port, normally `7332`; the managed launcher assumes its documented default |
| `KENDR_OPENCODE_EXPERIMENTAL_HISTORY` | OpenCode | Opt into the audited experimental history hook |
| `KENDR_OPENCODE_EXPERIMENTAL_SYSTEM` | OpenCode | Opt into the audited experimental system hook |
| `KENDR_OPTIMIZER_OBSERVE_OUTPUT` | Pi | Analyze finalized assistant messages for diagnostics without replacing them |
| `KENDR_OPTIMIZER_BACKOFF_MS` | Hermes, NanoClaw | Circuit-open interval after a local transform failure |
| `KENDR_OPTIMIZER_TOKENIZER` | Hermes, NanoClaw | `approximate`, `cl100k_base`, or `o200k_base` where supported |
| `KENDR_OPTIMIZER_RISK_CEILING` | Hermes | `pass_through`, `representation_safe`, `recoverable`, `extractive`, or `learned`; default `representation_safe` |
| `KENDR_OPTIMIZER_MIN_GAIN_TOKENS` | Hermes | Non-negative minimum token gain, default `8` |
| `KENDR_OPTIMIZER_MIN_GAIN_PERCENT` | Hermes | Minimum percentage gain from 0 through 100, default `1` |
| `KENDR_OPTIMIZER_PRESERVE_RECENT` | Hermes | Recent-message count, default `6` |
| `KENDR_OPTIMIZER_MAX_TOOL_RESULT_CHARS` | Hermes | Tool-result policy bound, default `24000` |

Use the harness-specific references for exact bounds and applied surfaces:
[Claude Code](../integrations/claude-code/README.md),
[OpenCode](../integrations/opencode/README.md),
[Pi](../integrations/pi-agent/README.md),
[OpenClaw](../integrations/openclaw/README.md),
[Hermes](../integrations/hermes-agent/README.md), and
[NanoClaw](../integrations/nanoclaw/README.md).

`KENDR_PRESERVE_BEGIN` and `KENDR_PRESERVE_END` are literal protected-content
markers, not environment variables.

### Updater test feature

The official binary does not permit update-authority overrides. If
`KENDR_UPDATE_API_URL` is set in an official build, the explicit update fails
closed. A separately compiled `update-test-server` fixture permits it only for
a numeric-loopback HTTP server with `KENDR_ALLOW_INSECURE=1`; that feature is
for native release tests and is not packaged.

## Automation recipes

Inspect the selected receipt fields without writing a file:

```bash
kendr-opt optimize --input examples/request.json --compact |
  jq '{status: .receipt.status, before: .receipt.original.tokens, after: .receipt.optimized.tokens, reduction: .receipt.estimated_input_reduction_percent}'
```

Check for an update without parsing human text:

```bash
kendr-opt update --check --json | jq -e '.schema_version == "kendr.update/v1"'
```

List engine IDs and risks:

```bash
kendr-opt engines --compact | jq -r '.[] | [.id, .risk] | @tsv'
```

For automation, always invoke a concrete command. Do not depend on the
no-argument dashboard/help choice, human color, dashboard layout, or passive
update notices.

## Troubleshooting

- **No supported harness found:** install the host CLI, ensure its executable is
  on `PATH`, then run `kendr-opt setup --list` and `kendr-opt setup <harness>`.
- **Adapter path already exists:** inspect it first. Use `--force` only when
  Kendr should own that exact same-name adapter or exclusive OpenClaw slot.
- **Port 7331 is already open:** `run` assumes the existing listener is the
  service to use and does not stop it. Verify `/healthz` before launching.
- **Explicit dashboard fails in a pipe:** use `kendr-opt --help` or a concrete
  machine command. The full-screen interface deliberately requires a terminal.
- **Update refuses an install destination:** use the owning package manager or
  reinstall from the official release. Reserve `--force` for a knowingly
  standalone binary.
- **Need provider configuration:** configure it in the host harness. Do not
  point a provider base URL at Kendr.
