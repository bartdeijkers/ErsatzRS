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

Ranking this run: manual maintainer regeneration (outside the daily routine,
which next reconciles `641b21e..HEAD`) after the 2026-06-12 merges — the
filter-graph watermark slice (`9460f1d`: SetWatermark loop options, the
NCalc opacity-expression translator, live NVENC/CUDA HLS proof closing
`nvenc-cuda-live-reverification`) and the nvenc decode CUDA-filter-gate debt
triage (`a9e957e`). The previous run had wrongly kept a completed prompt as a
"for reference" tombstone; per the updated
[rules](automation/gap-prompt-rules.md), completed scope is now always
replaced so every slot stays actionable.

Selection is unchanged in *slugs* but rewritten in *scope*: every remaining
gap is phase-named and `In progress` except `remote-streams-browser`
(`Not started`), so **foundational before leaf UI** keeps the two FFmpeg
pipeline gaps and the app-wide-foundational `settings` config/i18n gap on top.
Prompt 1 is regenerated against `ffmpeg-command-generation`'s four open
ADR 0014 debts (its prior `av1-nvenc-live-fallback` scope closed in
`3661a41` with live proof on 2026-06-11). Prompt 2 is regenerated against
`ffmpeg-filter-graph`'s single remaining debt (the watermark and live-proof
scope merged in `9460f1d`). Prompt 3 (`settings`) is carried over unchanged —
its two blocking debts saw no commits.

---

