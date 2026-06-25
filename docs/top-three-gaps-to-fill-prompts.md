<!--
generated: 2026-06-25
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

Selection (2026-06-25, current repo state plus the healthcheck closeout in this
PR): `cargo xtask progress gaps` is clear, every tracked `[[debt]]` is resolved,
and the previous healthcheck and provider-import prompts are closed. Ranking:
(1) **in-app TSDuck MPEG-TS troubleshooting** — ADR 0013 remains Proposed / Not
implemented and is the highest-value Support diagnostic; (2) **Docker
dependency-layer caching** — still open in `phase.toml` and `plans/INDEX.md`,
with fresh evidence from the healthcheck Docker proof that the release image
spends about 24 minutes in the in-container Rust release build; (3) **startup
configuration cleanup** — the remaining Phase 7 TODO to replace direct
`DATABASE_URL` environment access with the repository configuration layer.

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

## 2. Build the next ErsatzRS build hardening chore: Docker dependency-layer caching (slug `docker-build-caching`)

Use the repo workflow from AGENTS.md. This is a **spec-backed build/runtime
hardening chore**, not an ErsatzTV parity port: it is governed by the open
`docs/progress/phase.toml` TODO and the `plans/INDEX.md` Deferred Contract Work
entry. The goal is to make Docker image builds stop recompiling the full
workspace after every source-only edit while preserving the release binary and
hydration-bundle contracts.

Before editing:
1. Read `Dockerfile`, `BUILD.md`, `RELEASING.md`, `.github/workflows/release.yml`,
   `docs/progress/phase.toml`, `plans/INDEX.md`, and
   `docs/reference/release-bundle.md`.
2. Inspect `.dockerignore`, workspace member manifests, `Cargo.lock`,
   `.cargo/config.toml`, the Leptos/Stylance build behavior, and Docker runtime
   asset copy paths.
3. Build a Source Map for dependency-manifest staging, source-copy invalidation,
   SQLX offline metadata, Leptos CSS/site assets, binary copy paths, and the
   runtime image's managed FFmpeg expectations.
4. Record the current timing baseline. The 2026-06-25 healthcheck proof saw
   `docker build --target runtime` spend `24m 02s` in the in-container Rust
   release build step before exporting the image.

Implement the slice end-to-end:
- Restructure the builder stage so dependency layers are keyed by manifests and
  lockfiles before copying the full source tree.
- Keep `SQLX_OFFLINE=true`, workspace features, release profile, and binary
  outputs equivalent to the current Dockerfile.
- Preserve the `ersatzrs-web` plus `ersatzrs-healthcheck` runtime image layout
  and the managed FFmpeg runtime behavior.
- Avoid committing host-specific cache paths, absolute maintainer paths, or
  machine-local `sccache` configuration.
- Update `docs/progress` and `CHANGELOG.md`; register a new progress slug if the
  build-cache behavior becomes a tracked runtime/build contract.

Verification:
- `docker build --target runtime --progress=plain -t ersatzrs-docker-cache-proof .`
  twice: one cold-ish build and one source-only rebuild after a harmless touched
  source/doc input, reporting both timings.
- `docker inspect` proves the runtime `HEALTHCHECK` still uses
  `ersatzrs-healthcheck`.
- `docker run --rm --entrypoint /usr/local/bin/ersatzrs-healthcheck
  ersatzrs-docker-cache-proof --help` proves the binary still runs.
- `cargo fmt --all -- --check`.
- `cargo clippy --workspace --all-targets -- -D warnings`.
- `cargo clippy --workspace --all-targets --features mysql -- -D warnings`.
- `cargo test --workspace --all-features`.
- `cargo xtask progress check`.

Report the Source Map, before/after Docker timings, changed cache boundaries,
runtime image proof, remaining Docker/GHCR release deferrals, verification run,
and changed files.

---

## 3. Build the next ErsatzRS startup chore: route `DATABASE_URL` through configuration (slug `startup-runtime-config`)

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
