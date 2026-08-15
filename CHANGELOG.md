# Changelog

All notable changes to Kendr Optimizer will be documented in this file. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once its pre-alpha contracts are declared stable.

## [Unreleased]

## [0.1.4] - 2026-08-16

### Added

- `kendr-opt dashboard`, with the visible `tui` alias, provides a read-only
  five-panel terminal guide for quick start, commands, harnesses, updates, and
  trust boundaries. It is responsive, keyboard navigable, and restores the
  terminal after leaving its confined alternate-screen session.
- Running `kendr-opt` without a subcommand opens the dashboard only when both
  standard input and standard output are terminals; non-interactive invocation
  prints long help and exits successfully instead.
- A standalone CLI reference documents command, output, exit-code, environment,
  and interactive behavior without requiring users to infer the contract from
  shell help.
- The publication-formatted technical whitepaper is a first-class GitHub
  Release asset covered by `SHA256SUMS` and GitHub's recorded SHA-256 digest;
  CI also checks its embedded source digest against the authoritative Markdown.

### Changed

- Human-facing `setup`, `setup --list`, runtime-error, and dashboard presentation
  uses the Kendr saffron palette when the relevant stream supports color. Labels
  remain meaningful in plain output, and `NO_COLOR` or `TERM=dumb` disables
  decoration.
- Machine-readable JSON, version output, update JSON, hosted-harness streams,
  and non-terminal service logs remain undecorated; `serve` logs are emitted on
  standard error.
- Bundled adapter package metadata, runtime version identifiers, marketplace
  metadata, and manual-install artifact names are synchronized at `0.1.4`.

### Security

- The dashboard is informational: it performs no provider, install, update, or
  network action. Existing explicit update verification and replacement gates
  remain authoritative.
- The downloadable whitepaper's checksum and source-identity gates detect
  publication drift inside the GitHub trust boundary; they are not a maintainer
  signature or an operating-system code signature. Markdown remains the
  authoritative source.

## [0.1.3] - 2026-08-16

### Added

- `kendr-opt update --check` reports the newest published channel release when
  it passes every update gate, while `kendr-opt update` downloads, verifies,
  and installs it; both commands support versioned JSON output and explicit
  `preview` or `stable` channels.
- `kendr-opt update --reinstall` provides a same-version repair path through the
  complete updater verification chain without permitting a downgrade.
- Interactive `setup` and `run` commands can show a rate-limited update notice;
  successful checks are cached for 24 hours, failures back off for six hours,
  and `KENDR_NO_UPDATE_CHECK=1` disables passive checks.
- Official installers now write a versioned, target-bound install receipt next
  to the executable so the CLI has an explicit marker for an installer-managed
  standalone installation.

### Changed

- Release publication and self-update eligibility now require GitHub immutable
  Releases. The updater selects releases by semantic version instead of relying
  on GitHub's `latest` designation, so pre-alpha prereleases remain discoverable
  on the default `preview` channel.
- Setup automatically replaces an older same-name Kendr OpenClaw adapter while
  continuing to require `--force` for an unmanaged or conflicting exclusive
  OpenClaw slot.
- Bundled adapter package metadata, runtime version identifiers, marketplace
  metadata, and manual-install artifact names are synchronized at `0.1.3`.

### Security

- The updater pins the public GitHub repository identity, requires a published
  immutable release, cross-checks GitHub SHA-256 asset digests with the exact
  `SHA256SUMS` asset set, validates archive layout and limits, smoke-tests the
  candidate, and rechecks release metadata before replacement.
- Executable replacement is restricted to a matching official install receipt
  unless `--force` explicitly authorizes a standalone binary. The installed
  candidate is validated again and the previous executable is restored when
  post-install validation fails.
- Updater traffic is limited to GitHub repository/release metadata and release
  assets; no prompt, tool output, provider credential, or Kendr.org request is
  sent. GitHub immutability and checksums provide integrity, not an independent
  maintainer signature or operating-system code signature.

## [0.1.2] - 2026-08-16

### Added

- `kendr-opt setup` installs repository-hosted adapters into detected OpenCode,
  Claude Code, Pi, OpenClaw, and Hermes installations without an npm or PyPI
  registry publication.
- `kendr-opt run` starts the local transform service, launches the selected
  harness, and stops the service when that harness exits.
- GitHub Releases now carry installable Node adapter tarballs, a Hermes wheel,
  and the guarded NanoClaw skill archive alongside the native CLI binaries.
- The README now includes a Claude Code install-to-execution walkthrough with
  a workload-specific local preflight comparison and explicit billing caveat.

### Changed

- The primary installation guide now uses a two-command setup and launch path.
- The OpenCode release includes a dependency-free, single-export local plugin
  bundle for global plugin-directory installation.

## [0.1.1] - 2026-08-12

### Added

- Checksum-verified, build-free CLI installers for PowerShell and POSIX shells.
- Native CLI archives for Windows x64, Linux x64 and ARM64, and macOS Intel
  and Apple Silicon, with bundled project and third-party license notices.
- A tag-gated release pipeline that smoke-tests every native binary, assembles
  deterministic archives, verifies release asset digests, and publishes only
  after the complete CI suite succeeds.

### Changed

- Public branch naming and live repository text now use Kendr-only identity.

## [0.1.0] - 2026-08-12

### Added

- Provider-neutral contracts, native optimization engines, safety validation,
  receipts, a CLI, and a loopback transform service.
- Audited host integrations and reproducible peer-payload benchmark evidence.
- Community governance, conduct, support, citation, and contribution templates.
- A repository-hygiene check for private development controls, generated
  artifacts, and machine-specific paths.

### Changed

- Crate manifests now include publication metadata, the project README and
  license, and registry versions alongside local workspace dependency paths.
- Project, package, support, and citation links now use the canonical
  `Kendr-AI/Kendr-Optimizer` repository.
- Continuous integration now checks repository hygiene on branches and runs the
  complete validation suite, including release metadata checks, for version tags.
- Official checkout and language-setup actions now use their Node 24-compatible
  major versions.

### Fixed

- Generation-policy arithmetic now handles the full unsigned token-count range
  without wrapping signed savings or bypassing configured thresholds.
- Runtime request decoding now enforces the strict fields and object boundaries
  declared by the published `kendr.optimize/v1` JSON Schema.
- Benchmark evidence verification no longer treats GitHub's public `runner`
  service account name as private data.

Benchmark artifact revisions have their own immutable evidence and reports in
`releases/`; they are not software version entries in this changelog.
