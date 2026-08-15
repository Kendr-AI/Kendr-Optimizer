# Support

KendrOptimizer is pre-alpha software. Community support is provided on a
best-effort basis, without a response-time or compatibility guarantee.

## Getting help

Before opening an issue:

1. Review the README, architecture, threat model, and harness compatibility
   documentation.
2. Reproduce the problem with the current `main` branch when practical.
3. Remove prompts, recovery capsules, provider credentials, and other sensitive
   application data from receipts and logs.

Use a
[bug report](https://github.com/Kendr-AI/Kendr-Optimizer/issues/new/choose)
for reproducible defects and a feature request for proposed behavior. Include
the KendrOptimizer version or commit, interface or integration, operating
system, Rust version, minimal input, expected behavior, actual behavior, and a
redacted receipt when relevant.

Questions about deploying or modifying a third-party harness should include the
exact upstream version or commit. The project can support the documented adapter
boundary, but it cannot guarantee or troubleshoot every provider, gateway, or
downstream model.

## Supported versions

During pre-alpha development, fixes target the current `main` branch and the
latest tagged pre-release. Older versions may be asked to upgrade before a
report is investigated.

## Updating the CLI

Official installs follow the channel in their install receipt. Current
installers record `preview`, which includes published pre-alpha releases:

```bash
kendr-opt update --check
kendr-opt update --check --json
kendr-opt update
kendr-opt update --json
```

Use `--channel stable` to ignore prereleases. The published `v0.1.2` binary
predates the updater, so install the first newer release manually once from the
[GitHub Releases page](https://github.com/Kendr-AI/Kendr-Optimizer/releases).
Future releases can then update themselves.

Interactive setup and run commands may show a passive update notice. Successful
checks are cached for 24 hours, and CI or stderr without a terminal does not
trigger them. Set `KENDR_NO_UPDATE_CHECK=1` to disable passive checks while
reproducing an issue; explicit update commands still work.

The updater refuses mutable GitHub Releases and checks GitHub asset digests,
`SHA256SUMS`, archive structure, and the candidate executable before replacing
the current binary. These are integrity checks, not a maintainer or OS code
signature. If another package manager owns the executable, update it through
that package manager rather than using `--force`.

## Security and conduct

Do not report vulnerabilities, exposed credentials, private prompt content, or
conduct incidents in public issues. Follow [SECURITY.md](SECURITY.md) for
vulnerabilities and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for conduct reports.
