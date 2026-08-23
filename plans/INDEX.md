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

- `drawtext-overlay-leg.md` — queued after AMM and before Phase 9A; adds an
  additive FFmpeg `drawtext` path for live, reloadable text while retaining
  `drawvg` as the default static/vector-rich backend. The resulting controller
  and overlay contract can also be reused by Phase 10's video jukebox.
- `addon-nfo-sidecars-and-local-replacements-closeout.md` — blocking handoff
  for generic v2 add-on metadata/NFO sidecars, observable content-kind
  semantics, transactional YAML/NFO publication, exact Local replacements,
  provider-owned multilingual assets/tests, and POSIX/Windows add-on parity.
  Its two remaining blockers — live MySQL runtime migration proof and packaged
  Windows add-on execution — are also recorded as open debts in
  `docs/progress/parity/addon-platform.toml`.
- `addon-list-replacement-ux.md` — reshape the Local replacement flow for an
  unavailable add-on list item, written after walking the flow end to end
  against a real unavailable episode.
- `addon-list-artwork-and-metadata-projection.md` — two follow-up slices found by
  inspecting a real imported list: nothing ever fetches the add-on's artwork to
  disk although the whole surrounding chain exists, and every projection writes
  one metadata row while the genres, tags, studios, actors, and artwork captured
  in the NFO reach no table at all.
- `channel-packages.md` — first draft for Phase 11: a `.zip` uploaded under
  `/channels` carrying channel identity, artwork, graphics elements, and a feed
  URL whose items are routed to the add-on that handles their host and imported
  without an operator filling in a form. Draft only; the product and feed-format
  ADR is the blocking first milestone.
- `addon-list-editor-item-classification.md` — move the add-on list media kind
  to each item, grow the list presentation to the full `LibraryMediaKind` set
  (seven projections, of which two exist today), and split the flat replacement
  table into counted, paged non-playable / unknown / playable sections. Follows
  the ADR 0072 resolver slice.
- `phase-8-hardware-residue.md` — hand-off plan for the five Phase 8 parity
  debts that an NVIDIA-only machine cannot close: HDR10 characterization and
  functional graph probes on QSV/VAAPI/AMF/VideoToolbox/RKMPP, the three
  unmeasured `p8b-02` benchmark candidates, the remaining Profile 5 production
  acceptance, and the evidence-gated `p8c-04` Vulkan-encoding decision.
  Explicitly deferred after Phase 8 and independent of Phases 9 and 10;
  includes the measured proof that each item is hardware-blocked.
- `windows-beta-release-hardening.md` — pre-publication Windows candidate
  archive, beta workflow hardening, release-policy decisions, extractor
  hardening, and redacted live acceptance gates required before the next
  immutable beta after `v1.0.21-beta.1`.
- `teletext-support.md` — Phase 9 plan to decide the teletext product and
  standards contract, prove optional FFmpeg/libzvbi support on all six release
  targets, persist stream/page metadata, deliver selectable pages, and add
  localized management, health, and runtime proof.
- `application-layer-and-video-jukebox.md` — Phase 10 plan for a durable "The
  Box" music-video request queue behind a stable authenticated add-on API. The
  prerequisite `ersatzrs-app` extraction is retained as completed context;
  fine-grained future state is tracked in `docs/progress/milestones.toml`.
- `docker-prebuilt-binary-images.md` — release Docker plan to assemble GHCR
  images from already-built Linux release binaries instead of compiling Rust
  inside the Docker build.
- `docker-persistent-build-cache.md` — release Docker plan to add persistent
  BuildKit cache export/import for repeated beta and release attempts.
- `docker-sequential-platform-publishing.md` — release Docker plan to build and
  push one platform image at a time, then combine the final multi-arch
  manifest.
- `web-server-wasm-crate-split.md` — structural refactor plan to separate the
  native SSR server binary from the WASM hydration library so native release
  builds never compile the web `cdylib`.

### Reference material kept alongside the plans

These are not implementation plans and are not expected to be completed or
pruned:

- `ux-complaint-inventory.md` — evidence base: all 400 `ErsatzTV/legacy` issues
  classified by close reason, with the `NOT_PLANNED` pile clustered by theme,
  used to decide where ErsatzRS should deliberately diverge from inherited
  ErsatzTV UX instead of porting it faithfully. Cite it when a slice argues for
  a divergence; the divergence itself still belongs in an ADR or
  `docs/progress/parity/`.
- `clippy-exemption-ledger.html` — generated snapshot of the workspace's
  `#[allow(clippy::…)]` sites, rebuilt by `cargo xtask clippy-ledger`.
- `references/` — captured upstream material referenced by the plans above.

### Pruned on 2026-08-18

Sixteen fully implemented plans were removed once their substance lived in
`docs/progress/`. Their evidence is in the parity files, milestones, ADRs, the
changelog, and git history:

- The seven add-on boundary slices (`ab-01`–`ab-07`), whose debts and evidence
  are entries in `docs/progress/parity/addon-host-boundary.toml`.
- `addon-sideload-install.md` and `addon-provider-supplied-chapters.md`
  (`addon-platform.toml`, `remote-stream-playlists.toml`).
- `ffmpeg-9-hdr-dolby-vision-and-pipeline-hardening.md` — the completed Phase 8
  program, milestones `p8a-01`–`p8c-03`; its deferred `p8c-04` decision and
  hardware-blocked residue moved to `phase-8-hardware-residue.md`.
- `mpeg-troubleshooting-tsduck.md` — ADR 0013 plus `support-pages.toml`; its
  TSDuck install steps moved to
  `docs/how-to/troubleshoot-startup-and-playback.md`.
- `fix-playwright-e2e-app-boundary-regressions.md` and
  `epg-timeline-scroll-container.md` — each defect is a resolved debt with its
  CI run recorded in the relevant parity file.
- `ci-self-hosted-target-dir.md` — implemented in `.github/workflows/ci.yml`.
- `sqlite-write-transaction-triage.md` — `initial-schema-database-backends.toml`.
- `module-split-survey.md` — `p7c-04`; its locked selection rule is the
  `module-split-selection-rule` entry in `application-use-case-layer.toml`.

A milestone that is still open must name a live plan under `plans/`; a
`Complete` milestone may omit the pointer, which is what makes this pruning
legal. `cargo xtask progress check` enforces both halves.

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
