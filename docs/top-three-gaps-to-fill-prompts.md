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

Selection (2026-06-24, after the Channels Edit Numbers closeout): the previous
#1 Channels route gap is now resolved and `channels-admin-ui` is back to
`Ported` after focused Rust tests, hydration smoke, and seeded Playwright proof.
`cargo xtask progress gaps` now reports only `content-variants` as an
in-progress parity/spec feature. The completed-scope rule refills from open
phase/spec TODOs. Ranking: (1) **content-variants Phase 1 UI surfacing** — the
next coherent slice of ADR 0018 after Phase 0, with remaining debt already
structured under the slug; (2) **ersatzrs-healthcheck binary** — open Phase 7
runtime hardening TODO to replace the Docker image's tool-based health check
with a purpose-built binary; (3) **in-app TSDuck MPEG-TS troubleshooting** —
ADR 0013 remains Proposed / Not implemented and would turn the manual TSDuck
gate into an optional Support tool.

Pin note (2026-06-24): the ErsatzTV upstream checkout was unreachable this run
(org egress policy returned `403` for `github.com/ErsatzTV/legacy` and
`github.com/ErsatzTV/ErsatzTV`), so the baseline could not be re-pinned. No
new pinned-baseline parity prompt was re-derived from upstream. This manual
closeout removes the completed Channels prompt and refills with spec-backed
chores that have no ErsatzTV baseline dimension. A future window containing new
pinned-baseline parity work must not be reconciled until the pin is reachable
again.

---

## 1. Build the next ErsatzRS spec chore: content-variants Phase 1 UI surfacing (slug `content-variants`)

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

---

## 2. Build the next ErsatzRS runtime hardening chore: purpose-built healthcheck binary (slug `ersatzrs-healthcheck`)

Use the repo workflow from AGENTS.md. This is a **spec-backed Phase 7 runtime
hardening chore**, not an ErsatzTV parity port: it is governed by the open
Phase 7 TODO in `docs/progress/phase.toml` and the deferred-contract entry in
`plans/INDEX.md`. The goal is to replace the Docker image's current external
tool health check with a small `ersatzrs-healthcheck` binary that probes
ErsatzRS readiness without requiring shell, curl, or platform-specific TCP
tricks in the runtime image.

Before editing:
1. Read `Dockerfile`, `docs/progress/phase.toml`, `plans/INDEX.md`,
   `ARCHITECTURE.md`, and `BUILD.md`.
2. Inspect the existing `/healthz` route, startup health/readiness reporting,
   release archive binary layout, Docker runtime stage, and CI/release Docker
   checks.
3. Build a Source Map for health endpoint semantics, binary packaging, Docker
   `HEALTHCHECK`, release archive inclusion, and cross-platform build targets.
4. Decide whether the binary should call only `/healthz` or also expose a local
   diagnostics mode; keep the first slice narrow unless the contracts require
   more.

Implement the slice end-to-end:
- Add an `ersatzrs-healthcheck` binary in the existing workspace style with no
  secrets, no local-path assumptions, and no OS-specific behavior outside
  documented `cfg` guards.
- Have it probe the configured host/port or URL with sensible defaults for the
  Docker image (`127.0.0.1:8409/healthz`) and return standard process exit
  codes for healthy/unhealthy states.
- Update the Dockerfile to copy and use the new binary in `HEALTHCHECK`,
  keeping the runtime image compatible with Linux Docker and the existing
  managed FFmpeg runtime.
- Include the binary in release/archive packaging if the release scripts expect
  all supported service binaries to ship together.
- Update docs/progress, `plans/INDEX.md`, and `CHANGELOG.md` once the Docker
  health check no longer depends on external tools.

Verification:
- Focused tests for argument/default URL handling and exit-code mapping.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- A Docker build or the repo's Docker check script, proving the image contains
  and runs `ersatzrs-healthcheck`.
