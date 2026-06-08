# Goal prompt - HLS segmenter state machine and MPEG-TS wrapper parity

Standing brief to close the transport-orchestration parity gaps found after the
channel 1 report: HLS Segmenter skips after about 24 seconds and the stream
stops around 48 seconds. This is a parity implementation goal, not a workaround.

## Goal

ErsatzRS must match ErsatzTV's behavior for:

- HLS Segmenter worker lifetime and state transitions.
- HLS Segmenter startup readiness and persisted segmenter settings.
- MPEG-TS (`StreamingMode.TransportStreamHybrid`, `mode=ts`) as a wrapper
  around the live HLS segmenter.
- MPEG-TS (Legacy) (`StreamingMode.TransportStream`, `mode=ts-legacy`) as the
  distinct legacy concat/direct branch.

When complete, channel playback must not skip or stop at short-content seams
because of transport-mode orchestration differences, and the four user-visible
streaming modes must route through the same effective process model as ErsatzTV.

## Source map

- ErsatzTV HTTP entry points:
  `$ERSATZTV_SRC/ErsatzTV/Controllers/IptvController.cs`
  - `GetTransportStreamVideo`
  - `GetHttpLiveStreamingVideo`
  - `GetLivePlaylist`
  - `GetHlsDirectStream`
- ErsatzTV HLS segmenter:
  `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/HlsSessionWorker.cs`
  `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/Commands/StartFFmpegSessionHandler.cs`
  `$ERSATZTV_SRC/ErsatzTV.Core/Interfaces/Streaming/IHlsSessionWorker.cs`
  `$ERSATZTV_SRC/ErsatzTV.Core/Interfaces/Streaming/IFFmpegSegmenterService.cs`
- ErsatzTV MPEG-TS process paths:
  `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/Queries/GetConcatProcessByChannelNumberHandler.cs`
  `$ERSATZTV_SRC/ErsatzTV.Application/Streaming/Queries/GetWrappedProcessByChannelNumberHandler.cs`
  `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs`
  - `ConcatChannel`
  - `WrapSegmenter`
- ErsatzTV MPEG-TS script support:
  `$ERSATZTV_SRC/ErsatzTV.Infrastructure/FFmpeg/MpegTsScriptService.cs`
  `$ERSATZTV_SRC/ErsatzTV.Core/Domain/ConfigElementKey.cs`
- ErsatzTV FFmpeg/HLS command shape:
  `$ERSATZTV_SRC/ErsatzTV.FFmpeg/OutputFormat/OutputFormatHls.cs`
  `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/PipelineBuilderBase.cs`
  `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegPlaybackSettingsCalculator.cs`
- ErsatzRS target modules:
  `crates/ersatzrs-web/src/transport/iptv/{mod,hls,stream,playlist,variant}.rs`
  `crates/ersatzrs-core/src/hls.rs`
  `crates/ersatzrs-infra/src/hls_worker.rs`
  `crates/ersatzrs-infra/src/playback.rs`
  `crates/ersatzrs-ffmpeg/src/{pipeline,concat,input,options}.rs`
  `crates/ersatzrs-web/src/admin/server_fns/settings.rs`
  `docs/progress/parity/{hls-variant-playlists-segmenter,mpeg-ts-channel-streaming,settings}.toml`

## Current diagnosis

The exposed HLS and IPTV playlist surfaces are mostly present, but the runtime
orchestration is not ErsatzTV-equivalent:

- ErsatzTV `StartFFmpegSessionHandler` reads
  `ffmpeg.segmenter.timeout_seconds`, disables timeout for KeepRunning
  channels, refreshes graphics elements, starts the worker, and waits for
  `ffmpeg.segmenter.initial_segment_count` segments before returning the
  multi-variant playlist. ErsatzRS starts with a hard-coded 30 second timeout
  and returns the variant playlist immediately.
- ErsatzTV `HlsSessionWorker` owns `_transcodedUntil`, `PlaylistStart`,
  `_channelStart`, `_hasWrittenSegments`, `_slugSeconds`, discontinuity state,
  on-demand timeshift/checkpoint updates, and a Seek/Zero/Slug x
  WorkAhead/Realtime state machine. ErsatzRS currently polls the active playout
  item at `Utc::now()`, seeks to `now - PlayoutStart`, spawns a single HLS
  command, and repeats after process exit.
- ErsatzTV `TransportStreamHybrid` / `mode=ts` calls `WrapSegmenter`, which
  reads from `/iptv/channel/{n}.m3u8?mode=segmenter` and emits MPEG-TS. ErsatzRS
  routes `.ts` to one direct stream path regardless of `mode`.
- ErsatzTV `TransportStream` / `mode=ts-legacy` calls the legacy concat path.
  ErsatzRS ignores `?mode=ts-legacy`.
- ErsatzTV can replace the MPEG-TS wrapper command with a configured MPEG-TS
  script. ErsatzRS persists and displays `ffmpeg.default_mpegts_script`, but no
  runtime stream path loads or executes those scripts.

## Locked decisions

