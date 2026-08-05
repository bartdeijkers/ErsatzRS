# AGENTS.md

This file provides guidance to AI coding agents when working in this repository.
`CLAUDE.md` is a symlink to this file.

## Project

ErsatzRS is a from-scratch Rust reimplementation of [ErsatzTV](https://github.com/ErsatzTV/legacy) - a mediaserver that turns personal media libraries (Plex/Jellyfin/Emby/Local) into linear TV channels served over IPTV (M3U/HLS) and HDHomeRun. The goal is full feature parity with ErsatzTV on a simpler deployment (single static binary) while exploiting modern FFmpeg features that ErsatzTV does not yet use.

ErsatzRS is internationally oriented. Subtitle and alternate-audio track
handling, selection, metadata, health reporting, and admin management are
high-priority parity surfaces; preserve them carefully when changing scanning,
media metadata, playback selection, scheduling, troubleshooting, or UI flows.

The authoritative project state lives across these sources - read the
relevant ones before starting non-trivial work:

- **`cargo xtask progress next`** for the current phase, focus, in-progress
  features, and latest session. The structured source of truth is
  [`docs/progress/`](docs/progress/README.md) (per-feature parity, route and
  hydration coverage, session log); [STATUS.md](STATUS.md) is now a slim
  pointer at it. Prefer the `progress` queries over reading large files.
- **[ARCHITECTURE.md](ARCHITECTURE.md)** for the crate layout and
  dependency rules.
- **[plans/INDEX.md](plans/INDEX.md)** for locked decisions, the contract
  index, and deferred work.

## Supported Platforms

ErsatzRS targets the same set of platforms as ErsatzTV. All six must build and run correctly:

| Rust target triple | RID | OS | Architecture |
|---|---|---|---|
| `x86_64-unknown-linux-gnu` | `linux-x64` | Linux | x86-64 |
| `aarch64-unknown-linux-gnu` | `linux-arm64` | Linux | AArch64 |
| `armv7-unknown-linux-gnueabihf` | `linux-arm` | Linux | ARMv7 |
| `x86_64-apple-darwin` | `osx-x64` | macOS | x86-64 |
| `aarch64-apple-darwin` | `osx-arm64` | macOS | Apple Silicon |
| `x86_64-pc-windows-msvc` | `win-x64` | Windows | x86-64 |

**Platform parity rule:** No Rust source file may hard-code an OS or architecture assumption
outside of a `#[cfg(target_os = …)]` / `#[cfg(target_arch = …)]` guard, and every such
guard must have a correct counterpart for all other supported platforms or must be
explicitly documented as intentional (e.g. a Unix-only integration test that is skipped on
Windows). Leptos `#[cfg(target_arch = "wasm32")]` island splits are exempt — they are
required by the SSR/WASM hydration model and do not affect native targets.

**Intentional platform exception — `crates/ersatzrs-launcher`:** this is a
Windows-only system-tray launcher (ships in the win-x64 release as `ErsatzRS.exe`).
Its real logic is under `#[cfg(windows)]`; on every other target it compiles to a
no-op `main`, so `cargo build --workspace` stays green everywhere. It is the single
crate that opts out of the workspace `unsafe_code = "forbid"` lint (its `Cargo.toml`
omits `[lints] workspace = true`), for a few small Win32 FFI calls with no safe
wrapper: console control to send a graceful Ctrl+C to the server on "Exit", and a
`MessageBoxW` to report a fatal startup failure (the launcher has no console). This
is the launcher's documented counterpart to the Unix-only-test carve-out above.

## Current status

**Phase 7 — Hardening and Parity Completion** (Phases 0–6 complete). The Cargo
workspace, FFmpeg pipeline, playout scheduling engines, media source scanning,
IPTV/HDHomeRun endpoints, and the admin UI slices through Trakt list
integration are all in place. The current startup/runtime parity work now
includes shared database URL resolution, explicit mediaserver bootstrap for
default startup data, initial readiness plus downgrade/schema checks wired
into startup health reporting, and the first non-web ErsatzTV health-check
engine slice in `ersatzrs-core::health` / `ersatzrs-infra::health`. Keep the
health-check engine current whenever porting work changes FFmpeg capabilities,
hardware acceleration support, schema readiness, media item state handling,
metadata persistence, schedules, or provider availability; it is an explicit
indicator for ErsatzTV port completeness. For the live focus and remaining
gaps, run `cargo xtask progress next` (the authoritative source) rather than
relying on a fixed priority here; structured detail lives in
[`docs/progress/`](docs/progress/README.md).

When adding code, follow the workspace layout in
[ARCHITECTURE.md](ARCHITECTURE.md).

## Commands

always respect the build target-dir defined in `/config.toml` or `/.cargo/config.toml`.
this could mean elevated rights are needed. ask/apply elevated rights, since the path
is added on purpose. Never use a different cargo build path.

> **Stale-binary trap:** the real binary lives under the configured target-dir
> (resolve it from `config.toml` / `.cargo/config.toml`). A leftover binary at
> the default `./target/debug/` path will still run but silently mislead — always
> invoke and inspect the binary under the configured target-dir.

```bash
# Build
cargo build --workspace

# Lint (mirror CI: two passes, neither with --all-features, so feature-gated
# `#![cfg(not(feature = "mysql"))]` test files don't escape lint).
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo clippy --workspace --all-targets --features mysql -- -D warnings

# Test closeout (nextest for unit/integration tests, then Cargo doctests)
./run.sh test-closeout

# Inner-loop verification
# Use focused commands while iterating; reserve the full parity gate for
# closeout. If your target-dir override points at a slow mount (this project
# once used a 9p scratch disk at /mnt/f, where Rust's many-small-files I/O is
# ~20-100x slower than ext4), each recompile is especially expensive; the
# default ./target on ordinary local disk applies unless you've added your
# own override. Cargo keys its cache on (features x targets x profile x
# check-vs-build), so flipping --features / --all-features / --all-targets /
# the wasm target between runs re-runs rustc over the whole tree. Keep the loop
# in ONE configuration:
#   ./run.sh check -p <crate>        # fast type-check, no codegen/link
#   ./run.sh test -p <crate>         # required focused/local test runner
# The checked-in `.config/nextest.toml` runs through all failures, performs no
# retries, and reports tests that exceed 60 seconds. Agent hooks block direct
# `cargo test` commands: use `./run.sh test-closeout` for the canonical local
# gate, which runs nextest for unit/integration tests and Cargo only for
# doctests. CI intentionally retains its direct Cargo test selection.
#   ./run.sh dev                     # build scanner companion, then Leptos watch
# Run the full multi-config gate (two clippy passes, ./run.sh test-closeout,
# cargo leptos build, cargo xtask parity-check) ONCE at closeout, not per edit.
# sccache (cache on ext4) makes the unavoidable config switches cheap, but it is
# the maintainer's local shell optimization and is enabled per-shell/run.sh, not
# machine-wide — agents need not (and under a sandboxed uid cannot) enable it, so
# do not set RUSTC_WRAPPER yourself. See BUILD.md "Inner-loop strategy". For
# admin UI work, start with route/module filters such as:
#   ./run.sh test -p ersatzrs-web <route_or_module_filter>
#   cargo clippy -p ersatzrs-web --all-targets -- -D warnings
#   cargo xtask hydration-lint -p ersatzrs-web
# Run `cargo xtask parity-check web` once before claiming admin UI parity
# complete, and rerun it after broad dependency, feature, route, or shared
# primitive changes. If a focused check misses important parity coverage, add a
# targeted regression instead of repeatedly using the full gate as the only
# signal.
# For streaming/runtime parity, run `cargo xtask smoke` locally before marking
# playback-sensitive work done. It boots the real web host against a temporary
# SQLite database, seeds a tiny local library, probes IPTV MPEG-TS and HLS
# endpoints, and fails on known FFmpeg/segmenter runtime signatures.

# For deeper playback-correctness confirmation, run the TSDuck end-to-end
# playback gate (a required manual gate, NOT part of CI; pairs with `smoke`):
#   scripts/e2e/tsduck-playback-gate.sh gate
# It boots ersatzrs-web on a throwaway DB, seeds the calibration clips
# (scripts/calibration/, auto-generated on first run and gitignored) as a cyclic
# playout, then captures and TSDuck-validates the served streams — continuity-
# counter/PCR/PTS integrity, PSI structure, audio/video PIDs — across the
# MPEG-TS (legacy/hybrid) and HLS (segmenter/direct) pipelines, and exits
# non-zero on any failure. `baseline` validates the static clips and `url
# <stream>` validates an already-running channel. See scripts/e2e/README.md.

# Run the full mediaserver (web UI + IPTV/HDHomeRun endpoints) on SQLite.
# Defaults to ~/.ersatzrs/ErsatzRS.db when DATABASE_URL is unset.
# Startup resolves or downloads the managed ErsatzRS-ffmpeg runtime; pending
# migrations run automatically.
# The admin UI's interactive buttons require Leptos hydration assets in
# crates/ersatzrs-web/target/site/pkg. Build them first with:
#   cargo install cargo-leptos --locked
#   (cd crates/ersatzrs-web && cargo leptos build)
cargo run --bin ersatzrs-web
# HYDRATION BUILD CONSISTENCY: the SSR binary and the WASM bundle must come from
# a consistent build, or the admin UI renders but never hydrates (every island
# button/dialog dead) — the recurring "wasm checksum mismatch" that has broken
# multiple releases. `cargo leptos build` then a SEPARATE `cargo run/build --bin`
# can drift (esp. file-hashing); prefer `./run.sh dev` (cargo leptos watch) or
# `./run.sh serve`, which keep them in lockstep. Before tagging a release run the
# gate `scripts/release/verify-web-bundle.sh` (also enforced in release CI). See
# BUILD.md "Hydration build consistency (the wasm checksum trap)".

# Verify the server is up.
curl http://127.0.0.1:8409/healthz

# Run the full mediaserver with IPTV JWT enforcement enabled.
JWT__IssuerSigningKey=dev-secret cargo run --bin ersatzrs-web

# Run the full mediaserver against MySQL/MariaDB.
DATABASE_URL='mysql://user:pass@127.0.0.1:3306/ersatzrs' cargo run --features mysql --bin ersatzrs-web

# Run a one-shot media scan against the same database.
# Defaults to ~/.ersatzrs/ErsatzRS.db when DATABASE_URL is unset.
cargo run --bin ersatzrs-scanner -- --provider local --library-id 1

# The ignored scanner integration tests need LD_LIBRARY_PATH pointed at the
# FFmpeg 9 libs, or they silently skip exercising ffprobe:
#   LD_LIBRARY_PATH=<ffmpeg-9>/lib ./run.sh test -p ersatzrs-scanner --run-ignored ignored-only

# Dependency audit
cargo deny check

# Snapshot review (after test changes). cargo-insta is already installed, so use
# it directly (no `cargo install cargo-insta` step needed). Use `cargo insta
# review` to inspect changes interactively, or `cargo insta accept` to accept
# all pending `.snap.new` snapshots non-interactively.
cargo insta review

# Keep target/ pruned to avoid slow cold rebuilds. ROUTINE maintenance is
# cargo-sweep, which prunes only stale artifacts and preserves the warm cache:
#   cargo install cargo-sweep && cargo sweep --time 30
# Run `cargo clean` once `target/` grows beyond 40 GiB; an oversized target/
# (100+ GB) is what makes cold builds feel like "10+ min";
# a healthy target/ rebuilds cold in ~2.5 min. Swapping the linker does NOT
# help (mold trialed, no gain — see BUILD.md). Between 40 GiB cleanups, use
# `cargo sweep` for routine stale-artifact pruning. `./run.sh dev|build|serve`
# and `cargo xtask parity-check` check target/ size before expensive work.
cargo sweep --time 30   # routine; `cargo clean` when target/ exceeds 40 GiB
```

Non-Rust checks available now:

```bash
# Verify contract files exist
test -f README.md && test -f CONTRIBUTING.md && test -f STYLE.md && test -f ARCHITECTURE.md && test -f PORTING.md

# Commitlint (requires Node.js)
npx commitlint --from HEAD~1
```

Porting progress lives in `docs/progress/` (text source of truth) with a
gitignored, auto-rebuilt `progress.db` index. Query it instead of reading large
status files:

```bash
cargo xtask progress next                 # phase, focus, in-progress features, latest session
cargo xtask progress milestones --runnable  # open milestones whose dependencies are complete
cargo xtask progress milestones --id <id> --full   # one milestone's acceptance and evidence
cargo xtask progress gaps                 # everything not yet Ported (what's left for a complete port)
cargo xtask progress parity --feature <slug> --full   # one feature's full summary + ADRs
cargo xtask progress routes --unrouted    # advertised-but-404 admin routes
cargo xtask progress sessions --limit 10  # recent session/handoff log
cargo xtask progress adr list             # derived ADR index
cargo xtask progress check                # validate the text sources (CI gate)
cargo xtask architecture-lint             # enforce app boundary + admin debt ratchets
cargo xtask smoke                         # boot temp web host and probe IPTV/HLS playback
```

## Language Contract

- Repository-bound content is English: code, comments, commit messages, docs,
  ADRs, issue templates, changelog entries, and skill files.
- User-facing admin UX/UI must be localized whenever it is built or changed.
  Add or update translations for every supported culture in
  `crates/ersatzrs-web/src/admin/languages/`, keep `i18n.rs` as routing/helper
  glue rather than a translation dump, and follow the translation boundary in
  [ADR 0012](docs/adr/0012-admin-ui-nl-localization.md).
- Interactive conversation with the maintainer may be Dutch or English.
- When translating existing notes, preserve technical decisions and replace
  Dutch section titles with English equivalents.

## Changelog Contract

[CHANGELOG.md](CHANGELOG.md) is the main human-readable release overview.
Agents must keep it current for every user-visible, parity-contract,
runtime/build, dependency, security, release, or documented behavior change.

- Follow [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/):
  keep `[Unreleased]` at the top, newest releases first, ISO `YYYY-MM-DD`
  release dates, linkable release headings, and only the standard change
  categories `Added`, `Changed`, `Deprecated`, `Removed`, `Fixed`, and
  `Security`.
- Use [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/)
  to decide the entry shape: `feat` generally becomes `Added`, `fix` becomes
  `Fixed`, `perf`/`refactor`/behavioral `docs` work generally becomes
  `Changed`, dependency/tooling/release work goes under the category that best
  describes user or maintainer impact, and breaking changes must be called out
  explicitly in the relevant entry.
- Write curated release notes, not a commit log dump. Combine related commits
  into one clear entry, omit internal noise, and preserve important parity debt
  or behavior changes in plain English.
- Do not create a dated release section unless performing the release. During
  normal development, add entries under `[Unreleased]`.
- Keep release instructions, PR checklists, and repo-local changelog skills
  aligned with this contract when they change.

## Agent Entry Points

This file is the single source of truth for AI agents. Symlinks ensure every
agent framework finds it:

- `AGENTS.md` — read by OpenAI Codex, GitHub Copilot, and generic agents.
- `CLAUDE.md` → symlink to `AGENTS.md` — read by Claude Code.
- `CODEX.md` → symlink to `AGENTS.md` — read by OpenAI Codex CLI.
- `.github/copilot-instructions.md` — loaded by GitHub Copilot Chat (browser
  and VS Code). Contains a short pointer to this file plus key commands.
- `skills/` — canonical repo-local skill contracts. Agent-specific skill
  directories under `.claude/skills/` and `.codex/skills/` are compatibility
  mirrors only; update `skills/` first and keep prompts pointed at canonical
  paths such as `skills/parity-slice.md`.

## Contract Index

Read the relevant contract before changing that area:

- [`docs/progress/`](docs/progress/README.md) for current phase, per-feature
  parity, route/hydration coverage, and the session log (query via
  `cargo xtask progress …`). [STATUS.md](STATUS.md) is a slim pointer at it.
- **[CONTRIBUTING.md](CONTRIBUTING.md) — mandatory for all development, not an
  area-specific contract.** Every change must respect its Ground Rules and PR
  checklist: English-only repository content; no secrets, tokens, private media
  paths, or personal library data in source, fixtures, logs, snapshots, or docs;
  **no maintainer-machine absolute paths** (`/home/<user>/...`, `/Users/<user>/...`,
  `C:\Users\...`, or other local mount paths) — use repo-relative paths and the
  `$ERSATZTV_SRC` env var; small reviewable changes; Conventional Commits; and
  ErsatzTV behavior preserved unless an ADR records the deviation.
- [CHANGELOG.md](CHANGELOG.md) for the main release overview; keep
  `[Unreleased]` current for notable changes.
- [BUILD.md](BUILD.md) before changing the build cycle, profile flags,
  `run.sh`, or anything that builds the WASM bundle / SSR binary / release
  artifacts — covers `./run.sh dev|check|test|test-closeout|build|serve|seed`,
  dev profile tuning, cargo-leptos hot-reload, compile-cost troubleshooting,
  the required nextest/Cargo-doctest runner split, and the mandatory
  **hydration build consistency** ("wasm checksum") rules + the
  `scripts/release/verify-web-bundle.sh` release gate.
- [TEST.md](TEST.md) before adding admin-UI tests or hydration playbooks, or
  before changing Leptos hydration, islands, shared UI primitives, router
  setup, or Cargo dependencies for an admin UI bug — covers the Leptos
  hydration rules (page wrapper vs `#[island]`, stable DOM shape, no
  prop-bearing islands across `Suspense`), the hydration/UI bug containment
  playbook, command-first Playwright/browser verification, and the per-page
  playbook index. Defers deep-dive testing conventions to
  `docs/how-to/testing.md` and `docs/how-to/snapshot-tests.md`.
