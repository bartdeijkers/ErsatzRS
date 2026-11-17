# ADR 0138: Replace chrono with Jiff

## Status

Accepted 2026-11-17.

## Implementation

Implemented

The workspace, developer tooling and add-on validation use Jiff. SQLite
binds and reads retain the repository's existing text encodings. The
migration and integrated evidence are tracked under `ucu16-jiff-migration`
and the UCU-16 continuation milestone.

## Context

The application uses chrono for UTC instants, civil dates and fixed elapsed
durations. Jiff already supplies the time-zone rules used for programme windows
and Classic scheduling. RustSec RUSTSEC-2026-0543 reports chrono as unmaintained
with no patched version. The maintainer chose to migrate to Jiff.

## Decision

Use Jiff for date and time operations. Start with the version already pinned by
Classic and the programme clock. Keep SQLite text encodings explicit at the
repository boundary. Preserve the current EF-compatible UTC text and deliberate
RFC 3339 encodings by column, including field precision and null handling.

The dependency-light `ersatzrs-time` crate is shared by the domain model and
the standalone add-on protocol. It keeps timestamps compact and ordered by UTC.
Jiff owns civil calendar calculations, local offset resolution and signed elapsed
arithmetic. The shared codec preserves the established seconds/nanoseconds tuple,
fraction grouping, leap-second notation and extended sentinel years. Gregorian
cycles preserve dates beyond Jiff's civil range. Those dates remain useful for
sentinels and storage and are subject to Jiff's range when resolving named zones.

The add-on contract retains its independent version and Git distribution
under ADR 0066. Assemble local source archives with
`cargo package --offline --no-verify --exclude-lockfile -p ersatzrs-time -p ersatzrs-addon-contract`.
Cargo 1.97.1's temporary registry fails to verify the private time dependency.
Extract both archives outside the workspace and use
`patch.crates-io.ersatzrs-time.path` to point Cargo at the extracted time
crate. Build the extracted contract's binaries and examples to verify the
archives without publishing either crate.

Use signed elapsed durations for playback and fixed intervals. Resolve recurring
local schedule times with civil dates and the existing choice of time-zone
transition rules. Preserve ordering, persisted schedule anchors, cutoff
semantics and the exact time encodings in versioned add-on and Next contracts.
For each wire boundary, distinguish its accepted range from the range of
Jiff's `Timestamp`. The teletext add-on protocol accepts the full four-digit
UTC year range, including the final day of year 9999. Preserve that behavior
and its precision instead of replacing the wire fields with `Timestamp` directly.

## Consequences

The feature graph must contain no chrono dependency, including the SQLx and
tooling edges. Full dependency policy and normal application qualification
remain required. Existing media databases and configuration must reopen with
the same timestamps, offsets and schedule behavior. Verify summer and winter
time transitions, subsecond and signed duration behavior, and external wire
data before accepting the migration.

## Alternatives considered

- A temporary advisory exception would keep the maintenance dependency in use.
- SQLx 0.9 has no Jiff feature. Using the repository's explicit text encoding
  retains the existing SQLite format without a third date and time library.

## References

- [RustSec maintenance advisory](https://rustsec.org/advisories/RUSTSEC-2026-0543.html)
- [Jiff](https://github.com/BurntSushi/jiff)
- [SQLite strategy](0080-standardize-on-sqlite.md)
- [UCU-16 continuation](../progress/milestones.toml)
