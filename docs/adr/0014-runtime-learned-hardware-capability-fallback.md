# ADR 0014: Runtime-learned hardware-capability fallback

## Status

Accepted (2026-06-12).

## Implementation

Partial — implemented for the NVIDIA graphs (2026-08-07, milestone `p8b-01`),
not yet for the other backends. Decode
capability fallback covers the HLS segmenter and direct IPTV `.ts` spawns
(architecture gate, CUDA-filter decode gate, refresh-triggered denial
invalidation). The operation-scoped Vulkan/libplacebo graph probe, denial, health
visibility, and safe fallback decided in the 2026-08-07 revision now exist:
`ersatzrs_ffmpeg::graph_probe` owns the operations, bounded functional probes,
and the failure classifier built from captured FFmpeg 9 stderr;
`ersatzrs_infra::hardware_graph` runs the probes and persists denials keyed by
the complete FFmpeg/ffprobe binary and selected-device identity; the web host
probes at startup and re-probes on an explicit capability refresh. Runtime
publication is generation-scoped and atomic: a settings save validates and
probes the complete candidate before one database transaction and one in-memory
publication replace the active paths, report, graph state, and denial state.
Failures leave the prior runtime intact, and feedback from a child built under
an older generation cannot mutate the replacement runtime.

The native NVIDIA probe and every generated NVIDIA graph are explicitly bound
to CUDA device index zero. The report records that adapter's UUID, model, and
driver version instead of unioning capabilities across adapters. The two
profile-5 conversion operations are modelled separately because their SDR/H.264
and HDR10/HEVC graphs have different encoders and output contracts. Only the
NVIDIA graphs are modelled; QSV,
VAAPI, AMF, VideoToolbox, RKMPP, and V4L2M2M graphs are still selected from
advertised capabilities alone.

`scripts/e2e/nvenc-filter-gate-proof.sh` live-proves the encoder-only NVENC path
on a matching FFmpeg build that reports `cuda` and `h264_nvenc` while omitting
`scale_cuda`, `yadif_cuda`, and `overlay_cuda` (`ParityDebt
nvenc-filter-gate-live-proof` resolved 2026-06-20).

## Context

ErsatzRS gates hardware decode/encode decisions on a runtime capability
report: native probes through `ersatzrs-hwprobe` (`cuvidGetDecoderCaps`,
NVENC encode-session probes, VideoToolbox framework enumeration) with FFmpeg
sample encode/decode smoke probes as the unavailable-library fallback (see
[`docs/progress/parity/ffmpeg-command-generation.toml`](../progress/parity/ffmpeg-command-generation.toml)).

ErsatzTV makes the same decision from a static GPU model/architecture table
in its hardware capability checker. That table is conservative — it cannot
over-report, so unsupported sources are routed to software decode before
spawn — but it goes stale as new hardware ships and cannot express
driver-level variation.

Runtime evidence shows the probe-based model can over-report. On 2026-06-10
(`ErsatzRS.log`), the channel 4 HLS segmenter exited with code 69 and "Your
platform doesn't support hardware accelerated AV1 decoding", repeated
`Error submitting packet to decoder: Function not implemented`,
`cuCtxPushCurrent ... CUDA_ERROR_INVALID_VALUE`, and `Decode error rate 1
exceeds maximum` — even though the 2026-06-09 tightening already required a
positive native/probed NVIDIA AV1 decoder detail row before selecting CUDA
decode. A probe row can disagree with FFmpeg's own runtime acceptance
(profile/bit-depth/film-grain mismatch, driver gaps, or a stale persisted
report), and any gating decided before spawn can never fully predict the
runtime outcome. Today that failure mode is a dead channel.

The NVENC HDR path adds a second form of probe-versus-reality mismatch. It
selects a Vulkan/libplacebo-to-CUDA graph when the runtime advertises the
Vulkan hwaccel, but that single capability does not prove libplacebo is built,
the selected Vulkan and CUDA devices refer to the intended GPU, hardware frame
mapping works, or the driver accepts the complete graph. An operator-provided
FFmpeg 9 runtime is valid under ADR 0063 and may expose a different optional
filter/device combination from the first-party package.