## 1 — FFmpeg command generation: close the four ADR 0014 hardware-decode follow-ons (slug `ffmpeg-command-generation`) (completed)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg command generation — the
four open ADR 0014 hardware-decode debts (slug `ffmpeg-command-generation`,
debts `nvenc-decode-cuda-filter-gate`, `iptv-ts-learned-denial`,
`nvidia-architecture-gate`, `denial-refresh-invalidation`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

**Lead with the confirmed-blocking runtime debt**
`nvenc-decode-cuda-filter-gate` (quoted from
`docs/progress/parity/ffmpeg-command-generation.toml`): "NVENC hardware decode
is not gated on the CUDA filters the GPU graph emits
(`scale_cuda`/`yadif_cuda`/`overlay_cuda`) being present in the FFmpeg build.
On an encoder-only build (NVENC encoders + `cuda` hwaccel, no
`--enable-cuda-nvcc`/`--enable-cuda-llvm` filter kernels) the pipeline picks
hardware decode and fails at runtime with 'No such filter', and the ADR 0014
learned-denial stderr signatures do not match that error, so it never
self-heals." A working gate existed on the abandoned
`fix/nvenc-gpu-acceleration` branch (commit `b1246b0`, recoverable from
reflog); its other changes are superseded by the landed surface-tracking
approach — recover the gate, not the branch.

ErsatzTV source: `$ERSATZTV_SRC/ErsatzTV.FFmpeg/CommandGenerator.cs` plus the
hardware capability checker it drives — including the conservative NVIDIA GPU
model/architecture table for decode support. ErsatzRS target:
`crates/ersatzrs-ffmpeg/src/capabilities.rs` (`nvidia_decode`),
`crates/ersatzrs-web/src/transport/iptv/stream.rs` (direct `.ts` spawns), and
the capability-report refresh sites in
`crates/ersatzrs-web/src/host/bootstrap.rs` and
`crates/ersatzrs-web/src/admin/server_fns/settings.rs`.

Before editing:
1. Read [ADR 0014](adr/0014-runtime-learned-hardware-capability-fallback.md)
   in full — it is the contract all four debts trace to.
2. Read the ErsatzTV capability checker under `$ERSATZTV_SRC/` (at pinned
   commit `0d301df5e8512fdfccf3a611188d27ab1690ee31`), focusing on the
   architecture table and how unsupported sources route to software input
   decode while keeping the NVENC encoder.
3. Inspect the landed HLS-segmenter learned-denial slice (classification,
   one-shot retry, `ConfigElement` persistence) — the `.ts` port must mirror
   it, not reinvent it.
4. Build a Source Map; identify remaining `ParityDebt` (it must stay visible).

Implement the coherent end-to-end slice:
- Gate NVENC hardware decode on `report.has_filter` for the CUDA filters the
  GPU graph will emit, falling back to software input decode while preserving
  NVENC encode (closes `nvenc-decode-cuda-filter-gate`).
- Port the learned-denial loop to direct IPTV `.ts` spawns: classify
  decode-unsupported stderr, retry once with software input decode, persist
  the denial row (closes `iptv-ts-learned-denial`).
- Port ErsatzTV's conservative NVIDIA architecture gate as an additional AND
  for decode selection when GPU model information is available (closes
  `nvidia-architecture-gate`).
- Clear learned denial rows when the capability report is explicitly
  refreshed (FFmpeg path change, settings-save refresh, manual refresh)
  instead of reapplying persisted denials to fresh reports (closes
  `denial-refresh-invalidation`).

Add `insta` snapshot coverage for the newly gated argv and focused regressions
for the `.ts` retry decision and the denial-invalidation-on-refresh behavior.

Verify with focused `crates/ersatzrs-ffmpeg` tests (`cargo test -p
ersatzrs-ffmpeg`, including the pipeline snapshot suite), `cargo xtask smoke`,
and `cargo xtask progress check`. Live proof of the filter gate needs an
encoder-only FFmpeg build; if none is available this session, keep that
caveat as visible debt rather than flipping it closed on snapshots alone.
Report the Source Map, behavior implemented, remaining `ParityDebt`,
verification run, and changed files.

---

## 2 — FFmpeg filter graph: real clock semantics for opacity-expression time variables (slug `ffmpeg-filter-graph`) (completed)

Fix the next ErsatzTV parity gap in ErsatzRS: FFmpeg filter graph — give the
watermark opacity-expression time parameters their ErsatzTV clock semantics
(slug `ffmpeg-filter-graph`, debt
`opacity-expression-time-variable-semantics`).

Use the repo workflow from `AGENTS.md` plus `skills/parity-slice.md`.
The goal is fully functional ErsatzTV parity without stopgaps.

Current `ParityDebt` (quoted from
`docs/progress/parity/ffmpeg-filter-graph.toml`): "All opacity-expression time
parameters translate to the FFmpeg frame timestamp `T`: `time_of_day_seconds`
loses its wall-clock (seconds-since-midnight) meaning, `content_seconds`
ignores the seek/in-point offset, `channel_seconds` ignores channel uptime,
and `content_total_seconds` maps to `NAN`. ErsatzTV evaluates these per frame
in its graphics engine with real wall-clock/content/channel clocks." The
2026-06-12 slice already landed the NCalc-to-FFmpeg-eval translator, the
`SetWatermark` loop options, and the live NVENC/CUDA HLS proof (debt
`nvenc-cuda-live-reverification` is closed) — do not redo that work.

ErsatzTV source:
`$ERSATZTV_SRC/ErsatzTV.Infrastructure/Streaming/Graphics/OpacityExpressionHelper.cs`
(`GetOpacity` binds four distinct clocks per frame). ErsatzRS target:
`translate_identifier` in `crates/ersatzrs-ffmpeg/src/filter/simple.rs` plus
whatever pipeline/request context must carry the seek in-point, probed content
duration, channel start time, and spawn wall clock into the filter builder.

Before editing:
1. Read `GetOpacity` under `$ERSATZTV_SRC/` (at pinned commit
   `0d301df5e8512fdfccf3a611188d27ab1690ee31`) and list exactly which clock
   each parameter binds.
2. Inspect the translator, the existing opacity-expression snapshots, and
   [ADR 0005](adr/0005-drawvg-graphics-engine.md) — this debt exists
   because ErsatzRS evaluates expressions inside native FFmpeg filters rather
   than a per-frame graphics engine.
3. Decide the binding per identifier: spawn-time constant offsets added to
   `T` cover `content_seconds` (in-point + `T`), `channel_seconds` (uptime at
   spawn + `T`), and `time_of_day_seconds`
   (`mod(seconds-since-midnight-at-spawn + T, 86400)`); the probed duration
   covers `content_total_seconds`. Where a binding genuinely cannot be
   expressed in FFmpeg eval, record it as an explicit ADR 0005 deviation, not
   a silent `NAN`.
4. Build a Source Map; identify remaining `ParityDebt` (it must stay visible).

Implement the coherent end-to-end slice: thread the required clock context
into the filter builder, translate each time identifier to its real-semantics
expression, and update or extend ADR 0005 for any intentional residual
deviation. Add `insta` snapshot coverage per identifier (including the
in-point/seek case) and a focused regression for the midnight wraparound.

Verify with focused `crates/ersatzrs-ffmpeg` tests (`cargo test -p
ersatzrs-ffmpeg`, including the pipeline snapshot suite) and
`cargo xtask progress check`. Flip the parity status only with live runtime
proof; otherwise keep `In progress` with the snapshot-only caveat. Report the
Source Map, behavior implemented, remaining `ParityDebt`, verification run,
and changed files.

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
