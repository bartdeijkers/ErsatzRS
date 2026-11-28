# Test channel 1 playback in VLC

Run this gate to check whether a viewer can watch two five-minute airings,
switch audio every ten seconds and keep DVB teletext subtitles selected. It
uses the package's web server and scanner, a local library and a Classic
schedule. Each invocation creates a new SQLite database under the result
directory. You can leave the regular server running.

The launcher sets `ERSATZRS_DISABLE_HDHR_DISCOVERY=1` for both test-server
starts. Builds that support this switch leave UDP port 65001 free. Older
packages still try to claim the discovery port while serving HTTP on the
chosen loopback port.

## Requirements

The following combination ran on Windows x64. Install VLC with its DVB
teletext decoder and TSDuck with its command-line tools (`tsp`, `tsanalyze`,
`tstables`). Use the same architecture for Python and VLC. Use FFmpeg with
libx264, AC-3 and the libzvbi teletext decoder. Allow around 2 GiB on a local
disk per run and up to one additional five-minute airing for startup.

| Component | Version used |
| --- | --- |
| Python | 3.13.12 |
| NumPy, Pillow, python-vlc | 2.5.5, 13.0.0, 3.0.21203 |
| libVLC | 3.0.24 |
| FFmpeg and ffprobe | n9.1-ErsatzRS-2.1.0 |
| TSDuck | 3.44-4676 |

From a clean checkout, point `$package` at an extracted Windows ErsatzRS
package containing matching `ersatzrs-web.exe` and `ersatzrs-scanner.exe`.
The runner looks for FFmpeg and ffprobe beside those executables and then on
`PATH`. Use `--ffmpeg` and `--ffprobe` for another managed runtime location.
To exercise changes to the server itself, build the candidate following
[the build guide](../../BUILD.md) and use its web and scanner executables.

```powershell
$package = (Resolve-Path ./dist/win-x64).Path  # set to your extracted package
python -m venv .venv
$python = (Resolve-Path .venv/Scripts/python.exe).Path
& $python -m pip install numpy==2.5.5 pillow==13.0.0 python-vlc==3.0.21203
& $python scripts/e2e/channel1-vlc.py --server "$package/ersatzrs-web.exe" --out tmp/channel1-run
```

The integrated command generates and validates the clip, starts the test
server and writes a verdict. Supply `--port 18991` to choose a free loopback
port. The default asks the OS for an available port. Choose a **new** output
directory each time. To generate once and reuse exactly the same source:

```powershell
& $python scripts/calibration/channel1_fixture.py tmp/channel1-source --ffmpeg "$package/ffmpeg.exe"
& $python scripts/e2e/channel1-vlc.py --server "$package/ersatzrs-web.exe" --fixture "tmp/channel1-source/Channel One (2026).ts" --out tmp/channel1-repeat
```

## Source and playout

The generator complements the [existing calibration clips](../../scripts/calibration/gen-calibration-clips.sh).
It uses visible counters suited to frame-by-frame reading and reuses the
[existing DVB fixture packetizer](../../scripts/calibration/generate-teletext-fixture.py)
for page headers, row encoding, PES and PMT descriptors. The transport checks
extend the [existing TSDuck playback gate](../../scripts/e2e/tsduck-playback-gate.sh)
with simultaneous evidence from one viewer.

- One 1920×1080 progressive BT.709 clip runs for 7,500 frames. The seeded
  1080p SDR profile follows the source cadence, so the fixture supplies
  25 fps. Both top-left counters derive from the frame index. Frame zero
  reads `0000 / 00:00.00` and frame 7499 reads `7499 / 04:59.96`.
- Two stereo AC-3 tracks play continuous 750 Hz (English, A) and 1,500 Hz
  (Dutch, B), each at 48 kHz and −18 dBFS peak per channel. Before playback
  the harness checks the PMT and both channels of each audio track in
  successive 20 ms windows with a ±1 dB tolerance. It uses the video PTS
  to select exactly 14,400,000 samples per channel, including the first
  and last programme samples. Each 40 ms video frame carries a deterministic
  bit on both tones. Its two 20 ms halves alternate between −18.0 and
  −18.8 dBFS in the order specified by that bit, using a 4 ms ramp. This
  keeps the carrier audible and the peak and per-frame energy equal across
  tracks. Source validation correlates every decoded second on all four
  channels with the code at video time zero (minimum 0.97).
