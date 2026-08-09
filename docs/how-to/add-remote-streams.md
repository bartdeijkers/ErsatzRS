# Add Remote Streams

Use a Remote Streams library to schedule an HTTP, HTTPS, or RTSP source, or a
local producer script that writes a media stream to standard output.

## Create the library

In the admin UI, add a Local media source and create a library with media kind
**Remote Streams**. Add one or more library paths, then place a `.yml` or
`.yaml` definition in those paths.

Each definition requires `is_live` and either `url` or `script`:

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

The duration format is `HH:MM:SS` with an optional fractional second. RTSP URLs
are always treated as live. If both `url` and `script` are present, `url` wins
and both values remain stored so the definition can be changed later.

## Use a producer script

A script definition names an executable and its arguments:

```yaml
script: '"tools/example-stream.sh" "programme-id"'
is_live: false
duration: 00:45:00
title: Script-backed programme
```

ErsatzRS starts the command in the definition file's directory and connects its
standard output directly to FFmpeg. FFmpeg detects the media container from the
stream bytes, matching ErsatzTV; streamable MP4, WebM, and MPEG-TS output are
supported. It sets `FFMPEG_BIN` and `FFPROBE_BIN` to the managed runtime paths.
Quote paths or arguments that contain spaces.

Write only media bytes to standard output and send progress or diagnostic text
to standard error. The container must be readable from a non-seekable pipe;
MPEG-TS remains the safest choice for live producers.

Commands are not evaluated by a shell. Put pipelines, redirection, variable
expansion, or other shell features inside a `.sh`, `.bat`, or `.cmd` file and
invoke that file from `script`.

### Seek finite producer streams

For a finite producer that can seek its own remote input, pass `{seek}` as one
complete, quoted argument:

```yaml
script: '"C:\path\to\assets\extractors\youtube\youtube.bat" "https://www.youtube.com/watch?v=<video-id>" "{seek}"'
is_live: false
duration: "00:20:00"
title: "Example YouTube video"
```

ErsatzRS replaces that exact argument with the current playout offset and
removes the corresponding seek from the consumer. This lets the producer open
the source at the requested position instead of making FFmpeg discard the
beginning of a non-seekable pipe. Scripts without the marker keep the original
consumer-side behavior. The producer must accept `0` and timestamps in
`HH:MM:SS[.fraction]` form.

The bundled
[`youtube-windows.yml.example`](../../assets/extractors/youtube/youtube-windows.yml.example)
is the canonical Windows template. Its wrapper requires the managed
`FFMPEG_BIN` supplied by ErsatzRS and discovers a manually installed,
unbundled `yt-dlp.exe`; see the adjacent
[`readme.md`](../../assets/extractors/youtube/readme.md) for the supported
locations and a manual test procedure.

Remote Stream scripts are trusted local configuration. They run with the same
operating-system permissions as ErsatzRS. Keep credentials outside the YAML
when the producer can load them from a protected file or environment variable.

## Scan and use the item

Scan the library from **Media > Libraries**. A valid definition appears under
**Media > Remote Streams**. An invalid definition does not stop sibling files
from scanning; an existing item with a newly invalid definition is marked
unavailable until the definition is fixed and rescanned.

From a Remote Stream card you can:

- add the item to a collection;
- add the item directly to a playlist; or
- select several items and use the bulk actions.

Remote Stream playlist items use the same editors and scheduling paths as other
playlist media. A live item with no duration is intended for Classic Flood
scheduling, where it fills the available window. Configure a duration for
finite content and for scheduling modes that need a known item length.

## Troubleshoot

The Remote Stream health check validates stored definitions without contacting
the source or running scripts. It reports invalid definitions and warns when a
non-live item has no usable duration.

When playback fails:

1. rescan the Remote Streams library;
2. check **System > Health Checks** for structural errors;
3. verify that a URL is reachable from the ErsatzRS host, or run the producer
   script as the ErsatzRS service account;
4. confirm that script stdout is a media container FFmpeg can probe from a
   non-seekable pipe and that diagnostic text goes to stderr, not stdout.

Playback logs redact full Remote Stream URLs. A scheme and host, or the producer
executable name, remains visible so failures can still be correlated safely. A
producer failure also records the playback surface, channel number, effective
source-side seek, failure class, exit status, stderr byte count, and a sanitized
stderr tail of at most 2 KiB. Authentication values, cookies, signed URL
details, control characters, and local paths are removed before the excerpt is
logged.

The health check and diagnostic logging do not run the producer or contact its
source. They also do not add a retry. For rate-limited extractors such as
YouTube, inspect the first captured failure and validate the wrapper manually
once before trying channel playback again; repeated playback attempts can turn
a configuration error into a provider cooldown or ban.
