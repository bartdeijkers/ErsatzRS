<!--
generated: 2026-06-18
last_processed_commit: 04383968d96c96e1e0039ccd1b86697cda1ec9ef
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

Daily reconcile (2026-06-18, window `cfc5024..0438396`): 16 non-merge commits
merged. Most of the feature work in this window was already folded into the
prior pass's manual closeout addenda and self-documented by the commits
themselves: `fix(ffmpeg): prove opacity watermark clocks` (5f97230) closed
`ffmpeg-filter-graph` to `Ported`, `fix(scheduling): close classic filler
parity` (d30b418) closed `classic-scheduling`, `feat(admin): close scheduling
route parity` (735a896) closed `schedules-templates-decorations-deco-templates`,
and `fix(web): close support pages parity` (6065b00) closed `support-pages`.
All four dropped out of `cargo xtask progress gaps`.

New in this window beyond those closeouts:

- `feat(scanner): detect missing library items and skip them in playouts`
  (fdb22c0) ported ErsatzTV's previously-absent `MediaItemState` producer side
  (local `FlagFileNotFound` trailing pass; media-server item-id diffing to trash
  removed items and flag `RemoteOnly`; `playout.skip_missing_items` consumer
  gate). `scanner` stays `In progress` and gained a new structured debt,
  `media-server-unavailable-branch-unexercised`, recording that the `Unavailable`
  (State=2) flag path is wired but never taken because only movies/TV are synced
  and all three servers report remote-streaming support for those kinds.
- `feat(docs): track ADR implementation state separately from decision status`
  (320bc43) split ADR `## Status` (decision) from `## Implementation` (code
  state), migrated all 17 ADRs, linked debts via `adr=NNNN`, and added a
  `progress check` lint that fails when an ADR claims `Implemented` while a
  linked debt is open.
- `docs(status): revise ADR 0017 to seed five default FFmpeg profiles` (61e4fb2)
  and `docs: update ADR 0016/0017 …` (c1b040b, 0ba6ca4) retargeted the
  `default-profile-1080p60-retune` and HLS-capacity debts on
  `ffmpeg-command-generation` / `hls-variant-playlists-segmenter`; both stay
  `In progress`. The remaining release/Windows/server-path commits (cfcf906,
  f5cbeed, 8f4aefd, 0438396) and doc-status commits did not change parity status.

No parity status was advanced in this reconcile beyond what the commits already
recorded; `phase.toml` `next_steps` was trimmed to drop the now-`Ported` support
pages and to record that the FFmpeg/HLS items are ErsatzRS-specific deviations.

`cargo xtask progress gaps` now lists **4** features, all `In progress`:
`ffmpeg-command-generation`, `hls-variant-playlists-segmenter`,
`multi-collections-rerun-collections-playlists-filler-presets`, and `scanner`.
Per the updated **ADR-backed FFmpeg/HLS hardening** rule, the FFmpeg/HLS pair
may now consume a goal-prompt slot when pinned-baseline ErsatzTV gaps are drying
up. Its open debts are ErsatzRS-specific hardening/deviations carrying their own
ADRs
(`ac3-segmenter-max-delay`, `default-profile-1080p60-retune` under ADR 0017;
`active-transcode-limit-not-a-real-ceiling` under ADR 0016) plus a hardware-bound
NVENC live proof (`nvenc-filter-gate-live-proof`, folded into a future
encoder-capable verification session unless paired with behavior or release
packaging work). There are still **two** actionable pinned-baseline ErsatzTV
parity prompts below, and ADR 0008 fills slot 3 as manually promoted
parity-critical FFmpeg runtime infrastructure.

Ranking: `scanner` takes slot 1. It was worked on this window (re-ranked and
rewritten against its current three-debt state per the carry-over rule), is a
foundational backend gap that outranks leaf UI, and leads with its two
runtime-evidenced debts. `multi-collections-…-filler-presets` is slot 2,
carried over unchanged (no commits this window). Slot 3 is a manual maintainer
promotion for ADR 0008: completing first-party FFmpeg 8.1 release artifacts is
not a pinned-baseline ErsatzTV UI/backend gap, but it is parity-critical runtime
infrastructure because the current implementation is still `Partial` while
release payloads can depend on public "full" FFmpeg builds that may miss
`drawvg`, `zscale`, or acceptable version/configuration output.

---

## 1 - Scanner: real-world music-video NFO leniency, SQLite scan locking, and the media-server Unavailable branch (slug `scanner`)

