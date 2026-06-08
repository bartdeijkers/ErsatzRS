# Fix SD video color banding/dithering — port ErsatzTV colorspace normalization

## Context

SD interlaced footage (e.g. Star Trek: TNG DVD) streamed through ErsatzRS comes
out looking like an early-internet 256-color animated GIF: heavy color
quantization, banding on smooth gradients, and a fine dithering dot-pattern on
flat surfaces. Deinterlacing itself is fine (no combing), so the defect is in
color/format handling downstream of `yadif`.

The investigation found a single true divergence from ErsatzTV (which does **not**
have this problem): **ErsatzRS never applies a colorspace-normalization filter.**

- `colorspace_filter()` is fully implemented and snapshot-tested in
  `crates/ersatzrs-ffmpeg/src/filter/colorspace.rs`, but **no pipeline builder
  ever calls it** (only the test file references it).
- ErsatzTV's `SetPixelFormat` (`ErsatzTV.FFmpeg/Pipeline/SoftwarePipelineBuilder.cs:180-186`,
  mirrored in every hardware builder) inserts
  `colorspace=…all=bt709:format=yuv420p:range=tv` whenever the output should be
  bt709 and the source is not. SD/DVD content is bt601/bt470bg + limited range,
  so ErsatzTV **always** normalizes it.
- The FFmpeg `colorspace` filter defaults to `dither=none` and does a
  full-precision matrix/range/format conversion. With it **missing**, ErsatzRS
  leaves the bt601→bt709 + range + `yuv420p` conversion to swscale's *implicit*
  path inside `scale=…:flags=fast_bilinear` and at the encoder boundary —
  swscale's implicit conversion applies **ordered dithering by default**, which
  is exactly the GIF-like dot pattern + banding observed.

**Rejected hypotheses (confirmed not the cause):** the scaler flag
(`fast_bilinear`) and the `veryfast` / 8 Mbps rate control are **identical** to
ErsatzTV, which renders the same footage cleanly — so they are red herrings.
`yadif=1` framerate doubling is unrelated (visual, not motion/timestamp).

Two supporting gaps must be closed for the fix to work:

1. **Source color metadata is discarded.** `SourceColorParams` carried only
   `is_hdr` / `is_bt2020_ten`. `derive_color_params()` read
   `color_range/space/transfer/primaries` from ffprobe/DB but threw everything
   else away. The `ColorParams` struct that `colorspace_filter()` consumes
   (range/space/transfer/primaries + `is_bt470_bg`/`is_unknown`/`is_mixed`) was a
   separate, unwired type.
2. **Gate parity.** ErsatzTV gates on `desiredState.ColorsAreBt709`, which maps
   to `playbackSettings.NormalizeColors`. The DB already seeds `NormalizeColors = 1`
   (`db.rs:666`) and the editor default is `true`, so the running path is gated
   correctly — but the in-code `DesiredVideoState::h264_1080p()` factory set
   `normalize_colors: false`, opposite ErsatzTV's `true` default
   (`FFmpegProfile.cs:68`).

**Outcome:** the generated argv gains an explicit colorspace normalization node
(e.g. `colorspace=all=bt709:format=yuv420p:range=tv`,
`colormatrix=src=bt470bg:dst=bt709,format=yuv420p` for bt470bg sources, or
`setparams=range=tv:colorspace=bt709:color_trc=bt709:color_primaries=bt709` for
unknown-metadata sources), eliminating swscale's implicit dithering/banding and
matching ErsatzTV.

**Scope:** all pipeline builders (software + every hardware accel) — clean because
they all funnel through one shared `build_transcode_pipeline()`.

## Changes

### 1. Carry full source color metadata — `pipeline/mod.rs` + `infra/playback.rs`

- Extend `SourceColorParams` with the raw tag strings
  (`color_range/space/transfer/primaries`); keep existing `is_hdr` /
  `is_bt2020_ten` (used by tonemap dispatch). Struct is no longer `Copy`.
- Add `SourceColorParams::is_bt709()` mirroring ErsatzTV `ColorParams.IsBt709`,
  and `to_filter_color_params()` that derives `is_unknown`/`is_mixed`/
  `is_bt470_bg`/`is_bt2020_ten` (ErsatzTV `ColorParams` record semantics) from
  the raw tags and builds the filter-facing `ColorParams`.
