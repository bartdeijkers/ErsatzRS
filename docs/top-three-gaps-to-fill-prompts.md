<!--
generated: 2026-06-20
last_processed_commit: 417f9f25cc7fd25f960bd3226d390664523dfb22
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

Current reconcile (2026-06-20, through `417f9f2`): `cargo xtask progress gaps`
lists **1** remaining feature, so this file intentionally contains only one
actionable prompt:

- `hls-variant-playlists-segmenter`

The completed FFmpeg command-generation and media collection route-family
prompts were removed per
`docs/automation/gap-prompt-rules.md`: completed scope is replaced, never kept
with a completed banner. ADR 0008 FFmpeg runtime acquisition (ship without
FFmpeg; download/update from the `bartdeijkers/ErsatzRS-ffmpeg` releases at
startup; surface update availability on `/system/health`) is still
parity-critical infrastructure, but it is not currently emitted by
`cargo xtask progress gaps`; refresh this file from progress data before
promoting it over the remaining open feature.

---

## 1 - HLS variant playlists / segmenter: hard active transcode capacity (slug `hls-variant-playlists-segmenter`)

Fix the next ErsatzRS streaming runtime-hardening gap: make
`ffmpeg.segmenter.active_transcode_limit` a real ceiling for actively watched
HLS segmenter sessions (slug `hls-variant-playlists-segmenter`, debt
`active-transcode-limit-not-a-real-ceiling`).

Use the repo workflow from `AGENTS.md`. This is ADR-backed HLS runtime hardening
under ADR 0016, not a pinned-baseline ErsatzTV behavior gap, so do not force it
through `skills/parity-slice.md`. Read ADR 0016,
`plans/hls-transcode-capacity-and-admission.md`,
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, and the pinned
ErsatzTV HLS source only where it informs the existing segmenter/session
baseline.

Current `ParityDebt` from
`docs/progress/parity/hls-variant-playlists-segmenter.toml`:
- `active-transcode-limit-not-a-real-ceiling` (ADR 0016): the current priority
  bypass increments the active-transcode counter without an upper bound for
  recently accessed sessions, and every playlist/segment request calls
  `touch()`. With K viewers on K channels and limit N, K-N sessions can
  force-reserve, yielding K concurrent transcodes regardless of N.

Before editing:
1. Inspect `crates/ersatzrs-infra/src/hls_worker.rs`,
   `crates/ersatzrs-infra/src/hls.rs`, `crates/ersatzrs-core/src/hls.rs`,
   `crates/ersatzrs-web/src/transport/iptv/hls.rs`, the `/channels` active
   transcode UI/status path, and existing HLS worker tests.
2. Build a Source Map for playlist/segment touch -> watched/cooling state ->
   capacity reservation -> work-ahead reclamation -> preemption/admission ->
   error/slate response -> active-session reporting.
3. Preserve the already-ported HLS segmenter and StopOnDisconnect lifecycle
   behavior; do not regress `cargo xtask smoke` or TSDuck playback assumptions.
4. Keep MPEG-TS pipeline-agnostic counting or hardware-aware default tuning as
   visible future debt unless this slice proves it end-to-end.

Implement the coherent ADR 0016 slice: replace the unbounded priority bypass
with a hard capacity ceiling; decouple the watch window from idle retention;
stop work-ahead first, reclaim cooling sessions next, preempt the stalest
watched session only when configured behavior allows it, and otherwise serve an
"all streams in use" slate/error path that holds no active transcode slot. Keep
the active count accurate during cancellation, process exit, temp-dir cleanup,
playlist polling, and segment fetches.

Add focused tests for limit N with K>N watched channels, playlist/segment touch
not bypassing the cap, work-ahead reclamation before watched-session preemption,
slot release on cancellation/drop, and active-session/status reporting. Add or
update browser/UI coverage only if the visible `/channels` readout or controls
change.

Verify with focused HLS worker tests, `cargo xtask smoke`, the TSDuck playback
gate if segmenter lifecycle behavior changes, `cargo xtask progress check`, and
`cargo xtask progress gaps`. Update ADR 0016,
`plans/hls-transcode-capacity-and-admission.md`,
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, the changelog, and
this prompt file only when evidence supports the new state. Report the Source
Map, admission/preemption behavior, remaining `ParityDebt`, verification run,
and changed files.
