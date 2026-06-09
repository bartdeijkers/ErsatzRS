# Colorspace normalization parity — software (done) + GPU-resident hardware (planned)

This plan has two phases:

- **Phase 1 (shipped):** fix the SD "256-color GIF" banding/dither by porting
  ErsatzTV's `SetPixelFormat` colorspace branch for **software-resident** frames.
- **Phase 2 (planned):** close the remaining `ParityDebt` — apply the same
  normalization on **GPU-resident hardware-encode** paths (NVENC/VAAPI/QSV),
  mirroring ErsatzTV's per-builder download → format → re-upload dance. Tracked
  in `docs/progress/parity/hardware-colorspace-normalization.toml`.

## Context

SD interlaced footage (e.g. Star Trek: TNG DVD) streamed through ErsatzRS came
out looking like an early-internet 256-color animated GIF: heavy color
quantization, banding on smooth gradients, and a fine dithering dot-pattern on
flat surfaces. Deinterlacing was fine (no combing), so the defect was in
color/format handling downstream of `yadif`.

Root cause (single true divergence from ErsatzTV, which does **not** have this
problem): **ErsatzRS never applied a colorspace-normalization filter.**

- `colorspace_filter()` was fully implemented and snapshot-tested in
  `crates/ersatzrs-ffmpeg/src/filter/colorspace.rs`, but **no pipeline builder
  ever called it** (only the test file referenced it).
- ErsatzTV's `SetPixelFormat` (`SoftwarePipelineBuilder.cs:180-186`, mirrored in
  every hardware builder) inserts `colorspace=…all=bt709:format=yuv420p:range=tv`
  whenever the output should be bt709 and the source is not. SD/DVD content is
  bt601/bt470bg + limited range, so ErsatzTV **always** normalizes it.
- The FFmpeg `colorspace` filter defaults to `dither=none` and does a
  full-precision matrix/range/format conversion. Missing it, ErsatzRS left the
  bt601→bt709 + range + `yuv420p` conversion to swscale's *implicit* path, whose
  **default ordered dithering** is exactly the GIF-like dot pattern + banding.

**Rejected hypotheses (confirmed not the cause):** the scaler flag
(`fast_bilinear`) and the `veryfast` / 8 Mbps rate control are **identical** to
ErsatzTV, which renders the same footage cleanly. `yadif=1` framerate doubling
is unrelated (visual, not motion/timestamp).

---

# Phase 1 — software-resident colorspace normalization (SHIPPED)

