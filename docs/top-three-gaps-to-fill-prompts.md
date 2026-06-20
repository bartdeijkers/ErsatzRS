<!--
generated: 2026-06-20
last_processed_commit: c0d72c8ec91cb9c2da819ddf687e9ddec2df8bba
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

Current reconcile (2026-06-20, through `c0d72c8`): `cargo xtask progress gaps`
lists **1** remaining feature, so this file intentionally contains only one
actionable prompt:

- `hls-variant-playlists-segmenter`

The completed FFmpeg command-generation, media collection route-family, and HLS
active-transcode hard-ceiling prompts were removed per
`docs/automation/gap-prompt-rules.md`: completed scope is replaced, never kept
with a completed banner. ADR 0008 FFmpeg runtime acquisition (ship without
FFmpeg; download/update from the `bartdeijkers/ErsatzRS-ffmpeg` releases at
startup; surface update availability on `/system/health`) is still
parity-critical infrastructure, but it is not currently emitted by
`cargo xtask progress gaps`; refresh this file from progress data before
promoting it over the remaining open feature.

---

## 1 - HLS variant playlists / segmenter: capacity observability and pipeline-wide counting (slug `hls-variant-playlists-segmenter`)

Fix the next ErsatzRS streaming runtime-hardening gap: finish the remaining ADR
0016 active-transcode capacity follow-ups now that the HLS segmenter hard
ceiling is in place (slug `hls-variant-playlists-segmenter`).

Use the repo workflow from `AGENTS.md`. This is ADR-backed HLS runtime hardening
under ADR 0016, not a pinned-baseline ErsatzTV behavior gap, so do not force it
through `skills/parity-slice.md`. Read ADR 0016,
`plans/hls-transcode-capacity-and-admission.md`,
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, and the pinned
ErsatzTV HLS source only where it informs the existing segmenter/session
baseline.

Current `ParityDebt` from
`docs/progress/parity/hls-variant-playlists-segmenter.toml`:
- `active-transcode-observability-and-suggestion` (ADR 0016): the hard HLS
  ceiling exists, but the FFmpeg settings/troubleshooting pages still do not
  expose `Streams in use: X / N (peak Y)` or a hardware-aware suggested value
  from the existing capability probe.
- `pipeline-agnostic-transcode-capacity-counting` (ADR 0016): the ceiling
  currently covers HLS segmenter FFmpeg generations only. MPEG-TS legacy/hybrid
  transcodes still need to participate in the same capacity accounting before
  the setting can mean total streaming transcode capacity.

Before editing:
1. Inspect `crates/ersatzrs-infra/src/hls_worker.rs`,
   `crates/ersatzrs-infra/src/hls.rs`, `crates/ersatzrs-core/src/hls.rs`,
   `crates/ersatzrs-web/src/transport/iptv/hls.rs`, the `/channels` active
   transcode UI/status path, `crates/ersatzrs-web/src/transport/iptv/stream.rs`,
   FFmpeg settings/troubleshooting server functions, and existing HLS/stream
   tests.
2. Build a Source Map for HLS slot reservations -> active/peak snapshots ->
   FFmpeg settings/troubleshooting readouts -> hardware capability suggestion
   -> MPEG-TS direct/hybrid FFmpeg process startup/release -> active-session
   reporting.
3. Preserve the already-ported HLS segmenter and StopOnDisconnect lifecycle
   behavior; do not regress `cargo xtask smoke` or TSDuck playback assumptions.
4. Treat playable all-streams-in-use slate output as a follow-up unless this
   slice explicitly implements and runtime-proves it; do not hide it if you
   discover the current 503 error path is insufficient.

Implement the coherent remaining ADR 0016 slice: expose a process-local
active/peak capacity snapshot for the HLS ceiling, render it on FFmpeg settings
and troubleshooting in all supported admin languages, add a hardware-aware
suggested value without auto-applying it, and fold MPEG-TS legacy/hybrid
transcode processes into the same reservation/release accounting if the slice
can prove direct and wrapper streaming behavior end-to-end. If MPEG-TS counting
must remain separate, leave that as explicit `ParityDebt` with the proof gap and
close only the observability/suggestion debt.

Add focused tests for active/peak snapshot accounting, settings/troubleshooting
rendering, hardware suggestion selection, MPEG-TS reservation release on normal
exit and body-drop cancellation, and HLS segmenter regression coverage proving
the existing hard ceiling still holds.

Verify with focused HLS/stream/settings tests, `cargo xtask smoke`, the TSDuck
playback gate if any runtime streaming lifecycle changes, `cargo xtask
hydration-lint -p ersatzrs-web` if UI hydration changes, `cargo xtask progress
check`, and `cargo xtask progress gaps`. Update ADR 0016,
`plans/hls-transcode-capacity-and-admission.md`,
`docs/progress/parity/hls-variant-playlists-segmenter.toml`, the changelog, and
this prompt file only when evidence supports the new state. Report the Source
Map, snapshot/readout behavior, remaining `ParityDebt`, verification run, and
changed files.
