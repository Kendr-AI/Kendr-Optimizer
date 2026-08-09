# Security policy

KendrOptimizer processes untrusted prompts, schemas, logs, and tool results. Treat
all optimizer inputs and recovery capsules as sensitive application data.

## Reporting

Please do not disclose a suspected vulnerability in a public issue. Until a
dedicated security inbox is published, open a private security advisory in the
GitHub repository with reproduction steps, affected versions, and expected
impact.

## Security boundary

The core crate has no networking dependency and never executes content. The
optional HTTP process accepts transformation requests and does not forward them
to an LLM provider. It binds to loopback by default. Operators are responsible
for authentication and transport security if they expose it beyond loopback.

Recovery capsules may contain the complete original envelope so they must not be
logged, shared across tenants, or retained without an explicit TTL. Their
embedded SHA-256 digest detects accidental corruption but is not authentication:
an attacker who can change the payload can also replace the digest. Do not move
capsules across a trust boundary without host-supplied authenticated encryption,
a signature, or an HMAC.

Preflight receipts are designed not to copy raw prompt artifacts into diagnostic
details, but they still contain request identifiers, measurements, engine
dispositions, and hashes. Treat receipts as sensitive telemetry.

See [the full threat model](docs/threat-model.md).
