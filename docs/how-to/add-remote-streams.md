# Add Remote Streams

Use a Remote Streams library to schedule a direct HTTP, HTTPS, or RTSP source,
or let an installed add-on resolve a provider URL when playback starts.

## Create the library

In the admin UI, add a Local media source and create a library with media kind
**Remote Streams**. Add one or more library paths, then place a `.yml` or
`.yaml` definition in those paths.

A direct source requires `url` and `is_live`:

```yaml
url: https://stream.example/live/channel.m3u8
is_live: true
title: Example live channel
plot: A continuously available example feed.
content_rating: TV-G
```

For finite content, configure a duration when the source cannot report one:

```yaml
url: https://stream.example/program.mp4
is_live: false
duration: 01:30:00
title: Example programme
year: 2026
```

The duration format is `HH:MM:SS` with an optional fractional second. RTSP
URLs are always treated as live.

## Use a provider add-on

This part requires a development build plus a configured trusted repository.
The official repository and its production public key are not published yet.

Install, configure, check, and enable the provider under **Settings >
Add-ons**. Then add its stable reverse-DNS identity alongside the original
provider URL:

```yaml
url: https://www.youtube.com/watch?v=<video-id>
addon: org.ersatzrs.addon.yt-dlp
is_live: false
duration: "00:20:00"
title: Example remote video
```

ErsatzRS asks the enabled add-on to play the URL and connects its standard
output directly to managed FFmpeg. FFmpeg probes the media container from the
bytes; streamable MP4, WebM, and MPEG-TS are supported. For finite playback,
the host supplies the current playout offset so the add-on can seek the source
instead of forcing the consumer to discard the start of a non-seekable pipe.

The Beeld & Geluid package follows the same contract for one Schatkamer episode:

```yaml
url: https://schatkamer.beeldengeluid.nl/serie/<series-id>/<series-slug>/aflevering/<episode-id>
addon: org.ersatzrs.addon.beeldengeluid
is_live: false
title: Episode title
```

Do not place executable commands or secrets in Remote Stream YAML. The legacy
`script:` field and the old extractor setting are rejected. Add-on secret
settings store only an environment-variable name or secret-file path; the
accepted forms are defined by the [package reference](../reference/addon-packages.md).

## Synchronize an upstream playlist

Install an add-on that exposes both `remote-stream.list.v1` and
`remote-stream.play.v1`, then place one `<name>.playlist.yml` manifest in a
Remote Streams library:

```yaml
name: Example series
url: https://media.example/series/example
addon: org.example.ersatzrs.series
folder: episodes
sync:
  interval: 24h
  order: upstream
  on_removed: trash
create_playlist: true
```

The exact manifest fields and NDJSON result shape are in the [Remote Stream
playlist manifest reference](../reference/remote-stream-playlist-manifests.md).

After the next library scan, ErsatzRS writes ordinary generated definitions to
the configured sibling folder. Open the library's **Remote Stream playlists**
page to inspect state, synchronize now, troubleshoot one bounded enumeration,
or manage manifests created by the UI. Hand-authored manifests are shown
read-only so comments and key ordering cannot be lost.

Set `create_playlist: true` when the upstream order must be schedulable. The
resulting system playlist follows effective additions, removals, exclusions,
and reordering and remains read-only in the ordinary playlist editor.

## Scan and use the item

Scan the library from **Media > Libraries**. A valid definition appears under
**Media > Remote Streams**. An invalid definition does not stop sibling files
from scanning; an existing item with a newly invalid definition is marked
unavailable until it is fixed and rescanned.

From a Remote Stream card you can add the item to a collection or playlist, or
use the bulk actions for several items. A live item with no duration is intended
for Classic Flood scheduling. Configure a duration for finite content and for
scheduling modes that need a known item length.

## Troubleshoot

The Remote Stream health checks validate definitions, installed add-on state,
and playlist synchronization without contacting providers or running an
add-on. They report missing, disabled, incompatible, or modified packages and
warn when a non-live item has no usable duration.

When playback fails:

1. open **Settings > Add-ons**, run **Check**, and confirm the add-on is enabled;
2. rescan the Remote Streams library;
3. check **Support > Health Checks** for structural or package errors;
4. verify that the declared provider hosts and external commands are available
   to the ErsatzRS service account; and
5. inspect the first sanitized producer failure before retrying playback.

Playback logs redact full Remote Stream URLs, authentication values, cookies,
signed URL details, control characters, local paths, and resolved add-on
secrets. A scheme and host plus the package-local producer name remain visible
for correlation. Failures include the playback surface, channel number,
effective source-side seek, failure class, exit status, stderr byte count, and
a sanitized stderr tail of at most 2 KiB.

For rate-limited providers such as YouTube, validate configuration once before
trying channel playback again. Repeated playback attempts can turn one setup
error into a provider cooldown or ban.