- [STYLE.md](STYLE.md) before writing Rust, SQL, or ErsatzTV-parity code —
  covers Rust (naming, errors, async, logging, public API), Rust ↔ C# parity
  (serialized contracts, option mapping), and SQL (identifier casing,
  constraint/index naming, referential actions, migration rules). Skip for
  docs-only, status-log, or discussion turns.
- [ARCHITECTURE.md](ARCHITECTURE.md) for crate boundaries and dependency rules.
- [PORTING.md](PORTING.md) before porting any ErsatzTV behavior.
- [docs/how-to/testing.md](docs/how-to/testing.md) before adding tests.
- [docs/CONTRIBUTING.md](docs/CONTRIBUTING.md) before adding documentation.
- [docs/adr/](docs/adr/) before changing architecture or behavior contracts.
- [`docs/progress/parity/`](docs/progress/README.md) when moving a feature
  toward parity — edit the feature's `<slug>.toml` and run
  `cargo xtask progress check` (see the `update-parity` skill). Important
  missing parity must be recorded in the relevant parity file as visible
  `ParityDebt` or remaining-work text; do not leave it only in chat, commit
  notes, or a private plan.

## Health Check Parity

The health-check engine is part of the port-completeness contract. The current
non-web surface lives in `ersatzrs-core::health` and
`ersatzrs-infra::health`, with latest run results persisted to `ConfigElement`
under `health.results` and `health.summary`. When a feature changes runtime
requirements or parity-sensitive persisted state, update or add the matching
health check in the same slice so the health report remains an accurate
indicator of ErsatzTV parity.