- Preserve ErsatzTV behavior by default. Deviations need ADR coverage.
- Do not solve the 24s/48s symptom by changing only HLS flags unless source
  parity proves the state machine is already equivalent.
- Do not reintroduce ErsatzTV's external graphics renderer. Graphics remain the
  ADR 0005 drawvg/native-overlay divergence point.
- Keep HLS Direct copy-mode behavior separate: HLS Direct may stay on the
  direct copy stream path unless source evidence proves a specific parity gap.
- Keep raw media samples and generated stream artifacts out of git.

## Implementation phases

### Phase 0 - Prove the failing shape

- Reproduce channel 1 with the current local DB and the same streaming settings
  that triggered the report.
- Capture HLS segmenter playlist/segment evidence across at least 60 seconds.
- Record active playout rows, item boundaries, channel streaming mode, FFmpeg
  profile, HLS segmenter settings, and FFmpeg stderr tail.
- Repeat a bounded check for HLS Direct, MPEG-TS, and MPEG-TS (Legacy) to prove
  which paths share the failure and which paths only expose dispatch debt.
- Add the resulting notes to the relevant progress/session docs before code if
  they change the current diagnosis.

### Phase 1 - Route and process dispatch parity

- Extend `/iptv/channel/{number}.ts` handling to parse `mode`.
- Match ErsatzTV mixed-mode resolution:
  - Empty or `mixed` resolves the channel setting.
  - HLS channel modes redirect to `.m3u8`.
  - `mode=ts` selects the modern MPEG-TS wrapper.
  - `mode=ts-legacy` selects the legacy concat/direct branch.
- Preserve HDHomeRun behavior: HDHR `.ts` should force a TS mode like ErsatzTV
  and should not redirect to HLS.
- Add focused route/process-selection tests for configured HLS Segmenter, HLS
  Direct, MPEG-TS, and MPEG-TS (Legacy) channels.

### Phase 2 - Segmenter startup and settings enforcement

- Load `ffmpeg.segmenter.timeout_seconds` when starting HLS segmenter sessions.
- Disable idle timeout for `ChannelIdleBehavior.KeepRunning` once that channel
  setting is available in the Rust domain/repository path.
- Load and enforce `ffmpeg.segmenter.initial_segment_count` before returning a
  successful segmenter playlist response.
- Load and enforce `ffmpeg.segmenter.work_ahead_limit` in the worker state
  machine, not just in settings/troubleshooting output.
- Refresh playback graphics elements or equivalent rendered overlay state at
  session start if current Rust graphics selection can otherwise be stale.
- Add tests for configured timeout, configured initial segment count, and
  duplicate session start/touch behavior.

### Phase 3 - HLS segmenter state machine

- Port the ErsatzTV state fields that affect output continuity:
  `_transcodedUntil`, `PlaylistStart`, `_channelStart`, `_hasWrittenSegments`,
  discontinuity sequence mapping, `_slugSeconds`, and worker state.
- Port the state transitions:
  - `SeekAndWorkAhead`
  - `SeekAndRealtime`
  - `ZeroAndWorkAhead`
  - `ZeroAndRealtime`
  - `SlugAndWorkAhead`
  - `SlugAndRealtime`
  - `PlayoutUpdated`
- Request playback process data with explicit equivalents of:
  `StartAtZero`, `HlsRealtime`, `ChannelStart`, `PtsOffset`, target frame rate,
  and troubleshooting flag.
- Port on-demand playout timeshift and checkpoint updates needed by streaming.
- Port slug insertion and error-process fallback if ErsatzRS has the required
  filler/error primitives; otherwise record explicit `ParityDebt` with the
  missing schema/domain owner before claiming this phase closed.
- Keep fMP4 init-cache behavior from the existing Rust implementation and adapt
  the new state machine around it.
- Add unit tests for state transitions and integration tests for two short
  consecutive playout items that cross the 24s/48s window without duplicate
  seeks, missing segments, or premature worker shutdown.

### Phase 4 - HLS command parity details

- Emit `-hls_segment_options mpegts_flags=+initial_discontinuity` for the first
  MPEG-TS segmenter transcode when there is no PTS offset.
- Add `+latm` to HLS MPEG-TS segment options when the selected audio format is
  AAC-LATM.
- Preserve existing fMP4 handling and skip MPEG-TS-only flags for fMP4.
- Snapshot the generated FFmpeg argv for:
  - first MPEG-TS HLS segmenter transcode
  - subsequent MPEG-TS HLS segmenter transcode with PTS offset
  - AAC-LATM MPEG-TS HLS segmenter output
  - fMP4 HLS segmenter output

### Phase 5 - MPEG-TS hybrid wrapper

- Implement the ErsatzTV `WrapSegmenter` equivalent for
  `StreamingMode.TransportStreamHybrid` / `mode=ts`.
- Start or touch the HLS segmenter before wrapping its `.m3u8` URL.
- Read the live segmenter URL through a concat/input object equivalent to
  ErsatzTV's `ConcatInputFile`.
