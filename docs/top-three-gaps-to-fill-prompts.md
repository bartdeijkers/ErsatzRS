<!--
generated: 2026-06-12
last_processed_commit: 641b21edd86f6f91819fbb0540d15892109caced
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

Ranking this run (window `626783e..641b21e`): a docs/workflow-only window —
**no feature behavior merged**. The five commits implemented the workflow
review gates (the `cargo xtask smoke` harness, review-gate sections across the
`skills/` contracts and parity files), added the gap-prompt refinement rules
and ADR 0014 (runtime-learned hardware-capability fallback), fixed the daily
routine's commit scope, and a `cargo fmt` whitespace cleanup whose only Rust
touch was blank-line removal in the emby/jellyfin/plex/scanner modules. None of
these advance, complete, defer, or drop a parity feature, so no parity status
flips this run and every live `[[debt]]` stays open.

The three selected gaps are therefore **carried over unchanged** from the
2026-06-11 run (per the gap-prompt selection rules: a prior pick that saw no
feature commits is kept and noted as carried over). The ranking is unchanged:
every remaining gap is phase-named and `In progress` except
`remote-streams-browser` (`Not started`), so **foundational before leaf UI**
keeps the two FFmpeg pipeline gaps and the app-wide-foundational `settings`
config/i18n gap on top; the leaf-UI candidates (`support-pages`,
`multi-collections…`, `channels-admin-ui`, `collections-smart-collections`)
remain below the foundational pipeline/config tier. Alphabetical order within
the foundational tier: `ffmpeg-command-generation`, `ffmpeg-filter-graph`,
`settings`.

Prompts 1 and 3 keep the refinement framing from the
[rules](automation/gap-prompt-rules.md): prompt 1 is diagnosis-first (its
2026-06-09 gate fix already failed live on 2026-06-10) and absorbs the
hardware-bound `nvenc-cuda-live-reverification` proof from
`ffmpeg-filter-graph`; prompt 3 leads with the two confirmed-blocking runtime
debts and defers the mechanical i18n coverage sweep to a follow-up slice
(tracked as open debt in `settings.toml`).

---

