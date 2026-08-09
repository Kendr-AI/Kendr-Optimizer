# Governance

KendrOptimizer uses a lightweight maintainer-led model. The goal is to make
technical decisions in public, preserve the optimizer's safety boundary, and
keep benchmark evidence reproducible.

## Roles

**Contributors** propose changes through issues and pull requests. Anyone who
follows the Code of Conduct and contribution requirements may participate.

**Reviewers** are contributors with demonstrated subject-matter experience who
regularly provide useful review. Reviewers may approve changes, but approval by
a maintainer is required to merge.

**Maintainers** have write access to the repository. They triage reports,
protect releases, merge changes, publish packages, and enforce project policy.
Repository access controls are the authoritative record of current maintainers.

## Decision process

Routine fixes and documentation changes use lazy consensus through pull-request
review. A maintainer may merge when required checks pass, relevant feedback is
resolved, and there is no substantiated objection.

Changes to public contracts, safety guarantees, risk classification, benchmark
methodology, governance, or the product boundary require a public design issue
or decision record. The proposal must describe compatibility, security, quality,
and evidence implications. At least one maintainer must approve it, and material
objections must be resolved before merge.

Maintainers may act immediately to contain a security issue, credential leak,
malicious artifact, or release-integrity failure. The project will publish an
explanation after disclosure is safe.

When maintainers cannot reach consensus, the existing behavior remains in
place. The decision may be revisited with new evidence.

## Maintainer changes

Existing maintainers may invite a contributor who has shown sustained technical
judgment, constructive review, respect for project boundaries, and reliable
stewardship. Except when privacy or safety requires otherwise, the decision is
recorded publicly.

A maintainer may step down at any time. Access may be removed for prolonged
inactivity, repeated policy violations, compromised credentials, or conduct
that threatens users or the project. The maintainer concerned does not decide
their own conduct case.

## Releases and artifacts

Only maintainers publish crates or create project releases. A release must pass
continuous integration, document user-visible changes, and use the same source
revision as its published artifacts. Interdependent crates are published in
dependency order.

Published benchmark directories are immutable evidence. Corrections require a
new benchmark revision that records what changed; historical results are not
silently rewritten.

## Amendments

Governance changes use the significant-change process above. The pull request
must explain the motivation and any effect on contributor or maintainer rights.

