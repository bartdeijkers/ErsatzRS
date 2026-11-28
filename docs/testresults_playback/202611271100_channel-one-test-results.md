---
report_format: 1
report_id: "202611271100_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: FAIL
criteria:
  video: FAIL
  audio: FAIL
  teletext: FAIL
  transport: FAIL
  transition: FAIL
evidence_directory: tmp/channel1-player-clock
---

# Channel 1 playback results · 2026-11-27

**FAIL.** The source contained two AC-3 tracks and selectable DVB teletext page 888.
The first HLS segment already contained only the English audio track and no teletext
descriptor. VLC received the same stream layout and displayed the captions burned into
the video. All 59 scheduled switch attempts ran. None changed the audio track.

This run covers embedded subtitle page 888 in one continuous session. The
reported Kodi stop and reopen after ten seconds, with an existing live
transcode and browsable teletext pages, remains a separate reproduction.

The [13:46 follow-up on the corrected server](202611271346_channel-one-test-results.md)
verified both AC-3 PIDs and page 888 throughout the outgoing stream.
Stock VLC still interrupted audio at each selection and retained the
previous subtitle on the first measured frame. The tables below record
the original packaged baseline.

In the [14:20 follow-up](202611271420_channel-one-test-results.md), video
and page 888 shared the same presentation timestamps. Both opening frames
were blank. The receiver's audio interruptions still failed the gate.

The [15:54 coded-audio trial](202611271554_channel-one-test-results.md)
passed with the maintainer's revised two-second VLC switch-silence
allowance. It measured the A/V offset after all 59 selections.

## Run conditions

| Field | Value |
| --- | --- |
| Scheduled measurement window (UTC) | 2026-11-27 09:01:26.373 to 09:11:26.373 |
| Playback window | Two consecutive 300 s airings in one libVLC session. Media time zero is the first presented frame of the first full airing. Measured player duration 599.999 s. |
| Installation and route | Fresh SQLite `state/gate.db`, channel 1, Classic schedule, native MPEG-TS Hybrid, built-in wrapper, HTTP unicast at `http://127.0.0.1:53487/iptv/channel/1.ts` |
| Effective profile | `1920x1080 x264 ac3 6ch`, NVENC H.264 High/p5, 8,000 kbit/s video, 16,000 kbit buffer, AC-3 at 640 kbit/s, 6 channels, 48 kHz. The profile retains the source cadence of 25 fps. |
| Source | 1920×1080 progressive BT.709, 7,500 numbered frames. English A at 750 Hz and Dutch B at 1,500 Hz, both stereo AC-3, 48 kHz, measured −18.00 dBFS peak. Page 888 starts blank and then displays 5 through 295. |
| Receiver and tools | libVLC 3.0.24, 1,000 ms network cache, software video decoding. FFmpeg and ffprobe n9.1-ErsatzRS-2.1.0, TSDuck 3.44-4676. Python 3.13.12, python-vlc 3.0.21203, NumPy 2.5.5, Pillow 13.0.0. |
| Server SHA-256 | `d21ff5c697bde8cededddae77dea8c072b9e77dd140e3dac1fd0f69fff68283c` |
| Scanner SHA-256 | `d378acf2c528dc0dd77ac434860f5b15ab95a9a500bf3d97eec6d42839d64e8f` |
| Source clip SHA-256 | `bce61f654839c86a4c9add2341279f242e83bc22a09dcef2af60ad9652b771ee` |

## Comparison metrics

