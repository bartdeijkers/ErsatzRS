<!--
generated: 2026-06-24
last_processed_commit: f5eb4e50a7f8d1129791e6bc57c35c01fe57abd5
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md (ersatzrs-daily-progress)
rules: docs/automation/gap-prompt-rules.md
-->

# Top three chores to do - goal prompts

Actionable paste-and-run goal prompts, each closing **one** ErsatzTV parity gap,
ADR-backed runtime hardening gap, or **spec-backed chore** (the next slice of a
`Proposed` / `Not implemented` ADR, an active plan, or an open phase TODO).
Auto-prioritized across `docs/progress`, the ADR index, `plans/`, and
`phase.toml` per the [rules](automation/gap-prompt-rules.md); regenerated daily
by the [progress routine](automation/daily-progress-routine.md).

`cargo xtask progress gaps` is **one** input, not the whole backlog: it inspects
only whole-feature parity status and goes clear long before the specs are fully
implemented. The chores below come from the wider spec sweep (open debt,
unimplemented ADRs, active plans, and open phase TODOs) that the routine
performs.

**Parity baseline:** ErsatzTV *parity* chores are measured against ErsatzTV
commit
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31).
Spec-backed chores below are deliberate ErsatzRS decisions and may have no
ErsatzTV equivalent; each cites its governing ADR/plan/TODO.

Selection (2026-06-24): change window `52f28a1..f5eb4e5` is the single commit
`feat(core): add content variant search foundation` (ADR 0018 `content-variants`
Phase 0). That commit already self-reconciled `content-variants.toml`,
`phase.toml`, and `sessions.ndjson`, so this run advances the
`last_processed_commit` pointer over it and re-confirms the top three; no
parity status changed. `cargo xtask progress gaps` still reports the same three
open features: `hls-variant-playlists-segmenter`, `channels-admin-ui`, and
`content-variants`. None flipped to `Ported`/`Deferred`/`Dropped`, so the
completed-scope rule refilled no slots; the `content-variants` prompt continues
to target Phase 1 because Phase 0 is now done. Ranking: (1) **HLS segmenter
normalized append stutter** — named in `phase.toml` `current_focus` and backed
by user-visible playback evidence; (2) **Channels Edit Numbers** — confirmed
admin-route parity bug in an `In progress` pinned-baseline UI surface; (3)
**content-variants Phase 1 UI surfacing** — the next coherent slice of ADR 0018
after Phase 0, with remaining debt already structured under the slug.

