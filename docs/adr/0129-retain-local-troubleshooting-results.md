# ADR 0129: Retain local troubleshooting results

## Status

Accepted 2026-10-24 by explicit maintainer approval of Download Results only.

## Implementation

N/A — this decision retains the existing local archive/download implementation
and deliberately excludes upstream diagnostic uploads. UCU-12 owns verification
of the retained behavior and ordered row dispositions.

## Context

ErsatzTV v26.11.4 rows242–244 introduce age encryption, diagnostic uploads and
upload guidance. The archive is encrypted for ErsatzTV's maintainers and sent
to their service. ErsatzRS already builds Native/Next troubleshooting archives
and offers localized Download Results. No ErsatzRS upload destination,
encryption-recipient ownership or retention service is configured.

## Decision

Retain Download Results as the diagnostic-sharing flow. The user downloads the
archive and chooses whether and where to share it. Do not add Upload Results,
an upload service, its encryption dependencies or a background uploader for
this catch-up. Never reuse ErsatzTV's endpoint or recipient keys.

Record row242's upload-only encryption infrastructure as not applicable under
this decision, row243's upload behavior as a deliberate approved parity
deviation while retaining the existing archive, and row244's upload guidance
as not applicable. Keep guidance truthful about available downloads and the
supported Native engine; upstream's planned Native removal is not adopted.
These source dispositions do not themselves satisfy runtime or window gates.

Any future diagnostic-upload service requires a new accepted design covering
destination, encryption recipients, consent, bounded transfer/cancellation,
retention and deletion, plus explicit deployment authority.

## Consequences

No automatically uploaded diagnostic identifier is available. Users retain
control of sharing the archive. Existing archive path confinement, unique
relative names, media exclusion and bounded collection remain required;
this decision does not claim that diagnostic archives contain no private data.

## Alternatives considered

- Operate an ErsatzRS encrypted-upload service: declined by the maintainer.
- Reuse ErsatzTV's service and keys: ownership is not transferable.
- Remove diagnostic sharing: unnecessary; local downloads remain useful.

## References

- [ADR 0095](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- [UCU source ledger](../../plans/upstream-catch-up.md), rows242–244
- `docs/progress/milestones.toml`, milestone `ucu-12`
- `crates/ersatzrs-web/src/host/troubleshooting.rs`
