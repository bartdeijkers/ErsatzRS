# ADR 0017: Default FFmpeg profiles for multi-resolution streaming

## Status

Accepted (2026-06-17; revised — see Revision history).

## Implementation

Implemented — fresh database bootstrap seeds the five default profiles, web
startup applies seed-time hardware-acceleration detection from the captured
FFmpeg runtime report, and HLS segmenter output emits AC-3 `-max_delay` for
multichannel AC-3. The 2026-07-01 (v3) bitrate revision landed on 2026-07-02:
seeded profile video bitrates are 4000, 8000, 10000, 22500, and 30000 kbit/s
while video buffer sizes keep the v2 values (8000, 16000, 20000, 45000, and
60000 kbit).

The two HDR seeds became truthful HDR10 outputs on 2026-08-07, when
[ADR 0064](0064-hdr-and-dolby-vision-input-output-contract.md) landed the
persisted `OutputDynamicRange` contract and HDR10 output on the software and
NVENC paths. Backends with no characterized HDR10 graph demote to software
rather than emitting SDR, so an HDR-named seed no longer silently tone-maps to
BT.709.

The 2026-08-09 (v5) FFmpeg 9 preset retune landed the same day: seed-time
encoder presets are `veryfast` for software, `medium` for QSV, and `p5` with
tune `hq` for NVENC.

## Context

ErsatzRS inherited ErsatzTV's single conservative default FFmpeg profile
(`ensure_default_ffmpeg_profile` in
[`crates/ersatzrs-infra/src/db.rs`](../../crates/ersatzrs-infra/src/db.rs),
named `1920x1080 x264 aac`):

- H.264 High, **video preset unset** (so libx264 uses `medium`), no B-frames,
  8-bit, deinterlace on, framerate normalization off;
- video 2000 kbps / 4000 kbit buffer;
- AAC, 192 kbps, 2 channels, 48 kHz.

That single low-bitrate stereo profile optimizes for broad compatibility, not
for good-looking playback, and it forces every fresh install to hand-build a
profile before it can serve anything but a soft 1080p stereo stream. It also
offers nothing for HDR or 4K libraries and never uses available GPU encoders.

The target deployment is concrete: **up to two simultaneous channels
(`N = 2`, [ADR 0016](0016-active-hls-transcode-limit.md)) of 25 fps content,
AC-3 6-channel audio, served as HTTP unicast over 1 Gbps Ethernet**, on a host
that may or may not have a hardware encoder. Bitrates are sized for 25 fps and
for plenty of network headroom, prioritizing **smoothness first, then image
quality**.

The original HDR rows selected HEVC Main 10 and 10-bit output but did not add a
persisted output dynamic-range contract. Current command generation therefore
tone-maps every transcoded HDR source to BT.709 even when one of those rows is
selected. ADR 0064 separates bit depth from color semantics and owns the
correct source-to-output transformation and metadata rules.

Changing the shipped defaults away from ErsatzTV is a deliberate deviation, so
it is recorded here (porting rule).

### Findings — encoder hygiene (retained from the original revision)

The encoder hygiene that keeps an HLS segmenter smooth, and how ErsatzRS already
stands against it:

- **GOP must be fixed and segments aligned to it.** Already correct: `hls_output`
  in [`crates/ersatzrs-ffmpeg/src/pipeline/mod.rs`](../../crates/ersatzrs-ffmpeg/src/pipeline/mod.rs)
  emits a 2 s GOP (`-g` = fps × 2), `-keyint_min` = GOP, `-force_key_frames`
  every 2 s, `-flags cgop`, and 4 s segments (2 GOPs). The math adapts to each
  source's framerate.
- **Scene-cut must be disabled** so segment lengths stay uniform. Already
  effectively done: `keyint_min == keyint` disables scene-cut keyframes.
- **Rate control should be CBR-ish with a bounded VBV** (`maxrate` = target,
  `bufsize` from the profile; avoid very large buffers that defeat HLS
  smoothing). Wiring already present: `-b:v`/`-maxrate:v` plus `-bufsize:v`, and
  the audio equivalents, are emitted from the profile — the *values* are what
  this ADR sets. The 2026-07-01 revision intentionally changes only video
  bitrate targets; existing video buffer sizes stay unchanged.
- **Multichannel AC-3 needs tight A/V interleaving** (`-max_delay`) or audio can
  pop/drift at segment boundaries. Implemented for multichannel AC-3 HLS output.