- Page 888 is signaled as a DVB subtitle page. The first five seconds are
  blank. The value then advances from 5 to 295 in five-second steps. The
  complete current page, including its erase flag, is sent twice a second
  for a viewer tuning in mid-cue. Source cue PTS are compared with video
  start PTS before the server starts.
- The runner starts a clean host, uses its seeded channel 1 and profile
  `1920x1080 x264 ac3 6ch`, and configures native MPEG-TS Hybrid with the
  built-in wrapper and English subtitles in Any mode. The package selects
  effective hardware settings. The ordinary scanner indexes the file and
  the ordinary scheduler builds consecutive airings from a one-movie
  collection. The output contains the database, library, configuration,
  temporary files, working directory and logs for this run.
- `run.json` records the actual HTTP unicast URL ending in
  `/iptv/channel/1.ts`. A loopback recorder tees that single response to
  libVLC and `received.ts`. VLC opens once with 1,000 ms network caching,
  HTTP reconnect disabled, software video decoding and user preferences
  ignored. It requests track A and page 888 in preroll. A missing track or
  page remains observable during measurement.

## Measurements and limits

Time zero is the **presentation** of counter 0000 following the end of an
airing. The interval ends just before the third airing's first presentation.
The receiver anchors libVLC's native media time to that first presentation.
It interpolates between `MediaPlayerTimeChanged` events at normal playback
speed, requires an anchor less than 750 ms old and invalidates it on buffering.
That clock drives the 59 selections at 10 through 590 seconds. A frozen
visible counter cannot delay a command. Each attempt records the native and
interpolated times, the visible counter and the monotonic time. PCM
presentation timestamps and video display callbacks share libVLC's monotonic
clock. Subtitle onsets are compared with both the native clock and the
displayed programme counters.

| Evidence | Acceptance |
| --- | --- |
| Video | Exactly 15,000 ordered presentations with the matching visible cliptimes, one HTTP request, no buffering event within the interval and no presentation gap above 100 ms. Callbacks measure the video output before monitor refresh. |
| Audio | Measure libVLC's selected audio output as signed 16-bit mono PCM at 48 kHz. Use adjacent 192-sample (4 ms) windows, quadrature detection at both tones above −36 dBFS peak and silence below −48 dBFS RMS. Require both tracks and 59 successful selections with the expected tone in two successive windows. Allow one frame for issuing a command. A verified selection may contain up to 2,000 ms of silence, including discarded or missing PCM. The non-silent portion of command-to-audible latency must stay within 250 ms. Silence outside an accepted switch is limited to 20 ms, missing PCM outside it to zero, and dual-tone overlap to 4 ms throughout. Retain the raw command-to-tone latency, full-window silence and missing-sample counts. Times in the longest-interval reports are seconds from the display epoch. The measurement ends at VLC's audio output, before physical sound hardware. |
| A/V offset | Decode the content time in the selected PCM and compare with the presented video counter. Positive milliseconds mean audio is late. Measurement coverage requires valid windows at the start, after all 59 switches and throughout audible playback. An optional absolute limit can be set with `--max-av-offset-ms`. |
| Subtitles | Check the full sequence against composited video and the selected teletext page. Match each distinct subtitle bitmap against independently decoded source glyphs. Reject uncertain matches. Permit the previous text on the boundary frame only. The following frame must contain the new text, including blank after 295. Read both visible top-left counters independently throughout. |
| Transport | TSDuck examines program and PMT structure, both AC-3 PIDs, 888 subtitle type, continuity and PCR. The TS window is cut at the video PES positions of the first and third counter resets. Check audio packet timestamp continuity, video loop PTS and extracted cue PTS against video, with a one-frame cue tolerance. The PCR jitter reference is packet distance at the configured constant mux rate, with an 8,000 µs threshold. This file-based check supplies **no network arrival jitter measurement**. |
| Loop boundary | At 300 s require the 7499→0000 transition, counter time reset, 295 cleared within a frame, an audible B→A switch, and the rendered clip with played audio. |

## Inspect the results

