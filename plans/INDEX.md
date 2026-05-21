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
- `docs/reference/parity-matrix.md`
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

Feature-level porting plans live alongside this index:

- `plans/scan-languages-fact-row-sync.md` — closes the three TV-show-page
  parity gaps (per-show sync, playout-refresh channel, languages fact row)
  as ordered slices.
- `plans/scan-languages-fact-row-sync-mutable-sparkle.md` — follow-on plan
  that ports `ExtractEmbeddedShowSubtitles` on deep show scan (Subtitle-row
  population for Jellyfin/Emby episodes, config keys + cache layout, the
  ffmpeg subtitle/font extraction engine, and the deep-scan wiring), closing
  the last documented deferral of the plan above.

## Deferred Contract Work

These are intentionally deferred until code exists:

- Root `Cargo.toml` workspace lints.
- Real crate-level CI runs.
- mdBook or another documentation build system.
- Release artifact publishing.
- Docker multi-arch release flow.
- Claude hook automation in `.claude/settings.json`; hook schema should be
  validated before enabling it.

## Acceptance Checks

- Repository-bound documents are English.
- Root contract index exists in `AGENTS.md`.
- The authoritative implementation plan is English.
- ADRs 0001 through 0007 record the back-filled decisions.
- CI jobs skip Rust checks until `Cargo.toml` exists.
- Skills refer to contracts instead of duplicating long policy text.
