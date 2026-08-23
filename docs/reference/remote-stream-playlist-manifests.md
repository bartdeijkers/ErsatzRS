# Remote Stream playlist manifest reference

Remote Stream playlist manifests are an ErsatzRS extension that enumerates
items through the [signed add-on runtime](addon-runtime.md). A manifest filename
must end in `.playlist.yml` or `.playlist.yaml` and must live below a Local
library whose media kind is **Remote Streams**.

## Manifest fields

Unknown fields are rejected.

| Field | Required | Default | Contract |
|---|---:|---|---|
| `name` | No | Manifest stem | Display name and managed playlist name. |
| `url` | Yes | — | Absolute HTTP(S) upstream playlist or series URL without userinfo, supplied only to the add-on's `list` operation. |
| `addon` | Yes | — | Reverse-DNS identity of an installed, enabled add-on with list and play v1 capabilities. |
| `folder` | No | Manifest stem | One relative path component; generated definitions are written here. |
| `sync.interval` | No | `24h` | Positive compact duration of at least one minute: `m`, `h`, or `d`. |
| `sync.max_items` | No | `500` | `0` means unlimited; the hard safety limit is 10,000 entries. |
| `sync.order` | No | `upstream` | `upstream`, `newest_first`, or `title`. |
| `sync.on_removed` | No | `trash` | `trash` moves owned files to managed trash; `keep` marks them orphaned. |
| `defaults.liveness` | No | — | Operator override: `unknown`, `finite`, or `live`. |
| `defaults.is_live` | No | — | Legacy override; explicit false means finite and true means live. |
| `defaults.content_rating` | No | — | Default content rating. |
| `defaults.year` | No | — | Default year when an entry has no year. |
| `filter.include_title` | No | — | Rust regular expression that titles must match. |
| `filter.exclude_title` | No | — | Rust regular expression that titles must not match. |
| `filter.min_duration` | No | — | Minimum `HH:MM:SS[.fraction]`. |
| `filter.max_duration` | No | — | Maximum `HH:MM:SS[.fraction]`. |
| `exclude` | No | `[]` | Add-on entry IDs omitted from effective membership. |
| `create_playlist` | No | `false` | Maintain a read-only system playlist in effective entry order. |
| `tags` | No | `[]` | Additional tags combined with folder-derived tags. |

Example:

```yaml
name: Example series
url: https://media.example/series/example
addon: org.example.ersatzrs.series
folder: episodes
sync:
  interval: 24h
  max_items: 500
  order: upstream
  on_removed: trash
create_playlist: true
tags:
  - Example series
```

The selected add-on must implement `addon.check.v1`, a supported Remote Stream
list capability, and `remote-stream.play.v1`. Add-ons are installed,
configured, checked, and enabled under **Settings > Add-ons**; manifests never
contain command paths.