## Decision

Layer the hardware-capability decision, adding a runtime-learned negative
path on top of the existing probes:

1. **Pre-spawn gating stays probe-based** — native detail rows preferred,
   FFmpeg smoke probes as fallback. Unchanged.
2. **Port ErsatzTV's conservative architecture gate** as an additional AND
   for NVIDIA decode selection where GPU model information is available.
   This is the parity-faithful floor: when the upstream table says a codec
   is unsupported on the detected architecture, ErsatzRS must not select
   hardware decode regardless of what a probe row claims.
3. **Runtime-learned denial.** Streaming process supervisors (HLS segmenter,
   IPTV `.ts` spawns) classify stderr on abnormal exit. On a known
   hardware-decode-unsupported signature (e.g. FFmpeg's "Your platform
   doesn't support hardware accelerated <codec> decoding" class), the
   supervisor restarts the session **exactly once** with software input
   decode while preserving the hardware encoder, and persists a denial row
   (codec, bit depth, acceleration kind) into the capability report. Command
   generation treats a denial row as authoritative-negative, so every later
   command for that codec/depth/accel combination selects software decode
   upfront.
4. **Visibility.** Every learned denial is surfaced through the health-check
   engine and troubleshooting output (per the health-check parity contract
   in `AGENTS.md`), and the triggering stderr signature is recorded as
   runtime evidence in the affected `docs/progress/parity/<slug>.toml`.
5. **Operation-scoped graph capabilities.** Selecting a multi-device hardware
   graph requires every component used by that operation: hwaccel, filters,
   encoder, selected device, hardware frame mappings, and inter-device
   interoperability. An advertised component alone is not positive evidence
   for the graph. The first required operation is NVENC HDR tonemapping through
   Vulkan/libplacebo followed by CUDA upload and NVENC encode.
6. **Functional graph probe.** Capability refresh executes a bounded synthetic
   graph for complex operation-scoped paths and caches the result with the
   FFmpeg binary and device/driver identity. Missing optional hardware remains
   a normal negative capability, not a startup-wide FFmpeg failure.
7. **Runtime-learned graph denial.** Supervisors classify known Vulkan-device,
   libplacebo, hardware-map, and CUDA-interoperability initialization failures.
   They retry exactly once through the next positively proven graph, preserving
   hardware encode when possible and falling back to software when necessary.
   The operation-scoped denial is persisted, visible, and invalidated through
   the same capability-refresh lifecycle as decode denials. Arbitrary filter or
   encode failures remain non-retryable.

Denial rows persist in `ConfigElement` alongside the capability report and
are cleared when the capability report is explicitly refreshed by an FFmpeg
path change, a settings-save capability refresh, or a manual refresh — the
situations in which the underlying hardware/driver/binary may have changed.

Cached graph proofs and learned denials are reusable only for an exact runtime
identity: FFmpeg and ffprobe paths plus SHA-256 digests, and the selected CUDA
device index, UUID/model, and driver version. Legacy path-only/model-only rows,
partial records, malformed identities, same-path binary replacements, and
device or driver changes all force a new probe. Each accepted publication also
receives a monotonically increasing in-process generation. Commands carry that
generation from build through supervision, so late failures from an old child
are ignored instead of poisoning the active report or consuming its retry.

Settings changes use a staged commit boundary. Candidate binaries and graph
operations are checked without touching the global locators. Once valid, one
serialized commit persists the settings mutations and complete graph state,
clears obsolete decode denials, swaps the shared report, and publishes the
paired FFmpeg/ffprobe paths. Post-commit maintenance such as report-directory
creation and subtitle extraction starts only after that boundary.

## Consequences

A probe-vs-reality mismatch (the next AV1, a new codec, a driver quirk)
degrades to software decode after one failed spawn instead of leaving a dead
channel. This is a deliberate behavioral divergence from ErsatzTV, which
avoids the situation with a static table and otherwise fails: ErsatzRS keeps
the more precise probes *and* gains a self-healing floor.

The single-retry bound, the health-check row, and the recorded stderr
signature exist so the fallback cannot silently mask pipeline defects: a
learned denial is loud, queryable, and tied to evidence. Supervisors must
not retry on arbitrary FFmpeg failures — only on a classified
decode-unsupported or operation-scoped graph-initialization signature.

The same boundary now applies to complex hardware graphs: individual advertised
filters or hwaccels are necessary but insufficient. A graph is selected only
after a functional probe, and runtime fallback remains bounded and visible.
This increases capability-state detail and probe time, but prevents optional
Vulkan/libplacebo failures from taking a channel offline.

Costs: one extra spawn of startup latency the first time a combination
fails; new stderr-classification and denial-persistence code in
`ersatzrs-infra`; an invalidation policy that must stay aligned with the
capability-refresh triggers. Follow-up work: `insta` snapshot coverage for
denial-driven argv, a focused regression for the restart decision, and the
ErsatzTV architecture-table port in step 2.

## Alternatives Considered

- Static architecture table only (full ErsatzTV parity): rejected because it
  discards the precision of native probes and still fails outright on table
  gaps or new hardware.
- Probe-only status quo: rejected — demonstrated to over-report on
  2026-06-10, producing exit-69 dead channels.
- Retry without persisting the denial: rejected because every new session
  would pay the failed hardware spawn again.
- Blanket retry on any FFmpeg failure: rejected because it masks genuine
  pipeline defects; only classified decode-unsupported or operation-scoped
  graph-initialization signatures may trigger the fallback.
- Pre-spawn live decode self-test per source: rejected — adds per-item
  startup latency to every playback and still cannot cover all runtime
  states.
- Treat `vulkan` in `ffmpeg -hwaccels` as proof of the complete NVENC HDR
  graph: rejected because it says nothing about libplacebo, device affinity,
  hardware mappings, or Vulkan/CUDA interoperability.
- Make libplacebo globally mandatory: rejected because non-Vulkan pipelines
  and supported machines without Vulkan must continue to run normally.

## Revision history

- **2026-08-07.** Extended learned capability fallback from decoder tuples to
  operation-scoped multi-device graphs, beginning with the existing NVENC
  Vulkan/libplacebo HDR path. Added bounded functional probing, typed graph
  denials, health visibility, and one safe fallback attempt.
- **2026-08-09.** Made runtime state an atomic, generation-scoped publication;
  keyed cached proof and denial state to binary digests plus selected-device and
  driver identity; bound NVIDIA probing and execution to device zero; and added
  distinct Profile 5 SDR and HDR10 graph operations.

## References

- [`docs/progress/parity/ffmpeg-command-generation.toml`](../progress/parity/ffmpeg-command-generation.toml)
  (debt `av1-nvenc-live-fallback`)
- [`docs/progress/parity/ffmpeg-filter-graph.toml`](../progress/parity/ffmpeg-filter-graph.toml)
  (debt `nvenc-cuda-live-reverification`)
- [Runtime triage routine](../automation/runtime-triage.md)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0008: FFmpeg 8.1 runtime and multi-arch images](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
- [ADR 0063: FFmpeg 9 runtime and profile options](0063-ffmpeg-9-runtime-and-profile-options.md)
- [ADR 0064: HDR and Dolby Vision input and output contract](0064-hdr-and-dolby-vision-input-output-contract.md)
- [FFmpeg 9 HDR, Dolby Vision, and pipeline hardening plan](../../plans/ffmpeg-9-hdr-dolby-vision-and-pipeline-hardening.md)
- ErsatzTV hardware capability checker under `$ERSATZTV_SRC/ErsatzTV.FFmpeg/`
  at the pinned parity baseline commit