## Key architectural decisions (from the plan)

- **UI:** Leptos (full-stack Rust, SSR + islands hydration). Start server-rendered; add interactivity only where needed.
- **FFmpeg integration:** CLI spawn via `tokio::process::Command` - same model as ErsatzTV's `CliWrap` usage. **No libav bindings.** FFmpeg 9.x is the supported runtime line (for `drawvg` and the current output-option contract); detect on startup and reject older and unverified future major versions.
- **Graphics engine:** use FFmpeg's `drawvg` filter with SVG templates in `assets/` as the replacement for ErsatzTV's `ScriptGraphicsElement` + external renderer pipe. This is the main "why Rust rewrite" win - do not reintroduce an external graphics process.
- **Database:** `sqlx` with SQLite first, MySQL behind a feature flag. **Do not port ErsatzTV's 393 EF migrations 1:1** - collapse them into one initial schema derived from the final state of `$ERSATZTV_SRC/ErsatzTV.Infrastructure/Data/TvContext.cs` (99 DbSets) and evolve from there.
- **Web host:** `axum` + `tower` + `tokio`. Leptos mounts via `leptos_axum`.
- **CQRS:** no MediatR equivalent - plain `async fn` handlers in `ersatzrs-app`, repository traits in `ersatzrs-core`, sqlx impls in `ersatzrs-infra`.
- **Scheduling:** four engines to port from `$ERSATZTV_SRC/ErsatzTV.Core/Scheduling/` behind a shared trait - Block, YAML (`serde_yaml` + per-instruction dispatch), Classic, and Scripted (`mlua` for Lua 5.4).
- **Binaries:** `ersatzrs-web` (server) and `ersatzrs-scanner` (library scanner) - two binaries sharing the core crates.

