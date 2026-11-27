---
report_format: 1
report_id: "202611271136_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: FAIL
criteria:
  video: FAIL
  audio: FAIL
  teletext: FAIL
  transport: NIET GEMETEN
  transition: NIET GEMETEN
evidence_directory: tmp/channel-one-candidate-run01
---

# Channel 1 playback results · 2026-11-27 11:36 CET

**FAIL.** VLC received two selectable AC-3 tracks and page 888. Playback ended
in the first airing at visible clip time 01:29.12. The last frame was
presented 159.397 seconds after the first frame. The 300-second transition
and a complete 600-second transport window were unavailable.

## Run conditions

The scheduled window started at 10:36:34.999 UTC. The test used its own
SQLite database, the Classic schedule, the packaged default wrapper script
and HTTP unicast at `http://127.0.0.1:59803/iptv/channel/1.ts`. The source,
1080p SDR NVENC/AC-3 profile, libVLC 3.0.24, 1,000 ms cache and measurement
limits match the [full baseline](202611271100_channel-one-test-results.md).
The server SHA-256 was `3a80adcac597e61ced95b9bccf075270bbdcc0010a8852527f885289d554658b`.
See `versions.json` for the scanner and source hashes.

## Comparison metrics

A partial observation can establish a failure. Metrics that require the whole
window retain `NIET GEMETEN` when the available samples meet their limit.

| Metric ID | Unit | Acceptance | Observed | Verdict |
| --- | --- | --- | --- | --- |
| connection.http_sessions | count | 1 | 1 until the stream ended | NIET GEMETEN |
| connection.rebuffer_events | count | 0 | 53 buffering notifications below 100% | FAIL |
| video.presentations | frames | 15,000 | 3,142 before playback ended | FAIL |
| video.missing_counters | frames | 0 | 12 within the presented range 0–2228 | FAIL |
| video.repeated_counters | frames | 0 | 925 extra presentations | FAIL |
| video.reversed_counters | frames | 0 | 0 in the available frames | NIET GEMETEN |
| video.longest_content_hold | ms | One frame per counter | 31,881 at 50.486 s, counter 1088 | FAIL |
| video.max_display_interval | ms | ≤100 | 465 | FAIL |
| audio.selectable_tracks | count | 2 | 2 through the last observed frame, IDs 257 and 258 | NIET GEMETEN |
| audio.switch_attempts | count | 59 | 8 of 59, at 10 through 80 s | FAIL |
| audio.max_attempt_time_error | ms, player clock | ≤40 | 3 for the eight commands | NIET GEMETEN |
| audio.verified_switches | count | 59 | 8 audible changes, 0 within the 250 ms limit | FAIL |
| audio.max_switch_latency | ms, command to audible tone | ≤250 | 1,835 at the 10 s command | FAIL |
| audio.longest_pcm_silence | ms | ≤20 | 31,800 at 50.768 s, including missing output | FAIL |
| audio.longest_dual_tone | ms | ≤4 | 4 at 0.044 s in the available PCM | NIET GEMETEN |
| audio.missing_pcm_samples | samples | 0 | 4,061,969 in the recorded presentation interval | FAIL |
| teletext.page_888_selectable | yes/no | Yes | Selected at startup and last observation | NIET GEMETEN |
| teletext.numeric_cue_offset_counter | frames, visible counter | 0 or +1 | −4 or −5 for the 16 observed numeric boundaries | FAIL |
| teletext.numeric_cue_offset_player | frames, libVLC media clock | 0 or +1 | −4 or −5 for the 16 observed numeric boundaries | FAIL |
| teletext.blank_onsets | boundaries | 2 on time | First blank on time, second boundary unavailable | NIET GEMETEN |
| transport.ac3_pids | count | 2 throughout | 0x101 and 0x102 in retained segments and HTTP capture | NIET GEMETEN |
| transport.page_888_descriptor | yes/no | Present throughout | PID 0x103, type 0x02 in the retained PMT | NIET GEMETEN |
| transport.subtitle_pts_offset | frames from video | Within 1 | First HTTP cue −352 ms relative to video, −8.8 frames | FAIL |
| transport.continuity_errors | count | 0 | Full measurement window unavailable | NIET GEMETEN |
| transport.pts_dts_pcr_leaps | counts, PTS / DTS / PCR | 0 / 0 / 0 | Full measurement window unavailable | NIET GEMETEN |
| transport.pcr_jitter_excursions | count above 8,000 µs | 0 | Full measurement window unavailable | NIET GEMETEN |
| transport.network_arrival_jitter | µs | Requires live arrival timestamps | No live arrival timestamps | NIET GEMETEN |
| transition.at_300s | boundary | 7499→0000, 295→blank, audible B→A | Playback ended at visible counter 2228 of the first airing | NIET GEMETEN |

## Evidence

- HLS segments 0, 1, 17 and 18 contained AC-3 PIDs 0x101 and 0x102 and
  selectable page 888 on PID 0x103. See `hls-segments.json`,
  `received-pmt.xml` and `receiver.json`.
- All eight selection commands changed the audible tone. Command-to-tone
  latency ranged from 1.815 to 1.835 seconds. The longest stretch of
  *delivered silent samples* was 36 ms at 0.008 s. The longer silent span in
  the table includes missing PCM. See `audio.json` and `pcm.csv`.
- Segment 5 contained 4.800 s of video. Its published duration was 22.683644 s.
  The manifest later advertised a target duration of 111 s. The server logged
  HLS worker reclamation while the viewer was still connected. See
  `diagnostic/live.m3u8`, `diagnostic/live000005.ts` and `server.log`.
- In the first full airing the first numeric cue reached the HLS segment at
  +5.005 s relative to visible frame zero. The corresponding HTTP PES reached
  it at +4.648 s. VLC's rendered offsets are listed separately above. See
  `diagnostic/upstream-start.ts` and `subtitle-events.json`.