- Emit the upstream wrapper command shape:
  - no stdin
  - thread count 1
  - hide banner
  - loglevel error
  - no stats
  - standard format flags
  - map all streams
  - copy all encoders
  - readrate 1.0
  - MPEG-TS output to stdout
- Preserve access token propagation for the wrapped segmenter URL.
- Add runtime smoke proof that the `.ts?mode=ts` path produces a readable
  MPEG-TS sample while the HLS segmenter remains active.

### Phase 6 - MPEG-TS legacy branch

- Implement the ErsatzTV `ConcatChannel` equivalent for
  `StreamingMode.TransportStream` / `mode=ts-legacy`.
- Reuse existing `ConcatPlaylist` support only if it matches the current
  ErsatzTV source URL shape; otherwise update it deliberately with snapshots.
- Ensure legacy and hybrid MPEG-TS paths are observably different in tests.
- Add focused snapshot coverage for the legacy command shape.

### Phase 7 - Default MPEG-TS script runtime

- Add an MPEG-TS script loader/service that reads ErsatzTV-shaped script folders
  from the configured app-data script location.
- Parse `mpegts.yml` with a structured YAML parser.
- Render `HlsUrl`, `ChannelName`, and `FFmpegPath` into the selected script.
  Use a maintained template engine or a small safe renderer only if it can
  match the known ErsatzTV template contract.
- Execute Windows scripts on Windows and shell scripts through `bash` on Unix,
  following ErsatzTV's platform split and the repo platform parity rule.
- Use the configured `ffmpeg.default_mpegts_script`; fall back to the default
  wrapper command and warn when the configured script cannot be found.
- Add unit tests for script discovery, render data, missing script fallback, and
  platform-specific command selection under cfg guards.

### Phase 8 - Runtime proof and progress closeout

- Run focused tests for IPTV/HLS dispatch, HLS worker state, MPEG-TS wrapper,
  MPEG-TS legacy, and FFmpeg command snapshots.
- Run a real FFmpeg smoke for channel 1 or an equivalent deterministic short
  playout fixture:
  - HLS Segmenter for at least 60 seconds
  - HLS Direct for at least one playlist reload
  - MPEG-TS wrapper for at least 60 seconds
  - MPEG-TS Legacy for at least 60 seconds
- Use `ffprobe` or TSDuck diagnostics when available to prove readable MPEG-TS
  output, continuity, and audio/video streams.
- Update:
  - `docs/progress/parity/hls-variant-playlists-segmenter.toml`
  - `docs/progress/parity/mpeg-ts-channel-streaming.toml`
  - `docs/progress/parity/settings.toml`
  - `docs/progress/sessions.ndjson`
- Run `cargo xtask progress check`.

## Acceptance criteria

- HLS Segmenter no longer skips around 24 seconds or stops around 48 seconds in
  the reported channel 1 scenario.
- HLS Segmenter session start waits for configured readiness and enforces
  configured idle/work-ahead behavior.
- `/iptv/channel/{n}.ts`, `/iptv/channel/{n}.ts?mode=ts`, and
  `/iptv/channel/{n}.ts?mode=ts-legacy` resolve exactly like ErsatzTV.
- Modern MPEG-TS wraps the HLS segmenter; legacy MPEG-TS is a distinct branch.
- The default MPEG-TS script setting affects runtime streaming, not just UI.
- HLS Direct remains green and is not regressed by the segmenter/wrapper work.
- Progress docs contain no stale "Ported" claim for behavior that still has
  `ParityDebt`.

## Verification commands

Use focused checks while iterating, then broaden before closeout:

```bash
cargo test -p ersatzrs-web --features ssr iptv
cargo test -p ersatzrs-infra hls_worker
cargo test -p ersatzrs-ffmpeg pipeline_snapshots
cargo check -p ersatzrs-web --features ssr
cargo xtask progress check
```

Run broader gates before claiming the parity slice complete:

```bash
cargo fmt --all -- --check
cargo clippy -p ersatzrs-web --all-targets -- -D warnings
cargo clippy -p ersatzrs-infra --all-targets -- -D warnings
cargo clippy -p ersatzrs-ffmpeg --all-targets -- -D warnings
cargo test --workspace --all-features
```

## Out of scope

- Reworking HLS Direct unless a focused source audit finds a direct-only gap.
- Changing scheduler semantics except for on-demand timeshift/checkpoint calls
  required by the segmenter state machine.
- TSDuck troubleshooting implementation; it can be used as proof if available,
  but it is tracked separately.
- Replacing drawvg/native graphics with ErsatzTV's external graphics process.

## Definition of done

- The source map above has been re-read against current `$ERSATZTV_SRC`.
- Phases 1-7 are implemented or any remaining behavior is explicit
  `ParityDebt` with an owner and blocker.
- Focused unit/snapshot tests cover routing, state transitions, command argv,
  wrapper/legacy split, and script fallback.
- Runtime proof covers the reported 24s/48s window for HLS Segmenter and both
  MPEG-TS modes.
- Progress docs and session log are updated, and `cargo xtask progress check`
  passes.
