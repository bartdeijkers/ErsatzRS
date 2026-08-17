# ADR 0071: Project add-on media lists as television shows

## Status

Accepted (2026-08-16)

## Implementation

Implemented (working tree, 2026-08-16)

## Context

ErsatzTV models Remote Streams and television episodes as separate media
hierarchies. It has no operation that changes the library presentation of a
provider-owned Remote Stream list. ErsatzRS add-on lists initially exposed only
their generated Remote Stream rows, even when every entry was an episode of one
show. The list editor also exposed chapter input at list scope, although
chapters describe one full source item.

Provider enumeration must remain bounded NDJSON add-on IPC. Descriptive edits
must survive database rebuilds and scans, so database-only episode metadata is
not durable enough for this extension.

## Decision

An add-on media list stores an active and requested presentation. `Remote
Streams` retains the existing model. `Television Show` creates a managed Show,
Season, and Episode projection while preserving the provider Remote Stream as
the operational source. Each projected episode shares the scanned Remote
Stream `MediaVersion`; generated playlists select the Episode identity and the
playback pipeline continues to resolve the same version and URL. Reverting the
presentation first detaches those version links and refuses the operation when
a projected episode is referenced outside the managed list.

The managed folder is the metadata boundary. `tvshow.nfo` owns show metadata,
and every generated YAML definition has a same-stem NFO owning item metadata.
Edits use optimistic ETags and write UTF-8 Kodi-style XML before the database
projection is reconciled. Provider-generated files update until an external or
admin edit adopts user ownership; invalid user XML is retained and reported
instead of overwritten.

Administration has a dedicated page per persisted item. Chapters move to that
page and refresh only the selected full source plus its derived fragments.
Chapter fragments remain children and are not projected as standalone
television episodes.

## Consequences

- Television browsers, schedules, generated playlists, and playback use normal
  Episode identities without duplicating or downloading provider media.
- NFO files are the durable descriptive source; database rows are a rebuildable
  search and scheduling projection.
- A failed conversion remains visible as requested-versus-active state and in
  health diagnostics.
- Reverting or deleting a projection can be blocked by user-owned references,
  preventing silent schedule or collection damage.
- This is an explicit ErsatzRS extension beyond the pinned ErsatzTV model.

## Alternatives Considered

- Change only the item content-kind override: rejected because Remote Streams
  would still have no Show/Season/Episode hierarchy.
- Copy the remote source into a second media version: rejected because the two
  runtime identities could drift and would duplicate stream metadata.
- Store edits only in projection tables: rejected because rescans and database
  rebuilds would discard operator metadata.

## References

- [ADR 0065: Managed Remote Stream playlists](0065-managed-remote-stream-playlists.md)
- [ADR 0068: Keep add-on metadata provider-neutral in the host](0068-addon-metadata-sidecars-and-local-replacements.md)
- [ADR 0070: Treat Schatkamer hyperlinks and chapters as durable media sources](0070-schatkamer-url-sources-and-chapter-fragments.md)
- [Add-on media-list contract reference](../reference/addon-media-lists.md)