## 1 — FFmpeg command generation: diagnose and self-heal the live AV1 hardware-decode fallback (slug `ffmpeg-command-generation`)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg command generation — the
AV1 hardware-decode software fallback on the live HLS-segmenter path (slug
`ffmpeg-command-generation`, debt `av1-nvenc-live-fallback`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

**This is a repeat failure — diagnose before changing any gate.** A fix for
this exact debt already landed on 2026-06-09 (AV1 CUDA decode now requires a
positive native/probed NVIDIA decoder detail row), and the live failure
persisted on 2026-06-10. Current `ParityDebt` (quoted from
`docs/progress/parity/ffmpeg-command-generation.toml`): "the channel 4 HLS
segmenter still exits with code 69 / 'Your platform doesn't support hardware
accelerated AV1 decoding' plus repeated `Error submitting packet to decoder:
Function not implemented`, CUDA context errors (`cuCtxPushCurrent ...
CUDA_ERROR_INVALID_VALUE`), and `Decode error rate 1 exceeds maximum`". Do not
re-tighten the gate on snapshot proof alone — that loop already failed once.

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.FFmpeg/CommandGenerator.cs` plus the
hardware capability checker it drives — focus on the NVIDIA GPU
model/architecture table ErsatzTV uses to decide AV1 decode support. ErsatzRS
target: `crates/ersatzrs-ffmpeg` (decoder selection, capability gating), the
`ersatzrs-hwprobe` AV1 detail rows, and the supervisor/spawn paths in
`crates/ersatzrs-infra` (`hls_worker.rs`) and
`crates/ersatzrs-web/src/transport/iptv/stream.rs`.

Before editing:
1. Diagnose the live mismatch first: log or capture the exact spawned argv
   *and* the capability-report AV1 rows at spawn time on the failing channel
   (the live path reads the process-wide `ffmpeg_runtime` report). Determine
   which it is: the native `cuvidGetDecoderCaps` row over-reports relative to
   FFmpeg's own runtime check (profile/bit-depth/film-grain), the persisted
   report is stale, or the gate is bypassed on this path.
2. Read the ErsatzTV capability checker under `$ERSATZTV_SRC/` (at pinned
   commit `0d301df5e8512fdfccf3a611188d27ab1690ee31`), focusing on how the
   architecture table routes unsupported AV1 sources to software input decode
   while keeping the NVENC encoder.
3. Build a Source Map (decoder table, capability gate, architecture table,
   supervisor restart point, ErsatzRS target modules, intentional deviations).
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice, driven by the diagnosis: port
ErsatzTV's conservative architecture gate as an additional AND for NVIDIA
decode selection, fix any report staleness found, and implement the
runtime-learned fallback per
[ADR 0014](adr/0014-runtime-learned-hardware-capability-fallback.md) — on the
classified decode-unsupported stderr signature, restart the session exactly
once with software input decode while preserving NVENC encode, persist the
denial row, and surface it through the health-check engine. Add `insta`
snapshot coverage for the denial-driven argv and a focused regression for the
restart/fallback decision, not just the static snapshot.

Verify with focused `crates/ersatzrs-ffmpeg` tests (`cargo test -p
ersatzrs-ffmpeg`, including the pipeline snapshot suite), `cargo xtask smoke`,
and `cargo xtask progress check`. The live verification session on NVENC/CUDA
hardware must also re-capture the HLS-segmenter runtime proof that closes
`ffmpeg-filter-graph` debt `nvenc-cuda-live-reverification` (one hardware
session, two debts) — follow `docs/automation/runtime-triage.md` for the
evidence. Report the Source Map, diagnosis outcome, behavior implemented,
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
3. The live NVENC/CUDA re-verification (debt `nvenc-cuda-live-reverification`)
   is folded into prompt 1's verification session — it needs the same
   hardware. If that session has not run yet, record that the proof is
   snapshot-only and keep the debt visible.
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

## 3 — Settings: blocking runtime debts — FFmpeg path empty-means-auto-detect + concurrent transcode session limit (slug `settings`)

Fix the next ErsatzTV parity gap in ErsatzRS: Settings — the two
confirmed-blocking runtime debts on `/settings/ffmpeg` and the streaming
runtime (slug `settings`, debts `ffmpeg-path-empty-autodetect-form` and
`concurrent-transcode-session-limit`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Both debts are confirmed blocking in real use on 2026-06-10 (quoted from
`docs/progress/parity/settings.toml`):
- "the FFmpeg settings inputs are still `required`, so empty-means-auto-detect
  is exercised only by startup/the server fn, not the form UI ... the settings
  page could not be saved until the FFmpeg path field was filled in manually,
  even though the runtime had already auto-detected FFmpeg 8.1.1."
- "ErsatzRS started 3 concurrent transcode sessions for one viewing session,
  while ErsatzTV caps concurrent transcode sessions at 2 by default — the
  session-limit setting and its runtime enforcement are not ported." Note the
  excess sessions plausibly aggravate the CUDA context errors tracked in
  prompts 1–2, so this enforcement may independently reduce those failures.

The settings i18n full-coverage sweep and four-language browser proof are
**explicitly deferred** to a follow-up slice: keep debt
`settings-i18n-browser-proof` open in `settings.toml` and do not close or
shrink it in this slice.

ErsatzTV source: `ErsatzTV/Pages/Settings*.razor` (FFmpeg tab),
`$ERSATZTV_SRC/ErsatzTV.Core/Domain/ConfigElementKey.cs`, and the runtime
enforcement site where ErsatzTV caps concurrent transcode sessions. ErsatzRS
target: `crates/ersatzrs-web/src/admin/pages/settings.rs`,
`crates/ersatzrs-web/src/admin/server_fns/settings.rs`,
`crates/ersatzrs-web/src/admin/languages/`, and the transcode spawn/session
paths in `crates/ersatzrs-infra` and `crates/ersatzrs-web/src/transport/`.

Before editing:
1. Read the ErsatzTV FFmpeg settings page and `ConfigElementKey.cs` under
   `$ERSATZTV_SRC/` (at pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`), identifying the empty-path
   auto-detect behavior, the session-limit config key with its default of 2,
   and where ErsatzTV enforces the cap at session start.
2. Inspect the current ErsatzRS settings form/server fn, the `FFmpegLocator`
   startup auto-detect, and the streaming session spawn sites to determine
   where the limit must be enforced.
3. Build a Source Map and a Visual Source Map for `/settings/ffmpeg`.
4. Identify remaining `ParityDebt`; it must stay visible, not hidden in prose.

Implement the coherent end-to-end slice: relax the `required` FFmpeg path
inputs so an empty value saves and means auto-detect (matching the
startup/server-fn behavior already ported), localize any new or changed
strings across `en-us`/`pl`/`pt-br`/`nl` per ADR 0012's translation boundary,
and port the concurrent-transcode session-limit `ConfigElement` plus its
runtime enforcement so a new session beyond the cap is rejected/queued the way
ErsatzTV does. Preserve SSR/browser-safe fallbacks, follow `TEST.md` hydration
rules, add focused tests (including a regression for the session cap), and
update the parity file (and `plans/settings-parity-slice.md`).

Visual verification: serve via `./run.sh serve` after rebuilding Leptos
assets, open `/settings/ffmpeg` in each of the four cultures, save with empty
FFmpeg paths, and confirm the form saves and shows the auto-detected result.

Verify with `cargo xtask parity-check web`, `cargo xtask smoke` (the session
limit is streaming-runtime behavior), and `cargo xtask progress check`. Report
Source Map, Visual Source Map, behavior implemented, remaining `ParityDebt`,
verification run, and changed files.
