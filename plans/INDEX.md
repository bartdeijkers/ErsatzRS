# ErsatzRS Long-Term Project Contracts

## Purpose

ErsatzRS is a multi-year Rust and FFmpeg 8.1 reimplementation of ErsatzTV. This
plan records the contract layer that must exist before substantial code is
written: governance, style, architecture, documentation, porting, testing,
operations, release process, CI, and repeatable AI-agent workflows.

The repository is developed solo at first, but the contracts assume a future
public open-source project.

## Locked Decisions

| Decision | Choice | Consequence |
| --- | --- | --- |
| Language | English throughout repository-bound content | Docs, code comments, ADRs, changelog, templates, commits, and skills use English. |
| Contribution mode | Solo first, public OSS later | Governance exists early, but process stays lightweight. |
| Commit style | Conventional Commits | CI can lint commits and release notes can be generated later. |
| Changelog | Keep a Changelog in `CHANGELOG.md` | The root changelog is the main human-readable release overview and must stay current under `[Unreleased]`. |
| Documentation model | Diataxis | Docs are split into tutorials, how-to guides, reference, and explanation. |
| Porting rule | ErsatzTV behavior by default | Deliberate deviations require ADRs. |
| AI workflows | Repo-local `.claude/skills/` | Skills travel with the repository and stay reviewable. |

## Implemented Contract Files

### Root Governance

- `README.md`
- `CONTRIBUTING.md`
- `CODE_OF_CONDUCT.md`
- `SECURITY.md`
- `SUPPORT.md`
- `GOVERNANCE.md`
- `CHANGELOG.md`
- `RELEASING.md`

### Code and Tooling Policy

- `STYLE.md`
- `.editorconfig`
- `.gitattributes`
- `rustfmt.toml`
- `clippy.toml`
- `deny.toml`
- `rust-toolchain.toml`
- `commitlint.config.cjs`

### Architecture and Porting

- `ARCHITECTURE.md`
- `PORTING.md`
- `docs/adr/0000-template.md`
- `docs/adr/0001-record-architecture-decisions.md`
- `docs/adr/0002-ui-framework-leptos.md`
- `docs/adr/0003-ffmpeg-cli-spawn.md`
- `docs/adr/0004-collapsed-initial-schema.md`
- `docs/adr/0005-drawvg-graphics-engine.md`
- `docs/adr/0006-sqlx-over-sea-orm.md`
- `docs/adr/0007-conventional-commits.md`
- `docs/reference/parity-matrix.md` (now a stub pointing at `docs/progress/`)
- `docs/progress/` — structured porting-progress source of truth (per-feature
  parity, route/hydration coverage, phase state, session log). A gitignored
  `progress.db` index is rebuilt from it by `cargo xtask progress`.
- `docs/explanation/deviations.md`

### Diataxis Documentation

- `docs/README.md`
- `docs/CONTRIBUTING.md`
- `docs/tutorials/.gitkeep`
- `docs/how-to/testing.md`
- `docs/how-to/snapshot-tests.md`
- `docs/how-to/logging.md`
- `docs/how-to/add-migration.md`
- `docs/how-to/add-ffmpeg-pipeline.md`
- `docs/reference/configuration.md`
- `docs/explanation/architecture.md`
- `docs/explanation/test-strategy.md`

### GitHub Process

- `.github/pull_request_template.md`
- `.github/ISSUE_TEMPLATE/bug_report.yml`
- `.github/ISSUE_TEMPLATE/feature_request.yml`
- `.github/ISSUE_TEMPLATE/porting_task.yml`
- `.github/ISSUE_TEMPLATE/config.yml`
- `.github/workflows/ci.yml`
- `.github/workflows/release.yml`
- `.github/dependabot.yml`

### Repo-Local Claude Skills

- `.claude/skills/port-ersatztv.md`
- `.claude/skills/explain-ersatztv.md`
- `.claude/skills/new-crate.md`
- `.claude/skills/ffmpeg-snapshot-test.md`
- `.claude/skills/add-migration.md`
- `.claude/skills/new-adr.md`
- `.claude/skills/changelog-entry.md`
- `.claude/skills/update-parity.md`
- `.claude/skills/verify-phase.md`

