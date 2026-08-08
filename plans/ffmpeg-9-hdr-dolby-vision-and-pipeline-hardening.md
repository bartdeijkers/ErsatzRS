# FFmpeg 9 HDR, Dolby Vision, and pipeline hardening plan

## Status

Complete (2026-08-07). This plan sequenced the accepted future-state
contract in [ADR 0064](../docs/adr/0064-hdr-and-dolby-vision-input-output-contract.md)
and the follow-up hardening in ADRs [0003](../docs/adr/0003-ffmpeg-cli-spawn.md),
[0014](../docs/adr/0014-runtime-learned-hardware-capability-fallback.md), and
[0017](../docs/adr/0017-default-ffmpeg-profile-1080p60.md). Live status and
completion evidence belong in `docs/progress/`; this document owns sequencing
and acceptance boundaries only.

This was the Phase 8 program. All seven required milestones — `p8a-01`,
`p8a-02`, `p8b-01`, `p8b-02`, `p8c-01`, `p8c-02`, and `p8c-03` — are complete,
so Phase 9 teletext support is unblocked. The evidence-gated first-class
Vulkan-encoder decision `p8c-04` remains deferred, as anticipated: no
multi-vendor runtime evidence was produced, so the separate ADR it requires does
not exist.

Residual gaps are tracked as visible parity debt rather than left in this plan.
The follow-up review's defects and the residue debts an NVIDIA-only machine
could close are done, including operator-editable HDR10 metadata, Dolby Vision
overlay/subtitle/seek characterization, and stream-copy rotation reporting. The
five that need hardware or Dolby Vision fixtures are handed off in
[`phase-8-hardware-residue.md`](phase-8-hardware-residue.md): HDR10 on
non-NVIDIA backends, graph probes beyond the NVIDIA graphs, the three unmeasured
`p8b-02` candidates, Dolby Vision profiles 5 and 7, and the `p8c-04` Vulkan
decision.

## Goal

Make ErsatzRS output SDR and HDR10 deliberately, handle Dolby Vision inputs
without silent wrong-color playback, make the existing Vulkan/libplacebo path
self-healing, and adopt measured FFmpeg 9 performance/support improvements
without weakening ErsatzTV parity or the six-platform release contract.

## Non-goals

- Do not advertise Dolby Vision output or transcoding.
- Do not make Vulkan a persisted encoder choice without a later ADR.
- Do not replace FFmpeg CLI spawning or introduce libav bindings.
- Do not optimize from argv shape alone; performance changes require runtime
  output, quality, and resource evidence.
- Do not make unsupported hardware paths mandatory on release targets.

## Current evidence

- `FFmpegProfile` has codec, profile, bit depth, and tonemap algorithm fields
  but no output dynamic-range contract.
- Every transcoded HDR source currently enters the HDR-to-BT.709 tonemap path,
  including the two seeded profiles named `hdr`.
- NVENC HDR tonemapping already uses Vulkan/libplacebo and transfers the result
  to CUDA, but selection checks the advertised Vulkan hwaccel rather than the
  complete graph or device interoperability.
- The required-runtime filter list does not require libplacebo, and the learned
  denial classifier covers decode-unsupported signatures rather than Vulkan or
  libplacebo initialization failure.
- FFmpeg 9 removes `-filter_complex_script`; its long-graph replacement is
  `-/filter_complex <path>`.
- FFmpeg 9 exposes `dovi_split`, animated WebP decoding/demuxing, additional
  CUDA/Vulkan filters, and improved AMF HDR/hardware-memory support. These are
  capabilities to characterize, not automatic product support.

## Workstream 1: Output dynamic range and HDR10 metadata

Tracked by Phase 8A milestone `p8a-01` and
`docs/progress/parity/hdr-dolby-vision-metadata.toml`.

1. Add the `OutputDynamicRange` domain enum, additive SQLite/MySQL migration,
   repository mappings, application validation, and localized editor select.
2. Default all pre-existing rows to `SdrBt709`, preserving current effective
   playback. Seed fresh ADR 0017 HDR profiles as `Hdr10Bt2020Pq`; keep the
   other three seeds as `SdrBt709`.
3. Reject invalid HDR10 combinations and make the field informational/disabled
   for video copy.
4. Persist typed source mastering/content-light metadata and carry it with the
   selected stream.
5. Implement the complete SDR/HDR10 source-output matrix, including a
   documented SDR reference-white mapping for stable HDR10 output.
6. Emit or strip static metadata only after the picture transform has fixed its
   meaning.
7. Add health checks for invalid profiles, missing required HDR metadata, and
   unsupported hardware/output combinations.

Acceptance requires SQLite/MySQL round trips, all localized editor variants,
validation regressions, argv snapshots, ffprobe side-data assertions, decoded
fixture comparisons, and direct MPEG-TS/HLS programme-transition proof.

## Workstream 2: Dolby Vision characterization and input handling

Tracked by Phase 8A milestone `p8a-02`.

1. Acquire redistributable or locally generated profile 5, 7, and 8.1 fixtures
   with known expected output; record any fixture licensing restriction.
2. Persist the ffprobe Dolby Vision configuration record and relevant frame or
   stream side data.
3. Characterize copy, software, and libplacebo paths independently for SDR and
   HDR10 output.
4. Use `dovi_split` only for proven multilayer/profile-7 operations. Make RPU
   preservation, consumption, and stripping explicit.
5. Reject unproven profile 5 paths with health/troubleshooting evidence rather
   than falling through to incorrect colors.
