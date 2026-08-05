# ADR 0062: Uniform media metadata facets

## Status

Accepted (2026-08-05).

## Implementation

Implemented 2026-08-05 across the additive SQLite/MySQL schema, local and Plex
scanners, search/smart collections, Smart Shuffle reference-data hydration,
and localized admin detail projections. Existing rows acquire local metadata
on their next authoritative scan; the migration does not inspect files. The
migration clears local-television folder ETags so unchanged seasons cannot
skip that first authoritative facet backfill.

## Context

ErsatzTV persists rich movie and television metadata, and its Music Video and
Other Video NFO readers already expose several shared fields. Songs, images,
and Plex Other Videos do not have one uniform end-to-end facet contract,
however. ErsatzRS consequently exposed different searchable and visible facts
depending on media kind even though the final ErsatzTV schema already uses
sparse multi-owner tables for genres, studios, and tags.

The requested contract needs ratings (including Kijkwijzer), studios, music labels,
languages, genres, and tags to behave consistently without conflating studio
and music label or changing imported ErsatzTV behavior when no new metadata exists.
It must also retain authoritative rescan semantics: disappearing sidecar data
must reveal embedded or fallback data, and stale rows must be removed.

## Decision

Add `ContentRating` to episode, music-video, song, and image metadata with the
existing slash-separated arbitrary-string representation. The episode column
is an ErsatzRS extension that preserves an episode-specific Plex or local NFO
rating while search falls back to the parent show's rating when it is absent.
Add sparse multi-owner
`MusicLabel` and `MetadataLanguage` relations parallel to `Studio`. Music
labels remain a distinct concept and are never stored as studios or provider
tags. ErsatzRS uses the explicit `MusicLabel`, `music_labels`, and
`music_label` names internally and in public contracts; provider `Label`
properties, `<label>` sidecar elements, and embedded `label` aliases remain
accepted as external input formats.

Normalize every multi-value facet by trimming, removing blanks, and
case-insensitive deduplication while preserving the first source order.
Canonicalize known languages through the existing ISO-639 service; preserve an
unknown non-empty language as lowercase raw data. Merge explicit and audio
stream language codes for search, display, and grouping. Images use explicit
language only.

Local metadata precedence is field-local: a non-empty same-basename sidecar,
then case-insensitive FFprobe format metadata, then fallback metadata. Embedded
multi-values split only on semicolon or NUL. Folder-derived song tags are a
lowest-precedence union. Successful scans transactionally replace the rows
owned by the available sources.
Persist embedded-only local metadata as ErsatzTV's `Embedded` metadata kind;
unrelated container tags such as `encoder` do not change provenance.

Retain ErsatzTV's video NFO roots and full Other Video fields. Add ErsatzRS
`<song>` and `<image>` sidecar roots because ErsatzTV has no equivalent
song/image sidecar contract. Enable the schema-supported Plex Other Video path,
including its existing provider ETag, missing/unavailable, path replacement,
remote playback, and stream behavior. Provider facets, people, GUIDs, and
artwork are also authoritatively replaced for Plex Movies and Episodes. Plex
artwork paths include their owning media-source ID; the token-hiding proxy
resolves that source's active connection and per-server token instead of a
process-global current server.

Expose `music_label:` in the shared search language. `language:` resolves display
names and `language_tag:` resolves canonical codes. Add opt-in Smart Shuffle
rules `content_rating`, `language`, and `music_label`; language grouping uses the
canonical code. Version-1 policies remain readable because the policy shape is
additive. This extends [ADR 0019](0019-similar-video-cooldown.md).

Add read-only detail routes `/media/music/videos/:id`,
`/media/music/songs/:id`, and `/media/browser/images/:id`, with
`/media/images/:id` as an alias. Cards and result projections use the canonical
routes. These are ErsatzRS extensions: ErsatzTV's corresponding list cards do
not provide all three detail destinations. Their action bodies are hydrated
islands fed by SSR-resolved payloads so collection, playlist, and media-info
actions remain interactive without changing the server-rendered DOM shape.

## Consequences

Every supported metadata owner can use the same facet projection, search
fields, smart collections, and grouping rules. Missing optional metadata
remains valid and does not create a health warning.

The schema gains two deliberately sparse tables with many nullable foreign
keys. This follows the existing final ErsatzTV schema pattern and keeps owner
deletion cascading, at the cost of wider tables and one index per owner.

Sidecars and embedded tags are scanner-owned read-only metadata. Manual editing
and cross-provider ownership resolution remain out of scope.

Plex Other Video discovery treats Plex personal-media movie sections
(`com.plexapp.agents.none` or `Plex Video Files Scanner`) as Other Videos.
Normal Plex movie sections remain Movies.

## Alternatives Considered

- Store music labels in `Studio` or `Tag`: rejected because it loses meaning and
  makes search/grouping ambiguous.
- Infer image language from directories: rejected because a folder name is not
  reliable language metadata and would make rescans environment-dependent.
- Split embedded values on commas: rejected because commas are valid inside
  studio, music-label, and person names.
- Backfill from files in the migration: rejected because database migration
  execution must not depend on mounted library files or FFprobe availability.
- Keep list-only music-video, song, and image navigation: rejected because the
  uniform linked-facet and action contract needs stable item destinations.

## References

- [ADR 0019: Smart Shuffle for schedule content selection](0019-similar-video-cooldown.md)
- [ADR 0060: Local television folder identity](0060-local-television-folder-identity.md)
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/Nfo/OtherVideoNfoReader.cs`
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Plex/PlexOtherVideoLibraryScanner.cs`
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Plex/PlexOtherVideoRepository.cs`
- [`STYLE.md`](../../STYLE.md)
