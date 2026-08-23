# Add-on media-list contract reference

`media-list.list.v1` through `media-list.list.v5` define the frozen list and
item record schemas. Their original one-shot compatibility path enumerates one
provider-selected list through an enabled add-on. The host invokes the add-on
with positional operation `list`, sets
`ERSATZRS_MEDIA_LIST_URL` to the configured source hyperlink, and applies a
60-second timeout and a 16 MiB stdout limit.

The source hyperlink is validated before the provider is invoked. It must be
an absolute HTTP or HTTPS URL without userinfo, and its host must satisfy the
installed package's [`network_hosts`](addon-packages.md#permission-declarations)
declaration. A rejection is reported as an `unsupported-url` operator error.
Previously saved lists remain visible with that failure in administration.
The host forwards the URL parser's canonical serialization, so the add-on
receives the same authority and host that passed validation.

Every item record may carry a `content_kind`, and that value is the entry's
effective classification: ErsatzRS stores it, shows it per entry in the list
editor, and uses it to choose that entry's NFO root. The list-wide value is only
the default new entries inherit and an explicit bulk action. The host's `song`
and `image` classifications have no wire equivalent; a package is asked to
classify freely (`auto`) rather than being sent a value this contract rejects.

The add-on is optional in the administration dialog: when the selection is left
empty, ErsatzRS resolves it from the URL host against installed packages'
schema-v4 [`default_source_hosts`](addon-packages.md#permission-declarations)
declarations, then against the configured default remote stream processor. The
resolved add-on is stored with the list and can be changed afterwards from the
list editor, which re-imports the list through the newly selected package. When
nothing resolves, or when two packages claim the host equally, the operator must
choose.

Output is UTF-8 newline-delimited JSON. Blank lines are ignored. The first
non-blank line must be one list record; every remaining line must be one item
record. Unknown fields are rejected, and the bounded contract error names the
offending field. Optional arrays treat an omitted field and explicit `null` as
the same absent value.

Capability versions are frozen wire contracts. A new optional or required
field must be introduced under a new capability version; the host does not
silently accept speculative fields emitted against an existing version.

Version 3 adds structured Remote Stream fragment and artwork fields. The host
rejects these fields from v1 or v2 providers.

Version 4 adds the shared rich metadata document, song and image content
classifications, and an explicit collection role. The same document is used at
list and item scope by every provider and by the one ErsatzRS review editor;
provider-specific metadata fields or editors are not part of the host contract.
Provider executables remain responsible for complete extraction. In particular,
the official YouTube-compatible package runs the updateable `yt-dlp` binary for
the full playlist/video JSON and uses Deno only to translate that result to v4;
the mapper and host do not duplicate yt-dlp's provider logic.

Version 5 adds one provider-neutral `liveness` field for Remote Stream items.
It is `unknown`, `finite`, or `live`. Older versions adapt conservatively:
legacy true means live, a positive duration means finite, and false, omitted,
or null without a positive duration remains unknown. Non-Remote-Stream items
must not publish a liveness value.

## Resumable import orchestration

An add-on may additionally declare `media-list.import.v1` and at least one
supported `media-list.list.vN` capability. The import capability changes how
work is transported and checkpointed; it does not define a second item schema.
The host selects the highest common list schema and invokes the package with
positional operation `discover` or `enrich`. Each invocation receives one
strict JSON document on standard input and emits bounded UTF-8 NDJSON.

### Discover

One `discover` request has these fields:

| Field | Type | Requirement |
|---|---|---|
| `source_url` | string | Canonical validated source URL; 1 to 8,192 UTF-8 bytes. |
| `record_capability` | string | Supported frozen `media-list.list.v1` through `media-list.list.v5` schema. |
| `cursor` | string or null | Opaque continuation value; 1 to 4,096 UTF-8 bytes without control characters. Omitted on the first page. |
| `limits.max_items` | positive integer | Host-selected page cap, at most 250. |
| `limits.max_output_bytes` | positive integer | Host-selected output cap, at most 4 MiB. |

The first NDJSON line is a `page` control record:

| Field | Type | Requirement |
|---|---|---|
| `record_type` | string | Exactly `page`. |
| `complete` | boolean | `true` only for the final page. |
| `next_cursor` | string or null | Required when `complete` is false and omitted when it is true. |
| `total_hint` | non-negative integer or null | Presentation hint only; at most 10,000 and never proof of completeness. |
| `revision_token` | string or null | Stable provider revision, 1 to 384 UTF-8 bytes without control characters. |

The first page then emits exactly one list record before up to 250 item stubs.
Continuation pages emit item records only. Discover retains every overview
field the provider already exposes, but it must not open an item-detail page as
a fallback. The host rejects duplicate identities or ranks, cursor loops,
no-progress pages, changing revision tokens, output beyond 4 MiB, and more than
10,000 items. Each page invocation has a 45-second deadline and commits only
after validation. Without a revision token, the host verifies a second complete
overview pass before making the draft publishable.

### Enrich

One `enrich` request has these fields:

| Field | Type | Requirement |
|---|---|---|
| `source_url` | string | The same validated list source URL. |
| `record_capability` | string | The selected frozen list-record schema. |
| `provider_id` | string | Identity of exactly one committed overview item. |
| `overview_fingerprint` | string | Non-empty fingerprint of the committed overview stub; at most 384 UTF-8 bytes. |
| `item` | item record | The committed stub; its `provider_id` must match the request. |

The first output line is an `outcome` control record. `complete` and
`unavailable` require exactly one following item record with the requested
identity; an unavailable result must also set item availability to
`unavailable`. `transient_failure` and `permanent_failure` require a bounded
lowercase diagnostic `code` and carry no item, so a failed detail request can
never erase the published overview. Only a transient failure may set
`retry_after_seconds`, capped at 86,400. Each invocation has a 45-second
deadline and a 1 MiB output cap; serialized requests are capped at 512 KiB.

The list detail policy is `automatic` by default or explicitly `manual_only`.
Automatic enrichment starts only after the verified overview has been
published. Manual-only lists retain overview metadata and offer the same
per-item Fetch details action before or after publication. Both paths use one
atomic claim and the same provider-host limiter. The global Add-ons settings
**Minimum detail wait** and **Maximum detail wait** accept positive whole
seconds, default to 2 and 16, and require minimum no greater than maximum. A new
value applies to subsequent random samples without restart, but never shortens
an already persisted next-eligible time. Provider `Retry-After` and retry
backoff may extend it.

Overview pages, per-item outcomes, leases, retries, and progress are durable in
SQLite and MySQL. A restart resumes committed work without refetching completed
items. No incomplete or inconsistent overview generation can publish, while
optional item details never block first publication.

## List record

| Field | Type | Requirement |
|---|---|---|
| `record_type` | string | Exactly `list`. |
| `provider_id` | string | Non-empty identity stable within the add-on; at most 384 UTF-8 bytes. |
| `name` | string | Non-empty display name. |
| `description` | string or null | Optional provider description. |
| `metadata` | metadata document or null | Version 4 only. Provider snapshot used by the shared review editor. |

## Item record

| Field | Type | Requirement |
|---|---|---|
| `record_type` | string | Exactly `item`. |
| `provider_id` | string | Non-empty, unique within the list, and at most 384 UTF-8 bytes. |
| `rank` | integer | Non-negative and unique within the list. |
| `display_title` | string | Non-empty provider-facing item label. |
| `title` | string | Non-empty local matching title. |
| `year` | integer or null | Optional matching year. |
| `season` | integer or null | Optional season number. |
| `episode` | integer or null | Optional episode number. |
| `kind` | string | `movie`, `show`, `season`, `episode`, or `remote_stream`. |
| `guids` | string array or null | Optional local metadata GUID candidates; omitted and `null` both mean no candidates. |
| `source_url` | string or null | Required for `remote_stream`; stable URL used after the generated library is scanned. |

Version 2 adds these item fields:

| Field | Type | Requirement |
|---|---|---|
| `availability` | string | `available`, `unavailable`, or `unknown`; omitted v1 values remain `available`. |
| `availability_reason` | string or null | Optional provider diagnostic code or short reason; at most 2,048 UTF-8 bytes. |
| `content_kind` | string | `auto`, `television_episode`, `movie`, `music_video`, or `other_video`. |

Version 3 adds these item fields:

| Field | Type | Requirement |
|---|---|---|
| `duration_seconds` | non-negative integer or null | Known full-item or fragment duration in whole seconds. |
| `additional_image_urls` | string array or null | Optional non-empty provider artwork URLs. |
| `parent_provider_id` | string or null | Required with `fragment_start_seconds`; identifies the full item and must differ from this item. |
| `fragment_start_seconds` | non-negative integer or null | Inclusive fragment start in whole seconds. |
| `fragment_end_seconds` | non-negative integer or null | Optional exclusive end; when present it must be greater than the start. |

Version 4 adds these item fields and values:

| Field | Type | Requirement |
|---|---|---|
| `metadata` | metadata document or null | Provider snapshot used by the shared review editor. |
| `collection_role` | string | `auto`, `primary`, or `extra`. Rank remains provider order and never implies that the first movie is the main feature. |
| `content_kind` | string | Version 4 additionally accepts `song` and `image`. |

Version 5 adds this item field:

| Field | Type | Requirement |
|---|---|---|
| `liveness` | string or null | `unknown`, `finite`, or `live`; only meaningful for `kind: remote_stream`. |

## Shared metadata document

The optional `metadata` object has the same strict shape on list and item
records. Missing fields keep their compatibility fallback: list title/plot
come from `name`/`description`, while item title, identity, GUIDs, and legacy
artwork keep their existing record values. Unknown fields are rejected.

| Field | Type | Notes |
|---|---|---|
| `title`, `original_title`, `sort_title` | string or null | Display, source-language, and sorting titles. |
| `outline`, `plot`, `tagline` | string or null | Short and full descriptions. |
| `show_title`, `collection` | string or null | Parent series or collection identity where available. |
| `season`, `episode` | integer or null | Season is non-negative; episode is positive. |
| `year` | integer or null | From year 1 through five years beyond the current year. |
| `release_date` | string or null | Exact `YYYY-MM-DD` date. |
| `content_ratings`, `genres`, `tags`, `studios`, `music_labels`, `languages` | string array | Empty members are rejected. |
| `writers`, `directors`, `artists`, `producers` | string array | Provider-neutral credit groups. |
| `original_broadcasters`, `broadcasters` | string array | Source and current broadcast/channel identities. |
| `people` | person array | Each person has a non-empty `name` and optional `role`, integer `order`, and bounded HTTPS `thumb`. |
| `artwork` | artwork array | Each candidate has bounded HTTPS `url`, `role` (`poster`, `fanart`, `thumb`, or `other`), optional positive dimensions, and optional language. |
| `guids` | string array | Stable provider or external metadata identities. |

Each repeated field accepts at most 1,024 values and the encoded document is at
most 256 KiB. Artwork and person-thumbnail URLs are absolute HTTPS URLs without
userinfo, are limited to 2,048 bytes, and must include a host. These are input
bounds; publication separately validates and materializes artwork rather than
letting an NFO depend on an untrusted remote URL forever.

## Review and publication

An initial storage-backed import persists provider documents as a `draft`.
Provider snapshots and operator overrides stay separate; the editor shows their
effective values, lets the operator include or exclude each item, and preserves
provider rank for the optional original-order playlist. Only explicit
**Publish** makes included rows visible to generated NFO/YAML, library
projection, scanning, and playlist membership. Existing lists migrate as
published so an upgrade does not silently withdraw working media.

Refresh replaces provider snapshots while preserving operator overrides and a
movie's explicit primary/extra role. Newly discovered or changed data returns
the list to review, and publication errors remain visible on the list and in
Health Checks. Excluded items retain their provider identity and metadata so
they can be reviewed and re-included later.

Provider values and operator overrides are compared per editable field. A
refresh updates an untouched field, preserves an operator-owned value, and
records a reviewable conflict when both sides changed. Large repeated fields,
including tags, remain structured arrays even though the editor presents them
as compact line-separated text.

Publish stages the complete managed folder and database projection as one
journaled revision. An exact backup remains available until NFO, artwork,
playlist, projection, and revision advancement all succeed. Any failure restores
the previous files and database state; startup recovery completes the same
rollback for an interrupted publication. Revision guards reject stale or
concurrent publishers.

Artwork candidates may be bounded HTTPS URLs or bounded JPEG, PNG, and WebP
uploads. ErsatzRS validates and materializes poster, fanart, thumb, and other
roles as local managed files before publication, so an NFO never depends on a
remote URL remaining available. A file replaced by an operator stays
operator-owned and is not overwritten by provider refresh.

The persisted `AddonMediaListChapter` row links the list, full source item, and
fragment item and stores chapter index, title, start, and optional end. A
refresh reconciles fragment items by provider ID, rebuilds these links, and
deletes fragments omitted from the new chapter input. It does not retain an
obsolete fragment as an `omitted_by_provider` tombstone.

An explicit `availability` value outside the three documented strings rejects
the complete enumeration with a bounded provider-neutral contract error. The
host does not translate provider-specific availability values. Add-ons must
normalize missing, null, or provider-specific values before emitting a record.

The add-on owns provider-specific availability and classification. ErsatzRS
does not contain provider keyword tables or language-specific classification
assets. An imported list can set a host-side content-kind override; `auto`
keeps each item's add-on-provided kind.

One invocation may return at most 10,000 items; exceeding that explicit
contract bound fails the complete synchronization rather than truncating it. A second list record,
duplicate item identity, duplicate rank, invalid UTF-8, malformed JSON, or a
record beyond any bound rejects the complete enumeration before persistence.

ErsatzRS keys persisted lists by add-on ID plus provider ID. Refresh reconciles
items by provider ID, clears their previous local match, then reruns GUID and
title/year/number matching. Generated playlists remain host-owned and keep the
provider rank as playlist order. An existing item omitted from a later snapshot
is retained after the current provider ranks with `unavailable` and reason
`omitted_by_provider`. Its replacement mapping remains intact. Returning the
same provider ID updates that row in place and restores provider playback.

Deleting a media list also deletes its generated system playlist. Uninstalling
an add-on is blocked while it owns media lists through the normal administration
flow. A forced uninstall deletes those list snapshots and their generated
system playlists, while leaving unrelated user playlists intact.

## File-backed list storage

A schema-v3 manifest can opt into host-owned storage:

```toml
[media_list_storage]
path_setting = "MEDIA_STORAGE_PATH"
default_subdirectory = "provider_media"
source_name = "provider"
```

`path_setting` must name a declared `path` setting without a static default.
The host renders an absolute default below the ErsatzRS profile/SQLite folder,
while an operator can save another absolute path. The package must implement a
supported media-list list capability, Remote Stream list v1 or v2, and
`remote-stream.play.v1`.

Enabling the add-on creates the root and an add-on-owned local media source if
missing. Adding a list creates one portable subfolder, uses that folder name as
the Remote Streams library name, and writes `list.playlist.yml` inside it. The
ordinary Remote Stream scanner owns enumeration and generated YAML/NFO pairs,
so no permanent video download or separate interpreter is introduced. A scan is
queued immediately after import; the normal scheduled scanner remains the
recovery path.

Changing the configured root while the add-on is disabled causes the host to
repoint its managed libraries on the next enable/startup. Old files are retained.
Deleting a list unregisters its library but also retains the folder: ErsatzRS
does not recursively delete content below an operator-configurable path.

## Unavailable items and local replacements

Version 2 retains unavailable items in provider order. An administrator can
link one exact Local media version to a persisted item, including in advance
while provider playback is available. The candidate must be an
available scanned movie, episode, music video, or other video with a media file
and a non-attached video stream.

The generated playlist uses the pinned local version only when availability is
explicitly `unavailable`. `unknown` continues to use the provider match and
never activates the replacement. When the provider reports `available` again,
the provider match immediately wins. Missing or invalid Local targets remain as
stale mappings for administration and health reporting. The picker,
administration projection, generated playlist, and health check use the same
validity rule: the target must be an available Local movie, episode, music
video, or other video; the exact version must own that item and have a media
file plus a non-attached video stream.

## NFO ownership and television presentation

Managed lists can be presented as their original Remote Streams or as one
television show. ErsatzRS generates a valid list-level `tvshow.nfo` from the
reviewed metadata before the first television projection; a hand-created empty
placeholder is neither required nor valid metadata. Each full provider item becomes a normal Show/Season/Episode
projection; chapter fragments remain children of that full item. The projected
episode shares the scanned Remote Stream media version, so schedules and
playlists retain the add-on playback URL while using television identity.

The list folder contains `.ersatzrs-nfo-state.json`, which records generated
hashes and ownership separately from the playlist sidecar. Provider output can
update a generated NFO. Editing through administration or changing a valid NFO
outside ErsatzRS adopts user ownership, after which refresh preserves it.
Invalid user XML is never replaced automatically. Admin saves use an ETag and
fail on a concurrent file edit instead of overwriting it.

Descriptive fields such as titles, plot, numbering, dates, ratings, genres,
tags, people, studios, artwork, and GUIDs are stored in the NFO. Identity,
title, plot, date, and episode numbering are projected into the television
tables used by browsing and scheduling; the NFO remains authoritative for the
richer fields. Operational URL, availability, duration, and fragment bounds
remain owned by provider/YAML state. Chapter text is configured on the
dedicated page for one full item, not on the list settings page.
When a storage-backed Remote Stream add-on supplies `chapter_input` through
`remote-stream.item.v2`, the host retains it separately from the operator
override. Operator text wins until **Restore provider chapters** clears that
override. Both sources use the same provider-neutral validation and fragment
derivation path.
