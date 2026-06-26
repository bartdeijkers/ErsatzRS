<!--
generated: 2026-06-26
last_processed_commit: e06be38d0dbd6fb73c71190c879e102230b21d33
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

Selection (2026-06-26, change window
[`7bf064d..e06be38`](https://github.com/bartdeijkers/ersatzrs/compare/7bf064d...e06be38)):
the substantive commit in the window is `c978cd1` *"feat(infra): classify
provider content variants"*, which closed the previous pick-3
(`content-variants` `provider-extras-versions` debt): Plex/Jellyfin/Emby
movie-library sync now reuses a shared `ersatzrs-domain` content-variant
classifier after each provider page (including ETag-skipped rows), importing
server-reported extras as hidden `ContentKind=Extra` rows and linking provider
alternate versions to their canonical primary via `ParentMediaItemId`. That
commit already reconciled `content-variants.toml` (debt `resolved = true`,
summary + plan updated, session line appended); `cargo xtask progress gaps` is
clear, **every** tracked parity `[[debt]]` is `resolved = true`, and ADR 0018 is
now `Implemented`. The remaining window commits are tooling/dependency noise
(agent hooks, contributing-contract reviewer, dependabot bumps) with no parity
impact. The dead ADR-0018 provider-import prompt is removed and the slot
refilled from the ranking.

Ranking: (1) **ersatzrs-healthcheck binary** — open Phase 7 `phase.toml`
`known_todo` and top `plans/INDEX.md` deferred item (phase focus first); carried
over, no commits against it this window. (2) **in-app TSDuck MPEG-TS
troubleshooting** — ADR 0013 remains Proposed / Not implemented, a
maintainer-requested diagnostic capability and the only `Proposed`/`Not
implemented` ADR left; carried over, no commits against it this window. (3)
**module-split Tier-1 `infra/health.rs` registry split** — the next coherent
slice of the active `plans/module-split-survey.md` backlog (its own suggested
first/pilot item), replacing the now-closed ADR-0018 provider-import pick; a
foundational, behavior-preserving refactor of the health-check port-completeness
contract.

Pin note (2026-06-26): the ErsatzTV `git clone` path is still blocked by the org
egress policy (`403` for `github.com/ErsatzTV/legacy`), but the pinned snapshot
was obtained via the `codeload.github.com` tarball at exactly
`0d301df5e8512fdfccf3a611188d27ab1690ee31` (the SHA is in the URL, so the
extracted tree is the commit's tree) and `$ERSATZTV_SRC` was pointed at it.
Parity for this run was therefore measured against the pin; the baseline was
**not** advanced.

---

## 1. Build the next ErsatzRS runtime hardening chore: purpose-built healthcheck binary (slug `ersatzrs-healthcheck`)

Use the repo workflow from AGENTS.md. This is a **spec-backed Phase 7 runtime
hardening chore**, not an ErsatzTV parity port: it is governed by the open
Phase 7 TODO in `docs/progress/phase.toml` and the deferred-contract entry in
`plans/INDEX.md`. It will create a new tracked slug — register
`docs/progress/parity/ersatzrs-healthcheck.toml` in the same slice. The goal is
to replace the Docker image's current external tool health check with a small
`ersatzrs-healthcheck` binary that probes ErsatzRS readiness without requiring
shell, curl, or platform-specific TCP tricks in the runtime image.

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
- Update docs/progress (register the new slug), `plans/INDEX.md`, and
  `CHANGELOG.md` once the Docker health check no longer depends on external tools.

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

## 2. Build the next ErsatzRS support tooling chore: in-app TSDuck MPEG-TS troubleshooting (slug `tsduck-mpeg-troubleshooting`)

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
   retention by default, repair tools, deeper HLS/fMP4 analysis, teletext analysis
   and any normal playback-path dependency.

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

---

## 3. Build the next ErsatzRS spec chore: module-split Tier-1 `infra/health.rs` registry split (plan `plans/module-split-survey.md`)

Use the repo workflow from AGENTS.md. This is a **spec-backed refactoring
chore**, not an ErsatzTV parity port: it is governed by the active
[`plans/module-split-survey.md`](../plans/module-split-survey.md) backlog. Do
the **single first/pilot slice only** — candidate 3, `infra/health.rs` — not the
whole Tier-1 roadmap (the other two Tier-1 files, `media_browser.rs` and
`yaml.rs`, are separate future slices). The goal is a behavior-preserving
refactor with **no stopgaps**: zero logic, SQL, serialized-shape, or public-name
changes, mirroring the i18n split (`admin/i18n.rs` + `admin/languages/*.rs`) — a
thin registry/orchestrator in `mod.rs` re-exporting per-item submodules, plus a
completeness gate. Record any shortfall as visible plan/implementation debt
rather than hiding it in prose.

This is a deliberate ErsatzRS decision with **no ErsatzTV equivalent** — do not
invent an `etv_source`. The health-check engine is a port-completeness contract
(AGENTS.md "Health Check Parity") that must grow as parity expands, which is the
scaling argument for the split.

Before editing:
1. Read `plans/module-split-survey.md` (the reference pattern, candidate 3, the
   "Suggested order" and per-candidate verification sections), `ARCHITECTURE.md`,
   and `STYLE.md`.
2. Read `crates/ersatzrs-infra/src/health.rs` and map its sections: the
   `make_health_check_service()` factory/registry and each `HealthCheck` struct +
   `impl`. Confirm the `HealthCheck` trait and `HealthCheckService` run-loop live
   in `crates/ersatzrs-core/src/health.rs` and must **stay put**.
3. Enumerate the public surface (every `pub` item re-exported from the module) so
   you can prove it is preserved after the split.

Implement the slice end-to-end:
- Convert `crates/ersatzrs-infra/src/health.rs` → `infra/health/`: `mod.rs` keeps
  `make_health_check_service()` as the registry (the `ALL_TEXT_KEYS` analogue),
  shared imports/helpers, and `pub use` re-exports so every external import path
  stays identical.
- `checks/<name>.rs` — move each health-check struct + `impl HealthCheck`
  verbatim into its own file (e.g. `ffmpeg_version`, `ffmpeg_capabilities`,
  `ffmpeg_reports`, `hardware_acceleration`, `vaapi_driver`, `vaapi_device`,
  `file_not_found`, `unavailable`, `empty_schedule`, `zero_duration`,
  `movie_metadata`, `episode_metadata`, `downgrade`, `provider_connectivity`).
- Leave `crates/ersatzrs-core/src/health.rs` (trait + run-loop) untouched.
- Add a completeness gate: a test asserting every check the factory registers has
  a non-empty title and is runnable; the `health.results` / `health.summary`
  `ConfigElement` output must be unchanged.
- Match surrounding style; add no new dependencies.
- If the split cannot be fully completed in this slice, record the remaining
  scope in `plans/module-split-survey.md` rather than leaving it implicit.

Verification:
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`
  (feature-gated test files escape lint otherwise).
- `cargo build --workspace && cargo test --workspace`.
- Focused health proof: `cargo test -p ersatzrs-infra health` — the registered
  check set and `health.results` / `health.summary` output must not change.
- `cargo xtask progress check`.

Report the before/after layout and line counts, the preserved public surface
(proof nothing was renamed), the completeness-gate test added, the verification
runs, and changed files. Stop and report if any health output, registered-check
set, or test result changes — that means the refactor altered behavior.
