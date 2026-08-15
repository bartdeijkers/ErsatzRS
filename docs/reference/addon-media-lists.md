# Add-on media-list contract reference

`media-list.list.v1` or `media-list.list.v2` enumerates one provider-selected
list through an enabled add-on. The host invokes the add-on with positional operation `list`, sets
`ERSATZRS_MEDIA_LIST_URL` to the configured source hyperlink, and applies a
60-second timeout and a 16 MiB stdout limit.

The source hyperlink is validated before the provider is invoked. It must be
an absolute HTTP or HTTPS URL without userinfo, and its host must satisfy the
installed package's [`network_hosts`](addon-packages.md#permission-declarations)
declaration. A rejection is reported as an `unsupported-url` operator error.
Previously saved lists remain visible with that failure in administration.
The host forwards the URL parser's canonical serialization, so the add-on
receives the same authority and host that passed validation.

Output is UTF-8 newline-delimited JSON. Blank lines are ignored. The first
non-blank line must be one list record; every remaining line must be one item
record. Unknown fields are rejected.

## List record

| Field | Type | Requirement |
|---|---|---|
| `record_type` | string | Exactly `list`. |
| `provider_id` | string | Non-empty identity stable within the add-on; at most 384 UTF-8 bytes. |
| `name` | string | Non-empty display name. |
| `description` | string or null | Optional provider description. |

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
| `guids` | string array | Optional local metadata GUID candidates. |
| `source_url` | string or null | Required for `remote_stream`; stable URL used after the generated library is scanned. |

Version 2 adds these item fields:

| Field | Type | Requirement |
|---|---|---|
| `availability` | string | `available`, `unavailable`, or `unknown`; omitted v1 values remain `available`. |
| `availability_reason` | string or null | Optional provider diagnostic code or short reason; at most 2,048 UTF-8 bytes. |
| `content_kind` | string | `auto`, `television_episode`, `movie`, `music_video`, or `other_video`. |

An explicit `availability` value outside the three documented strings rejects
the complete enumeration with a bounded provider-neutral contract error. The
host does not translate provider-specific availability values. Add-ons must
normalize missing, null, or provider-specific values before emitting a record.

The add-on owns provider-specific availability and classification. ErsatzRS
does not contain provider keyword tables or language-specific classification
assets. An imported list can set a host-side content-kind override; `auto`
keeps each item's add-on-provided kind.

One invocation may return at most 10,000 items. A second list record,
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
while an operator can save another absolute path. The package must implement
media-list list v1 or v2, Remote Stream list v1 or v2, and
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
