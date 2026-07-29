# ADR 0057: Pipe Remote Streams directly and expose playlist actions

## Status

Accepted (2026-07-29)

## Implementation

Implemented (2026-07-29). Remote Stream YAML definitions are scanned into the
persisted model, URL and script sources are probed, and IPTV/HLS playback uses
the resolved source instead of opening the YAML file. Script stdout is piped
directly into the managed FFmpeg process. The Remote Streams browser exposes
Add To Playlist on each card and persists ErsatzTV `CollectionType` value 70.
HLS evaluates liveness at its transcode cursor, consumer termination cancels
and reaps stalled producers independently of stdout, and producer diagnostics
use a bounded rolling stderr tail.

## Context

ErsatzTV exposes script-backed streams through an internal
`/ffmpeg/remote-stream/{id}` HTTP endpoint. FFmpeg then reads from that
loopback endpoint. The extra HTTP hop makes cancellation, producer ownership,
timeouts, and error attribution harder to keep within one playback lifecycle.

ErsatzTV's Remote Stream list offers a per-card Add To Collection action.
Remote Streams are valid playlist items in the shared model, but making that
workflow available requires selecting an item through the bulk toolbar.

Remote Stream definitions and scripts are administrator-owned local
configuration. They may contain signed URLs or arguments that must not appear
in health output or normal playback logs.

## Decision

ErsatzRS resolves one typed Remote Stream source from the persisted definition.
`url` takes precedence when both `url` and `script` are present, matching the
upstream controller. RTSP sources are always treated as live.

URL sources are supplied directly to managed FFmpeg/ffprobe with bounded
network reads and HTTP reconnect options. Script commands are tokenized without
a shell, started in the definition directory, and receive `FFMPEG_BIN` plus
`FFPROBE_BIN` where applicable. Their MPEG-TS stdout is connected directly to
the consumer's stdin. Windows `.bat` and `.cmd` files use the platform command
interpreter because they are not executable images; other commands remain
direct child processes.

The FFmpeg process, producer process, cancellation signal, stderr drains, and
response/HLS worker share one ownership chain. Script VOD seeking is emitted
after the non-seekable input. Live Remote Streams start at zero, disable input
read-rate pacing, and force HLS realtime admission instead of work-ahead.
Liveness is evaluated at the HLS transcode cursor rather than wall-clock time,
so work-ahead never opens a scheduled live source early. Consumer exit,
cancellation, and ffprobe timeout signal the producer independently of stdout
traffic and wait for it to stop. Producer stderr is drained continuously into
a fixed-size rolling tail.
Source diagnostics use a scheme-and-host or executable-basename label; signed
URLs, credentials, query strings, and script stderr are not exposed.

The Remote Streams card menu adds the existing localized Add To Playlist flow.
The backend validates and stores the item as `CollectionType::RemoteStream`
(database value 70). Bulk selection and Add To Collection remain available.

## Consequences

Remote Stream playback has no private loopback HTTP endpoint and no orphaned
producer hop. IPTV and HLS use the same resolver, reconnect policy, liveness
rules, and process ownership. A blocked or overly verbose producer cannot
outlive its consumer or grow stderr memory without bound. Scanner probing
exercises the same URL or script contract used during playback.

Scripts are trusted local configuration and run with the ErsatzRS process
account's permissions. The parser deliberately does not provide shell
expansion, pipelines, redirection, or environment interpolation; operators who
need those features must put them in a script file and invoke that file.

Per-card playlist insertion is an ErsatzRS usability extension over the pinned
ErsatzTV Remote Stream list. Persisted playlist rows remain compatible with
ErsatzTV's existing `RemoteStream = 70` collection type.

## Alternatives considered

- Recreate the internal loopback endpoint. Rejected because it adds a second
  streaming lifecycle without adding an authorization or compatibility
  boundary.
- Run every command through a shell. Rejected because quoting becomes
  platform-dependent and administrator values gain implicit shell expansion.
- Keep playlist insertion bulk-only. Rejected because a single-card workflow
  should not require entering selection mode when the model already supports
  Remote Stream playlist items.

## References

- `$ERSATZTV_SRC/ErsatzTV/Controllers/InternalController.cs`
- `$ERSATZTV_SRC/ErsatzTV/Pages/RemoteStreamList.razor`
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/RemoteStreamFolderScanner.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/Streaming/YamlRemoteStreamDefinition.cs`
- [Remote Stream how-to](../how-to/add-remote-streams.md)