The URL is rejected before provider invocation when its host does not satisfy
the installed package's
[`network_hosts`](addon-packages.md#permission-declarations) declaration. The
failure uses the localized `unsupported-url` diagnostic and names only the
offending host, never the complete URL.
The stored manifest and add-on environment use the URL parser's canonical
serialization, so the invoked add-on interprets the same authority that the
host validated.

## Enumeration result

For `remote-stream.list.v1`, ErsatzRS invokes the package-local entrypoint with
the operation argument `list` and supplies the manifest URL in
`ERSATZRS_REMOTE_STREAM_PLAYLIST_URL`. Standard output must be UTF-8 NDJSON
with one object per line. Each line requires `id`, `url`, and `title`. Unknown
fields are rejected with a bounded error that names the offending field.
Optional arrays treat an omitted field and explicit `null` as the same absent
value.

Capability versions are frozen wire contracts. Add optional or required
fields under a new capability version instead of emitting them speculatively
against an existing version.

Optional playback fields are `duration_seconds`, `thumbnail_url`, and the
legacy `is_live`. Optional metadata fields are:

| Field | Shape | Consumer mapping |
|---|---|---|
| `plot` | string | Remote Stream plot; XMLTV `<desc>`. |
| `year` | integer | Fallback year when no `release_date` is present. |
| `release_date` | RFC 3339 string | Full publication/broadcast instant; XMLTV `<date>` uses its calendar date. |
| `content_rating` | string | Provider content rating; known rating-system prefixes may be projected by consumers. |
| `genres` | string array or null | Genre facets and XMLTV `<category>` values. |
| `tags` | string array or null | Subject/keyword facets and XMLTV `<category>` values. |
| `collection` | string | Provider collection, retained separately and indexed as a tag. |
| `people` | `{name, role}` array or null | Credited people mapped to matching XMLTV credit roles where supported. |
| `producers` | string array or null | Producer credits and studio facets. |
| `original_broadcasters` | string array or null | Original broadcasters and studio search facets. |
| `broadcasters` | string array or null | Current broadcasters and studio search facets. |

Version 2 also accepts stable `provider_id`, `availability`,
`availability_reason`, `content_kind`, `original_title`, `sort_title`,
`outline`, `tagline`, `show_title`, `season`, `episode`, and `guids` fields.
`availability` may be omitted for the v1-compatible `available` default; an
explicit value must be `available`, `unavailable`, or `unknown`. Every other
value rejects the complete enumeration with a bounded provider-neutral
contract error. Add-ons normalize their provider vocabulary before emitting a
record.
The add-on owns provider-specific classification; a saved list content-kind
override takes precedence when it is not `auto`.

Version 3 adds `additional_image_urls`, `parent_provider_id`,
`fragment_start_seconds`, and `fragment_end_seconds`. Version 4 adds
`liveness`, whose only values are `unknown`, `finite`, and `live`. Legacy true
adapts to live, a positive duration adapts to finite, and false/omitted without
a positive duration remains unknown. A v4 entry that supplies contradictory
`liveness` and `is_live` values is rejected. Generated definitions write the
three-state `liveness` field. In a hand-authored manifest, explicitly setting
legacy `defaults.is_live: false` remains an operator decision for finite media;
omitting both default fields leaves the provider value in control.

The host writes playback and availability fields to generated YAML and writes
descriptive metadata to a same-stem UTF-8 `.nfo` sidecar. Television episodes
use `episodedetails`, music videos use `musicvideo`, and movies and other videos
use `movie`. Standard Kodi-style fields are used where available; collection,
full release timestamp, producer, and original/current broadcaster distinctions
use the `urn:ersatzrs:nfo:1` namespace. Kodi, Jellyfin, and Emby can ignore
unknown namespaced elements. Plex requires a metadata agent to consume NFO and
is not a native sidecar compatibility target.

Duplicate IDs, a malformed line, or output after the configured item bound
rejects the whole enumeration.
The process is limited to 120 seconds, 8 MiB of stdout, 64 KiB of stderr, and
10,000 entries. A non-zero exit exposes at most 1 KiB of redacted stderr.

For `remote-stream.play.v1`, the host invokes `play`, supplies the item URL in
`ERSATZRS_REMOTE_STREAM_URL`, supplies the current offset in
`ERSATZRS_REMOTE_STREAM_SEEK`, and reads media bytes from standard output.

## Retained-item metadata and chapters

An add-on may declare `remote-stream.item.v1` to enrich only the full entries
that remain after host limits, filtering, and exclusions. The host invokes
operation `item`, passes newline-separated provider identities in
`ERSATZRS_REMOTE_STREAM_ITEM_IDS`, and accepts one optional metadata record per
identity. Enrichment is additive and non-fatal: an unavailable capability or
invalid response leaves the enumerated entry unchanged.

`remote-stream.item.v2` adds optional `chapter_input`. It is at most 64 KiB of
provider-neutral timestamp/title lines such as `0:00 Introduction`. The add-on
owns provider parsing and whole-second rounding. ErsatzRS validates the text
with the same parser used for operator input and derives v3 child fragments
without another add-on invocation. Omit the field when no chapters are known.

Provider text is stored separately from the operator value. A non-empty
operator value always wins during synchronization, and the item editor can
clear that override to restore the most recently supplied provider chapters.
The generated fragment provider ID and `addon-chapter://` GUID depend only on
the add-on ID, parent provider ID, and whole-second start, so refresh identity
is stable without embedding provider-specific rules in the host.

## Owned state

The expansion folder contains `.ersatzrs-playlist.json` plus generated `.yml`
definitions and `.nfo` sidecars. Only files named in that versioned state are
synchronization-owned.
Hand-authored sibling definitions are never rewritten or removed. A failed
enumeration records a redacted failed outcome and preserves the last complete
definitions and playlist. Changing the manifest, add-on identity, installed
version, or verified package hash forces a refresh before the interval expires.

Entries without a positive duration remain visible for diagnosis but are not
added to the managed playlist. Health reports missing durations, failed or
stale synchronization, unavailable add-ons, and truncation.
