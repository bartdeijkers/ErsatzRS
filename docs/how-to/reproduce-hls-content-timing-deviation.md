# Reproduce an HLS content-timing deviation

Use this guide to reproduce the HLS segmenter's content-timing calibration
evidence.

## Prerequisites

- a running ErsatzRS server;
- FFmpeg and ffprobe available to the diagnostic shell;
- TSDuck commands `tsp` and `tsanalyze`;
- the repository calibration clips; and
- a channel whose playout cycles those clips and uses the HLS segmenter.

The examples use channel `5` and write under `diagnostics/calibration-channel5`.
Change both values for another calibration channel.

The capture and analysis commands below use PowerShell on Windows. Translate
line continuation and path separators when running the same tools from another
shell.

## Generate the calibration clips

From the repository root:

```bash
scripts/calibration/gen-calibration-clips.sh
```

The clips contain a 50 ms audio pip at each whole second and a visible absolute
timecode and frame counter.

## Confirm the channel URL

Request the channel playlist:

```powershell
Invoke-WebRequest -Uri http://localhost:8409/iptv/channels.m3u -UseBasicParsing |
  Select-Object -ExpandProperty Content
```

Confirm it contains the calibration channel and its segmenter URL, for example:

```text
http://localhost:8409/iptv/channel/5.m3u8?mode=segmenter
```

## Capture five minutes

```powershell
New-Item -ItemType Directory -Force -Path diagnostics\calibration-channel5
tsp -I hls "http://localhost:8409/iptv/channel/5.m3u8?mode=segmenter" `
  -P until --seconds 300 `
  -O file "diagnostics\calibration-channel5\channel5_calibration_5min.ts"
```

Do not restart the server or channel during the capture.

## Check transport structure

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

Record continuity, PCR, PTS/DTS, PSI, PID, synchronization, and scrambling
results. A clean transport report does not by itself prove content continuity.

## Extract audio for pip analysis

```powershell
ffmpeg -hide_banner -y `
  -i "diagnostics\calibration-channel5\channel5_calibration_5min.ts" `
  -map 0:a:0 -vn -ac 1 -ar 48000 -c:a pcm_s16le `
  "diagnostics\calibration-channel5\channel5_calibration_5min_mono48.wav"
```

Measure the pip positions with a 1 ms absolute-amplitude envelope and a
threshold near 20 percent of peak amplitude. Compare each detected pip with its
nearest whole-second index. Report missing or duplicate indices separately from
phase changes.

## Compare exact video frames

Extract the first frame and frame 18,000:

```powershell
ffmpeg -hide_banner -y `
  -i "diagnostics\calibration-channel5\channel5_calibration_5min.ts" `
  -vf "select='eq(n,0)+eq(n,18000)'" -fps_mode:v passthrough `
  "diagnostics\calibration-channel5\frame_index_%02d.png"
```

At 60 fps, frame 18,000 is exactly 300 seconds after frame 0. Read the visible
absolute time from both images and compare the observed delta with five
minutes. Record the observed signed delta; do not substitute a result from an
earlier capture.

## Probe an item rollover

Choose frame numbers immediately before, at, and after an observed audio phase
change. Replace the placeholders below with frames from the current capture:

```powershell
ffmpeg -hide_banner -y `
  -i "diagnostics\calibration-channel5\channel5_calibration_5min.ts" `
  -vf "select='eq(n,<frame-before>)+eq(n,<frame-at>)+eq(n,<frame-after>)'" `
  -fps_mode:v passthrough "diagnostics\calibration-channel5\frame_probe_%02d.png"
```

Compare the visible clip and absolute time immediately before and after the
seam. Record whether the outgoing item finishes late, finishes early, or the
incoming item begins at a non-zero phase.

## Correlate the server log

Use **Support** -> **Logging** or search the local `ErsatzRS.log` for the capture
interval. Include HLS worker, FFmpeg, warning, error, missing-playout, and idle
session messages. Redact media paths and access tokens before sharing output.

## Report the result

Keep these findings separate:

- transport-structure errors from TSDuck;
- audio-pip cadence and phase changes;
- visible frame-counter deviation;
- the exact playout seam where content changes; and
- matching server events.

This separation prevents a structurally clean transport stream from being
misreported as content-time continuity.
