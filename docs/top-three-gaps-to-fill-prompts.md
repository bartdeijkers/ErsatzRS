<!--
generated: 2026-06-11
last_processed_commit: 626783e2102890d07c779588e1133c283431e1e3
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
[`0d301df`](https://github.com/ErsatzTV/legacy/commit/0d301df5e8512fdfccf3a611188d27ab1690ee31)
(`remove external dependencies (bugsnag, trakt)`). Upstream work after that
commit is out of scope until the pin in
[`automation/parity-baseline.md`](automation/parity-baseline.md) is advanced.

Ranking this run (window `52e1028..626783e`): a backend/fix window. The
feature commits landed snapshot-covered fixes for the 2026-06-10
`ffmpeg-filter-graph` runtime debt (CUDA surface tracking after the
software-decode upload, still-image pipelines kept in software on the nvenc
path so the error screen renders instead of a bare 503, subtitle-burn
no-cue/Windows-path double-escape) and corrected `scanner` `VideoScanKind` to
ErsatzTV numbering; the rest was CI/release/Windows/docs. By the selection
rules every remaining gap is phase-named and `In progress` except
`remote-streams-browser` (`Not started`), so **foundational before leaf UI**
decides the top three: the two FFmpeg pipeline gaps (which both got fresh
runtime-relevant work and still carry live `ParityDebt`) plus the
app-wide-foundational `settings` config/i18n gap. The leaf-UI picks from the
prior run (`support-pages`, `multi-collections…`) saw no commits this window
and drop below the foundational pipeline/config gaps; they will resurface once
the foundational debt closes. Alphabetical order within the foundational tier:
`ffmpeg-command-generation`, `ffmpeg-filter-graph`, `settings`.

---

## 1 — FFmpeg command generation: live AV1 hardware-decode fallback (slug `ffmpeg-command-generation`)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg command generation — the
AV1 hardware-decode software fallback on the live HLS-segmenter path (slug
`ffmpeg-command-generation`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current remaining `ParityDebt` (quoted from
`docs/progress/parity/ffmpeg-command-generation.toml`): "despite the 2026-06-09
AV1 decode-fallback tightening above, the channel 4 HLS segmenter still exits
with code 69 / 'Your platform doesn't support hardware accelerated AV1
decoding' plus repeated `Error submitting packet to decoder: Function not
implemented`, CUDA context errors (`cuCtxPushCurrent ... CUDA_ERROR_INVALID_VALUE`),
and `Decode error rate 1 exceeds maximum` — the software-decode fallback for
unsupported AV1 hardware decode is not effective on the live segmenter path, so
AV1 sources fail instead of falling back like ErsatzTV."

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.FFmpeg/CommandGenerator.cs` (and the
NVENC/CUDA pipeline builder + hardware capability checker it drives). ErsatzRS
target: `crates/ersatzrs-ffmpeg` (decoder selection, capability gating, and the
NVENC software-fallback `hwupload` path) plus the segmenter spawn paths in
`crates/ersatzrs-infra`.

Before editing:
1. Read the ErsatzTV source above under `$ERSATZTV_SRC/` (at pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`), focusing on how ErsatzTV decides
   AV1 hardware decode is unsupported and how it routes those sources to a
   software input decoder while keeping the NVENC encoder.
2. Inspect the current ErsatzRS decoder-selection / capability-gating code, the
   `ersatzrs-hwprobe` AV1 detail rows, and the live HLS-segmenter spawn path —
   determine why the existing fallback is bypassed at runtime (the snapshot
   argv may already be correct while the live segmenter command is not).
3. Build a Source Map (decoder table, capability gate, NVENC fallback step,
   segmenter spawn, ErsatzRS target modules, intentional deviations).
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: make unsupported AV1 hardware decode
fall back to software input decode on the live segmenter path while preserving
NVENC encode, matching ErsatzTV's gating and argv. Add `insta` snapshot
coverage for the AV1 fallback argv on the segmenter command path, and (where
feasible) a focused regression that exercises the fallback decision, not just
the static snapshot. Update the parity file: keep visible any debt that cannot
be proven without live AV1 hardware.

Verify with focused `crates/ersatzrs-ffmpeg` tests (`cargo test -p
ersatzrs-ffmpeg`, including the pipeline snapshot suite) and
`cargo xtask progress check`. Report the Source Map, behavior implemented,
remaining `ParityDebt`, verification run, and changed files.

---

## 2 — FFmpeg filter graph: live NVENC/CUDA re-verification + dynamic graphics watermarks (slug `ffmpeg-filter-graph`)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg filter graph — re-verify
the NVENC/CUDA conversion-chain fixes on the live segmenter path and close the
resource-backed dynamic graphics watermark gap (slug `ffmpeg-filter-graph`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current parity state (from `docs/progress/parity/ffmpeg-filter-graph.toml`): the
NVENC/CUDA unconvertible-chain failures recorded on 2026-06-10 now have fixes
landed (window `52e1028..626783e`, snapshot-covered) — CUDA surface tracking
after the software-decode `hwupload` fallback feeds `scale_cuda` (so pad/crop/
subtitle-burn emit the required `hwdownload`), still-image pipelines kept in
software on the nvenc path (so the error screen renders instead of a bare 503),
and subtitle-burn no-cue / Windows-path double-escape robustness. Remaining
`ParityDebt` quoted from the summary: "these fixes are verified by pipeline
snapshots only; fresh live HLS-segmenter runtime proof on the NVENC/CUDA path
has not been re-captured this window, so the status stays `In progress` until
re-verified. Resource-backed dynamic graphics watermarks also remain tracked
under the graphics-engine plan."

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` (and the
hardware pipeline builders that insert hwdownload/hwupload/format transitions).
ErsatzRS target: `crates/ersatzrs-ffmpeg` (complex-filter label routing, the
hardware surface tracker, watermark/graphics overlay chaining).

Before editing:
1. Read the ErsatzTV source above under `$ERSATZTV_SRC/` (at pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`), focusing on hardware
   download/upload/format insertion around `scale_cuda`, `loop`, and overlay
   nodes, and on how dynamic (resource-backed) graphics watermarks are fed.
2. Inspect the current ErsatzRS filter-graph builder, the surface-location
   tracker, the existing pipeline snapshots for the still-image and CUDA paths,
   and the graphics-engine plan for dynamic watermarks.
3. Re-verify the landed fixes against a live NVENC/CUDA HLS-segmenter run if
   hardware is available; if not, record that the proof is snapshot-only and
   keep the debt visible.
4. Build a Source Map; identify remaining `ParityDebt` (it must stay visible).

Implement the coherent end-to-end slice: close the resource-backed dynamic
graphics watermark path to ErsatzTV parity (emit the `drawvg` node per the
graphics-engine divergence rule rather than an external renderer), and tighten
any remaining hardware conversion-chain transitions the snapshots do not yet
cover. Add `insta` snapshot coverage for the new/changed argv. Update the parity
file: flip to `Ported` only with live runtime proof; otherwise keep `In
progress` with the snapshot-only caveat.

Verify with focused `crates/ersatzrs-ffmpeg` tests (`cargo test -p
ersatzrs-ffmpeg`, including the pipeline snapshot suite) and
`cargo xtask progress check`. Report the Source Map, behavior implemented,
remaining `ParityDebt`, verification run, and changed files.

---

## 3 — Settings i18n full coverage (slug `settings`)

Fix the next ErsatzTV parity gap in ErsatzRS: Settings (FFmpeg / Logging /
HDHomeRun / Scanner / Playout / UI / XMLTV) — routes
`/settings/{ffmpeg,logging,hdhr,scanner,playout,ui,xmltv}` (slug `settings`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current parity state (from `docs/progress/parity/settings.toml`): the settings
routes are ported with ErsatzTV-shaped forms, raw HTML POST fallbacks backed by
`ConfigElement`, the ported save side effects (logging filter reload, FFmpeg
runtime/capability refresh, report-folder creation, subtitle extraction
queueing, playout rebuild queueing, XMLTV refresh), and a `FFmpegLocator` port
that resolves and persists the ffmpeg/ffprobe paths at startup and on save. The
persisted `pages.language` value drives the SSR shell and typed translations
across a large but not-yet-complete set of admin routes; ErsatzRS intentionally
adds an `nl` culture beyond ErsatzTV's en/pl/pt-br (ADR 0012). Remaining
`ParityDebt` quoted from the summary: "the FFmpeg settings inputs are still
`required`, so empty-means-auto-detect is exercised only by startup/the server
fn, not the form UI ... confirmed blocking in real use on 2026-06-10: the
settings page could not be saved until the FFmpeg path field was filled in
manually, even though the runtime had already auto-detected FFmpeg 8.1.1." Also:
"ErsatzRS started 3 concurrent transcode sessions for one viewing session, while
ErsatzTV caps concurrent transcode sessions at 2 by default — the session-limit
setting and its runtime enforcement are not ported." And: "Full admin-route
text coverage and four-language browser proof remain active i18n `ParityDebt`;
see `plans/settings-parity-slice.md`."

ErsatzTV source: `ErsatzTV/Pages/Settings*.razor`,
`$ERSATZTV_SRC/ErsatzTV.Core/Domain/ConfigElementKey.cs`. ErsatzRS target:
`crates/ersatzrs-web/src/admin/pages/settings.rs`,
`crates/ersatzrs-web/src/admin/server_fns/settings.rs`,
`crates/ersatzrs-web/src/admin/i18n.rs`,
`crates/ersatzrs-web/src/admin/languages/`.

Before editing:
1. Read the ErsatzTV `Settings*.razor` pages and `ConfigElementKey.cs` under
   `$ERSATZTV_SRC/` (at pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`), enumerating every visible label,
   helper, option, and validation message on each settings tab, plus the
   session-limit config key if porting that behavior.
2. Inspect the current ErsatzRS settings pages, server fns, `i18n.rs` routing
   glue, the per-culture `languages/` modules, and `plans/settings-parity-slice.md`.
3. Build a Source Map and a Visual Source Map for each settings route; list the
   admin routes still missing four-language text coverage.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: close the remaining admin-route text
coverage so every visible settings string is localized across
`en-us`/`pl`/`pt-br`/`nl`, keep `i18n.rs` as routing/helper glue (ADR 0012's
translation boundary, not a translation dump), relax the `required` FFmpeg
inputs so empty-means-auto-detect is exercised from the form, and — if in scope
— port the concurrent-transcode session-limit setting plus its runtime
enforcement. Preserve SSR/browser-safe fallbacks, follow `TEST.md` hydration
rules, add focused tests, and update the parity file (and
`plans/settings-parity-slice.md`).

Visual verification: serve via `./run.sh serve` after rebuilding Leptos assets,
open each touched settings route in each of the four cultures, and compare
same-viewport geometry and translated text against the ErsatzTV reference (field
labels, option lists, helper/validation text, clipped/overflowing strings).

Verify with `cargo xtask parity-check web` and `cargo xtask progress check`.
Report Source Map, Visual Source Map, behavior implemented, remaining
`ParityDebt`, verification run, and changed files.
