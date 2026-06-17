# ADR 0014: Runtime-learned hardware-capability fallback

## Status

Accepted (2026-06-12).

## Implementation

Partial — core implemented for the HLS segmenter and direct IPTV `.ts` spawns (architecture gate, CUDA-filter decode gate, refresh-triggered denial invalidation); live proof on an encoder-only FFmpeg build remains open (`ParityDebt nvenc-filter-gate-live-proof`).

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

Denial rows persist in `ConfigElement` alongside the capability report and
are cleared when the capability report is explicitly refreshed by an FFmpeg
path change, a settings-save capability refresh, or a manual refresh — the
situations in which the underlying hardware/driver/binary may have changed.

## Consequences

A probe-vs-reality mismatch (the next AV1, a new codec, a driver quirk)
degrades to software decode after one failed spawn instead of leaving a dead
channel. This is a deliberate behavioral divergence from ErsatzTV, which
avoids the situation with a static table and otherwise fails: ErsatzRS keeps
the more precise probes *and* gains a self-healing floor.

The single-retry bound, the health-check row, and the recorded stderr
signature exist so the fallback cannot silently mask pipeline defects: a
learned denial is loud, queryable, and tied to evidence. Supervisors must
not retry on arbitrary FFmpeg failures — only on the classified
decode-unsupported signature class.

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
  pipeline defects; only the classified decode-unsupported signature class
  may trigger the fallback.
- Pre-spawn live decode self-test per source: rejected — adds per-item
  startup latency to every playback and still cannot cover all runtime
  states.

## References

- [`docs/progress/parity/ffmpeg-command-generation.toml`](../progress/parity/ffmpeg-command-generation.toml)
  (debt `av1-nvenc-live-fallback`)
- [`docs/progress/parity/ffmpeg-filter-graph.toml`](../progress/parity/ffmpeg-filter-graph.toml)
  (debt `nvenc-cuda-live-reverification`)
- [Runtime triage routine](../automation/runtime-triage.md)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0008: FFmpeg 8.1 runtime and multi-arch images](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
- ErsatzTV hardware capability checker under `$ERSATZTV_SRC/ErsatzTV.FFmpeg/`
  at the pinned parity baseline commit