For the 48 kHz AC-3 fixture, consecutive PES audio frame timestamps should
advance by 1,536 samples (32 ms). The transport analysis lists steps differing
by more than two 90 kHz ticks for each audio PID. This also catches a remuxer
replacing a duplicate timestamp with a one-tick advance at a programme boundary.

The visual matcher compares the complete word bitmap and its aspect ratio.
It accepts a score below 0.24 only when the next-best word is more than 0.006
away. The transport gate inspects each PMT repetition, allows up to 500 ms
between declarations and up to 100 ms between consecutive AC-3 PTS. Video
PTS must advance between half a frame and one and a half frames per picture.

Open `result.json` first. Exit code zero means that **every** criterion passed.
A failed measurement returns a nonzero exit code. `NIET GEMETEN` identifies
unavailable evidence. A source validation error stops playback. The negative
case where only one audio track arrives and captions are burned into the
picture still yields measurements for video, audio, transport and visible
subtitle timing.

| Files in the result directory | What to inspect |
| --- | --- |
| `versions.json`, `profile.json`, `airings.json`, `run.json` | Package and source hashes, tool versions, effective profile and channel, UTC schedule and endpoint |
| `source-validation.json`, `source-pmt.xml`, `source-888.srt` | Source tones, PMT subtitle descriptor and extracted cue PTS |
| `received.ts`, `window.ts`, `window-pmt.xml`, `transport-verdict.json`, `continuity.log`, `pcrverify.log` | Bytes delivered, bounded TS, PMT/PIDs and clock findings |
| `hls-segments.json`, `hls-start/`, `hls-pair/` | Two consecutive published segments during preroll and another retained pair after playback, with their probes and PMTs. Compare them with the source and final HTTP stream to locate track loss. |
| `receiver.json`, `video.json` | libVLC version, session, every presented counter, missing/repeated/reversed frames and display intervals |
| `vlc.s16le`, `pcm.csv`, `audio.json` | Output samples, presentation times, requested/actual switches, first audible onset, longest silence, missing PCM, dual tone and latency |
| `broadcast-audio.json` | Independent decode of each received AC-3 PID. Checks the tones during each command-to-audible interval and measures each transmitted track's coded A/V offset every 10 s on the TS clock. Both PIDs must retain the correct tone throughout, with at most 20 ms of consecutive unexpected blocks, 4 ms of dual tone and zero wrong-only tone blocks. The two-second allowance applies at the VLC output. |
| `audio.json` → `between_switches` | For 59 audible switches, counts whole 4 ms presentation windows outside the actual command-to-onset intervals. Reports unexpected tones, silence, overlap and incomplete PCM coverage. Whole missing-sample runs are exempt only when they begin at or after the actual command and end by the end of the first confirmed new-tone block. |
| `av-offset.json` | Signed offset and measurement time for each decoded window, per-track ranges, median and largest absolute offset. Each switch links the preceding window to the first window starting at audible onset. The 300 s record also appears in `transition/measurements.json`. |
| `subtitle-masks/`, `subtitle-labels.json`, `subtitle-events.json` | Actual visible subtitle bitmaps, classified values, changes and mismatches |
| `transition/transition.mkv`, `transition/measurements.json` | Lossless rendered image and played audio around 300 s, with corresponding presentation and selection records |
| `state/gate.db`, `scanner.log`, `server.log` | Disposable SQLite installation and service diagnostics |

Open `transition/transition.mkv` in VLC. Step through the last frames of the
first airing and the first frames of the second. Read both counters, inspect
the bottom third for the 295→blank change and listen for B→A. Cross-check
against the timestamped measurements. To rerun the objective analysis on
the retained recording:

```powershell
& $python scripts/e2e/channel1_metrics.py tmp/channel1-run --ffmpeg "$package/ffmpeg.exe" --ffprobe "$package/ffprobe.exe"
```

### Read the A/V offset

At an audible onset, read `latency_s` for the selection delay and
`first_offset_ms` for synchronization after sound returns. The decoder
matches the next 1.280 s of PCM to the full 300 s timecode and timestamps
the estimate at the start of that window. It repeats this through each
audible stint, including a final window ending before the next command.
It interpolates between adjacent actually presented counters on VLC's
monotonic clock. Monitor and speaker latency are outside this measurement.