## Plan Documents

Feature-level plans live alongside this index. Completed plans are kept for
historical context; the authoritative live state is always `docs/progress/`
(query with `cargo xtask progress …`), not these files.

### Active

- `hls-transcode-capacity-and-admission.md` — implements ADR 0016 (hard transcode
  capacity ceiling + admission/preemption ladder) and ADR 0017 (default 1080p60 /
  AC-3 6ch FFmpeg profile + `-max_delay`). Open debts on
  `hls-variant-playlists-segmenter` and `ffmpeg-command-generation`.
- `goal-nvenc-pipeline-parity.md` — close the remaining NVENC/CUDA pipeline gaps
  (`ffmpeg-command-generation` debt `nvenc-filter-gate-live-proof`).
- `goal-hls-segmenter-mpegts-wrapper-parity.md` — HLS segmenter state machine and
  MPEG-TS wrapper orchestration (`hls-variant-playlists-segmenter`, In progress).
- `release-self-built-ffmpeg.md` — replace public FFmpeg downloads in the release
  workflow with first-party FFmpeg 8.1 builds and bundled runtimes per RID.
- `ffmpeg-zscale-runtime-remediation.md` — ensure self-built/release FFmpeg ship
  `zscale` (`--enable-libzimg`) for the fail-fast startup capability check.
- `module-split-survey.md` — standing refactoring backlog: large files to split,
  with ready-to-run prompts (lift items one at a time).
- `workflow-review.md` — review of the parity-progress pipeline; its
  recommendations are tracked under Deferred Contract Work below.

### Completed / historical

These slices shipped (target feature is `Ported`) or their investigation
concluded; kept for context, with live state in `docs/progress/`.

- `graphics-engine-parity-slice.md` + `goal-graphics-engine.md` — drawvg graphics
  engine within ADR 0005, no external process (`drawvg-graphics-engine`, Ported).
- `colorspace-normalization-parity.md` — software + GPU-resident colorspace
  normalization (`hardware-colorspace-normalization`, Ported).
- `settings-parity-slice.md` — settings route family + file-driven i18n
  (`settings`, Ported).
- `remote-streams-browser-parity-slice.md` — remote-streams browser
  (`remote-streams-browser`, Ported).
- `media-sources-hydration-anomaly.md` — Emby/Plex media-source hydration
  investigation (`media-source-configuration`, Ported).
- `hls-mpegts-review-fixes.md` — HLS/MPEG-TS review fixes
  (`mpeg-ts-channel-streaming`, Ported).
- `mpeg-troubleshooting-tsduck.md` — TSDuck MPEG-TS diagnostics (ADR 0013;
  `scripts/e2e/tsduck-playback-gate.sh`).
- `ffmpeg-path-parity.md` — FFmpeg/ffprobe locator + DB-path resolution
  investigation (shipped).
- `launcher-releasing.md` — Windows tray launcher + graceful shutdown
  (`ersatzrs-launcher`, shipped; code signing researched but deferred).

## Deferred Contract Work

These are intentionally deferred until code exists:

- Root `Cargo.toml` workspace lints.
- Real crate-level CI runs.
- mdBook or another documentation build system.
- Release artifact publishing.
- Docker multi-arch release flow.
- Claude hook automation in `.claude/settings.json`; hook schema should be
  validated before enabling it.
- Workflow improvements from the parity-progress pipeline review
  (`workflow-review.md`): LLM-agnostic skills/contracts, summary hygiene plus
  runtime-triage routine, evidence-gated parity status, and a
  `cargo xtask smoke` runtime gate.

## Acceptance Checks

- Repository-bound documents are English.
- Root contract index exists in `AGENTS.md`.
- The authoritative implementation plan is English.
- ADRs 0001 through 0007 record the back-filled decisions.
- CI jobs skip Rust checks until `Cargo.toml` exists.
- Skills refer to contracts instead of duplicating long policy text.
