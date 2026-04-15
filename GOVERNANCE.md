# Governance

ErsatzRS starts as a single-maintainer project and is designed to become a public
open-source project over time.

## Decision Model

The maintainer makes final decisions on scope, architecture, releases, and
project policy. Significant decisions must be recorded as ADRs when they affect:

- Crate boundaries.
- Runtime architecture.
- Database schema strategy.
- Public APIs.
- External dependencies used across multiple crates.
- Behavior that deliberately diverges from ErsatzTV.

## Contributions

External contributions are welcome once the public repository is active, but
they must follow [CONTRIBUTING.md](CONTRIBUTING.md). A contribution may be
declined when it expands scope too early, conflicts with the porting contract,
lacks tests for risky behavior, or bypasses an ADR requirement.

## Scope Changes

ErsatzRS targets long-term feature parity with ErsatzTV. New behavior that is not
parity work should be marked as one of:

- A deliberate divergence documented in an ADR.
- A deferred idea outside the current phase.
- A rejected idea with rationale.

## Releases

Release policy is documented in [RELEASING.md](RELEASING.md).
