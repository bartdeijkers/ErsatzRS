<!--
generated: 2026-06-05
last_processed_commit: ef671bb02715144080a36e8c14e91895990d570d
parity_baseline_commit: 0d301df5e8512fdfccf3a611188d27ab1690ee31
generator: docs/automation/daily-progress-routine.md
rules: docs/automation/gap-prompt-rules.md
-->

# Top three gaps to fill — goal prompts

Three paste-and-run goal prompts, each closing **one** ErsatzTV parity gap.
Auto-prioritized from `docs/progress` (phase focus first, then `In progress`
before `Not started`, foundational before leaf UI). Regenerated daily by the
[progress routine](automation/daily-progress-routine.md); see the
[rules](automation/gap-prompt-rules.md). For the live gap list run
`cargo xtask progress gaps`.

**Parity baseline:** gaps are measured against ErsatzTV commit
[`0d301df`](https://github.com/ErsatzTV/ErsatzTV/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Ranking this run: all three carried over — the window (ef671bb) added only
docs/automation/ scaffolding; no feature parity commits landed. Phase 7 focus
leads with health/troubleshooting coverage (first item in `next_steps`), then
the foundational FFmpeg pipeline gaps that everything downstream depends on.

---

## 1 — Support pages: Health / Logs / Troubleshooting (slug `support-pages`)

Fix the next ErsatzTV parity gap in ErsatzRS: Support pages — routes
`/system/health`, `/system/logs`, `/system/troubleshooting` (slug
`support-pages`).

Use the repo workflow from `AGENTS.md` plus `.codex/skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current remaining ParityDebt (from `docs/progress/parity/support-pages.toml`):
dynamic nav badge counts and full log search/sort paging are not yet ported.
ErsatzTV source: `ErsatzTV/Pages/Index.razor`, `ErsatzTV/Pages/Logs.razor`,
`ErsatzTV/Pages/Troubleshooting/`. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/system/`.

Before editing:
1. Read the ErsatzTV source above under `$ERSATZTV_SRC/`, focusing on the log
   table paging/sort and the nav badge count sources.
2. Inspect the current ErsatzRS `system/` pages, routes, i18n languages, and
   `tests/e2e/support-pages.spec.ts`.
3. Build a Source Map (UI/pages, view models, handlers, repository/query paths,
   side effects, ErsatzRS target modules, intentional deviations) and a Visual
   Source Map for the touched admin routes.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: close log search/sort paging and the
dynamic nav badge counts to match ErsatzTV fields, actions, labels, validation,
persistence, and visible layout. Preserve SSR/browser-safe fallbacks, follow
`TEST.md` hydration rules, localize every new visible string across
`en-us`/`pl`/`pt-br`/`nl`, add focused tests, and update the parity file.

Visual verification: serve via `./run.sh serve` after rebuilding Leptos assets,
open each touched route in a fresh context, and compare same-viewport geometry
against the ErsatzTV reference (table column widths, pager placement, badge
positions) — not just DOM presence.

Verify with `cargo xtask parity-check web` and `cargo xtask progress check`.
Report Source Map, Visual Source Map, behavior implemented, remaining
`ParityDebt`, verification run, and changed files.

---

## 2 — FFmpeg command generation (slug `ffmpeg-command-generation`)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg command generation (slug
`ffmpeg-command-generation`). This is a backend/pipeline gap — no admin UI.

Use the repo workflow from `AGENTS.md` plus `.codex/skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current remaining ParityDebt (from
`docs/progress/parity/ffmpeg-command-generation.toml`): broader filter coverage
and deeper per-device hardware probes. ErsatzTV source:
`$ERSATZTV_SRC/ErsatzTV.FFmpeg/CommandGenerator.cs` (and the filter/codec
neighbors). ErsatzRS target: `crates/ersatzrs-ffmpeg`. See ADR 0003.

Before editing:
1. Read `CommandGenerator.cs` and the filter-graph/codec builders it drives,
   identifying the filters and per-device probe paths ErsatzRS does not yet
   emit.
2. Inspect the current `crates/ersatzrs-ffmpeg` builders and their `insta`
   snapshots.
3. Build a Source Map (ErsatzTV builders/filters, ErsatzRS target modules,
   intentional deviations); identify remaining `ParityDebt`.

Implement the slice: extend filter coverage and per-device hardware probing to
mirror the ErsatzTV `CommandGenerator` structure (preserve its filter-label
routing and concat handling; do not hand-roll a weaker substitute). Add or
update `insta` snapshot tests for every changed argv — that is the primary
regression gate. Update the parity file's remaining-work text.

Verify with the focused crate gates (`cargo test -p ersatzrs-ffmpeg`, then
`cargo clippy -p ersatzrs-ffmpeg --all-targets -- -D warnings`),
`cargo insta review`, and `cargo xtask progress check`.
Report Source Map, behavior implemented, remaining `ParityDebt`, the snapshot
diffs reviewed, verification run, and changed files.

---

## 3 — HLS variant playlists / segmenter (slug `hls-variant-playlists-segmenter`)

Fix the next ErsatzTV parity gap in ErsatzRS: HLS variant playlists / segmenter
(slug `hls-variant-playlists-segmenter`). Primarily backend/transport.

Use the repo workflow from `AGENTS.md` plus `.codex/skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current remaining ParityDebt (from
`docs/progress/parity/hls-variant-playlists-segmenter.toml`): fMP4 init-segment
cache parity that still matters after direct mode, then the broader Phase 7
items after HLS completion. ErsatzTV source:
`$ERSATZTV_SRC/ErsatzTV.FFmpeg/OutputFormat/OutputFormatHls.cs`,
`ErsatzTV/Controllers/IptvController.cs`,
`ErsatzTV.Application/Streaming/HlsSessionWorker.cs`,
`ErsatzTV.Core/FFmpeg/FFmpegStreamSelector.cs`. ErsatzRS target:
`crates/ersatzrs-web/src/transport/iptv/`, `crates/ersatzrs-core/src/hls.rs`,
`crates/ersatzrs-infra/src/{hls,hls_worker,playback}.rs`,
`crates/ersatzrs-ffmpeg`.

Before editing:
1. Read the ErsatzTV HLS output format and session worker, focusing on fMP4
   init-segment caching/lifetime.
2. Inspect the current ErsatzRS segmenter worker, session registry, and the
   `/iptv/session/{n}/{file}` + `#EXT-X-MAP`/`init.mp4` delivery path.
3. Build a Source Map; identify remaining `ParityDebt`.

Implement the slice: port fMP4 init-segment cache parity (lifetime, reuse,
cleanup) to match ErsatzTV, preserving the existing playlist-trim and
discontinuity behavior. Add focused crate/transport tests, including snapshot
coverage where the playlist/argv shape changes. Update the parity file.

Verify with the focused transport/crate gates and `cargo xtask progress check`.
If any IPTV playlist output is exercised live, capture a short runtime smoke as
evidence. Report Source Map, behavior implemented, remaining `ParityDebt`,
verification run, and changed files.