Fix the next ErsatzTV parity gap in ErsatzRS: Scanner hardening for real-world
music-video NFO leniency, concurrent SQLite scan locking, and exercising the
media-server `Unavailable` state branch (slug `scanner`, debts
`music-video-nfo-fragment-leniency`, `sqlite-scan-locking`, and
`media-server-unavailable-branch-unexercised`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps. Read ErsatzTV
source only at the pinned commit
`0d301df5e8512fdfccf3a611188d27ab1690ee31`.

Lead with the two runtime-evidenced debts; the coverage debt follows.

Current `ParityDebt` from `docs/progress/parity/scanner.toml`:
- `music-video-nfo-fragment-leniency` (2026-06-10): "Local music-video NFO
  parsing rejects real-world files that ErsatzTV accepts, falling back to
  filename metadata." Evidence: "ErsatzRS.log: hundreds of `NFO did not contain
  a <musicvideo> root element` errors during music-video scans."
- `sqlite-scan-locking` (2026-06-10): "Concurrent scan and web activity can
  abort library scans with SQLite database locked errors instead of serializing
  work like ErsatzTV." Evidence: "ErsatzRS.log: library scan failed
  library_id=3/5 with SQLite code 5 database is locked."
- `media-server-unavailable-branch-unexercised` (2026-06-17): the `Unavailable`
  (State=2) flag path is wired but never taken because only movies/TV are synced
  from Plex/Jellyfin/Emby and all three report `ServerSupportsRemoteStreaming=true`
  for those kinds, so a missing local file always resolves to `RemoteOnly`
  (State=3). This is coverage/breadth debt, not a runtime failure.

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.Scanner/` at the pinned commit,
especially the local music-video scanner/NFO parsing path, the scan
coordination/serialization behavior around library scans, and the
`MediaItemState` producers for kinds that lack remote streaming. ErsatzRS
target: `crates/ersatzrs-scanner`,
`crates/ersatzrs-infra/src/{plex,jellyfin,emby,media_server}.rs`,
`crates/ersatzrs-infra/src/repositories/`, scanner/infra tests under
`crates/ersatzrs-scanner/tests/` and `crates/ersatzrs-infra/tests/`, and the
scanner parity file.

Before editing:
1. Read the pinned ErsatzTV scanner source for local music-video NFO parsing
   and fallback behavior, scan transaction boundaries and scan
   concurrency/serialization, and which media kinds drive the `Unavailable`
   (non-remote-streaming) state.
2. Inspect the current ErsatzRS local music-video scanner, NFO parser, provider
   sync write paths, the `flag_unavailable_media_server_items` callers, SQLite
   pool/transaction usage, and existing scanner/infra tests and fixtures.
3. Build a Source Map for music-video scan input -> NFO parse -> metadata row
   writes -> fallback metadata, a second for web/scanner concurrent database
   access and lock handling, and a third for the media-server item-state
   producer paths (which kinds can reach `Unavailable`).
4. Keep every remaining shortfall visible as structured `ParityDebt`, not prose.

Implement the coherent scanner hardening slice: make local music-video NFO
parsing accept the real-world fragments ErsatzTV accepts while preserving the
existing strict row-shape and fallback semantics; make concurrent scan plus web
activity serialize/retry consistently enough to stop aborting with SQLite
database-locked errors; and either exercise the `Unavailable` branch for a kind
ErsatzTV produces it for or keep the exact remaining shortfall as open debt with
a tightened description. Do not weaken metadata parity, path hashing, or
provider enum values to make fixtures pass. If a production log sample is needed
but unavailable, build a minimal fixture from the observed failure shape and
keep any unproven runtime breadth as structured `ParityDebt`.

Add focused scanner/infra regressions for the accepted NFO fragment shape, the
metadata written from that NFO, the fallback for truly invalid NFO, the SQLite
locking scenario, and any media-server state-producer path you change. If the
locking fix touches shared DB behavior, add the narrow repository or integration
coverage that proves it does not hide real errors or break MySQL feature builds.

Verify with focused `ersatzrs-scanner` and affected `ersatzrs-infra` tests,
`cargo test --workspace --all-features` if shared DB behavior changed broadly,
`cargo xtask progress check`, and `cargo xtask progress gaps`. Update
`docs/progress/parity/scanner.toml`, the changelog, and this prompt file only
when the evidence supports the new status. Report the Source Maps, behavior
closed, remaining `ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

---

## 2 - Multi-collections / Rerun collections / Playlists / Filler presets: close route-family parity (slug `multi-collections-rerun-collections-playlists-filler-presets`)

Fix the next ErsatzTV parity gap in ErsatzRS: finish and prove the media
collection route-family parity closeout for `/media/multi-collections`,
`/media/rerun-collections`, `/media/playlists`, and `/media/filler/presets`
(slug `multi-collections-rerun-collections-playlists-filler-presets`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps. Read ErsatzTV
source only at the pinned commit
`0d301df5e8512fdfccf3a611188d27ab1690ee31`.

Current status from
`docs/progress/parity/multi-collections-rerun-collections-playlists-filler-presets.toml`:
"In progress: `/media/multi-collections` now ports the ErsatzTV
multi-collection list plus add/edit/delete editor surface ... `/media/playlists`
now ports the playlist group/tree and playlist editor route ... Generated/system
playlists intentionally diverge from ErsatzTV's disabled-editor presentation:
ADR 0011 keeps scheduling semantics but renders generated rows as read-only
projections with clearer Count and Play All labels."

ErsatzTV source: `ErsatzTV/Pages/MultiCollections*.razor`,
`ErsatzTV/Pages/RerunCollections.razor`,
`ErsatzTV/Pages/RerunCollectionEditor.razor`,
`ErsatzTV/Pages/Playlists.razor`, `ErsatzTV/Pages/PlaylistEditor.razor`,
`ErsatzTV/Pages/FillerPresets.razor`, and
`ErsatzTV/Pages/FillerPresetEditor.razor` at the pinned commit. ErsatzRS
target: `crates/ersatzrs-web/src/admin/pages/media/`,
`crates/ersatzrs-web/src/admin/server_fns/media_browser.rs`,
`crates/ersatzrs-web/src/admin/server_fns/filler.rs`,
`crates/ersatzrs-web/src/api/{collections,playlists,rerun_collections,filler}.rs`,
and `crates/ersatzrs-web/src/admin/languages/`.

Before editing:
1. Read the pinned ErsatzTV Razor pages, editor models, validation/save
   handlers, delete/copy actions, and localization resources for all four
   route families.
2. Inspect the current ErsatzRS pages, server functions, raw HTML POST
   fallbacks, API handlers, language modules, existing browser smoke notes, and
   ADR 0011 generated-playlist deviation.
3. Build a Source Map and Visual Source Map for the list and editor routes,
   including add/delete/copy actions, page-size persistence, validation,
   generated/system playlist behavior, smart-collection preview, and playout
   refresh side effects.
4. Identify remaining `ParityDebt`; if the route family is already complete,
   prove it and close the tracker instead of inventing new scope.

Implement only the coherent closeout slice: fix any mismatch found in source
review or browser proof, preserve existing SSR/raw POST fallbacks and
localized text, keep ADR 0011's intentional generated-playlist deviation
explicit, and avoid unrelated media-browser refactors. Add focused tests for
any fixed persistence, validation, preview, delete/copy, or playout-refresh
behavior.

Add browser coverage that exercises the high-risk workflows across the route
family: create/edit/delete multi-collections, rerun collections, playlists and
playlist groups/items, and filler presets; verify visible geometry, hydrated
actions or fallback forms, localized labels, and no console errors. If one route
still cannot be proven, keep the exact missing workflow as structured
`ParityDebt`.

Verify with focused `ersatzrs-web` tests, `cargo xtask hydration-lint -p
ersatzrs-web`, the added browser spec or documented Playwright run,
`cargo xtask parity-check web`, and `cargo xtask progress check`. Update
`docs/progress/parity/multi-collections-rerun-collections-playlists-filler-presets.toml`,
changelog, and this prompt file only when the evidence supports the new status.
Report the Source Map, Visual Source Map, behavior closed, remaining
`ParityDebt`, verification run, and changed files.
Remember that ErsatzTV could have a less than optimal solution. If you have a
better way, please create an ADR, but only after careful review in depth.
update changelog, docs and /docs/top-three-gaps-to-fill-prompts.md when done.

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

## Allowed future hardening prompts: FFmpeg command generation / HLS segmenter (slugs `ffmpeg-command-generation`, `hls-variant-playlists-segmenter`)

These two `In progress` features remain in `cargo xtask progress gaps`. Their
open debts are **not** pinned-baseline ErsatzTV parity gaps, but they are now
allowed to consume goal-prompt slots as ADR-backed FFmpeg/HLS runtime hardening
when fewer than three stronger pinned-baseline parity prompts remain:

- `ac3-segmenter-max-delay` and `default-profile-1080p60-retune`
  (`ffmpeg-command-generation`) and `active-transcode-limit-not-a-real-ceiling`
  (`hls-variant-playlists-segmenter`) are ErsatzRS-specific streaming
  hardening/deviations tracked under ADR 0016 / ADR 0017 and
  [`plans/hls-transcode-capacity-and-admission.md`](../plans/hls-transcode-capacity-and-admission.md),
  not behavior present in ErsatzTV at the pin. They should be framed as runtime
  hardening prompts, not parity-porting prompts.
- `nvenc-filter-gate-live-proof` is hardware-bound verification debt; per the
  fold-hardware-bound-verification rule it should ride a future encoder-capable
  verification session or a broader FFmpeg/HLS hardening prompt rather than
  become proof-only scope. This machine still lacks an encoder-only/no-CUDA-filter
  FFmpeg build.

They stay tracked as visible `ParityDebt`; advance them through their ADRs,
plan, and daily goal prompts when selected.
