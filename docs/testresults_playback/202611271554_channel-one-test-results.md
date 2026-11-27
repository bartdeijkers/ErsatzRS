---
report_format: 5
report_id: "202611271554_channel-one"
test_case: channel-one-vlc
platform: win-x64
overall: PASS
criteria:
  video: PASS
  audio: PASS
  teletext: PASS
  transport: PASS
  broadcast_audio: PASS
  transition: PASS
  av_offset_measurement: PASS
evidence_directory: tmp/channel-one-av-offset-run01
---

# Channel 1 playback results · 2026-11-27 15:54 CET

**PASS.** One session covered two full airings with a frame-locked audio timecode. The receiver decoded the content time after 59 of 59 selections. The maintainer allowed up to 2 s of silence during a verified VLC track selection. This report applies that rule to the original capture. Earlier reports retain their original limits.

## Run conditions

| Field | Value |
| --- | --- |
| Scheduled window (UTC) | 2026-11-27 14:54:36.7286054 through 2026-11-27 15:04:36.7286054 |
| Endpoint and playout | `http://127.0.0.1:52508/iptv/channel/1.ts`, HTTP unicast, native Hybrid, Classic, built-in wrapper, fresh `state/gate.db`, LAN discovery disabled |
| Effective profile | Seeded 1080p SDR, NVENC H.264 High/p5, 8,000 kbit/s video, source cadence of 25 fps, AC-3 at 640 kbit/s, six channels and 48 kHz |
| Receiver and tools | libVLC 3.0.24, 1,000 ms network cache and software video decoding. FFmpeg/ffprobe n9.1-ErsatzRS-2.1.0, TSDuck 3.44-4676, Python 3.13.12, python-vlc 3.0.21203, NumPy 2.5.5 and Pillow 13.0.0 |
| Server SHA-256 | `51a219a567c0530a31e385942ad10f3a53e6df40965bcb5d06f57d4132f92f82` |
| Scanner SHA-256 | `7176bfcd1db2e32fada78c5797ccefba429859fec9c7ebedc73bf2890130c4d8` |
| Source SHA-256 | `a4b0f332079242eeb008f57201f550657b42148883fa3ed400c1ac596328929a` |
| Offset measurement | 4 ms audio grid, 1.280 s coded windows, signed video time minus audio content time. No A/V tolerance was requested. |

## Comparison metrics

| Metric ID | Unit | Acceptance | Observed | Verdict |
| --- | --- | --- | --- | --- |
| connection.http_sessions | count | 1 | 1 | PASS |
| connection.rebuffer_events | count | 0 | 0 | PASS |
| video.presentations | frames | 15,000 | 15,000 in 599.999 s | PASS |
| video.missing_counters | frames | 0 | 0 | PASS |
| video.repeated_counters | frames | 0 | 0 | PASS |
| video.reversed_counters | frames | 0 | 0 | PASS |
| video.longest_content_hold | ms | One frame per counter | Each counter presented once | PASS |
| video.max_display_interval | ms | ≤100 | 50.0 | PASS |
| audio.selectable_tracks | count | 2 | [257, 258] | PASS |
| audio.switch_attempts | count | 59 | 59 | PASS |
| audio.max_attempt_time_error | ms, player clock | ≤40 | 3 | PASS |
| audio.verified_switches | count | 59 | 59 | PASS |
| audio.max_switch_latency | ms, command to audible tone | ≤2,000 ms silence plus ≤250 ms non-silent latency | 1,816 at 280 s | PASS |
| audio.longest_pcm_silence | ms | ≤2,000 during a switch, ≤20 elsewhere | 1,816 at 60.000 s | PASS |
| audio.longest_dual_tone | ms | ≤4 | 4 at 10.000 s | PASS |
| audio.missing_pcm_samples | samples | Only during accepted switches | 5,080,809 | PASS |
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
| transition.at_300s | boundary | 7499→0000, 295→blank, B→A within revised VLC limits | 7499→0000, 295→blank, audible B→A at 301.816 s | PASS |
| audio.max_switch_silence | ms | ≤2,000 | 1,817.562 at 60 s | PASS |
| audio.max_non_silent_switch_latency | ms | ≤250 | 0.000 at 10 s | PASS |
| audio.silence_outside_switches | ms | ≤20 | 0 | PASS |
| audio.missing_outside_switches | samples | 0 | 0 | PASS |
| transport.broadcast_tone_0x101 | ms, longest unexpected tone interval | ≤20, zero wrong-only tone blocks | 4 | PASS |
| transport.broadcast_tone_0x102 | ms, longest unexpected tone interval | ≤20, zero wrong-only tone blocks | 4 | PASS |
| av_offset.decoded_switches | count | 59 | 59 | PASS |
| av_offset.coverage | windows | All measured | 421 of 421 | PASS |
| av_offset.range_a | ms, positive means audio late | Measured range | -11.152 to +1.000 (median -1.833) | PASS |
| av_offset.range_b | ms, positive means audio late | Measured range | -12.444 to +0.976 (median -1.240) | PASS |
| av_offset.at_300s | ms, positive means audio late | Measured after return of A | -9.939 | PASS |

## Evidence

The same server binary as the [14:20 trial](202611271420_channel-one-test-results.md)
served the newly coded source. All 59 selections were audible, and all 421
VLC content-time windows decoded. The median signed offset was −1.333 ms.
The largest magnitude was −12.444 ms on B at 298.168 s. Across 60 independent
TS samples per PID, both tracks measured −1.333 ms throughout. The audio
code grid is 4 ms. All VLC offsets were smaller than one 40 ms video frame.

The receiver was already running when the two-second allowance was approved.
The live launcher initially applied the previous 250 ms limit. The final
offline gate used the same received bytes, rendered frames, selection
commands, flush events and played PCM. The initial measurements are retained
under `original-analysis/` in the run directory. One window ending at the
300 s splice contained two attenuated RMS blocks. The decoder treats
out-of-band blocks as erasures and requires at least 98% usable coverage
and unchanged correlation and uniqueness margins. The ordinary audio checks
still include those samples. Known-offset, ambiguity, erasure and switch-limit
controls passed.

TSDuck found no continuity or PTS/DTS/PCR jumps. Its PCR jitter reference
was packet distance at 10,368,000 bit/s. The four sampled HLS segments
carried both AC-3 PIDs and page 888. Each transmitted PID had a 4 ms silent
block near each airing end, within the 20 ms broadcast limit. The raw
switch latency uses the start of the first 4 ms new-tone window, while the
silence calculation also counts any missing samples in that partial block.

VLC issued B→A at 300.001 s. The A tone first registered at 301.816 s. The switch contained 1815.562 ms of silence. The first coded window after that onset measured -9.939 ms A/V offset. Positive values mean audio lags video. [Rendered boundary and PCM](202611271554_channel-one/transition.mkv) and [matching switch and offset](202611271554_channel-one/measurements.json).

[Receiver offsets and per-switch comparisons](202611271554_channel-one/av-offset.json), [PCM and all 59 transitions](202611271554_channel-one/audio.json), [source timecode checks](202611271554_channel-one/source-validation.json) and [per-PID decoded broadcast clocks](202611271554_channel-one/broadcast-audio.json).

[Video](202611271554_channel-one/video.json), [page 888](202611271554_channel-one/subtitle-events.json), [TSDuck clocks and continuity](202611271554_channel-one/transport-verdict.json), [HLS inventory](202611271554_channel-one/hls-segments.json) and [aggregate criteria](202611271554_channel-one/result.json).
