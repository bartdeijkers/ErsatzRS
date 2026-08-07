# ErsatzRS Long-Term Project Contracts

## Purpose

ErsatzRS is a multi-year Rust and FFmpeg 9.x reimplementation of ErsatzTV. This
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

### Diátaxis documentation

- `docs/README.md` — end-user navigation.
- `docs/CONTRIBUTING.md` — documentation policy and contributor navigation.
- `docs/tutorials/` — guided learning paths.
- `docs/how-to/` — operator, contributor, and release tasks.
- `docs/reference/` — operator and contributor lookup material.
- `docs/explanation/` — product and engineering background and tradeoffs.

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

- `ffmpeg-9-hdr-dolby-vision-and-pipeline-hardening.md` — current Phase 8 plan
  for the accepted SDR/HDR10 output contract, Dolby Vision input
  characterization, Vulkan/libplacebo runtime fallback, measured AMF/VAAPI/
  libplacebo improvements, safe long-filtergraph transport, source orientation,
  animated WebP, and the deferred evidence gate for first-class Vulkan encoding.
- `teletext-support.md` — Phase 9 plan to decide the teletext product and
  standards contract, prove optional FFmpeg/libzvbi support on all six release
  targets, persist stream/page metadata, deliver selectable pages, and add
  localized management, health, and runtime proof.
- `application-layer-and-video-jukebox.md` — Phase 10 plan for a durable "The
  Box" music-video request queue behind a stable authenticated add-on API. The
  prerequisite `ersatzrs-app` extraction is retained as completed context;
  fine-grained future state is tracked in `docs/progress/milestones.toml`.
- `docker-prebuilt-binary-images.md` - release Docker plan to assemble GHCR
  images from already-built Linux release binaries instead of compiling Rust
  inside the Docker build.
- `docker-persistent-build-cache.md` - release Docker plan to add persistent
  BuildKit cache export/import for repeated beta and release attempts.
- `docker-sequential-platform-publishing.md` - release Docker plan to build and
  push one platform image at a time, then combine the final multi-arch manifest.
- `module-split-survey.md` — standing refactoring backlog: large files to split,
  with ready-to-run prompts. The infra health pilot is implemented; lift the
  remaining media-browser, YAML scheduling, scripted scheduling, and provider
  dedup items one at a time and keep each refactor behavior-preserving.
- `web-server-wasm-crate-split.md` - structural refactor plan to separate the
  native SSR server binary from the WASM hydration library so native release
  builds never compile the web `cdylib`.
- `ci-self-hosted-target-dir.md` - CI running-time plan to give the default
  self-hosted `ci.yml` jobs a persistent, checkout-external `CARGO_TARGET_DIR`
  instead of restoring `target/` through the remote `Swatinem/rust-cache`
  round-trip every run, plus a `test_hosted` matrix cache-key fix and small
  tool-install/npm caching wins.
- `epg-timeline-scroll-container.md` - admin UI plan behind the channel watch
  route's programme guide never being a real scroll container, and the browser
  assertion that silently passed because of it.

## Deferred Contract Work

These are intentionally deferred or still open after pruning completed plan
documents:

- mdBook or another documentation build system.
- macOS release archives (`osx-x64`, `osx-arm64`) are intentionally omitted from
  the active release line to preserve hosted-runner credits; CI still builds and
  tests the six-RID platform matrix.
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
