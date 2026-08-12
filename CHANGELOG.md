# Changelog

All notable changes to Kendr Optimizer will be documented in this file. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once its pre-alpha contracts are declared stable.

## [Unreleased]

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
