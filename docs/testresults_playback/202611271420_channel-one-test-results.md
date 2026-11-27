---
report_format: 4
report_id: "202611271420_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: FAIL
criteria:
  video: PASS
  audio: FAIL
  teletext: PASS
  transport: PASS
  transition: FAIL
evidence_directory: tmp/channel-one-candidate-run07
---

# Channel 1 playback results · 2026-11-27 14:20 CET

**FAIL.** The same source was played twice on one HTTP connection. This build preserves video timestamps at MPEG-TS clock precision when the H.264 profile follows the source and embedded teletext is copied. The opening frame displayed a blank subtitle. VLC produced the selected tone on 59 of 59 switches. Latencies ranged from 1.813 to 1.832 s.

## Run conditions

| Field | Value |
| --- | --- |
| Scheduled window (UTC) | 2026-11-27 13:20:43.3175153 through 2026-11-27 13:30:43.3175153 |
| Endpoint and playout | `http://127.0.0.1:59241/iptv/channel/1.ts`, HTTP unicast, native Hybrid, Classic, built-in wrapper, fresh `state/gate.db`, LAN discovery disabled |
| Effective profile | Seeded 1080p SDR, NVENC H.264 High/p5, 8,000 kbit/s video, source cadence of 25 fps, AC-3 at 640 kbit/s, six channels and 48 kHz |
| Receiver and tools | libVLC 3.0.24, 1,000 ms network cache and software video decoding. FFmpeg/ffprobe n9.1-ErsatzRS-2.1.0, TSDuck 3.44-4676, Python 3.13.12, python-vlc 3.0.21203, NumPy 2.5.5 and Pillow 13.0.0 |
| Server SHA-256 | `51a219a567c0530a31e385942ad10f3a53e6df40965bcb5d06f57d4132f92f82` |
| Scanner SHA-256 | `7176bfcd1db2e32fada78c5797ccefba429859fec9c7ebedc73bf2890130c4d8` |
| Source SHA-256 | `bce61f654839c86a4c9add2341279f242e83bc22a09dcef2af60ad9652b771ee` |

## Comparison metrics

| Metric ID | Unit | Acceptance | Observed | Verdict |
| --- | --- | --- | --- | --- |
| connection.http_sessions | count | 1 | 1 | PASS |
| connection.rebuffer_events | count | 0 | 0 | PASS |
| video.presentations | frames | 15,000 | 15,000 in 600.000 s | PASS |
| video.missing_counters | frames | 0 | 0 | PASS |
| video.repeated_counters | frames | 0 | 0 | PASS |
| video.reversed_counters | frames | 0 | 0 | PASS |
| video.longest_content_hold | ms | One frame per counter | Each counter presented once | PASS |
| video.max_display_interval | ms | ≤100 | 43.0 | PASS |
| audio.selectable_tracks | count | 2 | [257, 258] | PASS |
| audio.switch_attempts | count | 59 | 59 | PASS |
| audio.max_attempt_time_error | ms, player clock | ≤40 | 3 | PASS |
| audio.verified_switches | count | 59 | 59 | PASS |
| audio.max_switch_latency | ms, command to audible tone | ≤250 | 1,832 at 110 s | FAIL |
| audio.longest_pcm_silence | ms | ≤20 | 1,832 at 50.000 s | FAIL |
| audio.longest_dual_tone | ms | ≤4 | 4 at 10.000 s | PASS |
| audio.missing_pcm_samples | samples | 0 | 5,167,113 | FAIL |
| teletext.page_888_selectable | yes/no | Yes | Yes | PASS |
| teletext.numeric_cue_offset_counter | frames, visible counter | 0 or +1 | 118 × 0 frames | PASS |
| teletext.numeric_cue_offset_player | frames, libVLC media clock | 0 or +1 | 118 × 0 frames | PASS |
| teletext.blank_onsets | boundaries | 2 on time | 2 on time | PASS |
| teletext.wrong_presented_frames | frames | 0 outside the allowed boundary tolerance, frame zero blank | 0 | PASS |
| transport.ac3_pids | count | 2 throughout | 0x101, 0x102 | PASS |
| transport.page_888_descriptor | yes/no | Present throughout | Page 888 in 6,000 PMTs | PASS |
| transport.subtitle_pts_offset | frames from video | Within 1 | -0.333 to -0.333 ms for 118 cues | PASS |
| transport.continuity_errors | count | 0 | 0 | PASS |
| transport.pts_dts_pcr_leaps | counts, PTS / DTS / PCR | 0 / 0 / 0 | 0 / 0 / 0 | PASS |
| transport.pcr_jitter_excursions | count above 8,000 µs | 0 | 0 of 30,250 checked PCRs | PASS |
| transport.ac3_frame_step | irregular intervals per PID | 0 (32 ms per AC-3 frame, ±2 ticks at 90 kHz) | 0 / 0 | PASS |
| transport.ac3_boundary_coverage | in-window gaps or overlaps per PID | 0 beyond 2 ticks at 90 kHz | 0 / 0 | PASS |
| transport.network_arrival_jitter | µs | Requires live arrival timestamps | No live arrival timestamps | NIET GEMETEN |
| transition.at_300s | boundary | 7499→0000, 295→blank, audible B→A | 7499→0000, 295→blank, audible B→A at 301.816 s | FAIL |