Pin note (2026-06-24): the ErsatzTV upstream checkout was unreachable this run
(org egress policy returned `403` for `github.com/ErsatzTV/legacy` and
`github.com/ErsatzTV/ErsatzTV`), so the baseline could not be re-pinned. No
parity-file edits were made and the two pinned-baseline parity prompts (#1, #2)
are carried over unchanged rather than re-derived from upstream. This does not
affect this window: the only merged change is `content-variants` spec work
(ADR 0018), which has no ErsatzTV-parity dimension. A future window containing
real pinned-baseline parity work must not be reconciled until the pin is
reachable again.

---

## 1. Fix the next ErsatzRS parity/runtime gap: HLS segmenter normalized append stutter (slug `hls-variant-playlists-segmenter`)

Use the repo workflow from AGENTS.md and the parity/runtime guidance in
`skills/parity-slice.md`. The goal is a genuinely smooth HLS segmenter path:
the first normalized MPEG-TS append must not visibly stutter or emit an
unjustified discontinuity. Do not hide a partial proof in prose; if a runtime
shortfall remains, keep `segmenter-normalized-append-stutter` open in
`docs/progress/parity/hls-variant-playlists-segmenter.toml`.

Before editing:
1. Read the current `hls-variant-playlists-segmenter` parity file and its open
   `segmenter-normalized-append-stutter` debt.
2. Inspect `crates/ersatzrs-infra/src/hls_worker.rs`,
   `crates/ersatzrs-ffmpeg/src/pipeline/mod.rs`, and the playlist trimming /
   discontinuity tests around normalized append offsets.
3. Build a Source Map for HLS worker decision points, FFmpeg output flags,
   playlist writer/trim behavior, and the live validation scripts
   (`cargo xtask smoke`, `scripts/e2e/tsduck-playback-gate.sh`).
4. Diagnose why the current focused tests are not enough: capture the runtime
   segment/playlist shape and decide whether the fix is command generation,
   worker append state, playlist metadata, timestamp normalization, or proof
   coverage.

Implement the slice end-to-end:
- Fix the runtime path that still permits visible stutter around the first
  normalized append; preserve real discontinuity markers when content actually
  requires them.
- Add or tighten focused tests that fail on the runtime shape that caused the
  stutter, not only on a narrow argv snapshot.
- Update `docs/progress/parity/hls-variant-playlists-segmenter.toml`,
  `docs/progress/phase.toml`, and `CHANGELOG.md` only when the runtime proof is
  strong enough to change status/debt.

Verification:
- Focused HLS worker / FFmpeg pipeline tests for the changed path.
- `cargo xtask smoke`.
- `scripts/e2e/tsduck-playback-gate.sh gate` unless the environment lacks
  TSDuck; if skipped, keep the debt open and say exactly why.
- `cargo xtask progress check`.

Report the Source Map, root cause, behavior implemented, runtime proof, any
remaining `ParityDebt`, verification run, and changed files.

---

## 2. Fix the next ErsatzTV parity gap in ErsatzRS: Channels Edit Numbers route (slug `channels-admin-ui`)

Use the repo workflow from AGENTS.md and `skills/parity-slice.md`. This is a
pinned-baseline admin UI parity bug: the `/channels` Edit Numbers action should
open the ErsatzTV channel-number editor, but it currently lands on the Add
Channel page. The goal is a fully working list action and editor workflow with
SSR/browser-safe fallbacks; do not close the debt without visible browser proof.

Before editing:
1. Read `docs/progress/parity/channels-admin-ui.toml`, especially
   `channels-edit-numbers-routes-to-add`.
2. Read the ErsatzTV channel-number source under `$ERSATZTV_SRC` from the
   channel page family (`ErsatzTV/Pages/Channel*.razor` and related handlers /
   view models).
3. Inspect current ErsatzRS `/channels` routes, nav/action targets, channel
   server functions, repository save semantics, i18n strings, and HTML POST
   fallbacks.
4. Build both a Source Map and Visual Source Map for the list action,
   channel-number editor route, validation, persistence, redirects, and mobile
   layout.

Implement the slice end-to-end:
- Route `/channels/numbers` (or the ErsatzTV-equivalent target) to a real
  channel-number editor instead of the Add Channel editor.
- Port the expected fields/actions/validation/persistence and no-JS fallback
  semantics from ErsatzTV.
- Localize every new visible string in all supported cultures under
  `crates/ersatzrs-web/src/admin/languages/`.
- Add focused server-function/component tests and a Playwright regression that
  clicks Edit Numbers from `/channels` and proves the editor saves or rejects
  invalid input correctly.
- Update `docs/progress/parity/channels-admin-ui.toml`, `CHANGELOG.md`, and the
  prompt file if the debt ranking changes.

Visual verification:
- Rebuild Leptos assets, serve via `./run.sh serve`, and capture the
  `/channels` action plus channel-number editor at desktop and mobile widths.
- Compare layout, spacing, table/action behavior, and validation states against
  the ErsatzTV reference; do not claim parity complete with a visible mismatch.

Verification:
- Focused `ersatzrs-web` tests for the channel-number route and save path.
- `npx playwright test <added-or-updated-channels-spec>`.
- `cargo xtask parity-check web`.
- `cargo xtask progress check`.

Report the Source Map, Visual Source Map, behavior implemented, remaining
`ParityDebt`, verification run, and changed files.

---

## 3. Build the next ErsatzRS spec chore: content-variants Phase 1 UI surfacing (slug `content-variants`)

Use the repo workflow from AGENTS.md. This is a **spec-backed chore**, not an
ErsatzTV parity port: it is governed by
[ADR 0018](adr/0018-extras-alternate-versions-and-stream-roles.md) and
[`plans/extras-alternate-versions-and-stream-roles.md`](../plans/extras-alternate-versions-and-stream-roles.md).
Do **not** force `skills/parity-slice.md`; there is no pinned ErsatzTV UI to
port. The goal is the complete **Phase 1 — `/media/*` UI surfacing** slice on
top of the Phase 0 schema/search foundation.

Scope is **only Phase 1** from the plan. Do not implement scanner import,
alternate-version linking, commentary render acceptance, or scheduling examples
in this slice. Any shortfall must remain visible in
`docs/progress/parity/content-variants.toml`.

Before editing:
1. Read ADR 0018, the plan's Phase 1 section, and
   `docs/progress/parity/content-variants.toml`.
2. Inspect the current media browse/detail pages, shared media-card/list
   primitives, media browser server functions, search result/action surfaces,
   persisted page preferences, routes, and all four language modules.
3. Build a Source Map and Visual Source Map for the `/media/*` browse pages,
   detail pages, variant toggle, variant badges, Extras / Special Features
   sections, Versions sections, and search-field reference text.
4. Identify the remaining Phase 2-5 debt you are explicitly deferring.

Implement the slice end-to-end:
- Add the persisted "Show special features / versions" browse toggle, defaulted
  off so existing browse pages remain primary-only.
- Render variant badges and detail-page Extras / Special Features / Versions
  sections from the Phase 0 `ContentKind`, `SubKind`, and parent linkage.
- Document or surface the new smart-collection/search tokens where the existing
  UI exposes search-field help.
- Preserve SSR and no-JS fallbacks; follow TEST.md hydration rules for any
  island changes.
- Localize all new visible strings in every supported culture.
- Add focused server-function/component tests and Playwright coverage using
  seeded variant rows.
- Update `docs/progress/parity/content-variants.toml`, `CHANGELOG.md`, and the
  prompt file if the debt ranking changes.

Visual verification:
- Rebuild Leptos assets, serve via `./run.sh serve`, and verify representative
  movie/show/season/music media pages with variants hidden by default and shown
  when toggled.
- Capture desktop and mobile screenshots and compare layout to the existing
  media surfaces; do not ship overlapping or untranslated UI.

Verification:
- Focused `ersatzrs-web` tests for the media browse/detail server functions and
  persisted toggle.
- `npx playwright test <added-or-updated-content-variants-spec>`.
- `cargo xtask parity-check web`.
- `cargo xtask progress check`.

Report the Source Map, Visual Source Map, Phase 1 behavior implemented,
remaining Phase 2-5 `ParityDebt`, verification run, and changed files.
