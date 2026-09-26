# ADR 0101: Identify provider subtitles by stream index

## Status

Accepted 2026-09-26

Records the upstream behavior included in the maintainer-authorized UCU-06
catch-up scope (row 92, ErsatzTV `f737de89`); introduces no parity deviation.

## Implementation

Implemented (UCU-06 row 92, 2026-09-26)
All 30 affected database and provider tests pass.
Authenticated retrieval and atomic native artifacts are wired into native and
troubleshooting playback. Next prepares provider text in its existing deferred
callback and bounded cache. Web compilation and Burn/Convert deferral tests pass.
Official Next and native FFmpeg decode both providers' selected subtitles and
Dutch audio; Next Burn/Convert preserves source-clock seek. Final review adds
stale-selection rejection and reuse of identical native artifacts. Strict infra/web all-target Clippy passes. UCU-06 owns the remaining window
acceptance and milestone CI.

## Context

Provider external subtitles may have no path or share a provider source path.
Using that path as identity can merge different language tracks. Hourly cleanup
also deletes valid provider sidecars when it assumes all sidecars need paths.
Saved provider Etags can prevent ordinary scans from restoring lost tracks.

## Decision

Match provider subtitles by metadata owner, subtitle kind and stream index.
Preserve row IDs across sidecar path changes and update their path metadata.
Retain existing embedded subtitle extraction invalidation. Keep local sidecar matching
by case-insensitive filename. Duplicate entries for one logical key retain the
existing deterministic first-entry behavior.

Delete empty-path sidecars only for local Movie, Episode, MusicVideo and
OtherVideo owners; retain ownerless cleanup and bounded atomic batches. Health
uses the same predicate as maintenance.

Run a one-time SQLite data migration clearing Jellyfin/Emby movie and episode
Etags where an external subtitle MediaStream exists. Ordinary library scans
restore missing rows. Preserve subtitle rows, extracted files and unrelated
Etags during this repair.

## Consequences

Distinct provider stream indexes remain selectable even with identical or absent
paths. Provider paths are metadata rather than identity. Fixtures must use the
provider's distinct indexes rather than inventing multiple tracks at one index.
Affected items need one rescan; no provider request occurs during migration.

## Alternatives Considered

- Full-path matching: merges provider tracks sharing one source path.
- Index plus path: replaces otherwise identical tracks when their path changes.
- Force every library to rescan: unnecessarily refreshes unaffected items.

## References

- [Upstream catch-up source map](../../plans/upstream-catch-up.md)
- ErsatzTV `f737de89`, `MetadataRepository.UpdateSubtitles`,
  `DeleteOrphanedSubtitlesHandler`, `Fix_MediaServerExternalSubtitles`.
