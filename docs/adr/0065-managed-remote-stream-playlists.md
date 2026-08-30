# ADR 0065: Managed Remote Stream playlists

## Status

Accepted (2026-08-10).

The extractor trust and execution portions are superseded by
[ADR 0066](0066-repository-backed-add-ons.md). Playlist manifests,
stable enumeration identity/order, atomic sidecars, managed ownership,
health, and ordered playlist materialization remain active.
Metadata sidecar ownership and v2 availability/classification behavior are
revised by [ADR 0068](0068-addon-metadata-sidecars-and-local-replacements.md).

## Implementation

Partial — implemented in Phase 8D/8E and revised under ADR 0066. The scanner,
SQLite/MySQL ownership migration, signed add-on list/play integration,
health/troubleshooting checks, localized admin management, tag facets, managed
playlist ordering, and playout refresh path are tracked by milestones P8D-02
through P8E-03 and the `remote-stream-playlists` parity record. Named extractor
configuration and raw single-stream scripts are no longer implemented.

Completed on 2026-08-24 under milestone `crf-02`. Stable identity deltas and
current-schedule dependency resolution now produce coalesced durable intents
only for Classic playouts that can consume the changed media or managed
playlist. The resolution marker precedes generated-file mutation, remains
active through the playlist/intent commit and ownership-sidecar publication,
and is completed only after publication. Startup replays interrupted work
before mutating workers start. SQLite and MySQL production-path regressions,
including live two-connection InnoDB races, prove restart continuity and
unrelated-row isolation; aggregate localized health reports stale or failed
work without exposing source paths.

Revised on 2026-08-30: version 4 playlist sidecars persist the validated
effective liveness beside each generated definition hash. Playlist health runs
recursive discovery and managed-file inspection on Tokio's blocking pool,
trusts liveness only while the current definition matches that hash, and no
longer parses one YAML definition per durationless entry on the async
maintenance executor. Older sidecars remain readable but are refreshed before
their cached result can be reused.

## Context

At the time of this decision, ErsatzTV and ErsatzRS modeled one Remote Stream as
one YAML definition containing either a URL or a playback script. That was
practical for small libraries, but it did not keep a folder of definitions
synchronized with an upstream series or playlist. ErsatzTV has no
playlist-ingestion counterpart to
`YamlRemoteStreamDefinition`; the feature is therefore an ErsatzRS extension.

Remote Stream folders already become tags, Remote Streams already participate
in ordinary playlists, and `PlaylistItem.SortOrder` already represents a stable
order. The missing boundary is a safe, repeatable way to enumerate an upstream
playlist, materialize ordinary definitions, and identify the files and database
playlist that the synchronization process may replace.

Enumeration executes an external program and can receive credential-bearing
playlist URLs. The contract must therefore prevent an arbitrary command found
inside a scanned library from being executed, bound process output and runtime,
redact diagnostics, and delete only state proven to be synchronization-owned.

## Decision

A Remote Stream playlist manifest is a YAML file whose name ends in
`.playlist.yml` or `.playlist.yaml`. The scanner recognizes that suffix before
ordinary Remote Stream definition parsing. A manifest named `Example.playlist.yml`
defaults to an expansion folder named `Example` beside the manifest. Generated
definitions in that folder remain ordinary Remote Stream YAML files, so the
existing folder-derived tag, scan, probe, playback, and scheduling behavior is
reused without a second media kind.

The manifest identifies an installed add-on by its stable reverse-DNS identity.
The package must be enabled and implement `addon.check.v1`,
`remote-stream.list.v1`, and `remote-stream.play.v1`. Unknown, disabled,
incompatible, or modified packages fail visibly; there is no command fallback,
and a manifest never embeds an executable path. Hand-authored single-stream
definitions use the same installed add-on boundary.

The add-on runtime invokes two capability operations:

```text
list  with ERSATZRS_REMOTE_STREAM_PLAYLIST_URL=<playlist-url>
play  with ERSATZRS_REMOTE_STREAM_URL=<item-url>
      and ERSATZRS_REMOTE_STREAM_SEEK=<seek>
```

`list` writes UTF-8 NDJSON to standard output, one object per item in upstream
order. `id`, `url`, and `title` are required non-empty strings. Optional fields
are `duration_seconds`, `thumbnail_url`, `is_live`, and the rich metadata fields
defined by the manifest reference. Rich metadata reuses ErsatzTV's common
Remote Stream metadata facets where they fit: genres, tags, studios, actors
with roles, release date, rating, and plot. Provider collection and original
versus current broadcaster values remain separately persisted because
flattening them would make faithful XMLTV/custom-template projection
impossible. Unknown fields are ignored. Duplicate IDs, malformed lines, a non-zero exit, timeout,
oversized output, or more than the configured maximum number of entries rejects
the entire enumeration. Standard error is retained only as a bounded, redacted
tail. `play` writes streamable media bytes to standard output and receives the
current seek in the established Remote Stream format. Media URLs are resolved
only during playback; synchronization never stores an expiring resolved stream
URL.