- At least `cargo check` or the existing platform matrix script for Linux,
  Windows, and macOS targets touched by the new binary.
- `cargo xtask progress check`.

Report the Source Map, implemented healthcheck behavior, Docker proof,
Windows/Linux/macOS compatibility proof, remaining runtime-hardening TODOs,
verification run, and changed files.

---

## 3. Build the next ErsatzRS support tooling chore: in-app TSDuck MPEG-TS troubleshooting (slug `tsduck-mpeg-troubleshooting`)

Use the repo workflow from AGENTS.md. This is a **spec-backed diagnostic
extension**, not an ErsatzTV parity port: it is governed by
[ADR 0013](adr/0013-tsduck-mpeg-troubleshooting.md) and
[`plans/mpeg-troubleshooting-tsduck.md`](../plans/mpeg-troubleshooting-tsduck.md).
The manual `scripts/e2e/tsduck-playback-gate.sh` gate exists, but it does not
satisfy the in-app Support tool. Normal IPTV, HLS, HDHomeRun, FFmpeg spawning,
startup readiness, Docker, and `/healthz` must not depend on TSDuck.

Scope v1 narrowly to channel-level MPEG-TS diagnostics at
`/system/troubleshooting/mpeg`. Do not add repair/mutation tools, replace
production streams, bundle TSDuck into Docker/release images, or implement
non-MPEG-TS container analysis in this slice.

Before editing:
1. Read ADR 0013, `plans/mpeg-troubleshooting-tsduck.md`,
   `docs/progress/phase.toml`, `plans/INDEX.md`, `ARCHITECTURE.md`, `STYLE.md`,
   and `TEST.md`.
2. Inspect the current IPTV/HLS MPEG-TS output paths, troubleshooting page
   routes, archive/sample helpers, health-check engine, language modules, and
   the existing manual TSDuck playback gate.
3. Build a Source Map for command construction, bounded sample capture,
   report parsing, archive writing, Support route rendering, health reporting,
   and redaction boundaries.
4. Identify which follow-ups remain out of scope: TSDuck packaging, raw sample
   retention by default, repair tools, deeper HLS/fMP4 analysis, and any normal
   playback-path dependency.

Implement the slice end-to-end:
- Add a small TSDuck diagnostic service that detects `tsp --version`, builds
  structured `tsp`/`tsbitrate` argv without a shell, and reports
  `NotAvailable` without failing startup.
- Capture bounded MPEG-TS samples for a selected channel/mode/duration, with a
  20 second default, 120 second maximum, sample-size guard, unique run id, and
  explicit raw-sample archive opt-in.
- Run read-only analysis with `continuity`, `pcrverify`, and `analyze --json`;
  parse PAT/PMT/service/PID summaries, continuity findings, PCR findings, and
  optional bitrate estimates.
- Produce a structured, redacted report and archive metadata, excluding raw
  `.ts` samples unless the user opted in.
- Add `/system/troubleshooting/mpeg`, link it from the Support Tools panel, and
  render an SSR-safe form plus latest report tables/excerpts.
- Localize every new visible label, status, help text, and error in all
  supported cultures.
- Add a non-blocking health-check row that reports TSDuck availability as
  informational or not applicable.
- Update docs/progress, plans/ADR implementation status if accepted, and
  `CHANGELOG.md` when the tool is usable.

Verification:
- Focused tests for argv construction, detection, report parsing, redaction,
  status mapping, and archive contents.
- Focused web tests for the Support link, `/system/troubleshooting/mpeg` SSR
  form, missing-TSDuck `NotAvailable`, and non-MPEG-TS `NotApplicable` states.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- A Playwright or browser proof for the troubleshooting route.
- `cargo xtask progress check`.

Report the Source Map, diagnostic behavior implemented, unsupported/missing
TSDuck behavior, archive/redaction proof, verification run, remaining
diagnostic follow-ups, and changed files.
