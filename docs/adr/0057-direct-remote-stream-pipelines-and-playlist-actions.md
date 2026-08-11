# ADR 0057: Pipe Remote Streams directly and expose playlist actions

## Status

Accepted (2026-07-29; revised 2026-08-09)

The raw `script:` source and command-parsing portions are superseded by [ADR
0066](0066-signed-repository-backed-add-ons.md). Direct producer-to-FFmpeg
piping, cancellation, bounded diagnostics, source-side seeking, and playlist
actions remain active for add-on-backed producers.

## Implementation

Implemented (revised 2026-08-11). Remote Stream YAML definitions are scanned
into the persisted model, direct URLs and installed add-on producers are
probed, and IPTV/HLS playback uses the resolved source instead of opening the
YAML file. Add-on stdout is piped directly into managed FFmpeg, consumer
termination cancels and reaps stalled producers, and diagnostics use a bounded
rolling stderr tail. The Remote Streams browser exposes Add To Playlist and
persists ErsatzTV `CollectionType` value 70. Raw `script:` sources and the
standalone `{seek}` command-argument contract have been removed; source-side
seek is now supplied through the versioned add-on environment.

## Context

ErsatzTV exposes script-backed streams through an internal
`/ffmpeg/remote-stream/{id}` HTTP endpoint. FFmpeg then reads from that
loopback endpoint. The extra HTTP hop makes cancellation, producer ownership,
timeouts, and error attribution harder to keep within one playback lifecycle.

ErsatzTV's Remote Stream list offers a per-card Add To Collection action.
Remote Streams are valid playlist items in the shared model, but making that
workflow available requires selecting an item through the bulk toolbar.

Before ADR 0066, Remote Stream definitions and scripts were
administrator-owned local configuration. Current definitions can still contain
signed URLs and add-on arguments that must not appear in health output or
normal playback logs.

## Active decision

ErsatzRS resolves a direct URL or an installed add-on producer from the
persisted definition. RTSP sources are always treated as live. Direct URLs are
supplied to managed FFmpeg/ffprobe with bounded network reads and reconnect
options. Add-on media stdout is connected directly to the consumer stdin
without forcing an input format, so FFmpeg and ffprobe detect the streamable
container from its bytes.

The FFmpeg process, producer process, cancellation signal, stderr drains, and
response/HLS worker share one ownership chain. The add-on runtime supplies a
finite producer's source offset through `ERSATZRS_REMOTE_STREAM_SEEK`; the
consumer does not drain that prefix again. Live Remote Streams start at zero,
disable input read-rate pacing, and force HLS realtime admission instead of
work-ahead.
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

## Historical decision — superseded by ADR 0066

The 2026-07-29 revision allowed a raw `script:` command in Remote Stream YAML,
tokenized it without a shell, selected platform command interpreters for batch
files, and used a standalone `{seek}` argument for source-side seeking. ADR 0066
replaced that trust and invocation model with installed package identities and
versioned capabilities. None of those raw-command rules is current behavior.

## Consequences

Remote Stream playback has no private loopback HTTP endpoint and no orphaned
producer hop. IPTV and HLS use the same resolver, reconnect policy, liveness
rules, and process ownership. A blocked or overly verbose add-on cannot outlive
its consumer or grow diagnostic memory without bound. Scanner probing exercises
the same direct-URL or installed-add-on contract used during playback.

Producer containers must be readable from a non-seekable pipe, and diagnostic
text must remain on standard error instead of contaminating the media bytes.
Source-side seeking makes late tune-in proportional to startup latency rather
than elapsed programme time.

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

## Revision history

- **2026-08-09.** Added the opt-in standalone `{seek}` producer contract for
  finite sources. Existing unmarked scripts keep consumer-side seeking.
- **2026-08-11.** ADR 0066 superseded raw scripts and `{seek}` arguments with
  signed add-on identities and `ERSATZRS_REMOTE_STREAM_SEEK`.

## References

- `$ERSATZTV_SRC/ErsatzTV/Controllers/InternalController.cs`
- `$ERSATZTV_SRC/ErsatzTV/Pages/RemoteStreamList.razor`
- `$ERSATZTV_SRC/ErsatzTV.Scanner/Core/Metadata/RemoteStreamFolderScanner.cs`
- `$ERSATZTV_SRC/ErsatzTV.Core/Streaming/YamlRemoteStreamDefinition.cs`
- [Add-on runtime reference](../reference/addon-runtime.md)
