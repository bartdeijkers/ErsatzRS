# HLS segmenter playback deviation root cause

This analysis records the 2026-06-15 five-minute calibration capture from the
HLS segmenter. The calibration clips carry a 50 ms audio pip every whole second
and a visible absolute timecode and frame counter.

The capture showed clean transport structure but drifting content time:

- TSDuck reported no transport errors, sync losses, continuity-counter
  discontinuities, PTS/DTS/PCR leaps, missing audio or video PIDs, or scrambling
  errors.
- Audio pips had no missing or duplicate one-second indices, but their phase
  changed around an HLS respawn and playout rollovers.
- Decoded frame 18,000, exactly 300 seconds at 60 fps, showed content 516.7 ms
  ahead of the expected absolute time.
- At one playout seam, the next item began roughly half a second early.
- The server log contained no warning, error, FFmpeg failure, or missing-playout
  event that explained the deviation.

## Root cause

The HLS segmenter stitches independently spawned FFmpeg generations by
scheduled playout time and output PTS continuity, not by measured decoded
content position.

The relevant path is `crates/ersatzrs-infra/src/hls_worker.rs`:

- `HlsSessionWorker::run` repeatedly spawns FFmpeg for bounded work-ahead or
  realtime slices instead of keeping one decode graph alive across item seams.
- `hls_playout_item_duration` bounds each spawn from the persisted playout item
  window and the worker request time.
- After a successful spawn, `transcoded_until` advances by the requested
  duration, assuming FFmpeg emitted exactly that amount of continuous decoded
  content.
- `probe_output_ts_offset` and `-output_ts_offset` preserve transport timestamp
  continuity for appended HLS generations, but they do not verify or correct
  the media phase of the next item.
- `content_seek_seconds` and the pipeline `StreamSeek` state carry whole
  seconds, so the spawned command cannot represent sub-second request offsets.

At ordinary HLS respawns, this mostly appears as small audio phase movement
from AAC and HLS packetization. At a real item boundary, the same model can cut
the outgoing item or start the incoming item early while still producing a
structurally valid MPEG-TS stream.

That distinction explains the apparently contradictory evidence: TSDuck proves
container and timestamp structure, while the audible pips and visible frame
counter prove content-timeline deviation. Transport validity alone is not
content continuity.

The source files are not the cause. Each MKV source has exactly 10,800 video
frames at 60 fps; its 180.021-second container duration comes from AAC
packetization. The server log also shows no runtime failure that would account
for the shift.

## Reproducing the evidence

The repeatable capture and analysis procedure now lives in
[Reproduce an HLS content-timing deviation](../how-to/reproduce-hls-content-timing-deviation.md).
Keeping the task procedure separate lets this page focus on the cause and
consequences.

## Fix direction

A complete fix must make HLS segmenter content time explicit:

- preserve sub-second seek and request positions through the HLS worker and
  FFmpeg command-generation path;
- reconcile appended generation boundaries against decoded media time, not
  only output PTS offset or scheduled playout finish;
- verify one-second audio-pip cadence and visible frame-counter continuity
  across work-ahead respawns and real item seams; and
- keep TSDuck structural checks without treating them as sufficient proof of
  content continuity.