The PCM envelope is sampled every 4 ms. At least 98% of the blocks must
contain fully delivered PCM with the selected tone at usable code levels.
Blocks more than 1 dB outside the observed 10th-to-90th percentile band
remain in the silence and tone checks but carry no timecode bit. The 90th/10th
percentile level ratio must be at least 1.04. Normalized correlation must
reach 0.90, with a margin of 0.10 over any match more than one video frame
away. Silence, weak or ambiguous codes and unavailable video timing yield
no offset. The reported offset is a window estimate on a 4 ms audio grid.
The decimal part also reflects the video callback time. The clip repeats,
so delays differing by exactly 300 s are indistinguishable. Values use the
interval [−150, +150) s.

With no A/V limit, `av_offset_measurement: PASS` means the measurement
was complete. Supply a limit to also fail the gate on an observed excess.
For example, add `--max-av-offset-ms 40` to either the live runner or the
offline analyzer for a one-frame limit at 25 fps. Earlier unmodulated
fixtures retain their other checks and give `NIET GEMETEN` for A/V offset.
Generate fresh media for this measurement. To run the detector controls
with injected offsets, silence and a delayed video clock:

```powershell
& $python scripts/e2e/channel1_av_offset_test.py -v
```

## Recorded runs

Compare rows by metric ID, unit and clock reference. Each report records the
server and fixture identities. An observed failure can end a run early. In
that case, checks requiring the whole 600 s window stay `NIET GEMETEN`.
Report format 2 retains all 28 original metric IDs and adds
`transport.ac3_frame_step`. Earlier reports omit that per-PID cadence check.
Format 3 adds `transport.ac3_boundary_coverage`. It checks portions of
packets or gaps that cross either end of the 600 s window, including the
packet starting just before frame zero. Revised reports identify the
earlier verdict and the evidence that changed it.
Format 4 also lists the count of incorrectly rendered subtitle frames.
It makes the existing requirement that frame zero be blank visible
alongside the permitted one-frame cue-onset tolerance.
Format 5 records the measured A/V content offset and the revised VLC
switch-silence allowance. Earlier captures retain their original verdicts
and limits. The same metric with a different limit has its own comparison row.

To print two archived reports side by side in timestamp order, run the following command.

```powershell
$reports = Get-ChildItem docs/testresults_playback/*_channel-one-test-results.md | Sort-Object Name | Select-Object -Last 2
python scripts/e2e/compare-channel1-results.py @($reports.FullName)
```

A dash indicates an absent metric or a different unit or limit. Each
`NIET GEMETEN` entry retains the reason recorded by that run.

| Report | Scope | Overall result |
| --- | --- | --- |
| [2026-11-27 11:00 · packaged baseline](../testresults_playback/202611271100_channel-one-test-results.md) | Two full airings | FAIL |
| [2026-11-27 11:36 · alternate audio and embedded page 888](../testresults_playback/202611271136_channel-one-test-results.md) | Partial first airing | FAIL |
| [2026-11-27 11:56 · corrected HLS duration and teletext timestamps](../testresults_playback/202611271156_channel-one-test-results.md) | Two full airings | FAIL |
| [2026-11-27 12:32 · first AC-3 priming correction](../testresults_playback/202611271232_channel-one-test-results.md) | Two full airings, final tone marker lost in separate control | FAIL |
| [2026-11-27 12:41 · corrected AC-3 handoffs](../testresults_playback/202611271241_channel-one-test-results.md) | Two full airings, concurrent build | FAIL |
| [2026-11-27 13:04 · isolated AC-3 handoff test](../testresults_playback/202611271304_channel-one-test-results.md) | Two full airings, preroll overlap at frame zero | FAIL |
| [2026-11-27 13:46 · AC-3 aligned at initial tune-in](../testresults_playback/202611271346_channel-one-test-results.md) | Two full airings, transport passed, stale subtitle on frame zero and audible switch gaps | FAIL |
| [2026-11-27 14:20 · source video and teletext clocks aligned](../testresults_playback/202611271420_channel-one-test-results.md) | Two full airings, video, transport and all 120 subtitle boundaries passed. VLC flushed audio on 59 switches | FAIL |
| [2026-11-27 15:54 · coded audio and revised VLC silence limit](../testresults_playback/202611271554_channel-one-test-results.md) | Two full airings, 59 switches, 421 decoded A/V measurements, maximum switch silence 1.818 s | PASS |
