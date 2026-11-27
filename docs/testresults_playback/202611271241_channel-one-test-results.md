---
report_format: 3
report_id: "202611271241_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: FAIL
criteria:
  video: FAIL
  audio: FAIL
  teletext: FAIL
  transport: FAIL
  transition: FAIL
evidence_directory: tmp/channel-one-candidate-run04
---

# Channel 1 playback results · 2026-11-27 12:41 CET

**FAIL.** A preroll packet overlaps 10.844 ms of measured audio on both
PIDs. Both AC-3 PIDs carried 18,750 starts in the window
and the TS contained all 15,000 video frames. VLC presented
14,969 frames, rendered the 220 s subtitle two frames late on its media
clock and took up to 1.824 s to switch audio. A Rust build overlapped the
215–241 s display disturbance. This run establishes the observed loss,
without establishing its cause.

## Run conditions

| Field | Value |
| --- | --- |
| Scheduled window (UTC) | 2026-11-27 11:41:56.449284 through 11:51:56.449284 |
| Endpoint and playout | `http://127.0.0.1:64289/iptv/channel/1.ts`, HTTP unicast, native Hybrid, Classic, built-in wrapper, fresh `state/gate.db` |
| Effective profile | Seeded 1080p SDR, NVENC H.264 High/p5, 8,000 kbit/s video, source cadence of 25 fps, AC-3 at 640 kbit/s, six channels and 48 kHz |
| Receiver and tools | libVLC 3.0.24, 1,000 ms network cache and software video decoding. FFmpeg/ffprobe n9.1-ErsatzRS-2.1.0, TSDuck 3.44-4676, Python 3.13.12, python-vlc 3.0.21203, NumPy 2.5.5 and Pillow 13.0.0 |
| Server SHA-256 | `0d7dcf3224c4d9105ca5c6324909a3a1d86a50750ded9300e2df1edaf1166ff2` |
| Scanner SHA-256 | `7176bfcd1db2e32fada78c5797ccefba429859fec9c7ebedc73bf2890130c4d8` |
| Source SHA-256 | `bce61f654839c86a4c9add2341279f242e83bc22a09dcef2af60ad9652b771ee` |

## Comparison metrics

| Metric ID | Unit | Acceptance | Observed | Verdict |
| --- | --- | --- | --- | --- |
| connection.http_sessions | count | 1 | 1 | PASS |
| connection.rebuffer_events | count | 0 | 0 | PASS |
| video.presentations | frames | 15,000 | 14,969 in 599.999 s | FAIL |
| video.missing_counters | frames | 0 | 33 | FAIL |
| video.repeated_counters | frames | 0 | 2 | FAIL |
| video.reversed_counters | frames | 0 | 0 | PASS |
| video.longest_content_hold | ms | One frame per counter | 126 at 240.220 s | FAIL |
| video.max_display_interval | ms | ≤100 | 364.0 | FAIL |
| audio.selectable_tracks | count | 2 | [257, 258] | PASS |
| audio.switch_attempts | count | 59 | 59 | PASS |
| audio.max_attempt_time_error | ms, player clock | ≤40 | 33 | PASS |
| audio.verified_switches | count | 59 | 59 | PASS |
| audio.max_switch_latency | ms, command to audible tone | ≤250 | 1,824 at 300 s | FAIL |
| audio.longest_pcm_silence | ms | ≤20 | 1,824 at 80.000 s | FAIL |
| audio.longest_dual_tone | ms | ≤4 | 4 at 140.000 s | PASS |
| audio.missing_pcm_samples | samples | 0 | 5,114,929 | FAIL |
| teletext.page_888_selectable | yes/no | Yes | Yes | PASS |
| teletext.numeric_cue_offset_counter | frames, visible counter | 0 or +1 | 118 × 0 frames | PASS |
| teletext.numeric_cue_offset_player | frames, libVLC media clock | 0 or +1 | 117 × 0 frames, 1 × 2 frames | FAIL |
| teletext.blank_onsets | boundaries | 2 on time | 2 on time | PASS |
| transport.ac3_pids | count | 2 throughout | 0x101, 0x102 | PASS |
| transport.page_888_descriptor | yes/no | Present throughout | Page 888 in 6,000 PMTs | PASS |
| transport.subtitle_pts_offset | frames from video | Within 1 | +4.844 to +4.844 ms for 118 cues | PASS |
| transport.continuity_errors | count | 0 | 0 | PASS |
| transport.pts_dts_pcr_leaps | counts, PTS / DTS / PCR | 0 / 0 / 0 | 0 / 0 / 0 | PASS |
| transport.pcr_jitter_excursions | count above 8,000 µs | 0 | 0 of 30,250 checked PCRs | PASS |
| transport.ac3_frame_step | irregular intervals per PID | 0 (32 ms per AC-3 frame, ±2 ticks at 90 kHz) | 0 / 0 | PASS |
| transport.ac3_boundary_coverage | in-window gaps or overlaps per PID | 0 beyond 2 ticks at 90 kHz | 1 / 1, overlap of 976 ticks at 0 s | FAIL |
| transport.network_arrival_jitter | µs | Requires live arrival timestamps | No live arrival timestamps | NIET GEMETEN |
| transition.at_300s | boundary | 7499→0000, 295→blank, audible B→A | 7499→0000, 295→blank, audible B→A at 301.824 s | FAIL |

## Evidence

The previous candidate had a one-tick audio interval at three handoffs
on each PID. This candidate's intervals ranged from 2,878 to 2,882 ticks,
within the original 2,880 ±2 limit. Subtracting ffprobe's six-decimal
`pts_time` values initially flagged six intervals per PID as 23 µs
off target. Reanalysis of the same captured bytes using integer 90 kHz
PTS found zero violations. Applying the same integer check to the
11:56 capture still finds its three one-tick intervals.
Format 2 awarded the transport criterion a PASS on the in-window packet
starts. Format 3 also examines the packet starting just before frame zero.
That packet overlaps the first 976 measured ticks on each PID.

In a separate 16 s encoder control, an earlier priming correction clipped
the last 4 ms tone marker. Trimming the programme before priming and
allowing the preroll to reach the encoder preserved 768,000 decoded
samples. The last 4 ms correlated 0.999998 with the input marker.

Two real VLC track switches on an unmodified excerpt of the 11:56 HTTP
recording also produced audible gaps at the Windows mixer. With the TS
demuxer and a 1,000 ms file cache, the mixer recorded 1.840 and 1.839 s
between the old and new tones. A separate short-mux source control with
the avformat demuxer and a 35 ms cache still had 281 and 277 ms gaps.
Only the test process tree was captured. These file controls are separate
from the HTTP gate. [Process-loopback measurements](202611271241_channel-one/native-audio-controls.json).

The longest silence inside delivered PCM was 12 ms at 0.000 s. The table also counts missing PCM after an output flush. Packet-distance PCR jitter used 10,368,000 bit/s. [Audio and all 59 switches](202611271241_channel-one/audio.json), [presented video](202611271241_channel-one/video.json), [rendered subtitles](202611271241_channel-one/subtitle-events.json), [HLS segment probes](202611271241_channel-one/hls-segments.json) and [transport clocks](202611271241_channel-one/transport-verdict.json).

VLC issued B→A at 300.000 s on the player clock. The A tone was present at 301.824 s on the presentation clock. [Rendered transition with PCM](202611271241_channel-one/transition.mkv) and [corresponding measurements](202611271241_channel-one/measurements.json).
