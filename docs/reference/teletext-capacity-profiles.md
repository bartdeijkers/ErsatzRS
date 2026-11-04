# Teletext capacity profiles

Authored teletext needs two explicit global settings:

- `teletext.authoring.policy`: bounds for preparing, validating, previewing
  and compiling drafts, including TV-guide source reads (version 2).
- `teletext.output.policy`: bounds for recovering the accepted generation and
  adding it to the final MPEG-TS output, including live updates and automatic
  guide refresh (version 4).

There are still no compiler defaults (ADR 0121). If either setting is absent,
ordinary playback continues without teletext. A **capacity profile** is a
reviewed, versioned pair of complete policies. The operator applies it
explicitly from the channel teletext page (**Teletext readiness** →
**Teletext capacity**). The action:

1. requires confirmation that it changes the settings of **all** teletext
   channels;
2. parses both documents with the same validators the runtime uses before
   anything is written;
3. writes missing or invalid settings, and leaves a setting that already
   equals the profile untouched;
4. replaces a valid hand-tuned setting only when **Also replace existing
   hand-tuned policies** is also ticked.

The status shown is `absent`, `profile`, `custom` (valid but hand-tuned),
`invalid` or `unreadable`. Raw policy text is never shown.

## `tvguide-16`: TV guide (Level 1/1.5), up to 16 pages per generation

Source: `crates/ersatzrs-web/src/host/teletext_capacity/`.

**Evidence.** This is exactly the policy pair under which the production
TV-guide add-on bundle is qualified by the integrated tests in
`crates/ersatzrs-web/tests/teletext_profile_preparation/tvguide_candidate/`:
authoring version 2 with guide-source limits, and output version 4 with live
updates and guide refresh. The unit test
`tvguide_profile_is_exactly_the_policy_pair_the_production_guide_is_qualified_with`
keeps the profile and that qualified composition identical, so a change to
one requires reviewing the other. The values come from the same reviewed base
document as the test fixture. The profile is not "whatever the fixture
contains": a fixture change fails that test until the profile is re-reviewed.

**What the bounds mean.** Every bound is a ceiling, not a reservation. Unused
capacity costs nothing. A larger input is rejected with a diagnostic and is
never truncated, and ordinary playback continues.

| Group | Main bounds | Why this size |
|---|---|---|
| `encoding.max_pages` | 16 | The index plus channel pages and their subpages in one generation. The TV-guide add-on paginates into subpages rather than dropping rows. Guides that need more pages are rejected visibly, not shortened. |
| `design` / `input` | 256 KiB documents, 64 KiB responses, 8 producers | The add-on response for 16 Level 1/1.5 pages stays well below these sizes. They bound memory and work for the worst accepted input. |
| `validation` / `composition` | 16 records/objects, 8,192 commands | Level 1/1.5 pages with X/26 enhancements. Level 2.5 resource banks are not this profile's purpose. |
| `encoding.*work*`, `normalization.*` | per-stage work credits | Deterministic work meters for compilation. They are independent of channel count. |
| `preview` | 16 pages, 168,000 pixels | Server-side preview of the same 16 pages. |
| `guide_limits` | 1,000 records, 1 MiB source, 10,000 model nodes | Upper bound for one guide refresh read. 1,000 programme records cover a multi-channel window. |
| `source.elementary_pid` | 512 (0x200) | Outside FFmpeg's default stream PIDs (0x100 upwards per stream) and PMT PID 0x1000. Collisions are still rejected at runtime. |
| `source.max_hold_ms` / `playout_delay_ms` / `max_lateness_ms` | 2,000 / 100 / 100 | Bounded holding of the final MPEG-TS for PSI/PCR admission. These limits add no latency beyond the playout delay. |
| `source.rate_tolerance_ppm`, `pcr_*` | 1,000 ppm, 14 ticks jitter, 36 packets slack | Constant-rate output from FFmpeg's CBR muxer. Deviations fail admission rather than producing timing errors. |
| `live_updates` | 25 ms poll, 2 s recovery | Accepted updates and guide refresh are picked up at the next carousel boundary. |