## Porting workflow

This is primarily a porting project, not greenfield design. When implementing a feature:

1. **Find the ErsatzTV source first.** The critical-file table in [PORTING.md](PORTING.md) maps features to the authoritative C# files under `$ERSATZTV_SRC/`. Read the C# before writing Rust.
2. **Preserve patterns, not syntax.** ErsatzTV's FFmpeg pipeline builders, filter label routing (`ComplexFilter.cs`), and `ffconcat`/`resetpts` concat handling are version-agnostic and should be mirrored closely in structure. LanguageExt `Option`/`Either` map to native `Option`/`Result`; `anyhow`/`thiserror` for errors. Follow the Rust ↔ C# parity and SQL rules in [STYLE.md](STYLE.md) — in particular, keep ErsatzTV JSON shapes, database identifier casing, and EF-style `PK_/FK_/IX_` names intact.
3. **Fetch parity-critical dependencies when warranted.** Do not hand-roll a
   weak substitute just to avoid a dependency fetch. If the ErsatzTV source
   relies on a real parser/evaluator/protocol library and Rust has a suitable,
   maintained equivalent, add it deliberately, verify licenses through the
   normal audit path, and record any missing or deferred parity in
   `docs/progress/parity/<slug>.toml`.
4. **Snapshot-test generated FFmpeg argv** the way ErsatzTV's `ErsatzTV.FFmpeg.Tests` does - use `insta`. This is the primary regression gate for the pipeline crate.
5. **Use the graphics engine as the divergence point.** Anywhere ErsatzTV pipes to an external graphics renderer, ErsatzRS should emit a `drawvg` filter node instead.

## Reference paths (on this machine)

- ErsatzTV source tree: `$ERSATZTV_SRC/` (see [PORTING.md](PORTING.md) for the env-var contract)
- Project contract index: [plans/INDEX.md](plans/INDEX.md)
- Porting progress (source of truth): [`docs/progress/`](docs/progress/README.md) — query with `cargo xtask progress next`
- HDHR protocol notes (internal to ErsatzTV repo): `$ERSATZTV_SRC/hdhr.md`

## Session Handoff

When ending a work session, update the progress index in `docs/progress/`:

1. Record the session:
   `cargo xtask progress add-session --agent "<name>" --summary "<one line>"`
   (appends to `docs/progress/sessions.ndjson`).
2. If parity changed, edit the relevant `docs/progress/parity/<slug>.toml`
   (status/summary/ADR) — see the `update-parity` skill.
3. If a phase checklist item, TODO, focus, or remaining work changed, update
   `docs/progress/phase.toml` (note unfinished work under `next_steps`).
4. Validate with `cargo xtask progress check`, then commit with
   `docs(status): update session log`.