Match rows across runs by metric ID and retain the units and clock reference.
For cue offsets, negative values mean early and positive values mean late.
One video frame is 40 ms. `NIET GEMETEN` indicates an unavailable measurement.
Use the [recorded runs and comparison command](../how-to/test-channel-one-vlc.md#recorded-runs)
to compare this baseline with later candidates using the same criteria.

| Metric ID | Unit | Acceptance | Observed | Verdict |
| --- | --- | --- | --- | --- |
| connection.http_sessions | count | 1 | 1 | PASS |
| connection.rebuffer_events | count | 0 | 0 | PASS |
| video.presentations | frames | 15,000 | 15,000 | PASS |
| video.missing_counters | frames | 0 | 14, counters 7486–7499 in the first airing | FAIL |
| video.repeated_counters | frames | 0 | 14 extra presentations of counter 1100 | FAIL |
| video.reversed_counters | frames | 0 | 0 | PASS |
| video.longest_content_hold | ms | One frame per counter | 599 at 43.998 s | FAIL |
| video.max_display_interval | ms | ≤100 | 43 | PASS |
| audio.selectable_tracks | count | 2 | 1 | FAIL |
| audio.switch_attempts | count | 59 | 59, every 10 s from 10 through 590 s | PASS |
| audio.max_attempt_time_error | ms, player clock | ≤40 | 3 | PASS |
| audio.verified_switches | count | 59 | 0 | FAIL |
| audio.max_switch_latency | ms, command to audible tone | ≤250 | No completed switch | NIET GEMETEN |
| audio.longest_pcm_silence | ms | ≤20 | 32 at 300.064 s | FAIL |
| audio.longest_dual_tone | ms | ≤4 | 4 at 0.000 s | PASS |
| audio.missing_pcm_samples | samples | 0 | 0 | PASS |
| teletext.page_888_selectable | yes/no | Yes | No | FAIL |
| teletext.numeric_cue_offset_counter | frames, visible counter | 0 or +1 | −13 for all 118 numeric changes | FAIL |
| teletext.numeric_cue_offset_player | frames, libVLC media clock | 0 or +1 | −13 for 67 changes, +1 for 51 | FAIL |
| teletext.blank_onsets | boundaries | 2 on time | 2 on time, at 0 and 300 s | PASS |
| transport.ac3_pids | count | 2 throughout | 1 throughout the received window | FAIL |
| transport.page_888_descriptor | yes/no | Present throughout | Absent across 6,003 PMTs | FAIL |
| transport.subtitle_pts_offset | frames from video | Within 1 | No subtitle PID to extract | NIET GEMETEN |
| transport.continuity_errors | count | 0 | 0 | PASS |
| transport.pts_dts_pcr_leaps | counts, PTS / DTS / PCR | 0 / 0 / 0 | 0 / 0 / 0 | PASS |
| transport.pcr_jitter_excursions | count above 8,000 µs | 0 | 0 of 30,250 checked PCRs | PASS |
| transport.network_arrival_jitter | µs | Requires live arrival timestamps | No arrival timestamps recorded | NIET GEMETEN |
| transition.at_300s | boundary | 7499→0000, 295→blank, audible B→A | 7485→0000, 295→blank, A remained selected | FAIL |

The decoded HTTP recording contained the same 14 repeated and 14 missing counters
as the VLC display callbacks. Audio measurements use VLC's presented mono PCM at
48 kHz in 4 ms windows. Tone detection uses −36 dBFS peak and silence detection
uses −48 dBFS RMS. The PCR reference is TS packet distance at 10,368,000 bit/s.

## Stream layout by stage

| Observation point | AC-3 PIDs | DVB subtitle page 888 |
| --- | --- | --- |
| Validated source | `0x101` English, `0x102` Dutch | PID `0x103`, English, descriptor type `0x02` |
| HLS segment 0 | `0x101` | Absent |
| HLS segment 1 | `0x101` | Absent |
| HLS segments 146 and 147 | `0x101` in both | Absent in both |
| HTTP MPEG-TS, full measurement window | `0x101` | Absent |
| libVLC | One selectable track, ID 257 | Unavailable |

## Loop boundary and evidence

At the boundary VLC went directly from `7485 / 04:59.40` with `295` to
`0000 / 00:00.00` with a blank subtitle. At player time 300.000 s the harness
requested A. A was still selected because B had never been available. The
32 ms silence began 64 ms after the boundary.
The [audio measurement log](202611271100_channel-one/audio.json) records
all 59 attempts, their media times and the available track IDs.

The 5.441 s lossless clip and its corresponding frame and selection records are
archived with this report. [Rendered transition with played PCM](202611271100_channel-one/transition.mkv)
and [transition measurements](202611271100_channel-one/measurements.json).

The following files are relative to the local evidence directory in the report header.
The [playback test runner](../../scripts/e2e/channel1-vlc.py) produces the same
artifact names for subsequent runs.

| Evidence | Files |
| --- | --- |
| Run, profile, identities and gate verdicts | `run.json`, `profile.json`, `versions.json`, `airings.json`, `result.json` |
| Source and HLS inventory | `source-validation.json`, `source-pmt.xml`, `hls-segments.json`, `hls-start/`, `hls-pair/` |
| Received transport and clocks | `window.ts`, `window-pmt-all.xml`, `transport-verdict.json`, `continuity.log`, `pcrverify.log` |
| Player output and all 59 attempts | `receiver.json`, `video.json`, `audio.json`, `vlc.s16le`, `pcm.csv`, `subtitle-events.json` |
| Rendered boundary clip with played audio and matching records | `transition/transition.mkv`, `transition/measurements.json` |
