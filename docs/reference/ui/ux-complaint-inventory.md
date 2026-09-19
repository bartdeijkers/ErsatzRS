# ErsatzTV UX complaint inventory

Evidence base for deciding where ErsatzRS should diverge from its inherited
ErsatzTV UX rather than port it faithfully.

**Compiled:** 2026-08-10
**Method:** all 400 issues in `ErsatzTV/legacy` classified by close reason
(275 `COMPLETED`, 117 `NOT_PLANNED`, 8 `DUPLICATE`), then the `NOT_PLANNED` pile
clustered by theme. The repository has **zero GitHub Discussions**, so the issue
tracker absorbed all "how do I" traffic — relevant to the support-burden story
below.

**Sources.** Primary: the `ErsatzTV/legacy` issue tracker (issue numbers cited
throughout are `github.com/ErsatzTV/legacy/issues/<n>`). Secondary: the
maintainer's archive announcement (#2839) and its follow-up comment quoting two
`r/ErsatzTV` posts. The former feature-request site `features.ersatztv.org` now
redirects to the project homepage and its vote data is gone.

## Important framing correction

ErsatzTV's UI is **not** widely criticised on visual quality. The one published
three-way comparison calls it "modern and responsive". The complaints are about
the **conceptual model** and the **absence of explanation** — users cannot tell
what to create in what order, and cannot tell why the scheduler produced the
playout it produced. Porting the visual design faithfully is fine; porting the
object model and the opacity is the liability.

## The maintainer's own diagnosis

ErsatzTV Legacy was archived in early 2026. From #2839:

> the current scope of the project has expanded beyond what I find personally
> useful or sustainable to maintain in my free time

The follow-up comment attributes the archive to "the support burden from scope
creep". Upstream's answer is a split, not a redesign:

- **ErsatzTV Next** — ground-up rewrite in **Rust**, in collaboration with
  **Tunarr**, scoped solely to reliable HLS stream generation. It deliberately
  does **not** schedule; it consumes a playout JSON schema produced by external
  tools. Repo: `ErsatzTV/next`, schema at `schema/playout.json`.
- **ErsatzTV Legacy** — unarchived to serve as a reference scheduler on top of
  Next. Feature-frozen, no PRs accepted, forks encouraged.

This matters to ErsatzRS in two ways. Scope was the root cause of the UX
complexity, so the clusters below are symptoms of one disease. And the advanced
scheduling that Next explicitly refuses to build is precisely what ErsatzRS
already has four engines for.

## Cluster 1 — The four-object model is the top conceptual complaint

Users must create a Channel, then a Schedule, then bind them via a Playout, with
Collections as a fourth object. Nothing on screen explains the order.

- **#1043** is the clearest first-run report in the tracker. A user on day one:
  "Why is the Playouts page not integrated into the Channels page? … could
  [Schedules] be combined with the Collections page? … Right now you have to
  create the channel, then create a schedule, and then finally assign that
  schedule to a playout."
- **#1013** "ability to schedule things in the scheduler" — the naming itself
  fails to convey what each object does.

**Divergence candidate:** a guided create-a-channel flow that produces all four
objects in one pass, with the standalone pages kept for advanced editing.

## Cluster 2 — Schedule item manipulation is tedious

- **#1344** `[UX] Schedule (Re-)Ordering` — items reorder only via up/down
  arrows, "for a schedule with a lot of shows/items, this becomes tedious";
  asks for drag/drop plus a sort A-Z action.
- **#1347** selection of individual files in the playout
- **#1346** playback controls in the playout
- **#1748** exclude episodes from schedule items
- **#1671** per-schedule-item rather than per-day schedule fall-through
- **#1344** is exactly the gap Tunarr markets against: a drag-and-drop visual
  guide editor that "makes schedule creation feel more like using a DVR".

## Cluster 3 — "Not a bug, it's configuration" = discoverability defects

Each of these was closed as user error. Each is a UI that failed to explain
itself. This is the highest-yield cluster because the fixes are small and local.

- **#2788** "ErsatzTV tells me that my FFmpeg and FFprobe paths don't exist,
  when they do." Resolution: you must supply the path to the *executable*, not
  the folder. The validation message did not say so.
- **#2807** `Limit To Date Range`. Resolution: "As it says at the top of the
  page: in priority order from top to bottom…" — on-screen text that was present
  and still missed.
- **#2787** mid-roll playing after a schedule item without chapters.
  Resolution: "This is by design. You can confirm by setting your scheduling log
  level to debug." Users are directed to raise a log level to understand
  scheduling behaviour.
- **#1909** `Limit To Date Range` doesn't schedule past dates for block
  playouts — the same feature confusing users a second time.
- **#825** `Shuffle Schedule Items` is missing documentation.

**Divergence candidate:** every validation failure names the fix, not just the
symptom; every "by design" scheduling behaviour is explained in the UI at the
point of decision rather than in a debug log.

## Cluster 4 — No visibility into why the playout looks like this

The deepest cluster. Users cannot distinguish "the scheduler is broken" from
"the scheduler did what I asked and I asked for the wrong thing."

