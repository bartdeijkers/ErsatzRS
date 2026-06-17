# ADR 0017: Default FFmpeg profile tuned for smooth 1080p60 streaming

## Status

Accepted (2026-06-17).

## Context

ErsatzRS inherited ErsatzTV's conservative default FFmpeg profile
(`ensure_default_ffmpeg_profile` in
[`crates/ersatzrs-infra/src/db.rs`](../../crates/ersatzrs-infra/src/db.rs),
named `1920x1080 x264 aac`):

- H.264 High, **video preset unset** (so libx264 uses `medium`), no B-frames,
  8-bit, deinterlace on, framerate normalization off;
- video 2000 kbps / 4000 kbit buffer;
- AAC, 192 kbps, 2 channels, 48 kHz.

That default optimizes for broad compatibility and low bitrate, not for smooth,
good-looking playback. Two parts of it actively hurt the target use case:

- **Preset `medium` cannot sustain two simultaneous 1080p60 software encodes in
  realtime** on a normal CPU, so the segmenter falls behind realtime and the
  stream stutters.
- **A 4000 kbit VBV buffer is far too small** for anything but a ~2 Mbps stream,
  so raising the bitrate alone would make `-maxrate` throttle constantly.

The target deployment is concrete: **two simultaneous 1080p60 channels with
AC-3 6-channel audio, software (CPU) encoding, served over a 100 Mbps LAN.** This
pairs with the capacity ceiling in
[ADR 0016](0016-active-hls-transcode-limit.md) (`N = 2`).

Changing the shipped defaults away from ErsatzTV is a deliberate deviation, so
it is recorded here (porting rule).

### Findings (folded in from the retired recommendations note)

The encoder hygiene that keeps an HLS segmenter smooth, and how ErsatzRS already
stands against it:

- **GOP must be fixed and segments aligned to it.** Already correct: `hls_output`
  in [`crates/ersatzrs-ffmpeg/src/pipeline/mod.rs`](../../crates/ersatzrs-ffmpeg/src/pipeline/mod.rs)
  emits a 2 s GOP (`-g` = fps × 2, i.e. 120 @ 60 fps), `-keyint_min` = GOP,
  `-force_key_frames` every 2 s, `-flags cgop`, and 4 s segments (2 GOPs).
- **Scene-cut must be disabled** so segment lengths stay uniform. Already
  effectively done: `keyint_min == keyint` disables x264 scene-cut keyframes.
- **Rate control should be CBR-ish with a tight VBV** (`maxrate` = target,
  `bufsize` ≈ 1× the bitrate; avoid 5 s+ buffers that defeat HLS smoothing).
  Wiring already present: `-b:v`/`-maxrate:v` plus `-bufsize:v`, and the audio
  equivalents, are emitted from the profile — only the *values* are wrong.
- **Multichannel AC-3 needs tight A/V interleaving** (`-max_delay`) or audio can
  pop/drift at segment boundaries. **Missing today** — the one genuine code gap.
- **`-muxrate` / `pkt_size`** from generic streaming advice apply to a constant
  UDP push to an external segmenter. ErsatzRS *is* the `-f hls` segmenter served
  over HTTP, so they do not apply and must not be added.

## Decision

Adopt a default profile and segmenter settings tuned for the 1080p60 / AC-3 6ch
/ 2-stream / 100 Mbps target, prioritizing **smoothness first, then image
quality**, on CPU.

### 1. New default FFmpeg profile

Rename the seeded default to `1920x1080 x264 ac3 6ch` with:

| Field | Current default | New default | Reason |
| --- | --- | --- | --- |
| Video format | H.264 | H.264 | broad client compatibility (HEVC remains opt-in) |
| Resolution | 1920×1080 | 1920×1080 | target |
| Video preset | unset (`medium`) | **`veryfast`** | only a fast preset sustains 2× 1080p60 software in realtime; smoothness beats marginal preset quality |
| Video profile | High | High | best quality at 8-bit |
| Allow B-frames | off | off | 15 Mbps is ample, so B-frame gains are marginal; off keeps latency/CPU predictable and segment alignment simple |
| Bit depth | 8-bit | 8-bit | standard SDR target |
| Video bitrate | 2000 kbps | **15000 kbps** | good 1080p60 quality at a fast preset |
| Video buffer | 4000 kbit | **15000 kbit** (≈1 s VBV) | makes `-maxrate` actually bind; 1 s is the HLS-smooth sweet spot |
| Audio format | AAC | **AC-3** | 6-channel surround target |
| Audio bitrate | 192 kbps | **640 kbps** | AC-3 6ch maximum |
| Audio buffer | 384 kbit | **640 kbit** (≈1 s) | match the video buffer window |
| Audio channels | 2 | **6** | surround |
| Audio sample rate | 48 kHz | 48 kHz | AC-3 standard |
| Deinterlace | on | on | mixed library sources |
| Normalize framerate | off | off | do **not** force 60 fps (see §3) |
| Hardware acceleration | None | None | CPU target |

