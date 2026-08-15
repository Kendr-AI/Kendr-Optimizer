# Contributing

Kendr Optimizer welcomes issues, benchmark fixtures, engine proposals, adapters,
and code. Production engines must be independently implemented and must not add
a runtime dependency on another optimizer. Participation is governed by the
[Code of Conduct](CODE_OF_CONDUCT.md).

Before submitting a change:

1. Read [the product boundary](docs/decisions/0001-product-boundary.md) and
   [provenance policy](docs/provenance.md).
2. Add a receipt-visible reason for every candidate, rejection, and no-op.
3. Declare the engine risk level, reversibility, cache behavior, and
   preconditions.
4. Add golden and adversarial tests. Recoverable transforms require an exact
   restoration test.
5. Run:

       cargo fmt --all -- --check
       cargo clippy --workspace --all-targets --all-features -- -D warnings
       cargo test --workspace --all-features --locked
       python -m unittest discover -s scripts/tests -p "test_*.py"
       python -m unittest discover -s benchmarks/runners/tests -p "test_*.py"
       python scripts/check_repository_hygiene.py
       python scripts/build_third_party_licenses.py --check
       python benchmarks/runners/verify_release.py --release releases/v0.1.0-benchmark.5 --require-complete-attempts
       python benchmarks/runners/rank_release.py --release releases/v0.1.0-benchmark.5 --output benchmarks/rankings/v0.1.0-benchmark.5 --check
       python scripts/check_whitepaper_pdf.py

Algorithm proposals should include a primary paper or official implementation
reference, its license, the gap being addressed, and a benchmark plan. A raw
compression-ratio improvement is not sufficient; downstream task quality and
total cost must be measured.

Published benchmark and ranking directories are immutable. Correct historical
evidence by creating a new revision, never by rewriting an existing one. Public
artifacts must not contain machine-specific user-profile paths, generated
caches, or repository-development assistant control files. Target-harness
plugin and skill packaging under `integrations/` remains part of the product and
is checked through narrow, audited exceptions.

Before pushing any release tag, a repository administrator must verify that
GitHub release immutability is enabled:

    gh api --hostname github.com repos/Kendr-AI/Kendr-Optimizer/immutable-releases \
      -H "X-GitHub-Api-Version: 2026-03-10" --jq '.enabled'

The result must be `true`. This endpoint requires repository Administration
read access, which the standard Actions `GITHUB_TOKEN` cannot request; the tag
workflow therefore also checks the published release's `immutable` field and
fails if GitHub does not seal it.

Contributions are accepted under Apache-2.0.
