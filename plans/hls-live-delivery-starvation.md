# HLS live-delivery starvation correction

## Status

Completed on 2026-09-02 from redacted Windows dogfood and focused deterministic
proof. It owns milestone `hls-live-01`. Runtime code, the manual TSDuck gate,
health reporting, and the official yt-dlp add-on were in scope; production
database mutation was not.

## Confirmed symptom and corrected measurement

VLC reported visible starvation before one minute on the Windows HLS Segmenter
channel. A continuous 130-second TSDuck pull captured 413,923 structurally
valid MPEG-TS packets, but the first corrective implementation incorrectly
divided those packets by the 10.368 Mbps PCR-derived bitrate and classified the
result as only 60 seconds of media. That calculation assumes constant bitrate
and is invalid for this variable-rate HLS output. The installed yt-dlp
2026.08.19 metadata-only probe selected 1080p30 AVC at about 4.56 Mbps, which
also explains why the observed byte rate was about 4.79 Mbps without proving
media-time starvation.

The delivery gate now derives media elapsed time directly from PCR offsets and
keeps byte-rate monitoring relative to the capture's own wall-clock average.
Follow-up Windows A/B evidence isolated a healthy raw producer, a severely
under-delivering NVENC consumer, and a `libx264` consumer delivering about 44
seconds of media in 45 seconds. The invalid packet-count inference is not
accepted as root-cause evidence.

The bounded server log contained no FFmpeg exit or HLS failure for the
observation window. It recorded only normal idle-session cleanup. A live
FFmpeg process can therefore fall behind without an actionable log or health
result.

## Invariants

- Playback acceptance measures timely delivery as well as packet integrity.
- Startup latency, steady-state delivery, and transport correctness remain
  separate results.
- A live source that delivers less media time than elapsed wall time cannot
  pass, even when every received packet is structurally valid.
- Host telemetry is provider-neutral and contains no URL, token, cookie, media
  title, or private path.
- YouTube extraction, rendition selection, and provider retry behavior remain
  in the yt-dlp add-on. The host owns generic process supervision, HLS
  publication, health, and delivery contracts.
- Live validation uses one bounded provider session after deterministic tests
  pass; repeated probes are not a substitute for instrumentation.

## Implementation slices

### 1. Delivery-aware TSDuck gate

- Record monotonic capture time, PCR/media duration, wall-clock average byte
  rate, and delivery ratio without assuming constant bitrate.
- Run TSDuck's `bitrate_monitor` over a segment-aware rolling interval and
  retain its summary without exposing the input URL.
- Fail on a PCR media-time/wall-time ratio below the
  configured threshold while preserving continuity, PSI, PID, timestamp, and
  scrambling checks.
- Add deterministic media-time and rolling-rate fixtures for healthy, bursty,
  stalled, and resumed delivery. Structural integrity remains a separate
  TSDuck result while the delivery classifier must fail the stalled fixture.

### 2. Provider-neutral HLS under-run telemetry

- Track playlist sequence and media-duration advancement against monotonic
  elapsed time in the HLS worker.
- Report startup latency, current buffered duration, delivery ratio, and
  no-progress intervals with stable channel identifiers only.
- Persist a bounded latest under-run observation for the health engine. Expire
  stale success/failure state deliberately so one old event is not a permanent
  warning.
- Add a health check and localized operator guidance for active or recent HLS
  delivery under-run.

### 3. Layer isolation and correction

- Measure the yt-dlp producer, the host consumer, and HLS publication during
  one instrumented Windows session.
- If the producer under-runs, constrain and refresh provider renditions inside
  the add-on and use bounded no-progress recovery there.
- If the host consumer or publisher under-runs, correct generic realtime
  pacing, transcode throughput, or segment supervision. Do not use a larger
  startup buffer as the sole response to sustained under-delivery.
- Any automatic source restart or fallback that changes ErsatzTV-visible
  behavior must be captured in the owning ADR before it is enabled.

Implemented through ADR 0084: play v2 supplies a generic maximum video height.
After at least 20 seconds of severe under-delivery, the HLS worker first
restarts an eligible hardware H.264 output with `libx264`, then may perform one
1080p-to-720p restart if delivery still falls behind. A healthy fallback is
retained for 24 hours in a 256-entry provider-neutral cache. Software decisions
are isolated by channel and a complete FFmpeg profile/resolution fingerprint;
quality decisions remain media-scoped. Loading a verified 720p decision starts
a fresh observation, so it can still escalate to software encoding instead of
being trapped in an already-active under-run state. Exhausting the fallbacks
clears the learned decision without creating a restart loop. The official
yt-dlp add-on maps the ceiling into its format selector and omits a zero-offset
input seek. The corrected native build plus shared five-minute VLC/TSDuck
repetition is deferred to the next applicable packaged-Windows dogfood and
playback acceptance run, where it can be exercised with the broader runtime
suite. It is not a closure gate for this slice and is not represented as
having passed here.

## Acceptance

- Deterministic healthy, bursty-within-budget, stalled, and resumed sources
  classify correctly without real-time sleeps in Rust tests.
- A 130-second synthetic classification with only 60 seconds of valid media
  fails the manual gate despite zero continuity, TEI, sync, PTS, DTS, and
  PCR-leap faults.
- Health distinguishes no observation, healthy delivery, recent under-run, and
  stale evidence without leaking source identity.
- Bounded Windows A/B evidence isolates producer delivery from the hardware and
  software consumers, and the software consumer sustains approximately realtime
  delivery with the same managed FFmpeg runtime.
- Focused infra/web tests, shell syntax, progress validation, runtime smoke,
  the manual TSDuck gate, and applicable closeout checks pass.
