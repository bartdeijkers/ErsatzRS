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

Feature-level plans live alongside this index only while they are still
actionable. Once a plan is fully implemented, superseded, or reduced to history,
prune the standalone plan document and rely on `docs/progress/`, ADRs, the
changelog, and git history for evidence. The authoritative live state is always
`docs/progress/` (query with `cargo xtask progress …`), not these files.

### Active / actionable

- `extras-alternate-versions-and-stream-roles.md` — implements ADR 0018
  (special features/extras, alternate versions, and stream-role/commentary
  selection): additive `MediaItem` content-variant columns, new search tokens
  (`content_kind`/`sub_kind`/`parent_title`/`audio_title`/`subtitle_title`),
  default `content_kind:primary` exclusion, `/media/*` surfacing, and scanner
  import of extras ErsatzTV discards. Parity slug `content-variants` will be
  created at the first implementation slice.
- `mpeg-troubleshooting-tsduck.md` — implements ADR 0013's **in-app** optional
  TSDuck Support tool for MPEG-TS troubleshooting at
  `/system/troubleshooting/mpeg`. The manual
  `scripts/e2e/tsduck-playback-gate.sh` gate exists, but the ADR-backed
  Support surface is still proposed / not implemented.
- `module-split-survey.md` — standing refactoring backlog: large files to split,
  with ready-to-run prompts. Lift items one at a time and keep each refactor
  behavior-preserving.

## Deferred Contract Work

These are intentionally deferred or still open after pruning completed plan
documents:

- mdBook or another documentation build system.
- macOS release archives (`osx-x64`, `osx-arm64`) are intentionally omitted from
  the active release line to preserve hosted-runner credits; CI still builds and
  tests the six-RID platform matrix.
- Docker/GHCR image publication is intentionally omitted from the active release
  line; releases currently ship Linux and Windows GitHub archives only.
- Claude hook automation in `.claude/settings.json`; hook schema should be
  validated before enabling it.

## Acceptance Checks

- Repository-bound documents are English.
- Root contract index exists in `AGENTS.md`.
- The authoritative live state is `docs/progress/` plus ADRs, not pruned plan
  history.
- ADRs 0001 through 0007 record the back-filled decisions.
- CI runs the workspace Rust gates and validates `docs/progress/`.
- Skills refer to contracts instead of duplicating long policy text.
