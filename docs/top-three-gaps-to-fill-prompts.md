<!--
generated: 2026-06-26
last_processed_commit: 6e0bc4bc196d230ec2e089d12fa11449b67ecf52
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

Selection (2026-06-26, current repo state plus change window
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
refilled from the ranking. Current `main` also closed the
`ersatzrs-healthcheck` chore on 2026-06-25, so healthcheck is not carried
forward.

Ranking: (1) **in-app TSDuck MPEG-TS troubleshooting** — ADR 0013 remains
Proposed / Not implemented and is the highest-value Support diagnostic. (2)
**startup configuration cleanup** — the remaining Phase 7 TODO to replace
direct `DATABASE_URL` environment access with the repository configuration
layer. (3) **module split survey Tier 1 pilot** — the next active plan chore
after Docker caching is the behavior-preserving health-check module split from
`plans/module-split-survey.md`, which validates the repo's scalable registry
pattern on the lowest-risk oversized file before larger admin/server-function
and YAML scheduling splits.

Pin note (2026-06-26): the ErsatzTV `git clone` path is still blocked by the org
egress policy (`403` for `github.com/ErsatzTV/legacy`), but the pinned snapshot
was obtained via the `codeload.github.com` tarball at exactly
`0d301df5e8512fdfccf3a611188d27ab1690ee31` (the SHA is in the URL, so the
extracted tree is the commit's tree) and `$ERSATZTV_SRC` was pointed at it.
Parity for this run was therefore measured against the pin; the baseline was
**not** advanced.

---

## 1. Build the next ErsatzRS support tooling chore: in-app TSDuck MPEG-TS troubleshooting (slug `tsduck-mpeg-troubleshooting`)

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
   retention by default, repair tools, deeper HLS/fMP4 analysis, teletext
   analysis, and any normal playback-path dependency.

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

## 2. Build the next ErsatzRS startup chore: route `DATABASE_URL` through configuration (slug `startup-runtime-config`)

Use the repo workflow from AGENTS.md. This is a **spec-backed Phase 7 startup
cleanup**, not an ErsatzTV parity port: it is governed by the open
`docs/progress/phase.toml` TODO:
`std::env::var("DATABASE_URL") in main.rs uses #[allow(clippy::disallowed_methods)]
— replace with a config crate in Phase 1.` The goal is to remove the direct
environment read from startup without changing default database resolution,
Docker behavior, MySQL support, or local developer commands.

Before editing:
1. Read `docs/progress/phase.toml`, `ARCHITECTURE.md`, `STYLE.md`, `BUILD.md`,
   `docs/reference/configuration.md`, and `.cargo/config.toml`.
2. Inspect startup database URL resolution in `crates/ersatzrs-web`,
   `crates/ersatzrs-infra::db`, scanner startup, Docker `DATABASE_URL`, MySQL
   feature handling, tests that cover database URL defaults, and logging/config
   boundaries.
3. Build a Source Map for config precedence: defaults, environment variables,
   Docker defaults, CLI/binary startup, scanner startup, SQLite default home
   path, and MySQL feature gates.
4. Identify any user-visible config docs that must change, and decide whether a
   dedicated progress slug is warranted for the startup-runtime config contract.

Implement the slice end-to-end:
- Replace direct `std::env::var("DATABASE_URL")` startup access with the
  repository configuration layer or a small shared config helper that preserves
  the existing precedence and error messages.
- Keep default SQLite resolution unchanged when `DATABASE_URL` is unset.
- Keep Docker's `DATABASE_URL=sqlite:///data/.ersatzrs/ErsatzRS.db` behavior
  unchanged.
- Keep MySQL/MariaDB URL recognition behind the existing feature behavior.
- Remove the no-longer-needed `#[allow(clippy::disallowed_methods)]` for this
  specific startup access, without weakening the lint globally.
- Update configuration docs, `docs/progress`, and `CHANGELOG.md` if behavior or
  documented startup semantics change.

Verification:
- Focused tests for SQLite default, env override, non-UTF-8/invalid override,
  Docker-style URL, and MySQL/MariaDB URL mapping.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- `cargo xtask progress check`.

Report the Source Map, preserved config precedence, removed lint allowance,
Docker/Linux/Windows/macOS compatibility proof, verification run, remaining
startup-runtime TODOs, and changed files.

---

## 3. Build the next ErsatzRS refactor chore: split health checks into registry submodules (slug `module-split-survey`)

Use the repo workflow from AGENTS.md. This is a **spec-backed
behavior-preserving refactor chore**, not an ErsatzTV parity port: it is
governed by [`plans/module-split-survey.md`](../plans/module-split-survey.md).
Scope this slice to the Tier 1 pilot only:
`crates/ersatzrs-infra/src/health.rs` -> `crates/ersatzrs-infra/src/health/`.
Do not bundle the much larger `media_browser.rs` or `yaml.rs` splits into this
first PR.

Before editing:
1. Read `plans/module-split-survey.md`, `ARCHITECTURE.md`, `STYLE.md`, and
   `docs/how-to/testing.md`.
2. Inspect `crates/ersatzrs-infra/src/health.rs`,
   `crates/ersatzrs-core/src/health.rs`, health-check tests, and all callers of
   `make_health_check_service`.
3. Build a Source Map for the service factory registry, shared helpers,
   each concrete `HealthCheck` implementation, result persistence callers, and
   test seams.
4. List every public item and registered check before moving code so the
   refactor can prove the public surface and registered set are unchanged.

Implement the slice end-to-end:
- Convert `crates/ersatzrs-infra/src/health.rs` into
  `crates/ersatzrs-infra/src/health/mod.rs` plus
  `health/checks/<name>.rs`, with one concrete health-check struct and impl per
  file.
- Keep `make_health_check_service()` as the thin registry in `mod.rs`; preserve
  all check titles, IDs, severities, ordering, repository calls, and result
  mapping.
- Keep `HealthCheck` and `HealthCheckService` in `ersatzrs-core`; do not move
  trait or run-loop ownership across crate boundaries.
- Add a focused completeness test that proves every registered check has a
  non-empty title and that the registered set/order matches the pre-split
  contract.
- Avoid behavior changes, SQL changes, public import churn, new dependencies,
  or unrelated cleanup.
- Update docs/progress and `CHANGELOG.md` only if the refactor changes a tracked
  contract or materially improves maintainer workflow.

Verification:
- Focused `ersatzrs-infra` health-check tests.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- `cargo xtask progress check`.

Report the Source Map, before/after layout and line counts, preserved registered
health-check set, verification run, remaining Tier 1 split follow-ups, and
changed files.