## Evidence

The SRT extractor reports milliseconds. Its −0.333 ms offset in the table
reflects that precision. At both loop starts the original received
video and subtitle PTS matched exactly on the transport clock.

The received subtitle PES offsets from video at 0 and 300 s were 0 and 0 ticks on the 90 kHz clock. Both offsets were +480 ticks in the preceding build. The first complete airing cleared the previous subtitle at visible frame offset 0, and the next at offset 0. [Boundary timestamps](202611271420_channel-one/boundary-packets.json), [opening frame zero](202611271420_channel-one/opening-frame-0-subtitle.png), [frame one](202611271420_channel-one/opening-frame-1-subtitle.png) and [preceding build](202611271346_channel-one-test-results.md).

Both audio PIDs had 18,750 and 18,750 packet starts. TSDuck checked PCR against packet distance at 10,368,000 bit/s. [Transport clocks](202611271420_channel-one/transport-verdict.json) and [sampled HLS segments](202611271420_channel-one/hls-segments.json).

VLC flushed pending PCM on 59 of 59 commands. At flush time the last queued buffer was scheduled to end 1,782.6–1,801.6 ms later. The longest silence in delivered PCM alone was 0 ms. The silence criterion includes missing playback intervals. [PCM and all switches](202611271420_channel-one/audio.json) and [decoded tones and flush timings](202611271420_channel-one/broadcast-audio.json).

- PID 0x101 (750 Hz) had 2 unexpected 4 ms tone blocks over both airings. Of the complete blocks inside a command-to-onset interval, 0 lacked the expected tone.
- PID 0x102 (1,500 Hz) had 2 unexpected 4 ms tone blocks over both airings. Of the complete blocks inside a command-to-onset interval, 0 lacked the expected tone.

On both received PIDs, the two silent 4 ms blocks started at 299.9947
and 599.9947 s relative to the video. These brief intervals are visible
in the decoded broadcast audio even though the TS packet cadence passed.
At each track selection the player discarded queued PCM and the missing
playback interval lasted over 1.8 s.

Between selections, all 123,078 complete 4 ms analysis windows outside
the actual command-to-onset intervals contained the expected single tone.
None was silent. Forty-three of these windows had partial PCM coverage.
The total missing-samples metric includes those samples and all switch gaps.

At the loop boundary VLC issued B→A at 300.002 s and the A tone became audible at 301.816 s. [Rendered transition with played PCM](202611271420_channel-one/transition.mkv) and [matching measurements](202611271420_channel-one/measurements.json).

The played PCM was below the −48 dBFS RMS silence threshold in every
4 ms window from 300.004 through the exclusive end at 301.816 s. At A's
onset the video showed frame 45 (`00:01.80`). The continuous tones identify
the selected track, but have no unique time marker from which to measure
the content's A/V offset after playback resumes.

The remaining acceptance condition is continuous audio on real track
selection. Across the 13:04, 13:46 and 14:20 captures, VLC requested a
flush within 1 ms on all 177 selections. Played locally through VLC's
ordinary Windows audio output, an excerpt of the received TS also had
gaps of 1.840 and 1.839 s. A receiver change can first be checked against
that excerpt before repeating the full HTTP test with the original limits.
[13:04 flushes](202611271304_channel-one/broadcast-audio.json),
[13:46 flushes](202611271346_channel-one/broadcast-audio.json) and
[Windows process-loopback measurements](202611271241_channel-one/native-audio-controls.json).

[Source validation](202611271420_channel-one/source-validation.json), [presented video](202611271420_channel-one/video.json), [rendered subtitles](202611271420_channel-one/subtitle-events.json), [effective profile](202611271420_channel-one/profile.json) and [tool versions](202611271420_channel-one/versions.json).