6. Prove overlays, subtitles, scaling, seek/restart, MPEG-TS, and HLS behavior.

Dolby Vision output remains out of scope. Any proposal to add it starts with a
new ADR and a codec/container/client support matrix.

## Workstream 3: Vulkan/libplacebo capability and fallback

Tracked by Phase 8B milestone `p8b-01` and ADR 0014.

1. Define an operation-scoped capability for the NVENC HDR graph: selected
   device, Vulkan hwaccel, libplacebo, CUDA upload/interoperability, required
   CUDA filters, and NVENC encoder.
2. Add a small functional graph probe at capability refresh. Cache its result
   by FFmpeg binary identity, driver/device identity, and operation.
3. Classify known Vulkan-device, libplacebo, hardware-map, and CUDA interop
   failures. Retry exactly once through the next proven graph while preserving
   hardware encode when possible.
4. Persist the learned denial, invalidate it with the existing capability
   refresh lifecycle, and surface it through health and troubleshooting.
5. Cover the real HDR-downscale-overlay shape that previously exposed CUDA
   surface-state drift.

Acceptance requires fake-tool classifier tests, capability serialization
tests, argv snapshots for primary/fallback graphs, a real compatible-GPU gate,
and end-to-end stream output proof. Absence of Vulkan on a supported platform
must remain a normal capability result, not a startup failure.

## Workstream 4: Hardware HDR performance and quality

Tracked by Phase 8B milestone `p8b-02`. Establish the HDR correctness oracle
from `p8a-01` before selecting faster graphs.

Benchmark representative 1080p and 4K HDR10/HLG inputs with scaling and an
alpha overlay. Record throughput, frame latency, CPU, GPU, device transfers,
output signaling, and decoded-image quality for:

- current NVENC libplacebo followed by CUDA scaling versus one fused
  libplacebo tone-map/scale/rotation pass;
- libplacebo peak detection enabled versus a reproducible static-metadata path;
- current VAAPI-to-OpenCL-to-VAAPI tonemapping versus native `tonemap_vaapi`;
- current AMF software tonemapping versus FFmpeg 9 AMF color conversion and
  hardware-memory mapping where the shipped runtime and driver expose them.

Adopt a candidate only when it preserves ADR 0064 output semantics and improves
a measured resource or latency dimension without a material quality regression.
Record unsupported driver/device combinations in the capability matrix rather
than adding platform assumptions to Rust source.

## Workstream 5: CLI and media-input support

Tracked by Phase 8C milestones `p8c-01` through `p8c-03`.

### Long filter graphs

Milestone `p8c-01` implements ADR 0003's portable threshold for
`-/filter_complex <path>`. Use a
private per-spawn file, structured argv, deterministic cleanup after child exit,
and sanitized diagnostics. Test short inline graphs, forced file transport,
spaces/non-ASCII paths, spawn failure, cancellation, and cleanup on every
supported native platform shape.

### Orientation and animated WebP

Milestone `p8c-02` persists and applies source display-matrix/orientation
metadata. Compare software,
`transpose_cuda`, and fused libplacebo rotation, preserving the output contract
and avoiding duplicate autorotation. Milestone `p8c-03` adds animated WebP input
fixtures for
watermarks/graphics and prove frame timing, looping, alpha, bounded resource
use, and both IPTV output modes. These are support improvements, not reasons to
require the filters on a platform that does not use the corresponding path.

## Workstream 6: First-class Vulkan encoding decision

Milestone `p8c-04` is deferred until the operation-scoped Vulkan gate and HDR
benchmarks exist. Revisit only with first-party runtime evidence for
`h264_vulkan`/`hevc_vulkan`, multi-vendor device selection, quality and latency,
MPEG-TS/HLS compatibility, fallback behavior, and the six-RID packaging matrix.

If the evidence supports a persisted `HardwareAccelerationKind::Vulkan`, write
a separate ADR before schema, profile-editor, or encoder-priority changes.

## Delivery order

1. `p8a-01`: establish truthful SDR/HDR10 output.
2. `p8b-01`: make the existing Vulkan path safe to select and self-healing.
3. `p8a-02`: characterize and implement Dolby Vision input behavior against the
   stable output contract.
4. `p8b-02`: benchmark and adopt hardware graph improvements.
5. `p8c-01`, `p8c-02`, and `p8c-03`: land independent long-graph,
   orientation, and animated-WebP support slices.
6. `p8c-04`: decide whether first-class Vulkan encoding is justified; this
   deferred decision does not block Phase 9.

Workstreams 1, 3, and the long-filtergraph portion of 5 can proceed in parallel
when separate changes do not touch the same pipeline builder sections.

Phase 9 begins only after the required Phase 8 milestones `p8a-01`, `p8a-02`,
`p8b-01`, `p8b-02`, `p8c-01`, `p8c-02`, and `p8c-03` are complete.

## Closeout gates

Each implementation slice updates the owning parity debt, health checks, and
`CHANGELOG.md`. The complete plan closes only after:

- focused crate tests and reviewed FFmpeg argv snapshots pass;
- both SQLite and MySQL mappings pass for persisted fields;
- all changed admin UI labels are localized and hydration/browser-proven;
- `./run.sh test-closeout`, both workspace Clippy configurations,
  `cargo leptos build`, and `cargo xtask parity-check` pass;
- `cargo xtask smoke` and the TSDuck playback gate pass for playback-sensitive
  changes;
- HDR/Dolby Vision output is validated from decoded frames and side data, not
  solely from command generation;
- `cargo xtask progress check` reports no tracker drift.
