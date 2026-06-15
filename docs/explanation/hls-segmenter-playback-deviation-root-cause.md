# HLS segmenter playback deviation root cause

This note records the 2026-06-15 channel 5 `calibration` capture analysis.
The channel was captured from:

```text
http://localhost:8409/iptv/channel/5.m3u8?mode=segmenter
```

The calibration clips from `scripts/calibration/gen-calibration-clips.sh`
carry a 50 ms audio pip every whole second and a visible ABS timecode/frame
counter. The five-minute HLS capture showed clean transport structure but
content timing drift:

- TSDuck reported zero transport errors, sync losses, continuity-counter
  discontinuities, PTS/DTS/PCR leaps, missing video/audio PIDs, or scrambling
  errors.
- Audio pips had no missing or duplicate one-second indices, but the phase
  changed by +32 ms near an HLS respawn, by -488 ms at the clip 3 to clip 1
  playout rollover, and by +32 ms at the later clip 1 to clip 2 rollover.
- Decoded video frame 0 showed `ABS 00:07:03.000`. Decoded frame 18,000
  (exactly 300 seconds at 60 fps) showed `ABS 00:03:03.517`; expected was
  `ABS 00:03:03.000`, so video content was 516.7 ms ahead.
- The rollover probe showed frame 116 seconds at `ABS 00:08:59.000`, then the
  next item already at `ABS 00:00:00.517` by decoded second 117. The segmenter
  switched content early by about 0.5 seconds instead of preserving the media
  timeline through the playout seam.
- `C:\Users\Bart\AppData\Local\ErsatzRS\logs\ErsatzRS.log` contained no
  WARN/ERROR/FFmpeg failure/no-active-playout lines during the capture. The
  only channel 5 line was an informational idle-session stop after capture.

## Root cause

The HLS segmenter currently stitches independently spawned FFmpeg generations
by scheduled playout time and output PTS continuity, not by measured decoded
content position.

The relevant path is `crates/ersatzrs-infra/src/hls_worker.rs`:

- `HlsSessionWorker::run` repeatedly spawns FFmpeg for bounded work-ahead or
  realtime slices instead of keeping one decode graph alive across item seams.
- `hls_playout_item_duration` bounds each spawn from the persisted
  `PlayoutItem.Start` / `Finish` window and the worker request time.
- After a successful spawn, `transcoded_until` is advanced by the requested
  duration (`decision.request_time + duration`), assuming FFmpeg emitted
  exactly that amount of continuous decoded content.
- `probe_output_ts_offset` and `-output_ts_offset` preserve transport timestamp
  continuity for appended HLS generations, but they do not verify or correct
  the content phase of the next media item.
- `content_seek_seconds` and the pipeline `StreamSeek` state carry only whole
  seconds (`i64`), so sub-second playout/request offsets cannot be represented
  in the spawned FFmpeg command.

At ordinary HLS respawns, this mostly appears as small audio-only phase
movement from AAC/HLS packetization. At a playout item boundary, the same model
can cut the outgoing item early or start the incoming item early while still
emitting a structurally valid MPEG-TS stream. That is why TSDuck container
checks pass while the audible one-second pips and visible ABS frame counter
prove a content-timeline deviation.

The calibration source files are not the cause: the MKV sources have exactly
10,800 video frames at 60 fps per clip, and their 180.021 s container duration
comes from AAC packetization. The server log also does not show a runtime
failure that would explain the deviation.

## Reproduction steps

These steps reproduce the 2026-06-15 evidence against a running ErsatzRS server
that already has channel 5 named `calibration` and seeded with the three
generated calibration clips.

1. Verify the calibration sources exist, or regenerate them:

   ```bash
   scripts/calibration/gen-calibration-clips.sh
   ```

2. Confirm channel 5 is exposed by the IPTV playlist:

   ```powershell
   Invoke-WebRequest -Uri http://localhost:8409/iptv/channels.m3u -UseBasicParsing |
     Select-Object -ExpandProperty Content
   ```

   The relevant entry should point to:

   ```text
   http://localhost:8409/iptv/channel/5.m3u8?mode=segmenter
   ```

