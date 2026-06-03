# Watermark Streaming Parity

## Context

Channel logo watermarks do not appear when viewing a channel on a TV. Investigation
shows the FFmpeg watermark **overlay infrastructure is fully built but never wired into
streaming**:

- `WatermarkOverlay`, `watermark_overlay_filter`, `watermark_opacity_filter`,
  `watermark_fade_filter`, `InputFile::watermark`, and the complex-filter routing
  (`crates/ersatzrs-ffmpeg/src/pipeline/mod.rs:653-673`) all exist and are
  snapshot-tested.
- **Nothing ever sets `request.watermark`.** The streaming command builders
  (`build_stream_command` in `transport/iptv/stream.rs`, `build_hls_command` in
  `hls_worker.rs`, and `dev_stream.rs`) load source video + subtitles but never load or
  attach a watermark.
- `Channel.watermark_id`, the global config key `ffmpeg.global_watermark_id`, and
  `PlayoutItem.watermark_ids` exist and the scheduler populates them, but the streaming
  path never consumes any of them. There is no watermark loader for streaming (there is a
  `GraphicsRepository` but no equivalent for watermarks).

The subtitle path is the proven template to mirror: `select_playback_subtitle` (infra
`playback.rs`) → `subtitle_input: Option<InputFile>` → `build_stream_command`.

**Outcome:** watermarks (all modes) render in the final video output for IPTV
(`.ts`/MPEG-TS), HLS, and the dev stream, faithfully porting ErsatzTV's behavior with no
stopgaps.

> The general graphics-element engine (image/text/subtitle/motion overlays) is tracked
> separately in **`graphics-engine-parity-slice.md`**. Watermarks render via native FFmpeg
> overlay filters; the graphics engine renders via drawvg. This plan owns watermarks only.

### Locked decisions (confirmed with maintainer)

- **Scope:** the watermark overlay subsystem — all watermark modes (Permanent,
  Intermittent, OpacityExpression).