Commits `591518b` (+ `playback.rs` folded into codex's `a081685`).

### 1. Carry full source color metadata — `pipeline/mod.rs` + `infra/playback.rs`

- Extended `SourceColorParams` with the raw tag strings
  (`color_range/space/transfer/primaries`); kept existing `is_hdr` /
  `is_bt2020_ten` (tonemap dispatch). Struct is no longer `Copy`.
- Added `SourceColorParams::is_bt709()` mirroring ErsatzTV `ColorParams.IsBt709`,
  and `to_filter_color_params()` deriving `is_unknown`/`is_mixed`/`is_bt470_bg`/
  `is_bt2020_ten` from the raw tags into the filter-facing `ColorParams`.
- `derive_color_params()` (`playback.rs`) now populates the raw tags and the
  source-stream query reads `ColorRange` (it previously omitted that column).

### 2. Wire the colorspace filter into the shared pipeline — `pipeline/mod.rs`

In `build_transcode_pipeline`, before the `-pix_fmt` decision: when
`desired.normalize_colors` && a source stream is present &&
`!source.color_params.is_bt709()`, compute
`colorspace_filter(&target_state, &cp, desired.pixel_format(), false, is_qsv)`
and push onto the end-of-chain `filter_chain.pixel_format` vector. Mirror
`ColorspaceFilter.NextState`: for non-"unknown" sources pin
`target_state.pixel_format = desired` so the standalone `-pix_fmt` is not
double-emitted; recompute `emit_pixel_format_output_option`.

**Gate (Phase 1 only):** `target_state.frame_data_location == Software`. This is
the line Phase 2 relaxes. In practice it already covers software + post-download
hardware paths (e.g. QSV after `vpp_qsv` `hwdownload`).

### 3. Default consistency — `pipeline/mod.rs`

`normalize_colors: true` in `DesiredVideoState::h264_1080p()` to match ErsatzTV's
`FFmpegProfile.New` default (DB seed + editor default already `true`).

### 4. Tests + docs

Added SD bt470bg/mixed/unknown software tests + a bt709 no-op test; regenerated
44 existing snapshots. Recorded the port in `ffmpeg-command-generation.toml`.

**Verification (done):** ffmpeg 120 tests, infra 99+ tests, clippy + fmt clean.

---

# Phase 2 — GPU-resident hardware colorspace normalization (PLANNED)

**Goal:** normalize non-bt709 sources on NVENC/VAAPI/QSV hardware-**encode**
paths where the frame stays on the device through to the encoder — the case
Phase 1's `Software` gate intentionally skips. Closes
`hardware-colorspace-normalization.toml`.

**Why it's deferred, not skipped:** emitting a bare `hwdownload` colorspace on a
GPU-resident path with no re-upload breaks the encode (the `auto_scale`-style
failure the existing NVENC `should_emit_pixel_format_output_option` guard warns
about). ErsatzTV avoids this with a vendor-specific sequence; we must port that
sequence, not just relax the gate.

### Authoritative ErsatzTV sequencing (port branch-for-branch)

Read these before writing code — they define the exact ordering/conditions:

- **NVENC** — `NvidiaPipelineBuilder.SetPixelFormat`
  (`ErsatzTV.FFmpeg/Pipeline/NvidiaPipelineBuilder.cs:415-500`): adds the
  `ColorspaceFilter` (its `hwdownload` prefix brings frames to software for the
  matrix/range conversion), then for the still-hardware / hardware-encode case
  restores GPU residency with `CudaFormatFilter` = `scale_cuda=format={fmt}`,
  falling back to `PixelFormatOutputOption` only in the documented branches
  (`softwareDecoder || (noPipelineFilters && hasColorspace) ||
  (hardwareDecoder && hardwareEncoder && noPipelineFilters)`).
- **VAAPI** — `VaapiPipelineBuilder.SetPixelFormat`
  (`VaapiPipelineBuilder.cs:335-380`): `ColorspaceFilter`, then a
  `HardwareDownloadFilter` (8↔10-bit aware), re-format to **NV12** (Intel VAAPI
  requires NV12 except for NUT output), and re-upload before the VAAPI encoder.
- **QSV** — `QsvPipelineBuilder.SetPixelFormat`
  (`QsvPipelineBuilder.cs:363-410`): same shape with `formatForDownload = NV12`,
  the `usesVppQsv`-widened gate (`!IsBt709 || usesVppQsv`) and
  `forceInputOverrides: usesVppQsv`. (Phase 1 already passes `is_qsv`/the
  vpp_qsv-widened gate to `colorspace_filter`; reuse that.)

### Reusable ErsatzRS primitives (do not hand-roll)

- `colorspace_filter()` already emits the correct `hwdownload,format=…` prefix
  for hardware frames via `build_hwdownload_prefix` — no change needed.
- `crate::hardware_download_filter(&FrameState)` — `hwdownload,format=…` per
  pixel format (`filter/hardware.rs:39`).
- `crate::hardware_upload_filter(accel, qsv_extra_hw_frames)` — `hwupload_cuda` /
  `hwupload=extra_hw_frames=N` (QSV) / VAAPI `format=…,hwupload`
  (`filter/hardware.rs:6`); `hardware_upload_vaapi_filter` for the VAAPI variant.
- **New, tiny:** `cuda_format_filter(pixel_format) -> "scale_cuda=format={}"`
  mirroring ErsatzTV `CudaFormatFilter` (the `scale_cuda=format=p010` shape
  already exists in `filter/tonemap.rs:58`); add it to `filter/hardware.rs` (or
  `filter/simple.rs`) with a unit snapshot.

### Implementation — `pipeline/mod.rs` (`build_transcode_pipeline`)

1. **Relax the gate.** Replace the Phase 1
   `frame_data_location == Software`-only guard with a branch on
   `target_state.frame_data_location`:
   - **Software (existing):** unchanged — push `colorspace_filter` result onto
     `filter_chain.pixel_format`, pin pixel format for non-unknown.
   - **Hardware (new):** push the `colorspace_filter` result (it already carries
     the `hwdownload` prefix → frames become software); set
     `target_state.frame_data_location = Software`; then append the vendor
     re-upload step so frames return to the device before the hardware encoder,
     and set `frame_data_location = Hardware`:
     - NVENC: `cuda_format_filter(desired_surface_fmt)` (and/or `hwupload_cuda`)
       per the NvidiaPipelineBuilder branch conditions.
     - VAAPI: re-format to NV12 + `hardware_upload_vaapi_filter` /
       `hardware_upload_filter(Vaapi, …)`.
     - QSV: re-format to NV12 + `hardware_upload_filter(Qsv, qsv_extra_hw_frames)`.
   - Honor ErsatzTV's "use `PixelFormatOutputOption` instead" fallback branches
     so `emit_pixel_format_output_option` stays correct (recompute after).
2. **Ordering:** the colorspace + re-upload must land in the video filter chain
   at the same point ErsatzTV places `SetPixelFormat` (after scale/overlay,
   before encode). Confirm against `filter/complex.rs` assembly — the
   `pixel_format` vector lands at graph end as `[vpf]`; verify the re-upload
   ends GPU-resident so the hardware encoder accepts it (no stray `auto_scale`).
3. **Keep Phase 1's NextState/`-pix_fmt` suppression** for the non-unknown case.

### Tests — `tests/pipeline_snapshots.rs`

- Add per-vendor full-pipeline snapshots for a **bt470bg** (and one **mixed**)
  hardware-encode source on **NVENC**, **VAAPI**, **QSV**, asserting:
  (a) the `colormatrix`/`colorspace` node is present, and (b) the frame is
  re-uploaded to the device (`scale_cuda=format=…` / `hwupload…`) before the
  hardware encoder, with the correct surface format.
- Add a bt709 hardware case asserting no colorspace node (no-op parity).
- Add the `cuda_format_filter` unit snapshot.
- Regenerate the existing GPU-resident hardware snapshots that legitimately
  change (`nvenc_pipeline_snapshot`, `vaapi_pipeline_snapshot`,
  `qsv_pipeline_snapshot`, etc.) and review each diff — they must show the
  download→normalize→re-upload sequence, **not** a dangling `hwdownload`.

### Verification (no local GPU — argv parity is the oracle)

1. `cargo test -p ersatzrs-ffmpeg --all-targets` + `cargo insta review`.
2. **ErsatzTV argv parity:** for each vendor, build the equivalent ErsatzTV
   command for a bt470bg source and diff the `-filter_complex` against ErsatzRS;
   they should match the normalize + re-upload sequence. This is the primary
   correctness check since the paths are runtime-untestable here.
3. Lint (mirror CI): `cargo clippy -p ersatzrs-ffmpeg --all-targets -- -D warnings`
   then the two-pass workspace clippy (with/without `--features mysql`) +
   `cargo fmt --all -- --check`.
4. Regression guard: software/post-download paths and bt709/`Copy` unchanged;
   no double `-pix_fmt`; hardware snapshots end GPU-resident.
5. If GPU hardware later becomes available: transcode a bt470bg clip on each
   accel and confirm the encode succeeds and banding is gone.

### Parity bookkeeping

- Flip `docs/progress/parity/hardware-colorspace-normalization.toml` `status`
  to `Ported` and update its summary to "Ported" with the per-vendor sequence.
- Update the pointer line in `ffmpeg-command-generation.toml` accordingly.
- `cargo xtask progress check` + `add-session`.

---

## Status

**Phase 1 — complete (software-resident):**

- [x] `SourceColorParams` extended + `is_bt709()` / `to_filter_color_params()`
- [x] `derive_color_params()` populates raw tags (added `ColorRange` to query)
- [x] Colorspace filter wired into `build_transcode_pipeline` (software frames)
- [x] `h264_1080p()` default flipped to `true`
- [x] Snapshot tests (bt470bg/mixed/unknown/bt709) + 44 existing regenerated
- [x] Verification: ffmpeg 120 tests, infra 99+ tests, clippy + fmt clean
- [x] Parity docs updated; GPU-resident gap carved into its own tracked feature

**Phase 2 — planned (GPU-resident hardware encode):**

- [ ] Add `cuda_format_filter` primitive (+ unit snapshot)
- [ ] Relax the `Software`-only gate; add per-vendor hardware branch
      (NVENC `scale_cuda=format` / VAAPI+QSV download→NV12→re-upload)
- [ ] Honor ErsatzTV's `PixelFormatOutputOption` fallback branches
- [ ] Per-vendor hardware snapshots + bt709 no-op + regenerate existing HW snaps
- [ ] ErsatzTV argv-parity diff per vendor; clippy + fmt
- [ ] Flip `hardware-colorspace-normalization.toml` to `Ported`; session log

## Key files

- `crates/ersatzrs-ffmpeg/src/filter/colorspace.rs` — colorspace filter (handles HW `hwdownload` prefix)
- `crates/ersatzrs-ffmpeg/src/filter/hardware.rs` — upload/download primitives; add `cuda_format_filter`
- `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs` — wiring/gate, `SourceColorParams`, `h264_1080p`
- `crates/ersatzrs-infra/src/playback.rs` — `derive_color_params`, stream construction
- `crates/ersatzrs-ffmpeg/src/filter/complex.rs` — end-of-chain `pixel_format` assembly
- `crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs` — snapshots
- `docs/progress/parity/hardware-colorspace-normalization.toml` — Phase 2 tracked gap
- ErsatzTV refs: `ColorParams.cs`, `ColorspaceFilter.cs`, `Cuda/CudaFormatFilter.cs`,
  `{Software,Nvidia,Vaapi,Qsv}PipelineBuilder.cs` `SetPixelFormat`