3. Capture five minutes with TSDuck:

   ```powershell
   New-Item -ItemType Directory -Force -Path diagnostics\calibration-channel5
   tsp -I hls "http://localhost:8409/iptv/channel/5.m3u8?mode=segmenter" `
     -P until --seconds 300 `
     -O file "diagnostics\calibration-channel5\channel5_calibration_5min.ts"
   ```

4. Run the structural TS checks:

   ```powershell
   tsanalyze --json "diagnostics\calibration-channel5\channel5_calibration_5min.ts" |
     Set-Content -Encoding utf8 "diagnostics\calibration-channel5\channel5_calibration_5min.tsanalyze.json"

   tsp -I file "diagnostics\calibration-channel5\channel5_calibration_5min.ts" `
     -P continuity -O drop 2>&1 |
     Set-Content -Encoding utf8 "diagnostics\calibration-channel5\channel5_calibration_5min.continuity.txt"

   tsp -I file "diagnostics\calibration-channel5\channel5_calibration_5min.ts" `
     -P pcrverify --jitter-max 8000 -O drop 2>&1 |
     Set-Content -Encoding utf8 "diagnostics\calibration-channel5\channel5_calibration_5min.pcrverify.txt"
   ```

5. Extract mono PCM for beep timing analysis:

   ```powershell
   ffmpeg -hide_banner -y `
     -i "diagnostics\calibration-channel5\channel5_calibration_5min.ts" `
     -map 0:a:0 -vn -ac 1 -ar 48000 -c:a pcm_s16le `
     "diagnostics\calibration-channel5\channel5_calibration_5min_mono48.wav"
   ```

   Use a 1 ms absolute-amplitude envelope and a threshold of 20% of the peak
   envelope to detect the 50 ms pips. In the captured evidence this found 313
   pips over the 312.597 s HLS capture tail, with no missing or duplicate pip
   indices.

6. Extract frame 0 and decoded frame 18,000 for the exact five-minute visual
   ABS comparison:

   ```powershell
   ffmpeg -hide_banner -y `
     -i "diagnostics\calibration-channel5\channel5_calibration_5min.ts" `
     -vf "select='eq(n,0)+eq(n,18000)'" -vsync 0 `
     "diagnostics\calibration-channel5\frame_index_%02d.png"
   ```

   Expected evidence:

   ```text
   frame_index_01.png: ABS 00:07:03.000
   frame_index_02.png: ABS 00:03:03.517
   expected frame 18,000: ABS 00:03:03.000
   deviation: +516.7 ms / +31 frames
   ```

7. Extract rollover probes around the observed audio discontinuities:

   ```powershell
   ffmpeg -hide_banner -y `
     -i "diagnostics\calibration-channel5\channel5_calibration_5min.ts" `
     -vf "select='eq(n,2580)+eq(n,2640)+eq(n,2700)+eq(n,6960)+eq(n,7020)+eq(n,7080)+eq(n,17760)+eq(n,17820)'" `
     -vsync 0 "diagnostics\calibration-channel5\frame_probe_%02d.png"
   ```

   Expected evidence:

   ```text
   frame_probe_01.png: ABS 00:07:46.000
   frame_probe_02.png: ABS 00:07:47.000
   frame_probe_03.png: ABS 00:07:48.000
   frame_probe_04.png: ABS 00:08:59.000
   frame_probe_05.png: ABS 00:00:00.517
   ```

8. Check the server log after capture:

   ```powershell
   Select-String `
     -Path C:\Users\Bart\AppData\Local\ErsatzRS\logs\ErsatzRS.log `
     -Pattern "hls_worker|ERROR|WARN|error|warn|failed|ffmpeg|channel_number=5|stopping idle" `
     -CaseSensitive:$false
   ```

   Expected evidence from the 2026-06-15 run: no WARN/ERROR/FFmpeg failure/no
   active playout lines during capture; only an informational idle-session stop
   after the capture completed.

## Fix direction

The fix must make HLS segmenter content-time continuity explicit:

- preserve sub-second seek/request positions through the HLS worker and FFmpeg
  command-generation path instead of truncating to whole seconds;
- reconcile appended generation boundaries against decoded media time, not only
  output PTS offset or scheduled `PlayoutItem` finish;
- add a calibration regression that verifies one-second audio pip cadence and
  visible ABS/frame-counter continuity across work-ahead respawns and real
  playout item seams;
- keep TSDuck structural checks, but do not treat them as sufficient proof of
  content continuity.
