<!--
generated: 2026-06-21
last_processed_commit: 56fd5dd586a3ddc50a3c3f714491fc29eefae7ea
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Actionable paste-and-run goal prompts, each closing **one** ErsatzTV parity gap
or ADR-backed parity-critical runtime hardening gap. Auto-prioritized from
`docs/progress`, with manual maintainer promotions called out inline.
Regenerated daily by the [progress routine](automation/daily-progress-routine.md);
see the [rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Current reconcile (2026-06-21, through `56fd5dd`): `cargo xtask progress gaps`
lists **1** remaining feature with a single open debt, so this file
intentionally contains only one actionable prompt:

- `hls-variant-playlists-segmenter` (debt `all-streams-in-use-playable-slate`)

The 2026-06-21 closeout resolved the two prior open debts —
`active-transcode-observability-and-suggestion` (live `Streams in use: X / N
(peak Y)` readout + hardware-aware suggested value on FFmpeg
settings/troubleshooting) and `pipeline-agnostic-transcode-capacity-counting`
(MPEG-TS direct/legacy/hybrid spawns folded into the shared
`transcode_capacity` ceiling). Those prompts were removed per
`docs/automation/gap-prompt-rules.md`: completed scope is replaced, never kept
with a completed banner. The single remaining ADR 0016 implementation item is
the playable all-streams-in-use slate, prompted below. ADR 0008 FFmpeg runtime
acquisition (ship without FFmpeg; download/update from the
`bartdeijkers/ErsatzRS-ffmpeg` releases at startup; surface update availability
on `/system/health`) remains parity-critical infrastructure but is not emitted
by `cargo xtask progress gaps`; refresh this file from progress data before
promoting it over the remaining open debt.

---

## 1 - HLS / MPEG-TS over-capacity: playable "all streams in use" slate (slug `hls-variant-playlists-segmenter`)

Fix the next ErsatzRS parity-infrastructure gap: replace the over-capacity HTTP
503 with the playable, slot-free "all streams in use" slate promised by ADR
0016, so a marginal viewer joining a full transcode pool sees a short looping
clip that flips to real content the instant a slot frees (slug
`hls-variant-playlists-segmenter`, debt `all-streams-in-use-playable-slate`).

Use the repo workflow from `AGENTS.md`. This is ADR-backed HLS/streaming runtime
hardening under ADR 0016, not a pinned-baseline ErsatzTV behavior gap, so do not
force it through `skills/parity-slice.md`. Read
[`docs/adr/0016-active-hls-transcode-limit.md`](adr/0016-active-hls-transcode-limit.md)
(decision points 3.4 and 4 and the Implementation section),
[`plans/hls-transcode-capacity-and-admission.md`](../plans/hls-transcode-capacity-and-admission.md),
and `docs/progress/parity/hls-variant-playlists-segmenter.toml`. Read the pinned
ErsatzTV HLS source (`$ERSATZTV_SRC/ErsatzTV.Application/Streaming/HlsSessionWorker.cs`,
`$ERSATZTV_SRC/ErsatzTV/Controllers/IptvController.cs`,
`$ERSATZTV_SRC/ErsatzTV.FFmpeg/OutputFormat/OutputFormatHls.cs`) only where it
informs the existing slug black-video / silent-audio slate generation that this
slate should reuse. Confirm `git -C "$ERSATZTV_SRC" rev-parse HEAD` is the
pinned baseline before reading.

Current `ParityDebt` from
`docs/progress/parity/hls-variant-playlists-segmenter.toml`:
- `all-streams-in-use-playable-slate` (ADR 0016): when the transcode pool is
  full the over-capacity admission path still returns HTTP 503
  (`HLS_AT_CAPACITY_MESSAGE` / `SERVICE_UNAVAILABLE` in
  `crates/ersatzrs-web/src/transport/iptv/hls.rs` and `stream.rs`) instead of a
  short, looping slate that holds no transcode slot and switches to real content
  once a slot frees.

Before editing:
1. Inspect the over-capacity refusal paths in
   `crates/ersatzrs-web/src/transport/iptv/hls.rs` and
   `crates/ersatzrs-web/src/transport/iptv/stream.rs`, the shared ceiling and
   reclaim signalling in `crates/ersatzrs-infra/src/transcode_capacity.rs`, the
   HLS worker's existing slug black-video + silent-audio / error-ASS slate
   generation in `crates/ersatzrs-infra/src/hls_worker.rs`, and the
   `/channels` active-transcode status path.
2. Build a Source Map for over-capacity admission -> slate clip generation
   (reusing the existing black-video/silent-audio path, NOT a real content
   transcode slot) -> slate served over both the HLS segmenter playlist and the
   MPEG-TS direct/hybrid responses -> automatic switch to real content when a
   slot frees, without leaking or holding a `transcode_capacity` reservation.
3. Preserve the already-ported hard ceiling, reclaim ladder, watch/retention
   decoupling, and StopOnDisconnect lifecycle; the slate must hold **no** slot
   and must not let active counts exceed the configured limit. Do not regress
   `cargo xtask smoke` or TSDuck playback assumptions.

Implement the slate end-to-end: when admission cannot acquire a slot, serve a
slot-free looping "all streams in use" slate (reusing the existing slug
slate-generation primitive) on both the HLS segmenter and MPEG-TS paths, keep
the configured ceiling truthful while the slate plays, and flip the session to
real content the moment a slot frees. If any path must remain a 503 (e.g. a
genuinely unrenderable slate), do not hide it in prose: leave
`all-streams-in-use-playable-slate` open with the specific uncovered path
recorded as visible `ParityDebt`.

Add focused tests for: slate served on full-pool admission without incrementing
the active count past the limit, slate held no slot (snapshot active/peak
unchanged by a refused newcomer), automatic switch to real content on slot
release, and regression coverage proving the hard ceiling and reclaim ladder
still hold for both HLS and MPEG-TS.

Verify with focused HLS/stream/capacity tests, `cargo xtask smoke`, the TSDuck
playback gate (`scripts/e2e/tsduck-playback-gate.sh gate`) since this changes
runtime streaming lifecycle, `cargo xtask hydration-lint -p ersatzrs-web` if any
admin UI changes, `cargo xtask progress check`, and `cargo xtask progress gaps`.
Update ADR 0016, `plans/hls-transcode-capacity-and-admission.md`,
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, the changelog, and
this prompt file only when evidence supports the new state. Report the Source
Map, slate behavior, remaining `ParityDebt`, verification run, and changed files.
