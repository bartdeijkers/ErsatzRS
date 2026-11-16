# ADR 0137: Preserve provider track identities with physical index maps

## Status

Proposed

## Implementation

Partial — row295's index calculation, stable identity storage, ordinary scan
repair and localized Health diagnostics have focused passing evidence. Next
ordinary AV/embedded tracks, probe hints and caption preparation consume the map
and bind it to mutable source authority. Native HLS and MPEG-TS/Direct final
requests use the same physical boundary, including transport fallback/retries.
Background extraction, qualified source-reader integration and actual Native
runtime proof remain in `ucu-16-continuation`; the row is not fully accepted.

## Context

Jellyfin indexes identify entries in a mixed provider list, including external
files. FFmpeg indexes identify streams in the physical input file. Upstream
1a566770 corrects the latter by subtracting preceding external entries.

ErsatzRS also persists exact reusable track selections and playout snapshots.
Replacing a saved track's identity with its physical index can silently change
its meaning. A controlled SQLite regression selects a Dutch external subtitle
at provider index2 before normalization, then selects an English embedded track
when its physical index becomes2. Existing selection intentionally prefers the
embedded stream when a physical subtitle and sidecar reuse an index; changing
that precedence would break an already tested contract.

## Decision

Keep provider track identities in persisted MediaStream, Subtitle, exact pins,
custom selectors and scheduling identities. Store a separate selected-version
map from provider identities to physical indexes. Only physical input routing
and independent native-layout qualification use the mapped values. External
subtitle retrieval continues to use the provider identity and its existing
authentication, ownership and preparation contracts.

Version the persisted Jellyfin projection so an ordinary claimed scan revisits
legacy records even with an unchanged or absent ETag. Publish the mapping and
projection revision transactionally with successful metadata replacement. Keep
old rows, cache files, disabled-library state and saved choices until success;
report remaining repair through Health. Missing or invalid mapping must not
silently become a guessed physical index for a Jellyfin version requiring it.

## Consequences

Existing source pins and seeded scheduling identities need no global rewrite.
Physical routing gets an explicit boundary with bounded SQLite mapping reads.
Native and Next must apply it consistently to selected video, audio, embedded
subtitles and qualified source paths. The map is untrusted provider evidence;
actual file identity, native stream layout, revision and ownership checks remain
mandatory. Local, Plex and Emby retain their existing index contracts.

## Alternatives Considered

- Normalize persisted identities and rewrite pins: rejected because sidecar and
  embedded indexes can collide and existing reusable/scheduled identities change.
- Prefer sidecars on collisions: rejected because it reverses an existing exact
  media-version selection contract and merely moves the ambiguity.
- Reject or clear every old pin: rejected because it discards valid user choices.
- Infer physical indexes at playback from persisted sidecars: insufficient;
  external audio is intentionally not represented as a physical source stream.

## References

- [Milestone evidence](../progress/milestones.toml), `ucu-16-continuation`
- [Catch-up plan](../../plans/upstream-catch-up.md), row295
- [ADR 0095](0095-native-pipeline-instead-of-ersatztv-next-engine.md)
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Jellyfin/JellyfinApiClient.cs`
