---
report_format: 2
report_id: "202611271156_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: FAIL
criteria:
  video: PASS
  audio: FAIL
  teletext: PASS
  transport: FAIL
  transition: FAIL
evidence_directory: tmp/channel-one-candidate-run02
---

# Channel 1 playback results · 2026-11-27 11:56 CET

**FAIL.** Video and rendered page 888 passed both airings. VLC switched to
the requested tone 59 times. Every switch exceeded the 250 ms latency limit,
and the receiver's audio output contained gaps. Both transport audio PIDs
also had three irregular AC-3 frame intervals.

## Run conditions

| Field | Value |
| --- | --- |
| Scheduled window (UTC) | 2026-11-27 10:56:15.183 to 11:06:15.183 |
| Endpoint and playout | `http://127.0.0.1:62835/iptv/channel/1.ts`, HTTP unicast, Classic, native Hybrid, packaged default MPEG-TS script, new `state/gate.db` |
| Profile and receiver | Seeded 1080p SDR NVENC H.264 High/p5, 8,000 kbit/s video, AC-3 640 kbit/s, 6 channels at 48 kHz. Source cadence 25 fps. libVLC 3.0.24, software decoding, 1,000 ms network cache. |
| Source and tools | Same five-minute synthetic fixture, FFmpeg and ffprobe n9.1-ErsatzRS-2.1.0, TSDuck 3.44-4676 and Python tool versions as the [packaged baseline](202611271100_channel-one-test-results.md). |
| Server SHA-256 | `ea12a45ace195d9b3f28b8680da3fa59eff1b483579bc5919bfc02d5eca12309` |
| Scanner SHA-256 | `cc021dc36313b08e379dae1a86b8cf131c25b0bb9b693b78b89b7b54b3f2251f` |
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
| video.max_display_interval | ms | ≤100 | 43 | PASS |
| audio.selectable_tracks | count | 2 | 2, VLC IDs 257 and 258 | PASS |
| audio.switch_attempts | count | 59 | 59, from 10 through 590 s | PASS |
| audio.max_attempt_time_error | ms, player clock | ≤40 | 3 | PASS |
| audio.verified_switches | count | 59 | 59 audible changes to the requested track | PASS |
| audio.max_switch_latency | ms, command to audible tone | ≤250 | 1,830 at the 290 s command | FAIL |
| audio.longest_pcm_silence | ms | ≤20 | 1,828 at 290.004 s | FAIL |
| audio.longest_dual_tone | ms | ≤4 | 4 at 10.000 s | PASS |
| audio.missing_pcm_samples | samples | 0 | 5,083,503 | FAIL |
| teletext.page_888_selectable | yes/no | Yes | Yes, from preroll through the end | PASS |
| teletext.numeric_cue_offset_counter | frames, visible counter | 0 or +1 | 0 for all 118 numeric changes | PASS |
| teletext.numeric_cue_offset_player | frames, libVLC media clock | 0 or +1 | 0 for all 118 numeric changes | PASS |
| teletext.blank_onsets | boundaries | 2 on time | 2 on time | PASS |
| transport.ac3_pids | count | 2 throughout | 2 stable AC-3 PIDs, 0x101 and 0x102 | PASS |
| transport.page_888_descriptor | yes/no | Present throughout | PID 0x103, English, type 0x02 across 6,003 PMTs | PASS |
| transport.subtitle_pts_offset | frames from video | Within 1 | +5.289 ms (+0.132 frame) for all 118 cues | PASS |
| transport.continuity_errors | count | 0 | 0 | PASS |
| transport.pts_dts_pcr_leaps | counts, PTS / DTS / PCR | 0 / 0 / 0 | 0 / 0 / 0 | PASS |
| transport.pcr_jitter_excursions | count above 8,000 µs | 0 | 0 of 30,250 checked PCRs | PASS |
| transport.ac3_frame_step | irregular intervals per PID | 0 (32 ms per AC-3 frame, ±2 ticks at 90 kHz) | 3 on each PID, 11 µs at 43.995, 299.995 and 599.995 s | FAIL |
| transport.network_arrival_jitter | µs | Requires live arrival timestamps | No live arrival timestamps | NIET GEMETEN |
| transition.at_300s | boundary | 7499→0000, 295→blank, audible B→A | 7499→0000 and 295→blank on frame zero. Audible B→A at 301.816 s | FAIL |

## Transport and receiver evidence

HLS segments 0, 1, 149 and 150 and the final HTTP stream carried both AC-3
tracks and page 888. Packet-distance PCR verification used 10,368,000 bit/s.
The 32 ms AC-3 frame cadence check found a one-tick advance (11 µs) on
**both** PIDs at 43.994667, 299.994667 and 599.994667 seconds. Around the
300 s seam, the final AC-3 packet of HLS segment 79 and the first packet of
segment 80 had the same PTS. The wrapper advanced the outgoing timestamp
by one tick. This fails the transport criterion despite zero TSDuck leap
and continuity errors. See `transport-verdict.json`, `diagnostic/seam.m3u8`
and `diagnostic/seam-http.ts` in the evidence directory.

All counters and clip times were readable in the presented video. The
rendered subtitle was blank on frame zero and changed on the intended frame
at every boundary. The reader classified individual glyphs against independently
decoded source glyphs, allowing a one-pixel raster shift. This resolved the
initially undecided words 85 and 95 with the original recognition thresholds.

The 59 command-to-tone latencies ranged from 1.766 to 1.830 s. The longest
silent span was 1.828 s, including missing presentation samples after VLC
flushed queued PCM. Within the PCM that VLC delivered, the longest silent
span was 28 ms at 44.000 s. Audio used 4 ms analysis windows, a −36 dBFS
tone threshold and a −48 dBFS RMS silence threshold. See [all 59 commands,
audible onsets and PCM measurements](202611271156_channel-one/audio.json).

A separate decoder control opened the original synthetic TS directly in
libVLC 3.0.24 and switched A→B→A. With default file caching the two new
tones started after 1.805 and 1.848 s. Both switches left a 32 ms gap
between delivered PCM blocks. At 100 ms file caching the latencies were
0.914 and 0.954 s, with the same two gaps. This reproduces a receiver-side
delay independently of the HLS splice defect. The gate metrics above use
only the recorded HTTP session.

At 300.002 s VLC accepted the B→A command. It presented the A tone at
301.816 s, a latency of 1.814 s. The displayed picture had already changed
from 7499 with 295 to 0000 with a blank page. [Rendered transition with
played PCM](202611271156_channel-one/transition.mkv) and [matching frame
and selection records](202611271156_channel-one/measurements.json).
