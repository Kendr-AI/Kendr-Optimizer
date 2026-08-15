# Changelog

All notable changes to Kendr Optimizer will be documented in this file. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once its pre-alpha contracts are declared stable.

## [Unreleased]

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
