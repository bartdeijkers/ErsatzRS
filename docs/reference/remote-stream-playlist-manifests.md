# Remote Stream playlist manifest reference

Remote Stream playlist manifests are an ErsatzRS extension governed by
[ADR 0065](../adr/0065-managed-remote-stream-playlists.md). A manifest filename
must end in `.playlist.yml` or `.playlist.yaml` and must live below a Local
library whose media kind is **Remote Streams**.

## Manifest fields

Unknown fields are rejected.

| Field | Required | Default | Contract |
|---|---:|---|---|
| `name` | No | Manifest stem | Display name and managed playlist name. |
| `url` | Yes | — | Upstream playlist or series URL passed only to `list`. |
| `extractor` | Yes | — | Case-insensitive name from `settings.remote_stream_extractors`. |
| `folder` | No | Manifest stem | One relative path component; generated definitions are written here. |
| `sync.interval` | No | `24h` | Positive compact duration of at least one minute: `m`, `h`, or `d`. |
| `sync.max_items` | No | `500` | `0` means unlimited; the hard safety limit is 10,000 entries. |
| `sync.order` | No | `upstream` | `upstream`, `newest_first`, or `title`. |
| `sync.on_removed` | No | `trash` | `trash` moves owned files to managed trash; `keep` marks them orphaned. |
| `defaults.is_live` | No | `false` | Applied when the extractor does not mark an item live. |
| `defaults.content_rating` | No | — | Default content rating. |
| `defaults.year` | No | — | Default year when an entry has no year. |
| `filter.include_title` | No | — | Rust regular expression that titles must match. |
| `filter.exclude_title` | No | — | Rust regular expression that titles must not match. |
| `filter.min_duration` | No | — | Minimum `HH:MM:SS[.fraction]`. |
| `filter.max_duration` | No | — | Maximum `HH:MM:SS[.fraction]`. |
| `exclude` | No | `[]` | Extractor IDs omitted from effective membership. |
| `create_playlist` | No | `false` | Maintain a read-only system playlist in effective entry order. |
| `tags` | No | `[]` | Additional tags combined with folder-derived tags. |

Example:

```yaml
name: Example series
url: https://media.example/series/example
extractor: example
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

## Extractor setting

`settings.remote_stream_extractors` is a database-backed JSON array. Names must
be unique case-insensitively, and every entry supplies both commands:

```json
[
  {
    "name": "example",
    "windows": "C:\\extractors\\example.bat",
    "unix": "/opt/ersatzrs/extractors/example.sh"
  }
]
```

The selected platform command must begin with an absolute path to a regular
file. ErsatzRS runs it directly, never through a shell.

## Extractor protocol

```text
<extractor> list <playlist-url>
<extractor> play <item-url> <seek>
```

`list` emits UTF-8 NDJSON on standard output. Each line requires `id`, `url`,
and `title`. Optional playback fields are `duration_seconds`, `thumbnail_url`,
and `is_live`. Optional metadata fields are:

| Field | Shape | Consumer mapping |
|---|---|---|
| `plot` | string | Remote Stream plot; XMLTV `<desc>`. |
| `year` | integer | Fallback year when no `release_date` is present. |
| `release_date` | RFC 3339 string | Full original publication/broadcast instant; XMLTV `<date>` uses its calendar date. |
| `content_rating` | string | Content rating; `nl:` values render with XMLTV system `Kijkwijzer`. |
| `genres` | string array | Genre facets and XMLTV `<category>` values. |
| `tags` | string array | Subject/keyword facets and XMLTV `<category>` values. |
| `collection` | string | Provider collection, retained separately and indexed as a tag; also an XMLTV `<category>`. |
| `people` | `{name, role}` array | Credited people. Roles such as `presenter`, `director`, `producer`, and `guest` map to matching XMLTV `<credits>` children; other roles remain `<actor role="…">`. |
| `producers` | string array | Production companies or producers; persisted as producer credits and studio facets. |
| `original_broadcasters` | string array | Original broadcasters, retained separately and included in studio search. |
| `broadcasters` | string array | Current broadcasters, retained separately and included in studio search. |

The generated episode YAML preserves these fields instead of flattening them
into one tag list. XMLTV custom templates receive the separate collection and
broadcaster values even though the XMLTV DTD has no programme-level collection
or broadcaster element. Duplicate IDs or one malformed line reject the whole
enumeration. The process is limited to 120 seconds, 8 MiB of stdout, a 16 KiB
diagnostic tail, and 10,000 entries.

`play` writes media bytes to standard output. The final argument is the current
source-side seek in the existing Remote Stream `{seek}` contract.

## Owned state

The expansion folder contains `.ersatzrs-playlist.json` and generated `.yml`
definitions. Only files named in that sidecar are synchronization-owned.
Hand-authored sibling definitions are never rewritten or removed. A failed
enumeration records a redacted failed outcome and preserves the last complete
definitions and playlist. Changing the manifest, configured extractor command,
or extractor executable identity forces a refresh before the interval expires.

Entries without a positive duration remain visible for diagnosis but are not
added to the managed playlist. Health reports missing durations, failed or
stale synchronization, unavailable extractors, and truncation.