- Update `derive_color_params()` (`playback.rs`) to populate the raw tags
  (keeping HDR/bt2020-ten logic intact).

### 2. Wire the colorspace filter into the shared pipeline — `pipeline/mod.rs`

In `build_transcode_pipeline`, before the `-pix_fmt` decision: when
`desired.normalize_colors` && a source stream is present &&
`!source.color_params.is_bt709()` (QSV also when vpp_qsv scaling is used),
compute `colorspace_filter(&target_state, &cp, desired.pixel_format(), false,
is_qsv)` and push onto the end-of-chain `filter_chain.pixel_format` vector.
Mirror `ColorspaceFilter.NextState`: when the source is not "unknown", set
`target_state.pixel_format = desired` so the separate `-pix_fmt` output option is
not double-emitted; recompute `emit_pixel_format_output_option` afterward.

### 3. Default consistency — `pipeline/mod.rs`

`normalize_colors: true` in `DesiredVideoState::h264_1080p()` to match
ErsatzTV's `FFmpegProfile.New` default (DB seed + editor default already `true`).

### 4. Snapshot tests — `tests/pipeline_snapshots.rs`

Add SD bt470bg/mixed/unknown software + an Nvenc hardware case asserting the
colorspace node; a bt709-source case asserting absence; regenerate legitimately
changed snapshots (`cargo insta`).

### 5. Parity bookkeeping — `docs/progress/`

Record the `SetPixelFormat` colorspace branch as ported in the relevant parity
TOML; `cargo xtask progress check` + `add-session`.

## Verification

1. `cargo test -p ersatzrs-ffmpeg --all-targets`; review with `cargo insta review`.
   Confirm SD snapshots contain the colorspace/colormatrix/setparams node.
2. Lint (mirror CI): `cargo clippy -p ersatzrs-ffmpeg --all-targets -- -D warnings`,
   then the two-pass workspace clippy (with/without `--features mysql`) + `cargo fmt --all -- --check`.
3. End-to-end argv check for an SD interlaced source; diff against ErsatzTV.
4. (Optional, FFmpeg 8.1 + sample clip) transcode before/after; banding/dither gone.
5. Regression guard: bt709 sources and `Copy` format unchanged; no double `-pix_fmt`.

## Status (complete — software-resident scope)

- [x] Plan copied to repo
- [x] `SourceColorParams` extended + `is_bt709()` / `to_filter_color_params()`
- [x] `derive_color_params()` populates raw tags (added `ColorRange` to the query)
- [x] Colorspace filter wired into `build_transcode_pipeline` (software-resident frames)
- [x] `h264_1080p()` default flipped to `true`
- [x] Snapshot tests added (bt470bg/mixed/unknown/bt709) + 44 existing regenerated
- [x] Verification: ffmpeg 120 tests, infra 99+ tests, clippy + fmt clean on changed crates
- [x] Parity docs updated (`ffmpeg-command-generation.toml`)

**Scope note:** Per the hardware-safety analysis, normalization is gated to
software-resident frames. GPU-resident encode paths (NVENC/VAAPI/QSV hardware
encode) need ErsatzTV's `hwdownload`→format→re-upload dance and are deferred as
`ParityDebt` (recorded in `ffmpeg-command-generation.toml`). The QSV path is
still normalized in practice because ErsatzRS downloads after `vpp_qsv`.

## Key files

- `crates/ersatzrs-ffmpeg/src/filter/colorspace.rs` — existing filter (reused unchanged)
- `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs` — wiring, `SourceColorParams`, `h264_1080p` default
- `crates/ersatzrs-infra/src/playback.rs` — `derive_color_params`, stream construction
- `crates/ersatzrs-ffmpeg/src/filter/complex.rs` — end-of-chain `pixel_format` assembly
- `crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs` — snapshots
- ErsatzTV refs: `SoftwarePipelineBuilder.cs:170-200`, `ColorParams.cs`, `ColorspaceFilter.cs`, `QsvPipelineBuilder.cs:363`