- **`-muxrate` / `pkt_size`** belong to a constant UDP push to an external
  segmenter. ErsatzRS *is* the `-f hls` segmenter served over HTTP, so they do
  not apply and must not be added.

## Decision

On a fresh database, seed **five** ready-to-use default FFmpeg profiles instead
of one, **auto-enable hardware acceleration when a usable encoder is detected**,
and keep the AC-3 segmenter fix. All five share AC-3 6-channel audio.

### 1. Five seeded profiles

Common to every profile: **AC-3, 640 kbps, 640 kbit buffer (≈1 s), 6 channels,
48 kHz**; deinterlace on (mixed library sources); framerate normalization off;
B-frames off (simple, predictable segment alignment); video buffer sizes remain
at the original v2 values and are intentionally not halved with the video
bitrate targets; software fallback preset `veryfast`.

| # | Name | Resolution | Codec / profile | Bit depth | Dynamic range | Video bitrate | VBV buffer |
| --- | --- | --- | --- | --- | --- | --- | --- |
| 1 | `1280x720 x264 ac3 6ch` | 1280×720 | H.264 High | 8-bit | SDR (BT.709) | 4 000 kbps | 8 000 kbit |
| 2 | `1920x1080 x264 ac3 6ch` **(default)** | 1920×1080 | H.264 High | 8-bit | SDR (BT.709) | 8 000 kbps | 16 000 kbit |
| 3 | `1920x1080 x265 10-bit hdr ac3 6ch` | 1920×1080 | HEVC Main 10 | 10-bit | HDR10 (BT.2020 / PQ) | 10 000 kbps | 20 000 kbit |
| 4 | `3840x2160 x265 ac3 6ch` | 3840×2160 | HEVC Main | 8-bit | SDR (BT.709) | 22 500 kbps | 45 000 kbit |
| 5 | `3840x2160 x265 10-bit hdr ac3 6ch` | 3840×2160 | HEVC Main 10 | 10-bit | HDR10 (BT.2020 / PQ) | 30 000 kbps | 60 000 kbit |

- **Codec split:** H.264 for 720p/1080p SDR (broad client compatibility); HEVC
  for everything 10-bit/HDR and for 4K (H.264 at 4K is impractical). HDR
  profiles are HEVC **Main 10**, 10-bit, BT.2020 + SMPTE-2084 (PQ) and must
  preserve HDR rather than tonemap to SDR; SDR profiles normalize to BT.709.
- **Bitrates are tuned for 25 fps** content and biased toward balanced quality
  and smoother resource use. The 2026-07-01 revision halves the v2 video
  bitrate targets while preserving the v2 video buffer sizes exactly — see
  Consequences for the headroom math.
- **The default-of-defaults** (the profile the single
  `ffmpeg.default_profile` config points at, used as the fallback when a channel
  has no explicit profile) is profile **#2** (`1920x1080 x264 ac3 6ch`). The
  other four are seeded and immediately selectable per channel.
- The three resolution rows these reference (1280×720, 1920×1080, 3840×2160) are
  already seeded by `ensure_default_resolution`.

Persist ADR 0064's `OutputDynamicRange` with these fresh-database values:

- profiles 1, 2, and 4: `SdrBt709`;
- profiles 3 and 5: `Hdr10Bt2020Pq`.

An additive migration defaults every pre-existing profile to `SdrBt709`, which
preserves the current effective transcode behavior and the existing rule that
saved profiles are not silently retuned. Administrators can explicitly select
HDR10 after reviewing codec/client compatibility. Video-copy profiles ignore
the field because they retain source bitstream signaling.

### 2. Hardware acceleration auto-enabled when detected

At seed time, detect the best available encoder and set
`hardware_acceleration` on all five profiles accordingly; if none is usable,
leave it `None` (software). Detection priority:
**Nvenc → Qsv → Vaapi → VideoToolbox → Amf → None.** This pairs with
[ADR 0014](0014-runtime-learned-hardware-capability-fallback.md): if a seeded
hardware path turns out unusable at runtime, the learned-capability fallback
demotes that spawn to software automatically, so an over-optimistic seed is
self-correcting rather than fatal.

Seed-time presets are chosen per encoder against the option domains FFmpeg 9
actually advertises:

| Detected acceleration | Seeded preset | Seeded tune |
| --- | --- | --- |
| None (libx264 / libx265) | `veryfast` | — |
| QSV | `medium` (target usage 4) | — |
| NVENC | `p5` | `hq` |
| VAAPI, VideoToolbox, AMF, V4L2M2M, RKMPP | unset | — |

