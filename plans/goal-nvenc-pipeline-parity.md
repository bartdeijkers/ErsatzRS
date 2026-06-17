# Goal prompt — NVENC/CUDA pipeline parity to completion

> **Status (2026-06-17): mostly shipped.** NVENC/CUDA pipeline parity is in place;
> the only open item is the hardware-only `nvenc-filter-gate-live-proof` debt on
> `ffmpeg-command-generation` (In progress). Live state in `docs/progress/`.

Standing brief to close the remaining parity gaps between ErsatzTV's
`NvidiaPipelineBuilder` and the ErsatzRS NVENC path in
`crates/ersatzrs-ffmpeg/src/pipeline/mod.rs`. Authoritative reference is
`$ERSATZTV_SRC/ErsatzTV.FFmpeg/Pipeline/NvidiaPipelineBuilder.cs` (896 lines) —
read it first.

## Goal

The ErsatzRS NVENC/CUDA transcode pipeline produces the **same effective
filtergraph and output options** as ErsatzTV for every source/overlay/output
combination, so hardware transcoding succeeds on real NVIDIA hardware for the
full matrix: 8-bit/10-bit, HDR, interlaced MPEG-2, watermark/graphics/subtitle
overlays, and H264/HEVC/AV1 — faithfully, no stopgaps.

## Locked decisions (do not relitigate)

- **Parity rule:** mirror ErsatzTV's `NvidiaPipelineBuilder` behavior; deviations
  need an ADR. See `PORTING.md` and the Rust ↔ C# parity rules in `STYLE.md`.
- **Divergence point stays the graphics engine** (drawvg), per ADR 0005 — not
  the HW pipeline.
- **Regression gate = `insta` snapshots** of the generated argv
  (`crates/ersatzrs-ffmpeg/tests/pipeline_snapshots.rs`), the same as
  `ErsatzTV.FFmpeg.Tests`. Argv snapshots are necessary but **not sufficient**:
  every change in this area must also be smoke-tested on real NVENC hardware,
  because the failures only surface on a GPU (the snapshots passed for both bugs
  fixed below while the pipeline still crashed at runtime).

## Progress so far (2026-06-08)

Two runtime-crash bugs on the NVENC + overlay path are fixed and snapshotted:

1. **`-pix_fmt` on a CUDA surface (exit 218,** `Impossible to convert ...
   overlay_cuda -> auto_scale`**).** `should_emit_pixel_format_output_option`
   now returns `false` for `Nvenc` when `target_state.frame_data_location ==
   Hardware`. Mirrors ErsatzTV `SetPixelFormat` keeping format on the GPU for
   hardware frames.
2. **`overlay_cuda` can't overlay `yuva420p` on `nv12` (exit 234).** The main
   surface is now converted to `yuv420p` on the GPU
   (`scale_cuda=format=yuv420p`) before an 8-bit CUDA overlay. Mirrors ErsatzTV
   `NvidiaPipelineBuilder.cs:239-271`.

Both verified at argv level (116 pipeline snapshots green); **real-GPU
validation by the maintainer is still pending.**

## Confirmed at parity (no work needed)

- Encoders `h264_nvenc` / `hevc_nvenc` / `av1_nvenc`; HEVC emits `-tag:v hvc1`,
  `-b_ref_mode`, `-preset:v` matching `EncoderHevcNvenc`.
- Green-line workaround gated to H264 — correct: ErsatzTV's
  `NvidiaGreenLineWorkaroundFilter` returns `[]` for non-H264 (1080→8px /
  720→16px crop only).
- `cuda_software_fallback_upload`, `-noautoscale` on the HW-decode path,
  decode/encode capability fallback to software.
- 10-bit → software-download path for overlays (`download_cuda_frames_for_
  software_overlay`).

## Work remaining (execute in order)

Ordered by value × certainty. Each item: ErsatzTV reference → ErsatzRS target →
approach → snapshot/test. Add a new `#[test]` snapshot per behavior; do not
fold multiple behaviors into one snapshot.

### Phase 0 — Verify-before-code (investigation only)

These three are *suspected* gaps; confirm whether ErsatzRS already covers them by
a different construction before writing any code. Record findings inline in this
file.

- **0a. Nut → software encode** (ETV `:72-75`). ErsatzTV forces software
  (rawvideo) encode when `OutputFormat is Nut` (the two-process segmenter pipe).
  Check `crates/ersatzrs-infra/src/hls_worker.rs`: does the ErsatzRS segmenter
  use a Nut pipe, or single-process MPEG-TS/HLS? The observed runtime failure was
  `-f mpegts` (single process) → likely **N/A**. If no Nut pipe exists, mark this
  closed and delete the item.
- **0b. 10-bit `p016 → p010` directly after decode** (ETV `:186-211`). ErsatzTV
  inserts `scale_cuda` to convert p016→p010 right after `DecoderImplicitCuda`
  ("ffmpeg 7.2+ uses p016 internally"), then a second `scale_cuda` to yuv420p if
  the target is 8-bit. ErsatzRS normalizes 10-bit only later via
  `scale_cuda=format=p010le` inside the scale step. Confirm the ErsatzRS scale
  step always runs (even when source size == output size) so 10-bit surfaces are
  always p010le before any filter/encoder. If yes, mark closed; if a no-scale
  path leaks p016, add the explicit post-decode conversion.