- **#2717** schedules aren't translating into playouts correctly
- **#1359** "Multiple" playout mode isn't working for me
- **#1959** shuffle in order: Collections (good) vs Multi-Collections (incorrect)
- **#850** investigate whether playouts starting at midnight is necessary
- **#1641** playout schedule after a DST change is incorrect
- **#847** adding new shows rebuilds playout
- **#906** request for an in-app monitoring page
- **#1858** constant spinning circle on the Playout page

**Divergence candidate:** per-playout-item provenance — which schedule item,
which collection, which ordering decision produced this entry, and why the next
one follows it. The debug-log answer in #2787 is proof the UI cannot currently
answer this.

## Cluster 5 — Hardware and FFmpeg failures are opaque

The largest cluster by volume, and the one ErsatzRS has already partly solved
with the health-check engine.

- **#2452** ffmpeg at 100% CPU despite AMF; **#1912**, **#1950** hardware
  acceleration not working
- **#2500**, **#2585** QSV undefined behaviour and failure to stop transcoding
  on non-graceful disconnect
- **#1983** Windows build missing iHD and i965 VAAPI drivers
- **#1070** VAAPI h264 filter issue; **#1350** amdgpu VAAPI with 2+ playouts
- **#1549** `error applying option itrc to filter colorspace`; **#1547** error
  initializing filters
- **#1526** cannot play many movies (`smpte2084`); **#1463** H.265/HDR10
- **#2707** hardware VP9 decode + fast-seek produces black video
- **#1498** HEVC returns 5xx while H.264 works
- **#1785** HLS Segmenter V2 introduces "boiling" artefacts vs V1
- **#2826** HLS segmenter severe random buffering — closed with "neither of the
  troubleshooting logs show a performance issue", i.e. the logs did not let
  either party reach a conclusion

**Position:** this validates the health-check engine as a differentiator. The
gap to close is turning each recurring failure signature into a named check with
a stated fix.

## Cluster 6 — Subtitle and audio-track behaviour surprises users

Directly relevant: this is a declared high-priority ErsatzRS parity surface, and
ErsatzRS has inherited the underlying trap.

- **#1721** subtitles defined in Schedules but don't appear
- **#1960** subtitle preferences not honoured
- **#1470** subtitles not working from Plex media
- **#1559** embed subtitles instead of burning them, to allow end-player
  selection; **#1138** embedded subtitles in HLS Direct mode
- **#1421** second audio track for channels; **#1381** different audio tracks
- **#1481** hearing-impaired subtitle mode

**Inherited trap:** subtitle mode is baked into the playout item at build time,
so the item's value wins over the live channel setting and a changed setting
requires a full Reset Playout rather than a Refresh. #1721 and #1960 are that
behaviour reported as a bug. ErsatzRS carries the same design, so it will
reproduce the same complaints unless the UI states the consequence at the point
of change.

## Cluster 7 — Operational lifecycle gaps

- **#1255** config/DB backup and export; **#1908** backup to migrate to another
  OS; **#994** move user data to another drive
- **#2533** Docker data written to the overlay2 writable layer instead of the
  bind mount, then lost
- **#1614** separate the admin web UI from the channels.m3u / video flux;
  **#1775** limit channel access per user; **#1837** reverse-proxy guidance
- **#2736** the REST API covers only ~11% of application functionality
- **#1577** "Windows web ui went from perfect to massively large and unusable"
  (display scaling regression)

## Cluster 8 — Search and smart collections are fragile

- **#2547** search is broken, cannot make smart collections
- **#1524** search index maybe broken; **#1895** search index failed to
  initialize, will delete and recreate
- **#2722** Tagalog (`tgl`) not allowed as a search query
- **#1867** media lists don't display the correct amount

## Suggested priority for ErsatzRS

Ranked by user pain per unit of work, not by cluster size.

1. **Cluster 3** — validation and "by design" messages. Small, local, each one
   removes a closed-as-user-error issue.
2. **Cluster 1** — a guided channel-creation flow over the existing four
   objects. No schema change required.
3. **Cluster 4** — playout provenance. The genuine differentiator; nothing in
   this space explains its own scheduling decisions.
4. **Cluster 6** — surface the bake-at-build-time consequence in the UI. Cheap,
   and prevents inheriting a known complaint verbatim.
5. **Cluster 2** — drag-and-drop reordering. Needed to be competitive with
   Tunarr, but it is table stakes rather than an advantage.
6. **Cluster 5** — continue converting failure signatures into named health
   checks. Already the strongest existing lead.

## Open strategic question

ErsatzRS targets parity with a **feature-frozen** codebase whose maintainer
concluded the scope was the problem, while upstream rebuilds the streaming half
in Rust with Tunarr and publishes a playout JSON schema for third-party
schedulers. Worth an explicit decision, and an ADR if it changes direction:
whether ErsatzRS also emits and consumes that schema, which would position its
four scheduling engines as the advanced scheduler that ErsatzTV Next
deliberately declines to build.
