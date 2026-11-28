---
report_format: 3
report_id: "202611271304_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: FAIL
criteria:
  video: PASS
  audio: FAIL
  teletext: PASS
  transport: FAIL
  transition: FAIL
evidence_directory: tmp/channel-one-candidate-run05
---

# Channel 1 playback results · 2026-11-27 13:04 CET

**FAIL.** Video and rendered page 888 passed in the same 600-second
session. Reanalysis found an AC-3 packet from preroll overlapping the first
25.856 ms of measured programme audio. All 59 track selections produced the requested tone.
Each took 1.801–1.820 s, exceeding the 250 ms limit. The longest silence
at the receiver was 1.820 s. This repeat used the final server build,
with LAN discovery disabled, a new SQLite database and no concurrent
build or other playout test.

## Run conditions

| Field | Value |
| --- | --- |
| Scheduled window (UTC) | 2026-11-27 12:04:10.439842 through 12:14:10.439842 |
| Endpoint and playout | `http://127.0.0.1:60128/iptv/channel/1.ts`, HTTP unicast, native Hybrid, Classic, built-in wrapper, fresh `state/gate.db` |
| Effective profile | Seeded 1080p SDR, NVENC H.264 High/p5, 8,000 kbit/s video, source cadence of 25 fps, AC-3 at 640 kbit/s, six channels and 48 kHz |
| Receiver and tools | libVLC 3.0.24, 1,000 ms network cache and software video decoding. FFmpeg/ffprobe n9.1-ErsatzRS-2.1.0, TSDuck 3.44-4676, Python 3.13.12, python-vlc 3.0.21203, NumPy 2.5.5 and Pillow 13.0.0 |
| Server SHA-256 | `9b5812dded9bb345def7be4e027caa395471e8f101534ea5db429d0364a74654` |
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
| audio.max_switch_latency | ms, command to audible tone | ≤250 | 1,820 at 560 s | FAIL |
| audio.longest_pcm_silence | ms | ≤20 | 1,820 at 20.000 s | FAIL |
| audio.longest_dual_tone | ms | ≤4 | 4 at 0.028 s | PASS |
| audio.missing_pcm_samples | samples | 0 | 5,132,726 | FAIL |
| teletext.page_888_selectable | yes/no | Yes | Yes | PASS |
| teletext.numeric_cue_offset_counter | frames, visible counter | 0 or +1 | 118 × 0 frames | PASS |
| teletext.numeric_cue_offset_player | frames, libVLC media clock | 0 or +1 | 118 × 0 frames | PASS |
| teletext.blank_onsets | boundaries | 2 on time | 2 on time | PASS |
| transport.ac3_pids | count | 2 throughout | 0x101, 0x102 | PASS |
| transport.page_888_descriptor | yes/no | Present throughout | Page 888 in 6,000 PMTs | PASS |
| transport.subtitle_pts_offset | frames from video | Within 1 | +4.856 to +4.856 ms for 118 cues | PASS |
| transport.continuity_errors | count | 0 | 0 | PASS |
| transport.pts_dts_pcr_leaps | counts, PTS / DTS / PCR | 0 / 0 / 0 | 0 / 0 / 0 | PASS |
| transport.pcr_jitter_excursions | count above 8,000 µs | 0 | 0 of 30,250 checked PCRs | PASS |
| transport.ac3_frame_step | irregular intervals per PID | 0 (32 ms per AC-3 frame, ±2 ticks at 90 kHz) | 0 / 0 | PASS |
| transport.ac3_boundary_coverage | in-window gaps or overlaps per PID | 0 beyond 2 ticks at 90 kHz | 1 / 1, overlap of 2,327 ticks at 0 s | FAIL |
| transport.network_arrival_jitter | µs | Requires live arrival timestamps | No live arrival timestamps | NIET GEMETEN |
| transition.at_300s | boundary | 7499→0000, 295→blank, audible B→A | 7499→0000, 295→blank, audible B→A at 301.820 s | FAIL |

## Evidence

All 15,000 counters advanced once. All 118 numeric subtitles appeared on
the intended frame, and both opening pages were blank. Both AC-3 PIDs
and the subtitle PID 0x103 remained signaled in all 6,000 PMTs. HLS
segments 0, 1, 149 and 150 carried both audio tracks and page 888.
Both audio PIDs had 18,750 starts in the window with intervals of 2,878–2,882 ticks
on the 90 kHz clock. Format 2 originally awarded transport a PASS on those
starts. A packet starting 553 ticks before frame zero also occupies 2,327
ticks inside the window. Format 3 includes this crossing packet in the
coverage verdict. The [12:41 run](202611271241_channel-one-test-results.md)
documents the encoder end-marker control and independent Windows audio
loopback measurements.

The calibrated source had the correct tone in every 20 ms window on both
channels of both tracks, including the first and last windows. In each
of the 59 command-to-onset intervals, every complete 4 ms analysis block
on each received PID still contained its own tone. At the 300 s boundary,
the block starting at exactly 300.000 s was silent. The command was issued
at 300.001 s. VLC requested a flush in the same clock millisecond as all
59 selection commands. The last queued buffer was scheduled to end
1,770.7–1,796.6 ms after the flush. [Source levels](202611271304_channel-one/source-validation.json)
and [received tones and VLC flushes per switch](202611271304_channel-one/broadcast-audio.json).

VLC 3.0.24 directs an audio output to discard pending samples on flush.
Keeping those samples in the recorded playback would conceal the gap.
[libVLC audio output contract](https://github.com/videolan/vlc/blob/3.0.24/include/vlc/libvlc_media_player.h#L610-L629).

The longest silence inside delivered PCM was 28 ms at 0.000 s. The table also counts missing PCM after an output flush. Packet-distance PCR jitter used 10,368,000 bit/s. [Audio and all 59 switches](202611271304_channel-one/audio.json), [presented video](202611271304_channel-one/video.json), [rendered subtitles](202611271304_channel-one/subtitle-events.json), [HLS segment probes](202611271304_channel-one/hls-segments.json) and [transport clocks](202611271304_channel-one/transport-verdict.json).

VLC issued B→A at 300.001 s on the player clock. The A tone was present at 301.820 s on the presentation clock. [Rendered transition with PCM](202611271304_channel-one/transition.mkv) and [corresponding measurements](202611271304_channel-one/measurements.json).
