# Add-on media-list contract reference

`media-list.list.v1` enumerates one provider-selected list through an enabled
add-on. The host invokes the add-on with positional operation `list`, sets
`ERSATZRS_MEDIA_LIST_URL` to the configured source hyperlink, and applies a
60-second timeout and a 16 MiB stdout limit.

Output is UTF-8 newline-delimited JSON. Blank lines are ignored. The first
non-blank line must be one list record; every remaining line must be one item
record. Unknown fields are rejected.

## List record

| Field | Type | Requirement |
|---|---|---|
| `record_type` | string | Exactly `list`. |
| `provider_id` | string | Non-empty identity stable within the add-on. |
| `name` | string | Non-empty display name. |
| `description` | string or null | Optional provider description. |

## Item record

| Field | Type | Requirement |
|---|---|---|
| `record_type` | string | Exactly `item`. |
| `provider_id` | string | Non-empty and unique within the list. |
| `rank` | integer | Non-negative and unique within the list. |
| `display_title` | string | Non-empty provider-facing item label. |
| `title` | string | Non-empty local matching title. |
| `year` | integer or null | Optional matching year. |
| `season` | integer or null | Optional season number. |
| `episode` | integer or null | Optional episode number. |
| `kind` | string | `movie`, `show`, `season`, `episode`, or `remote_stream`. |
| `guids` | string array | Optional local metadata GUID candidates. |
| `source_url` | string or null | Required for `remote_stream`; stable URL used after the generated library is scanned. |

One invocation may return at most 10,000 items. A second list record,
duplicate item identity, duplicate rank, invalid UTF-8, malformed JSON, or a
record beyond any bound rejects the complete enumeration before persistence.

ErsatzRS keys persisted lists by add-on ID plus provider ID. Refresh reconciles
items by provider ID, clears their previous local match, then reruns GUID and
title/year/number matching. Generated playlists remain host-owned and keep the
provider rank as playlist order. Every reconciliation rebuilds the generated
playlist projection, so items removed by the provider do not remain as stale
playlist rows.

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
`media-list.list.v1`, `remote-stream.list.v1`, and `remote-stream.play.v1`.

Enabling the add-on creates the root and an add-on-owned local media source if
missing. Adding a list creates one portable subfolder, uses that folder name as
the Remote Streams library name, and writes `list.playlist.yml` inside it. The
ordinary Remote Stream scanner owns enumeration and generated YAML definitions,
so no permanent video download or separate interpreter is introduced. A scan is
queued immediately after import; the normal scheduled scanner remains the
recovery path.

Changing the configured root while the add-on is disabled causes the host to
repoint its managed libraries on the next enable/startup. Old files are retained.
Deleting a list unregisters its library but also retains the folder: ErsatzRS
does not recursively delete content below an operator-configurable path.
