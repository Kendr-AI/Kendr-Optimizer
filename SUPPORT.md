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

## Security and conduct

Do not report vulnerabilities, exposed credentials, private prompt content, or
conduct incidents in public issues. Follow [SECURITY.md](SECURITY.md) for
vulnerabilities and [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) for conduct reports.