**Transport capacity.** The policy does not size the multiplex. The output
muxrate comes from the channel's FFmpeg profile (video plus audio bitrate,
plus 20 %). Teletext uses null packets in that constant-rate output. FFmpeg
writes each video frame contiguously, so a large frame leaves a stretch without
any null packet. Its length is roughly the frame size divided by the muxrate.
With the bounded deferral fix (ADR 0121), a batch may wait up to four further
20 ms intervals for free null packets. A multiplex that stays full for longer
reports `SustainedCapacity` instead of dropping data.

Measured with FFmpeg 6.1 and libx264 (1080p25 with noise, 8 Mbit/s video,
192 kbit/s AAC), the longest stretch without null packets was:

| Muxrate | VBV buffer | Longest stretch |
|---|---|---|
| 1.2 × content (9.8 Mbit/s) | 16,000 kbit | 0.78 to 1.1 s, plus 1.5 to 3.9 s at start-up |
| 1.5 × content | 16,000 kbit | about 0.42 to 0.46 s |
| 2.0 × content | 2,000 kbit | 0.06 to 0.14 s |

The figures are pauses in the *source* output. Without page-aware scheduling
(below), any pause longer than 100 ms that falls inside a page stopped the
stream.

The 100 ms deferral alone therefore covers only short bursts. The gate now
also schedules **per page**. EN 300 706 B.9 ("Maximum time between successive
packets of a page") applies within one page, not between pages. A new page
(its header), a magazine declaration or a cycle terminator therefore waits
while the output already received shows no room for the whole page within the
100 ms spacing. It waits at most 5 s (`SustainedCapacity` after that), and the
B.6 20 s acquisition bound still applies. Packets of a page that has started
still wait at most 100 ms.

**Managed runtime evidence (ersatzrs-ffmpeg 2.0.9, FFmpeg 9.0).** The same
check was repeated with the managed runtime and ErsatzRS's own argv. Two
30 s programmes (synthetic 1080p25 with noise) went through the HLS
segmenter command (libx264 high/veryfast, 8000k, VBV 16000k, GOP 60, AAC
192k, segment muxrate 9830400, `pcr_period` 20). They were then remuxed by
the built-in MPEG-TS wrapper command (`-c copy -max_interleave_delta 100000
-muxrate 9830400 -pcr_period 20`). Reading at `-readrate 1.0` produced
byte-identical output. The final stream had about 12 % null packets on
average. It had a stretch of 0.4 to 0.8 s without nulls at every 2 s
keyframe, 1.9 s at start-up and 2.7 s at the programme transition. With only
the 100 ms deferral the gate fails on this stream. With page-aware
scheduling, all 60 s pass:

- audio, video and PCR are byte-identical, with no continuity errors;
- the runtime's own libzvbi decoder receives pages 200 and 201 about 254
  times each (median every 0.2 s, at most 2.8 s apart around the transition).

The content is synthetic noise, which is harsher than typical library media.
Measurements on the maintainer's real media and channels remain part of the
end-to-end proof.

Evidence with the real FFmpeg captures above at **1.2 × content bitrate**
(`output_real_burst_tests.rs`, opt-in): no transport failure across 12 to 20 s
with null-free stretches up to 0.82 s and 3.9 s at start-up. Audio, video and
PCR packets are byte-identical, and there are no continuity errors.
FFmpeg's libzvbi decoder independently decodes pages 200 and 201 from the
delivered stream. Cost: during an I-frame burst a page arrives later (the
longest pause between teletext packets was about 0.86 s). The 100 ms value is
taken from the existing gate documentation and has not been re-checked
against the standard text. EN 300 472 clause 5 (40 ms maximum retention in
Bttx, no maximum gap between PES packets) is satisfied, because the PTS names
the interval that actually carries the batch.

**Not covered.** Larger guides (more than 16 pages), Level 2.5 resource-heavy
services and measured peak memory per platform. These need a new reviewed
profile.
