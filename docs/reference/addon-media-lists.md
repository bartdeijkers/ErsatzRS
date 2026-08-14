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
| `kind` | string | `movie`, `show`, `season`, or `episode`. |
| `guids` | string array | Optional local metadata GUID candidates. |

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