- **Dynamic opacity mechanism:** native FFmpeg `overlay` + `fade`/alpha-expression
  filters (no external Skia process), per ADR 0005 / AGENTS.md ("do not reintroduce an
  external graphics process").

### ErsatzTV reference files (read before porting each piece)

- `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/WatermarkSelector.cs` — selection hierarchy.
- `$ERSATZTV_SRC/ErsatzTV.Core/FFmpeg/FFmpegLibraryProcessService.cs:377-419` — static
  vs. engine decision.
- `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/SoftwarePipelineBuilder.cs:202-267`
  (`SetWatermark`).
- `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Filter/Watermark*.cs` (Scale, Opacity, Fade,
  OverlayWatermark) and `State/WatermarkState.cs`.
- `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/Image/WatermarkElement.cs` —
  intermittent/opacity-expression timing (port the math to FFmpeg fade points / alpha
  expression).

---

## Phase 1 — Watermark resolution & loading (data layer)

Port `WatermarkSelector` selection precedence (Deco → Playout → PlayoutItem → Channel →
Global), honoring `disable_watermarks`.

- Add a streaming resolver `select_playback_watermarks(pool, channel, active_item) ->
  Vec<ResolvedWatermark>` in `crates/ersatzrs-infra/src/playback.rs`, mirroring
  `select_playback_subtitle`. It loads `ChannelWatermark` rows (domain type already exists
  in `channel.rs:197`), resolves the global id from config key
  `CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID` (`domain/config.rs:20`), and applies precedence.
- Add a `get_watermark(id)` loader. Prefer extending `ChannelRepository` (already loaded
  in `stream.rs`) or add a small `WatermarkRepository` trait in
  `crates/ersatzrs-core/src/repositories/watermark.rs` + Sqlite impl in
  `crates/ersatzrs-infra/src/repositories/watermark.rs`, mirroring `graphics.rs`. Register
  in both `repositories/mod.rs`.
- Extend `load_active_media_path` (`transport/iptv/stream.rs:180`) to also surface the
  active `PlayoutItem` id + `disable_watermarks` + its `PlayoutItemWatermark` ids (join),
  so per-item watermarks resolve.

## Phase 2 — Image-source resolution

Resolve a `ChannelWatermark` to a filesystem image path:

- **Custom:** `data/watermarks/<image>` — reuse `WATERMARK_DIR`
  (`api/watermarks.rs:24`); the `image` field holds the filename.
- **ChannelLogo:** load channel logo `Artwork` where `ArtworkKind = 2` (LOGO,
  `infra/metadata.rs:70`) for the channel; resolve to its on-disk path.
- **Resource:** embedded resource (e.g. song-progress) — out of scope here; handled by the
  graphics engine (`graphics-engine-parity-slice.md`), not the static watermark overlay.

## Phase 3 — FFmpeg overlay parity (close the scaling gap; map all modes)

- **Scaling gap (real bug):** `WatermarkOverlay` (`filter/simple.rs:242`) has no
  `width_percent` and the pipeline emits no scale filter. Add `width_percent` and a
  `watermark_scale_filter` → `scale={round(width_percent/100*frame_w)}:-1`, porting
  `WatermarkScaleFilter.cs`. Apply when `size == Scaled`.
- **Still-image loop (real bug):** `InputFile::watermark` (`input.rs:66`) adds no input
  option; a PNG without `-loop 1` yields a single frame. Port `InfiniteLoopInputOption`
  (still image → `-loop 1`; video → `-stream_loop -1`) and apply at input build time.
- Map domain `ChannelWatermark` → `WatermarkOverlay` (location, size, margins, opacity,
  `place_within_source_content`, `width_percent`).
- **Permanent:** overlay + opacity + scale (current code path, once fed).
- **Intermittent:** port `WatermarkElement` timing to a set of `WatermarkFadePoint`s
  across the program (frequency/duration → fade-in/hold/fade-out), emitting the existing
  `watermark_fade_filter` chain (`fade=in|out:...:enable='between(t,..)'`). Native FFmpeg.
- **OpacityExpression:** translate the stored expression to a native FFmpeg alpha
  expression (e.g. `geq`/`sendcmd` driving `colorchannelmixer`/overlay alpha). Document
  the mapping; add a focused unit test for the translation.
- Update the watermark block in `pipeline/mod.rs:653-673` to add the scale filter, support
  multiple watermarks (chain overlays in `z_index` order), and append fade/expression
  steps.

## Phase 4 — Wire watermarks into streaming

Mirror the subtitle plumbing in all three builders:

- `transport/iptv/stream.rs` `serve_channel_stream`: after subtitle selection, call
  `select_playback_watermarks`, build `WatermarkOverlay` + `InputFile::watermark(path)`,
  pass into `build_stream_command`; inside, set `request.watermark` and push the input(s).
- `infra/hls_worker.rs` `build_hls_command`: same.
- `web/transport/dev_stream.rs`: same.

## Phase 5 — Tests & verification

- **Snapshot tests** (`crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs`, follow
  `filter_graph_with_watermark_subtitle_and_drawvg_snapshot`): permanent+scaled,
  channel-logo, intermittent (fade points), opacity-expression, and multiple watermarks
  (z-order). Review with `cargo insta review`.
- **Unit tests:** selection precedence, image-source resolution, fade-point generation,
  opacity-expression translation, still-image loop option.
- **End-to-end:** `cargo run --bin ersatzrs-web`, configure a channel with a Permanent
  channel-logo watermark, open the stream, and confirm the logo is composited
  (ffprobe/visual). Note `LD_LIBRARY_PATH` for local ffmpeg-8.1 per project notes.
- **Docs:** update `docs/reference/parity-matrix.md` (watermark/graphics rows), STATUS.md
  session log, and the health-check engine if runtime requirements change.

---

## Critical files

- `crates/ersatzrs-web/src/transport/iptv/stream.rs` — `serve_channel_stream`,
  `build_stream_command`, `load_active_media_path`.
- `crates/ersatzrs-infra/src/hls_worker.rs` — `build_hls_command`.
- `crates/ersatzrs-web/src/transport/dev_stream.rs`.
- `crates/ersatzrs-infra/src/playback.rs` — add `select_playback_watermarks`
  (mirror `select_playback_subtitle`).
- `crates/ersatzrs-core/src/repositories/{watermark.rs (new),mod.rs}` and
  `crates/ersatzrs-infra/src/repositories/{watermark.rs (new),mod.rs}` (mirror the existing
  `graphics.rs` repository pattern).
- `crates/ersatzrs-ffmpeg/src/filter/simple.rs` — `WatermarkOverlay` (+`width_percent`),
  `watermark_scale_filter` (new), fade.
- `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs:653-673` — scale / multiple / fade.
- `crates/ersatzrs-ffmpeg/src/input.rs` — `InputFile::watermark` loop input option.
- `crates/ersatzrs-domain/src/channel.rs` — `ChannelWatermark` (already complete).
- `crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs` (+ `tests/snapshots/`).

## Reused, do not reinvent

- `watermark_overlay_filter`, `watermark_opacity_filter`, `watermark_fade_filter`,
  `InputFile::watermark`, `ComplexFilter` routing — already built; feed them.
- `select_playback_subtitle` / subtitle plumbing — structural template for watermark
  loading; the `graphics.rs` repository — pattern for the new watermark repository.

## Verification checklist

1. `cargo fmt --all -- --check`
2. `cargo clippy --workspace --all-targets -- -D warnings` then a second pass with
   `--features mysql` (per project lint policy).
3. `cargo test --workspace --all-features` + `cargo insta review` for new snapshots.
4. Manual: run `ersatzrs-web`, view a channel with a Permanent channel-logo watermark on a
   real player/TV; confirm the logo composites at the configured location/opacity/scale.

## Progress — 2026-06-03

- FFmpeg watermark inputs now add `-loop 1`, support `WidthPercent` scaling, opacity,
  intermittent fade points, opacity-expression alpha filters, and multiple overlays chained in
  `ZIndex` order.
- IPTV `.ts` and HLS segmenter paths now call `select_playback_watermarks`, resolve custom and
  channel-logo image sources, honor `DisableWatermarks`, and attach watermark inputs/overlays to
  the FFmpeg request.
- `select_playback_watermarks` now mirrors the active ErsatzTV precedence stack available from
  the persisted runtime rows: active deco-template item, playout deco, playout-item watermarks,
  channel watermark, then global watermark.
- Playout-template date filters now use the repo's persisted JSON integer arrays, C# weekday
  numbering, and optional explicit start/end years when selecting the active deco template.
- `/dev/stream/bbb.ts` now has an explicit dev-only watermark query surface so the dev stream can
  exercise Permanent, Intermittent, and OpacityExpression overlays through the same FFmpeg request
  path.
- Proof run:
  - `cargo fmt --all -- --check`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo clippy --workspace --all-targets --features mysql -- -D warnings`
  - `cargo test --workspace --all-features`
  - `cargo test -p ersatzrs-ffmpeg --test pipeline_snapshots watermark -- --nocapture`
  - `cargo test -p ersatzrs-infra --test playback_subtitles`
  - `cargo test -p ersatzrs-web transport::dev_stream -- --nocapture`
  - `cargo test -p ersatzrs-web transport::iptv::stream::tests::stream_body_composites_watermark_from_fixture -- --ignored --nocapture`
  - `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`
  - `cargo clippy -p ersatzrs-web --all-targets --features mysql -- -D warnings`

## ParityDebt

- No open `ParityDebt` remains for static FFmpeg watermark overlays on the covered streaming
  paths. Resource-backed dynamic graphics watermarks are intentionally tracked by
  `plans/graphics-engine-parity-slice.md`.
