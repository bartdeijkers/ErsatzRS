---
report_format: 4
report_id: "202611271346_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: FAIL
criteria:
  video: PASS
  audio: FAIL
  teletext: FAIL
  transport: PASS
  transition: FAIL
evidence_directory: tmp/channel-one-candidate-run06
---

# Channel 1 playback results · 2026-11-27 13:46 CET

**FAIL.** Both airings used one libVLC session. The server aligned the initial mid-programme seek to the outgoing AC-3 packet cadence. The encoded transport scored PASS. VLC delivered 59 requested track changes with latencies from 1.805 to 1.824 s. The limit is 0.250 s.

## Run conditions

| Field | Value |
| --- | --- |
| Scheduled window (UTC) | 2026-11-27 12:46:15.5031465 through 2026-11-27 12:56:15.5031465 |
| Endpoint and playout | `http://127.0.0.1:62881/iptv/channel/1.ts`, HTTP unicast, native Hybrid, Classic, built-in wrapper, fresh `state/gate.db`, LAN discovery disabled |
| Effective profile | Seeded 1080p SDR, NVENC H.264 High/p5, 8,000 kbit/s video, source cadence of 25 fps, AC-3 at 640 kbit/s, six channels and 48 kHz |
| Receiver and tools | libVLC 3.0.24, 1,000 ms network cache and software video decoding. FFmpeg/ffprobe n9.1-ErsatzRS-2.1.0, TSDuck 3.44-4676, Python 3.13.12, python-vlc 3.0.21203, NumPy 2.5.5 and Pillow 13.0.0 |
| Server SHA-256 | `bd330c4379b6285ca104de2b4e792f5e78ff5f785edc92de9188b3a5f037c155` |
| Scanner SHA-256 | `7176bfcd1db2e32fada78c5797ccefba429859fec9c7ebedc73bf2890130c4d8` |
| Source SHA-256 | `bce61f654839c86a4c9add2341279f242e83bc22a09dcef2af60ad9652b771ee` |

## Comparison metrics

| Metric ID | Unit | Acceptance | Observed | Verdict |
| --- | --- | --- | --- | --- |
| connection.http_sessions | count | 1 | 1 | PASS |
| connection.rebuffer_events | count | 0 | 0 | PASS |
| video.presentations | frames | 15,000 | 15,000 in 600.001 s | PASS |
| video.missing_counters | frames | 0 | 0 | PASS |
| video.repeated_counters | frames | 0 | 0 | PASS |
| video.reversed_counters | frames | 0 | 0 | PASS |
| video.longest_content_hold | ms | One frame per counter | Each counter presented once | PASS |
| video.max_display_interval | ms | ≤100 | 43.0 | PASS |
| audio.selectable_tracks | count | 2 | [257, 258] | PASS |
| audio.switch_attempts | count | 59 | 59 | PASS |
| audio.max_attempt_time_error | ms, player clock | ≤40 | 3 | PASS |
| audio.verified_switches | count | 59 | 59 | PASS |
| audio.max_switch_latency | ms, command to audible tone | ≤250 | 1,824 at 160 s | FAIL |
| audio.longest_pcm_silence | ms | ≤20 | 1,820 at 20.000 s | FAIL |
| audio.longest_dual_tone | ms | ≤4 | 4 at 40.000 s | PASS |
| audio.missing_pcm_samples | samples | 0 | 5,135,049 | FAIL |
| teletext.page_888_selectable | yes/no | Yes | Yes | PASS |
| teletext.numeric_cue_offset_counter | frames, visible counter | 0 or +1 | 118 × 0 frames | PASS |
| teletext.numeric_cue_offset_player | frames, libVLC media clock | 0 or +1 | 118 × 0 frames | PASS |
| teletext.blank_onsets | boundaries | 2 on time | 2 on time | PASS |
| teletext.wrong_presented_frames | frames | 0 outside the allowed boundary tolerance, frame zero blank | 1, frame zero displayed 295 | FAIL |
| transport.ac3_pids | count | 2 throughout | 0x101, 0x102 | PASS |
| transport.page_888_descriptor | yes/no | Present throughout | Page 888 in 6,000 PMTs | PASS |
| transport.subtitle_pts_offset | frames from video | Within 1 | +5.000 to +5.000 ms for 118 cues | PASS |
| transport.continuity_errors | count | 0 | 0 | PASS |
| transport.pts_dts_pcr_leaps | counts, PTS / DTS / PCR | 0 / 0 / 0 | 0 / 0 / 0 | PASS |
| transport.pcr_jitter_excursions | count above 8,000 µs | 0 | 0 of 30,250 checked PCRs | PASS |
| transport.ac3_frame_step | irregular intervals per PID | 0 (32 ms per AC-3 frame, ±2 ticks at 90 kHz) | 0 / 0 | PASS |
| transport.ac3_boundary_coverage | in-window gaps or overlaps per PID | 0 beyond 2 ticks at 90 kHz | 0 / 0 | PASS |
| transport.network_arrival_jitter | µs | Requires live arrival timestamps | No live arrival timestamps | NIET GEMETEN |
| transition.at_300s | boundary | 7499→0000, 295→blank, audible B→A | 7499→0000, 295→blank, audible B→A at 301.824 s | FAIL |