The NVENC pair is deliberately **not** the `p7` + `ll` that
[ADR 0063](0063-ffmpeg-9-runtime-and-profile-options.md)'s legacy `llhq`
migration produces. That mapping exists to preserve already-saved profiles
bit-exactly; adopting it for fresh installs was a side effect, not a quality
decision. ErsatzRS serves 4-second HLS segments, so encoder latency is never
the constraint and `ll` only trades away compression efficiency. `p5` ("slow,
good quality") sits one step above FFmpeg 9's own `p4` default and stays
realtime at 2160p10 on Turing and newer, where `p7` is a genuine realtime risk
for seed #5. `hq` is FFmpeg 9's default tune.

QSV's `veryfast` is target usage 7 — the fastest and lowest-quality setting on
a fixed-function encoder where TU4 costs almost nothing — so the seed uses
`medium`. Software keeps `veryfast` so a CPU-only host stays realtime,
including on the 2160p seeds.

Admins can pick a different preset per channel. Making that true required
widening the preset allowlist, which ErsatzTV's
`AvailablePresets.ForAccelAndFormat` had pinned to the single value `veryfast`
for both software and QSV: ErsatzRS offers the full `libx264`/`libx265` ladder
(`ultrafast` … `veryslow`) and the full QSV target-usage ladder (`veryfast` …
`veryslow`). `placebo` is never offered — it cannot sustain realtime at any
resolution. This is a recorded ErsatzTV divergence.

Because the two ladders share names while meaning different things, an ADR 0014
demotion from hardware to software never reuses the hardware profile's preset
even when the string is a valid x264 name; it falls back to `veryfast`. QSV
`medium` is a fixed-function quality point, not a CPU budget.

### 3. Segmenter A/V interleaving

Add `-max_delay 500000` (0.5 s) to the HLS segmenter output when the audio is
multichannel AC-3, so 6ch audio interleaves tightly with video and does not pop
or drift at segment seams. (Applies to all five profiles, since all use AC-3
6ch.)

### 4. Scope of the change

The five profiles seed on **fresh databases only**; existing installs keep their
saved profiles (ErsatzRS does not overwrite user data). The seeded profiles are
starting points an admin can edit or delete per channel.

## Consequences

- **Network (1 Gbps unicast, `N = 2`):** per-stream payload ranges from
  ≈4.3 Mbps (720p) to ≈31 Mbps (4K HDR) including TS overhead. The worst case,
  two 4K HDR streams, is ≈66 Mbps — about **7 % of a 1 Gbps link**. Every other
  combination is well under that. Network is not the constraint.
- **Compute:** with hardware acceleration auto-enabled, even two 4K HDR streams
  are comfortable on a modern GPU. On a **software-only** host the 4K profiles
  are heavy; such hosts should prefer the ≤1080p profiles or accept high CPU,
  and `N` (ADR 0016) is sized to the real encoder, not the network.
- **Quality / features out of the box:** the five seeds provide 720p→4K and
  6-channel surround. The HDR-named seeds produce actual HDR10 output as of
  2026-08-07 on the software and NVENC paths (ADR 0064).
- **Pipeline prerequisites:** the HDR profiles require stable HEVC Main 10
  BT.2020/PQ output across SDR/HDR item transitions, correct static metadata,
  and equivalent software/hardware semantics. Software and NVENC satisfy this;
  the remaining hardware backends have no characterized HDR10 graph and demote
  to software rather than emitting SDR. Open work is tracked under ADR 0064.
- **Correctness:** `-maxrate` remains effective while video buffer sizes keep
  the known v2 values; `-max_delay` removes a real multichannel-AC-3 seam
  artifact.
- **Implementation:** `ensure_default_ffmpeg_profile` seeds the five profiles
  and points `ffmpeg.default_profile_id` at #2; web startup applies the
  detected encoder choice to that fresh seed only; HLS output emits
  `-max_delay 500000` for multichannel AC-3. The seeded video bitrate values
  carry the 2026-07-01 halving retune while video buffer sizes keep the v2
  values. Output-dynamic-range persistence and truthful HDR10 output remain
  future work.

## Alternatives Considered

- **Keep the single conservative ErsatzTV default.** Rejected: it gives poor
  quality, ignores HDR/4K, and never uses a present GPU.
- **Seed one profile and make the admin clone it for other resolutions.**
  Rejected: the five common targets (720p/1080p/1080p-HDR/4K/4K-HDR) are
  predictable enough to ship ready-made; first-run UX matters.
- **Force hardware acceleration on regardless of detection.** Rejected: fails
  hard on software-only hosts; detection + ADR 0014 runtime fallback is the
  safe form.
- **Bitrates sized to saturate the 1 Gbps link.** Rejected: needless — quality
  gains above these bitrates at 25 fps are marginal, and large bursts defeat
  HLS input smoothing. A tight ≈1 s VBV is kept instead.
- **Tonemap HDR sources to SDR in the default.** Rejected for the HDR profiles:
  their purpose is to preserve HDR; tonemapping remains available on the SDR
  profiles for HDR→SDR libraries.

## Revision history

- **2026-08-09 (v5 — implemented 2026-08-09).** Retune the seed-time encoder
  presets against the option domains FFmpeg 9 advertises. NVENC moves from
  `p7` + `ll` to `p5` + `hq`, QSV from `veryfast` (TU7) to `medium` (TU4), and
  software keeps `veryfast`. Widen the preset allowlist from ErsatzTV's
  single `veryfast` to the real `libx264`/`libx265` and QSV ladders so the
  "admins can pick a slower preset" promise is actually true, excluding
  `placebo`. Expose FFmpeg 9's new NVENC `uhq` tune, which is codec-gated:
  `hevc_nvenc` and `av1_nvenc` advertise it, `h264_nvenc` does not. Fix the
  ADR 0014 demotion path so a hardware preset name is never reused for a
  software encode. Existing saved profiles are untouched, and the ADR 0063
  legacy `llhp`/`llhq` migration is unchanged.
- **2026-08-07 (v4 — partially implemented).** Separate bit depth from output
  dynamic range through ADR 0064. Fresh SDR seeds target `SdrBt709`, fresh HDR
  seeds target `Hdr10Bt2020Pq`, and the additive migration preserves existing
  effective behavior by defaulting saved rows to SDR. Correct the implementation
  record: current HDR transcodes are tone-mapped to BT.709, so the HDR seed
  contract remains open.
- **2026-07-01 (v3 — implemented 2026-07-02).** Halve every seeded default
  profile video bitrate from the v2 targets while leaving `VideoBufferSize`
  untouched: 8000→4000, 16000→8000, 20000→10000, 45000→22500, and 60000→30000.
  The 1080p default therefore becomes 8000 kbit/s video with the existing
  16000 kbit video buffer.
- **2026-06-17 (v2).** Replaced the single CPU-only
  `1920×1080 x264 ac3 6ch` default (framed for a 100 Mbps LAN, smoothness on
  CPU) with **five** seeded profiles (720p / 1080p / 1080p HDR / 2160p /
  2160p HDR), all AC-3 6ch 640 kbps, with **bitrates optimised for 25 fps over
  1 Gbps unicast** (`N = 2`) and **hardware acceleration auto-enabled when
  detected**. The encoder-hygiene findings and the AC-3 `-max_delay` fix are
  unchanged. The original single-profile rationale is preserved in this history
  and in the Context's description of the inherited default.
- **2026-06-17 (v1 — original).** Retuned the single inherited default to
  `1920×1080 x264 ac3 6ch`, preset `veryfast`, video 15000/15000 kbit, AC-3
  640/640 kbit 6ch, for two 1080p60 software encodes over a 100 Mbps LAN.

## References

- [ADR 0016: HLS transcode capacity and admission control](0016-active-hls-transcode-limit.md)
- [ADR 0014: Runtime-learned hardware-capability fallback](0014-runtime-learned-hardware-capability-fallback.md)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0008: FFmpeg 8.1 runtime and multi-arch images](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
- [ADR 0063: FFmpeg 9 runtime and profile options](0063-ffmpeg-9-runtime-and-profile-options.md)
- [ADR 0064: HDR and Dolby Vision input and output contract](0064-hdr-and-dolby-vision-input-output-contract.md)
- [FFmpeg 9 performance and support progress](../progress/parity/ffmpeg-9-performance-and-support.toml)
- [`crates/ersatzrs-infra/src/db.rs`](../../crates/ersatzrs-infra/src/db.rs) — default profile/resolution seed
- [`crates/ersatzrs-ffmpeg/src/pipeline/mod.rs`](../../crates/ersatzrs-ffmpeg/src/pipeline/mod.rs) — `hls_output`, `add_output_options`
- [`crates/ersatzrs-domain/src/ffmpeg.rs`](../../crates/ersatzrs-domain/src/ffmpeg.rs) — profile/codec/HW enums