- **0c. Vulkan-HDR tonemap decoder completeness** (ETV `:88-92`, `:121-126`).
  ErsatzRS has `nvidia_vulkan_hdr_tonemap` + `-hwaccel vulkan`. Confirm it gates
  on (HW decode capability ∧ Vulkan available ∧ `ColorParams.IsHdr` ∧ not
  `ETV_DISABLE_VULKAN`) and uses an implicit Vulkan decoder. Fill any missing
  gate; otherwise mark closed.

### Phase 1 — Unknown color params guard (highest value)

ErsatzTV `:367-380`: when **encoder is Nvenc ∧ frames are Hardware ∧
`ColorParams.IsUnknown` ∧ has overlay**, add `NoAutoScaleOutputOption` (if not
already present) — "unknown color params can *change* during transcoding, which
can cause ffmpeg to insert an autoscale filter that cannot accept hardware
frames." This is the same crash class as the two bugs already fixed.

- **Prereq:** ErsatzRS does not model "unknown color params". Add an
  `is_unknown` concept to `SourceColorParams` (`pipeline/mod.rs:~393`), populated
  from probe metadata (color primaries/transfer/space all absent/unknown). Mirror
  ErsatzTV `ColorParams.IsUnknown`.
- **Change:** in the encoder/output-option assembly, emit `-noautoscale` when
  `effective_acceleration == Nvenc`, the encoder sees hardware frames, the source
  color params are unknown, and an overlay is present — guarded so it is not
  duplicated on the HW-decode path that already emits it.
- **Snapshot:** new test — NVENC + watermark + unknown color params + **software
  decode** (so `-noautoscale` is not already present from the HW-decode branch)
  → asserts a single `-noautoscale`.

### Phase 2 — MPEG-2 + deinterlace → software decode

ErsatzTV `:82-86`: `if ShouldDeinterlace && Codec == Mpeg2Video` → force
`decodeCapability = Software` ("mpeg2_cuvid has issues when yadif_cuda is used").

- **Change:** in `effective_decode_acceleration` (`pipeline/mod.rs:~1639`), return
  `HardwareAccelerationKind::None` for decode when the source codec is
  `Mpeg2Video` and the pipeline will deinterlace (`should_deinterlace`).
- **Snapshot:** new test — interlaced MPEG-2 source + NVENC profile → asserts
  software decode (no `-hwaccel cuda` for the input) while the encoder stays
  `*_nvenc`, and `yadif` (software) instead of `yadif_cuda`.

### Phase 3 — AV1 output guard

ErsatzTV `:77-80`: throw when `VideoFormat is Av1 && OutputFormat is not
HlsMp4` ("AV1 output is only supported with HLS Segmenter (fmp4)").

- **Change:** add an early validation in pipeline construction (or the request
  builder) returning a typed error when AV1 is requested for a non-fmp4 output.
  Keep the message ErsatzTV-shaped.
- **Test:** unit test asserting the error for AV1 + MpegTs and success for AV1 +
  HlsMp4.

### Phase 4+ — Items promoted from Phase 0 if confirmed as gaps

Implement 0a / 0b / 0c only if Phase 0 found a real gap, each with its own
snapshot.

## Working rules

- Read the ErsatzTV C# for the exact step before changing Rust; preserve filter
  order and option strings.
- One behavior → one focused snapshot test. Review every `.snap.new` diff
  token-by-token (`cargo insta review`) before accepting — confirm only the
  intended tokens changed.
- Mirror CI locally before claiming done:
  - `cargo fmt --all -- --check`
  - `cargo clippy -p ersatzrs-ffmpeg --all-targets -- -D warnings`
  - `cargo test -p ersatzrs-ffmpeg`
- Update `docs/progress/parity/` (the NVENC/playback pipeline parity entry) and
  add a session via `cargo xtask progress add-session`.
- After each phase that changes runtime argv, **ask the maintainer to smoke-test
  on real NVIDIA hardware** and paste any new ffmpeg stderr; fix the next
  format/order mismatch the same way (ErsatzTV parity).

## Out of scope (unless requested)

- VAAPI / QSV equivalents of these guards (same classes likely apply, but only
  NVENC is the reported/testable target now).
- The graphics-engine drawvg work (separate plan,
  `plans/goal-graphics-engine.md`).
- Playout/scheduling (the Classic Continue-build seam overlap ParityDebt is
  tracked in `docs/progress/parity/classic-scheduling.toml`).

## Definition of done

- Phases 1–3 implemented (plus any Phase 0 item confirmed as a real gap), each
  snapshot-tested; `fmt` + `clippy` + `cargo test -p ersatzrs-ffmpeg` green.
- The NVENC parity entry in `docs/progress/parity/` records what is closed and
  what (if anything) remains as explicit `ParityDebt`.
- Maintainer has smoke-tested the affected scenarios on real NVIDIA hardware and
  confirmed segments are produced (no exit 218/234/-22 class failures).