Playlist synchronization is serialized by the existing scan worker. It applies
manifest defaults, explicit exclusions, title and duration filters, maximum
item count, and selected ordering before writing files. The enumeration entry
`id` is the stable identity and determines a filesystem-safe generated file
name; ordinal position never determines identity. A versioned
`.ersatzrs-playlist.json`
sidecar records manifest and ordered-playlist hashes, timestamps, last outcome,
entry order, managed file names, validated effective liveness, exclusions, and
ownership. State and generated definitions are written with a sibling temporary
file and atomic rename.
The manifest hash includes the selected add-on identity, installed version, and
verified package content hash, so activating updated package content forces one
metadata refresh even when the normal synchronization interval has not elapsed.

Only files listed as owned by a valid sidecar for that manifest may be rewritten
or removed. An upstream removal follows the manifest's `trash` or `keep` policy;
unlisted hand-authored definitions are never modified. Unchanged manifest and
playlist hashes update only the last-success timestamp. Missing durations remain
materialized for diagnosis but are unschedulable, and health reports them.
Membership or order changes invalidate playouts that consume the affected tag,
collection, or managed playlist through the existing refresh mechanism.

When `create_playlist` is true, synchronization owns one ErsatzRS `Playlist`
whose items are Remote Streams in effective upstream order. Add nullable
`Playlist.ManagedSource` text, containing the Remote Stream library ID plus the
library-relative manifest path in a normalized, versioned value. It is unique
when non-null and is the sole database ownership marker. Synchronization creates
or updates the playlist and replaces its items transactionally after generated
definitions have been scanned and resolved to media-item IDs. It removes no
user-owned playlist and never changes a playlist whose marker points elsewhere.

Managed playlists are visible and schedulable through the ordinary playlist
surfaces but read-only in the editor. The UI directs changes to the owning
manifest. Turning `create_playlist` off removes the managed playlist only after
the marker is verified; deleting or invalidating a manifest does not silently
delete it. A failed synchronization preserves the last complete generated files,
sidecar, and ordered playlist.

Playlist state drives a persisted health check for unknown or unavailable
add-ons, failed or stale synchronization, missing durations, and truncation.
Troubleshooting may perform one bounded enumeration on demand and exposes only
the parsed count and bounded redacted diagnostics. Playlist URLs and local
package paths are not written to logs or health details.

## Historical decision — superseded by ADR 0066

The 2026-08-10 decision originally selected case-insensitive extractor names
from the JSON `settings.remote_stream_extractors` command allowlist and retained
raw `script:` for single-stream YAML. Its `list` and `play` subprocess shape was
the predecessor of the versioned add-on capabilities. ADR 0066 removed the
setting, raw commands, and compatibility fallback; those rules are no longer
current behavior.

## Consequences

Operators can turn an upstream series or playlist into tagged Remote Streams and
an optional ordered playlist without changing playback semantics. Stable IDs and
explicit ownership make reordering cheap and cleanup safe, while the database
marker lets existing scheduling consume upstream order directly.

Generated definitions can also carry provider-native descriptive, publication,
classification, credit, collection, and broadcaster metadata. Under ADR 0068,
the host writes descriptive metadata to a same-stem NFO sidecar while YAML owns
only playback and synchronization fields.

Enumeration depends on an installed, enabled package and network availability.
ErsatzRS installs and updates packages only through a configured signed
repository and never silently substitutes another provider. A provider that
omits durations may produce diagnostic-only entries until the add-on can supply
reliable values.

The schema intentionally diverges from ErsatzTV through the nullable
`Playlist.ManagedSource` ownership marker and three nullable Remote Stream
metadata fields: collection name, original broadcasters, and current
broadcasters. Playlist administration and repository projections must preserve
the marker so user actions cannot convert, overwrite, or delete
synchronization-owned state accidentally. The metadata fields preserve
provider distinctions that ErsatzTV's shared genre/tag/studio tables cannot
represent losslessly.

## Alternatives considered

- Store a raw list command in each manifest: rejected because scanning a media
  folder would become arbitrary command execution.
- Resolve direct media URLs during synchronization: rejected because signed URLs
  expire and playback already owns resolution, seeking, cancellation, and output
  validation.
- Encode order only as tags or filename prefixes: rejected because tags are
  unordered and prefixes would churn stable media identity when upstream order
  changes.
- Reuse `Playlist.IsSystem` as the ownership marker: rejected because system
  playlists and source-managed playlists have different lifecycles and owners.
- Delete every YAML file in the expansion folder on refresh: rejected because
  operators may safely keep hand-authored definitions beside managed entries.

## References

- [ADR 0057: Direct Remote Stream pipelines and playlist actions](0057-direct-remote-stream-pipelines-and-playlist-actions.md)
- [ADR 0011: Generated playlist read-only UI](0011-generated-playlist-read-only-ui.md)
- [Remote Stream playlist manifest reference](../reference/remote-stream-playlist-manifests.md)
- `$ERSATZTV_SRC/ErsatzTV.Core/Streaming/YamlRemoteStreamDefinition.cs`
