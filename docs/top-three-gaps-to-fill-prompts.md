<!--
generated: 2026-06-19
last_processed_commit: eb4dfe159af2dd9584378b50d552604fc8beeb21
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill - goal prompts

Three paste-and-run goal prompts, each closing **one** ErsatzTV parity gap or a
parity-critical runtime infrastructure gap. Auto-prioritized from `docs/progress`,
with manual maintainer promotions called out inline. Regenerated daily by the
[progress routine](automation/daily-progress-routine.md); see the
[rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Daily reconcile (2026-06-19, window `0438396..eb4dfe1`): 7 non-merge commits
merged. All parity-relevant feature work in this window was self-documented by
the commits themselves, so `docs/progress/` was already current; no parity
status changed:

- `fix(web): refresh scan-state admin views` (c676445) refreshed the hydrated
  libraries table, media-browser lists, `/media/trash`, and `/system/health`
  island so scanner-updated media state appears without a manual reload, and
  removed the non-ErsatzTV library-scan snackbar. It already updated the
  `library-management`, `media-browsers`, and `support-pages` summaries (all
  stay `Ported`) and added its own session line. This is admin/web UI work; it
  did **not** touch the `scanner` backend or its three open debts.
- `docs(status): record HDR tonemap proof debt` (91d2d7c) and
  `test(calibration): add HDR tone-map clip modes` (7cfdaa4) added the new
  `hdr-tonemap-calibration-live-proof` debt to `ffmpeg-command-generation`
  (stays `In progress`): HDR->SDR tonemapping matches ErsatzTV command/filter
  shape in snapshots and the calibration generator now emits HDR10 PQ / HLG
  masters plus SDR references, but no live ErsatzRS stream capture / pixel
  comparison has been run, so HDR stays command-generation/runtime debt.
- `feat(docs): add ADR 0018 for special features, alternate versions, and
  stream roles` (eb4dfe1) recorded ADR 0018 (`Status: Proposed`,
  `Implementation: Not implemented`) plus
  `plans/extras-alternate-versions-and-stream-roles.md`. Its parity slug
  `content-variants` is declared "to be created at first implementation slice",
  so no parity file exists yet and it is **not** an in-scope gap candidate
  (not in `cargo xtask progress gaps`, not FFmpeg/HLS hardening). Recorded in
  `phase.toml` `next_steps` as a planned future capability.
- The remaining commits did not change parity: `chore(cargo): use default
  target directory` (ce03b07), the automation-docs refinements
  (`docs(gap-prompt-rules)…` 8ba1efc), and the prior daily reconcile
  (`docs(status): daily reconcile … cfc5024..0438396` a3ae190).

`cargo xtask progress gaps` now lists **2** features after the 2026-06-19
route-family closeout:
`ffmpeg-command-generation` and `hls-variant-playlists-segmenter`; `scanner`
is now `Ported` after the 2026-06-19 hardening slice closed the music-video NFO
fragment, SQLite locking, and media-server `Unavailable` branch coverage debts,
and `multi-collections-rerun-collections-playlists-filler-presets` is now
`Ported` after the browser-proof/fallback closeout.
Per the **ADR-backed FFmpeg/HLS hardening** rule, the FFmpeg/HLS pair may
consume a goal-prompt slot when pinned-baseline ErsatzTV gaps are drying up.
Their open debts are ErsatzRS-specific hardening/deviations carrying their own
ADRs (`ac3-segmenter-max-delay`, `default-profile-1080p60-retune`, and
`hdr-tonemap-calibration-live-proof` under ADR 0017;
`active-transcode-limit-not-a-real-ceiling` under ADR 0016) plus a
hardware-bound NVENC live proof (`nvenc-filter-gate-live-proof`).

Ranking: completed scanner and media collection route-family scope is replaced.
`ffmpeg-command-generation` is the next live gap because its default-profile
and AC-3 segmenter debts are ADR-backed runtime hardening with direct
release/playback impact. The HLS segmenter remains the paired runtime-hardening
gap. The prior manual maintainer promotion for ADR 0008 (first-party FFmpeg 8.1
release artifacts) is still parity-critical runtime infrastructure, but it is
not a pinned-baseline ErsatzTV UI/backend gap; refresh this file from
`cargo xtask progress gaps` before starting another pasted goal.

---

## Completed 2026-06-19 - Multi-collections / Rerun collections / Playlists / Filler presets (slug `multi-collections-rerun-collections-playlists-filler-presets`)

This prompt slot is complete. The route family is now `Ported` in
`docs/progress/parity/multi-collections-rerun-collections-playlists-filler-presets.toml`.
The closeout added shared HTML POST fallback save/delete routes for
`/media/multi-collections`, kept ADR 0011's generated-playlist read-only UI
deviation explicit, and added
`tests/e2e/media-collection-route-family.spec.ts` to prove the
multi-collections, rerun collections, playlists, and filler presets route
surfaces with deterministic SQLite seed data, visible geometry checks,
localized English labels, fallback/hydrated controls, and no console/runtime
errors.

Run `cargo xtask progress gaps` before starting a new gap from this file; the
next live gap is now the FFmpeg command-generation hardening slice below.

---

## 2 - FFmpeg command generation: default profiles, AC-3 segmenter delay, and HDR live proof (slug `ffmpeg-command-generation`)

Fix the next ErsatzRS runtime-hardening gap in ErsatzRS: finish the ADR-backed
FFmpeg command-generation debts for default profile seeding, multichannel AC-3
segmenter delay, and HDR tonemap live proof (slug `ffmpeg-command-generation`,
debts `default-profile-1080p60-retune`, `ac3-segmenter-max-delay`, and
`hdr-tonemap-calibration-live-proof`). Fold `nvenc-filter-gate-live-proof` into
the validation run only if encoder-capable hardware/builds are available.

Use the repo workflow from `AGENTS.md`. This is ADR-backed FFmpeg/HLS runtime
hardening, not a pinned-baseline ErsatzTV behavior gap, so do not force it
through `skills/parity-slice.md`. Read ADR 0017, ADR 0014, ADR 0003,
`docs/progress/parity/ffmpeg-command-generation.toml`,
`plans/hls-transcode-capacity-and-admission.md` only for HLS interactions, and
the pinned ErsatzTV FFmpeg source where it defines the command/filter baseline.

Current `ParityDebt` from
`docs/progress/parity/ffmpeg-command-generation.toml`:
- `default-profile-1080p60-retune` (ADR 0017): fresh databases still seed one
  conservative `1920x1080 x264 aac` profile instead of five ready-to-use
  720p/1080p/1080p HDR/4K/4K HDR AC-3 profiles with seed-time hardware
  acceleration detection and profile #2 as the default.
- `ac3-segmenter-max-delay` (ADR 0017): HLS segmenter output does not emit
  `-max_delay 500000` for multichannel AC-3, so 6-channel AC-3 can pop or drift
  at segment boundaries.
- `hdr-tonemap-calibration-live-proof`: HDR->SDR command/filter shape is
  snapshot-covered and calibration clips exist, but no live ErsatzRS stream
  capture/pixel comparison has proven the path.

Before editing:
1. Inspect `crates/ersatzrs-infra/src/db.rs`, `crates/ersatzrs-domain/src/ffmpeg.rs`,
   `crates/ersatzrs-ffmpeg/src/pipeline/`, `crates/ersatzrs-ffmpeg/src/filter/`,
   calibration scripts, and current FFmpeg profile tests/snapshots.
2. Build a Source Map for fresh DB defaults -> hardware capability detection ->
   seeded profile rows -> command generation -> HLS segmenter output -> live
   HDR calibration proof.
3. Verify which parts are ADR 0017 deviations from ErsatzTV rather than
   pinned-baseline parity, and keep that distinction visible.
4. Keep hardware-bound or platform-bound shortfalls as structured `ParityDebt`,
   not prose.

Implement the coherent hardening slice: seed the five ADR 0017 profiles on
fresh databases without overwriting existing user profiles; set the default
profile pointer to the 1080p SDR profile; detect and apply the preferred
available hardware acceleration at seed time while preserving ADR 0014 runtime
fallback; emit `-max_delay 500000` only for multichannel AC-3 HLS segmenter
outputs; and run or script the HDR calibration live proof that streams HDR10 PQ
and HLG clips through ErsatzRS, decodes the output, and compares helper regions
against SDR references. Do not weaken existing command-generation snapshots or
replace proof with argv-only assertions.

Add focused tests/snapshots for the seeded profile rows, hardware-acceleration
selection fallback, AC-3 `-max_delay` gating, and HDR tonemap command paths.
If live HDR proof cannot run on the current machine, keep
`hdr-tonemap-calibration-live-proof` open with exact missing commands and
runtime prerequisites.

Verify with focused `ersatzrs-infra` DB seed tests, `ersatzrs-ffmpeg` snapshot
tests, the HDR calibration/live-stream proof when available, `cargo xtask smoke`
if streaming behavior changes, `cargo xtask progress check`, and
`cargo xtask progress gaps`. Update ADR 0017, `docs/progress/parity/ffmpeg-command-generation.toml`,
the changelog, and this prompt file only when evidence supports the new state.
Report the Source Map, behavior closed, remaining `ParityDebt`, verification
run, and changed files.

---

## 3 - ADR 0008: complete first-party FFmpeg 8.1 release artifacts and runtime validation

Fix the next ErsatzRS parity-infrastructure gap: complete ADR 0008's FFmpeg 8.1
runtime packaging work so every release artifact carries and validates the exact
FFmpeg/ffprobe runtime ErsatzRS requires for `drawvg`, HDR `zscale`/`tonemap`,
subtitles/text rendering, codec support, and startup capability reporting.

Use the repo workflow from `AGENTS.md`. This is not a pinned-baseline ErsatzTV
feature port, so do not force it through `skills/parity-slice.md` as if it were
admin or scanner behavior. The goal is to make the ADR 0008 approach
operationally stronger than ErsatzTV's separate `ErsatzTV-ffmpeg` repository:
single-repo release ownership, redistributable default artifacts, explicit
startup checks, and no hidden dependency on a user's system FFmpeg.

Current status from `docs/adr/0008-ffmpeg-8-1-runtime-and-multi-arch-images.md`:
`Status: Proposed`; `Implementation: Partial — the FFmpeg 8.1 runtime
requirement is detected and enforced at startup; multi-arch container images are
not yet produced.` The release workflow plan in
`plans/release-self-built-ffmpeg.md` records that public BtbN/Gyan-style builds
were too weak for ErsatzRS because they could miss `drawvg`, `zscale`, or
runtime-check-compatible version/configuration output.

Before editing:
1. Read ADR 0008, ADR 0005, `docs/explanation/deviations.md`,
   `plans/release-self-built-ffmpeg.md`, `docker/Dockerfile`,
   `docker-bake.hcl`, `.github/workflows/release.yml`,
   `crates/ersatzrs-ffmpeg/src/version.rs`, and current release documentation.
2. Inspect the existing runtime capability checks and confirm exactly which
   filters, configure flags, codecs, encoders/decoders, and hardware accelerators
   are required versus detected-only.
3. Compare the ErsatzTV custom-build lesson without copying its separate repo
   model: preserve ErsatzRS's redistributable default image policy, but make the
   shipped runtime just as deterministic as ErsatzTV's pinned FFmpeg contract.
4. Build a Source Map for release artifact creation -> bundled FFmpeg discovery
   -> startup capability report -> admin FFmpeg profile hardware presentation ->
   smoke/transcode proof.

Implement the coherent ADR 0008 closeout slice: build first-party FFmpeg 8.1
artifacts for all six release RIDs, stage them under
`ffmpeg/bin/{ffmpeg,ffprobe}[.exe]`, include the runtime library closure, update
runtime discovery so bundled binaries win after explicit environment overrides,
and remove public BtbN/Gyan download assumptions from release workflow and docs.
Keep published default artifacts redistributable: do not enable `--enable-nonfree`,
bundle CUDA/libnpp, or add non-redistributable codecs unless a new ADR accepts
that licensing/support model. If a GPU/nonfree variant is needed, keep it
separate and explicitly non-default.

Add validation that fails before artifact upload unless the relocated payload
passes parseable `ffmpeg -version` / `ffprobe -version`, `--enable-cairo`,
`drawvg`, `--enable-libzimg`, `zscale`, required codecs, a real `drawvg` smoke
transcode, and a real HDR-tagged `zscale`/`tonemap` smoke transcode. Preserve
PATH fallback for developer installs, but prove release archives run without a
system FFmpeg on clean Linux/macOS/Windows environments.

Verify with focused tests for bundled-path discovery and version parsing, the
release workflow's FFmpeg validation commands on every supported RID that can be
proven locally or in CI, Docker/image capability reports, `cargo xtask progress
check`, and a throwaway release/archive inspection before marking ADR 0008's
implementation complete. Update ADR 0008, release docs, changelog, and this
prompt file only when evidence supports the new state. Report the Source Map,
artifact matrix, validation results, remaining platform or hardware limitations,
and changed files.
Remember that ErsatzTV could have a less than optimal solution. If a better way
than ADR 0008 emerges during review, revise or supersede the ADR before changing
the implementation direction.

---

## Allowed future hardening prompts: HLS segmenter and hardware-bound FFmpeg proof

The remaining FFmpeg/HLS open debts are **not** pinned-baseline ErsatzTV parity
gaps, but they are allowed to consume goal-prompt slots as ADR-backed
FFmpeg/HLS runtime hardening when fewer than three stronger pinned-baseline
parity prompts remain:

- `active-transcode-limit-not-a-real-ceiling`
  (`hls-variant-playlists-segmenter`) is ErsatzRS-specific streaming
  hardening/deviation tracked under ADR 0016 and
  [`plans/hls-transcode-capacity-and-admission.md`](../plans/hls-transcode-capacity-and-admission.md),
  not behavior present in ErsatzTV at the pin. It should be framed as runtime
  hardening prompts, not parity-porting prompts.
- `nvenc-filter-gate-live-proof` is hardware-bound verification debt; per the
  fold-hardware-bound-verification rule it should ride a future encoder-capable
  verification session or a broader FFmpeg/HLS hardening prompt rather than
  become proof-only scope. This machine still lacks an encoder-only/no-CUDA-filter
  FFmpeg build.

They stay tracked as visible `ParityDebt`; advance them through their ADRs,
plan, and daily goal prompts when selected.
