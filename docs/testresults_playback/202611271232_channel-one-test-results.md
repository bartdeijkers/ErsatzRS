---
report_format: 2
report_id: "202611271232_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: FAIL
criteria:
  video: PASS
  audio: FAIL
  teletext: PASS
  transport: PASS
  transition: FAIL
evidence_directory: tmp/channel-one-candidate-run03
---

# Channel 1 playback results · 2026-11-27 12:32 CET

**FAIL.** This intermediate priming correction restored AC-3 packet cadence.
Video and page 888 passed, and 58 audible switches were verified. An
independent end-marker control found that the encoder duration limit
removed the last 4 ms tone marker from a 16 s programme. This candidate
was rejected. The [12:41 run](202611271241_channel-one-test-results.md)
records the correction that retained the marker.

## Run conditions

| Field | Value |
| --- | --- |
| Scheduled window (UTC) | 2026-11-27 11:32:49.525233 through 11:42:49.525233 |
| Endpoint and playout | `http://127.0.0.1:63559/iptv/channel/1.ts`, HTTP unicast, native Hybrid, Classic, built-in wrapper, fresh `state/gate.db` |
| Effective profile | Seeded 1080p SDR, NVENC H.264 High/p5, 8,000 kbit/s video, source cadence of 25 fps, AC-3 at 640 kbit/s, six channels and 48 kHz |
| Receiver and tools | libVLC 3.0.24, 1,000 ms network cache and software video decoding. FFmpeg/ffprobe n9.1-ErsatzRS-2.1.0, TSDuck 3.44-4676, Python 3.13.12, python-vlc 3.0.21203, NumPy 2.5.5 and Pillow 13.0.0 |
| Server SHA-256 | `f7776f57f37243cdd36e4e9ef89e97e924367c70e720c4b60a7e4c91219f24cd` |
| Scanner SHA-256 | `a173b2a9db07c8b83d76d2c4a4b019ff3cdb4f900fe07b04d8b4649fc6f17960` |
| Source SHA-256 | `bce61f654839c86a4c9add2341279f242e83bc22a09dcef2af60ad9652b771ee` |

## Comparison metrics

| Metric ID | Unit | Acceptance | Observed | Verdict |
| --- | --- | --- | --- | --- |
| connection.http_sessions | count | 1 | 1 | PASS |
| connection.rebuffer_events | count | 0 | 0 | PASS |
| video.presentations | frames | 15,000 | 15,000 in 600.009 s | PASS |
| video.missing_counters | frames | 0 | 0 | PASS |
| video.repeated_counters | frames | 0 | 0 | PASS |
| video.reversed_counters | frames | 0 | 0 | PASS |
| video.longest_content_hold | ms | One frame per counter | Each counter presented once | PASS |
| video.max_display_interval | ms | ≤100 | 50.0 | PASS |
| audio.selectable_tracks | count | 2 | [257, 258] | PASS |
| audio.switch_attempts | count | 59 | 59 | PASS |
| audio.max_attempt_time_error | ms, player clock | ≤40 | 3 | PASS |
| audio.verified_switches | count | 59 | 58 | FAIL |
| audio.max_switch_latency | ms, command to audible tone | ≤250 | 1,824 at 40 s | FAIL |
| audio.longest_pcm_silence | ms | ≤20 | 1,820 at 40.000 s | FAIL |
| audio.longest_dual_tone | ms | ≤4 | 4 at 10.000 s | PASS |
| audio.missing_pcm_samples | samples | 0 | 5,084,376 | FAIL |
| teletext.page_888_selectable | yes/no | Yes | Yes | PASS |
| teletext.numeric_cue_offset_counter | frames, visible counter | 0 or +1 | 118 × 0 frames | PASS |
| teletext.numeric_cue_offset_player | frames, libVLC media clock | 0 or +1 | 118 × 0 frames | PASS |
| teletext.blank_onsets | boundaries | 2 on time | 2 on time | PASS |
| transport.ac3_pids | count | 2 throughout | 0x101, 0x102 | PASS |
| transport.page_888_descriptor | yes/no | Present throughout | Page 888 in 6,001 PMTs | PASS |
| transport.subtitle_pts_offset | frames from video | Within 1 | +5.067 to +5.067 ms for 118 cues | PASS |
| transport.continuity_errors | count | 0 | 0 | PASS |
| transport.pts_dts_pcr_leaps | counts, PTS / DTS / PCR | 0 / 0 / 0 | 0 / 0 / 0 | PASS |
| transport.pcr_jitter_excursions | count above 8,000 µs | 0 | 0 of 30,250 checked PCRs | PASS |
| transport.ac3_frame_step | irregular intervals per PID | 0 (32 ms per AC-3 frame, ±2 ticks at 90 kHz) | 0 / 0 | PASS |
| transport.network_arrival_jitter | µs | Requires live arrival timestamps | No live arrival timestamps | NIET GEMETEN |
| transition.at_300s | boundary | 7499→0000, 295→blank, audible B→A | 7499→0000, 295→blank, B→A onset could not be verified | FAIL |

## Evidence

The longest silence inside delivered PCM was 28 ms at 299.972 s. The table also counts missing PCM after an output flush. Packet-distance PCR jitter used 10,368,000 bit/s. [Audio and all 59 switches](202611271232_channel-one/audio.json), [presented video](202611271232_channel-one/video.json), [rendered subtitles](202611271232_channel-one/subtitle-events.json), [HLS segment probes](202611271232_channel-one/hls-segments.json) and [transport clocks](202611271232_channel-one/transport-verdict.json).

VLC issued B→A at 300.000 s on the player clock. The receiver measurement reports B→A onset could not be verified. [Rendered transition with PCM](202611271232_channel-one/transition.mkv) and [corresponding measurements](202611271232_channel-one/measurements.json).