### 2. Segmenter A/V interleaving

Add `-max_delay 500000` (0.5 s) to the HLS segmenter output when the audio is
multichannel AC-3, so 6ch audio interleaves tightly with video and does not pop
or drift at segment seams.

### 3. Retained and excluded behavior

- **Retained (already correct):** 2 s closed GOP, `keyint_min == GOP`
  (scene-cut disabled), `-force_key_frames` every 2 s, 4 s segments,
  `-maxrate` = bitrate, and `-bufsize` from the profile.
- **Framerate normalization stays off.** "1080p60" is the *capacity target* (the
  box is sized to encode up to 1080p60), not a mandate to upconvert every source
  to 60 fps — that would multiply CPU cost for no benefit. The GOP/segment math
  already adapts to each source's framerate.
- **Excluded:** `-muxrate` and `pkt_size`. They belong to constant-rate UDP push,
  not the `-f hls`-over-HTTP segmenter.

### 4. Scope of the default change

The new values seed the default profile on **fresh databases only**; existing
installs keep their saved profiles (ErsatzRS does not overwrite user data). The
default is a starting point an admin can still edit per channel.

## Consequences

- **Network:** ≈15.6 Mbps payload per stream, ≈16–17 Mbps with TS overhead, so
  two streams ≈ 33 Mbps (≈38 Mbps under the doc's CBR framing) — comfortably
  under the ~80 Mbps practical ceiling of a 100 Mbps link.
- **CPU:** the default assumes a capable multi-core CPU that can hold 2× 1080p60
  at `veryfast`. **Preset is the primary load lever** — drop to `faster`/`fast`
  on a stronger box for more quality, or, with NVENC/QSV/VAAPI available, switch
  the profile to hardware and raise resolution/preset/count (and `N` in
  ADR 0016).
- **Quality:** a large jump from 2 → 15 Mbps video and stereo AAC → 6ch AC-3,
  while the tight 1 s VBV keeps segment sizes predictable for smooth HLS.
- **Correctness:** raising `bufsize` to ~1× makes `-maxrate` effective instead of
  decorative; adding `-max_delay` removes a real multichannel-AC-3 seam artifact.
- **Follow-up:** update the seed in `db.rs` and its assertions, add the
  `-max_delay` step with an `insta` argv snapshot, and refresh
  `CHANGELOG.md` when the change lands.

## Alternatives Considered

- **Keep ErsatzTV's conservative defaults.** Rejected: 2 Mbps/1080p with a tiny
  buffer and `medium` preset gives poor quality and cannot run two 1080p60
  streams in realtime.
- **A slower preset for higher quality.** Rejected: it cannot sustain 2× 1080p60
  on CPU, causing stutter — which violates the smoothness-first priority.
- **Force 60 fps normalization to literally hit "1080p60".** Rejected:
  needless CPU cost; most content is 24/25/30 fps and the GOP already adapts.
- **`-muxrate` CBR padding + `pkt_size` from the source note.** Rejected:
  UDP-push concepts that do not apply to the `-f hls`/HTTP segmenter.
- **A large multi-second VBV buffer for peak quality.** Rejected: it lets the
  encoder dump bursts that defeat HLS input smoothing.

## References

- [ADR 0016: HLS transcode capacity and admission control](0016-active-hls-transcode-limit.md)
- [ADR 0003: Invoke FFmpeg through the CLI](0003-ffmpeg-cli-spawn.md)
- [ADR 0008: FFmpeg 8.1 runtime and multi-arch images](0008-ffmpeg-8-1-runtime-and-multi-arch-images.md)
- [`crates/ersatzrs-infra/src/db.rs`](../../crates/ersatzrs-infra/src/db.rs) — default profile seed
- [`crates/ersatzrs-ffmpeg/src/pipeline/mod.rs`](../../crates/ersatzrs-ffmpeg/src/pipeline/mod.rs) — `hls_output`, `add_output_options`
- [Implementation plan](../../plans/hls-transcode-capacity-and-admission.md)
