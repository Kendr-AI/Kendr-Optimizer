# Changelog

All notable changes to Kendr Optimizer will be documented in this file. The
format follows [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the
project intends to follow [Semantic Versioning](https://semver.org/spec/v2.0.0.html)
once its pre-alpha contracts are declared stable.

## [Unreleased]

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
- Continuous integration now checks repository hygiene before publication.

Benchmark artifact revisions have their own immutable evidence and reports in
`releases/`; they are not software version entries in this changelog.