## Evidence

All 118 numeric cues first appeared on their scheduled frames. At the
start of the first measured airing VLC displayed the previous airing's
`295` for frame zero and cleared it on frame one (39 ms on the player
clock). This meets the one-frame change allowance and fails the opening
blank-frame check. At 300 s the first frame of the second airing was
already blank. At both boundaries the received subtitle PES had a PTS
480 ticks (5.333 ms) after the first video frame. The corresponding source
PTS coincide. The SRT-based metric above has millisecond precision.
[Source and received PES timestamps at the boundaries](202611271346_channel-one/boundary-packets.json).
[Rendered subtitle on opening frame zero](202611271346_channel-one/opening-frame-0-subtitle.png)
and [frame one](202611271346_channel-one/opening-frame-1-subtitle.png).

In a separate two-second encoder control, copying the fixture subtitles
while encoding video with NVENC reproduced the 480-tick offset. Preserving
the video timestamps with a 90 kHz encoder time base and passthrough frame
timing gave identical first video and subtitle PTS (126,480). This isolates
a timing difference before VLC. The standard profile and both full
airings in this report used their normal frame timing.

The two AC-3 PIDs had 18,750 and 18,750 packet starts in the 600 s window. Their respective step ranges in 90 kHz ticks were 2878–2882, 2878–2882. The preceding run had 2,327 overlapping ticks at the start of the window. TSDuck used packet distance at 10,368,000 bit/s for its PCR check. [Transport clocks](202611271346_channel-one/transport-verdict.json), [HLS segment inventories](202611271346_channel-one/hls-segments.json) and [preceding run](202611271304_channel-one-test-results.md).

VLC flushed pending PCM in the selection command's clock millisecond on 57/59 switches. The previous queued buffer was due to end 1,771.6–1,790.6 ms after the flush. The longest silence in delivered PCM alone was 0 ms. The silence criterion includes intervals with missing PCM. [Receiver PCM and switch measurements](202611271346_channel-one/audio.json) and [per-PID tones and flush times](202611271346_channel-one/broadcast-audio.json).

- Received PID 0x101 (750 Hz) had 2 unexpected 4 ms tone blocks over both programmes and 0 within the measured command-to-onset intervals.
- Received PID 0x102 (1,500 Hz) had 2 unexpected 4 ms tone blocks over both programmes and 0 within the measured command-to-onset intervals.

On each PID the silent blocks started at 0.000 and 300.000 s. VLC issued
its 300 s switch 1 ms into the second block. Only complete 4 ms blocks
between command and audible onset contribute to the per-switch counts.

At the loop boundary VLC issued B→A at 300.001 s. The A tone became audible at 301.824 s. [Rendered transition with played PCM](202611271346_channel-one/transition.mkv) and [matching frame and switch records](202611271346_channel-one/measurements.json).

[Source sample levels](202611271346_channel-one/source-validation.json), [presented frames](202611271346_channel-one/video.json), [rendered page 888](202611271346_channel-one/subtitle-events.json), [effective profile](202611271346_channel-one/profile.json) and [tool and executable identities](202611271346_channel-one/versions.json).
