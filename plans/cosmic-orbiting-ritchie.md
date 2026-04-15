# NasTv — Long-term project contracts

## Context

NasTv is a multi-year Rust/FFmpeg-8.1 reimplementation of ErsatzTV (see [sequential-dancing-valiant.md](./sequential-dancing-valiant.md)). The repository currently holds only `README.md`, `LICENSE`, `CHANGELOG.md`, and `CLAUDE.md`. Because the project will run for a long time and involves porting ~131K LOC of C#, we want to lock down the governance, style, documentation, and porting contracts **before** any code is written — to prevent drift, rework, and inconsistency across later phases.

ErsatzTV itself carries very little governance (zlib, single maintainer, external docs site, no `CONTRIBUTING`, no ADRs). NasTv consciously leans into heavier in-repo contracts because it is both (a) public open-source from day one and (b) a long-running porting project in which consistency quickly erodes without written rules.

### Locked-in decisions

| Decision | Choice | Consequence |
|---|---|---|
| Project mode | **Public open-source from day 1** | Full governance set required: CoC, SECURITY, issue/PR templates, external CONTRIBUTING, SUPPORT |
| Language | **English throughout the repository** | Existing Dutch files (`README.md`, `CHANGELOG.md`, `CLAUDE.md`) must be translated. Personal working plans under `/home/bart/.claude/plans/` no longer apply — all plans now live under `/home/bart/nastv/plans/` in English |
| Commit style | **Conventional Commits** (`type(scope): subject`) | CI-linted via `commitlint` / `cocogitto`; consider `git-cliff` as CHANGELOG generator |
| Scope this round | **Everything in the brainstorm list** (A through J) | Rolled out in 8 waves so each wave leaves the repository in a coherent interim state |

---

## Deliverables: concrete file list per category

Each item is a concrete file or config that should exist on disk after this round. Content hints are keywords only; the actual prose is written during execution.

### A. Governance root

| File | Content |
|---|---|
| `README.md` (rewrite) | EN; what NasTv is, status (“pre-alpha, actively porting”), highlights, link to `docs/`, link to upstream ErsatzTV, quickstart deferred until code exists |
| `CONTRIBUTING.md` | EN; dev setup (rust-toolchain, FFmpeg 8.1), workflow (trunk-based, short-lived branches), commit style (Conventional Commits), PR checklist (fmt/clippy/test/changelog/ADR-if-needed), code-review expectations, cross-references to `STYLE.md`, `TESTING.md`, `PORTING.md` |
| `CODE_OF_CONDUCT.md` | Contributor Covenant v2.1 verbatim + reporting contact |
| `SECURITY.md` | Responsible disclosure; supported versions; brief threat model (NasTv exposes HTTP + UDP on LAN, transcodes user media); report via GitHub private advisory |
| `SUPPORT.md` | Where users go for help: discussions / issue tracker / docs — explicitly not via security channels |
| `GOVERNANCE.md` | Short: single-maintainer model, how scope changes (via ADR), when external input is sought |
| `CHANGELOG.md` (translate + extend) | EN; Keep a Changelog 1.1.0; `[Unreleased]` with Added/Changed/Fixed/Removed/Security; note that entries under Unreleased are required per Conventional Commit |
| `LICENSE` | Kept as-is (zlib, aligned with ErsatzTV) |

### B. Code style and tool enforcement

| File | Content |
|---|---|
| `STYLE.md` (or `docs/explanation/coding-style.md`) | The *why*: Rust naming, module layout, error handling (`anyhow` only in binaries and entry points, `thiserror` in libraries, **no** `unwrap`/`expect` outside `main`/tests/doctests), async patterns (tokio), logging via `tracing`, `Result` vs panic, `#[must_use]` policy, public-API documentation requirement |
| `rustfmt.toml` | `edition = "2021"` (or 2024 once stable), `max_width = 100`, `imports_granularity = "Module"`, `group_imports = "StdExternalCrate"`, `reorder_imports = true` |
| `clippy.toml` | `msrv = "1.XX"`, `avoid-breaking-exported-api = true`, `disallowed-methods` list (e.g. `std::env::var` → use the config layer) |
| `Cargo.toml` (workspace) `[workspace.lints.rust]` / `[workspace.lints.clippy]` | Deny-lints: `unsafe_code = "deny"`, `rust_2018_idioms = "warn"`, `clippy::pedantic = "warn"` with explicit allow-list, `clippy::unwrap_used = "deny"`, `clippy::expect_used = "deny"`, `clippy::todo = "warn"` |
| `.editorconfig` | LF end-of-line, UTF-8, trim trailing whitespace, insert final newline, per-filetype indent (`rs=4`, `toml=2`, `md=2`, `yaml=2`) |
| `.gitattributes` | `* text=auto eol=lf`, `*.rs diff=rust`, lock files marked `linguist-generated` |
| `deny.toml` (cargo-deny) | License allowlist (MIT/Apache-2.0/BSD-*/zlib/ISC), `advisories = "deny"`, `bans.multiple-versions = "warn"`, `sources.unknown-registry = "deny"` |
| `rust-toolchain.toml` | Pin stable + profile, rustfmt/clippy components |
| MSRV policy (in `STYLE.md`) | “current stable minus 1”, bumped per release |

### C. Architecture contracts

| File | Content |
|---|---|
| `ARCHITECTURE.md` | Crate layout (from master plan), **layer dependency rules** as a directed graph: `nastv-domain` ← `nastv-core` ← `nastv-app` ← `nastv-web`/`nastv-scanner`; `nastv-infra` implements traits from `nastv-core`; `nastv-ffmpeg` sits alongside `nastv-core`. Explicit **no-cycles** rule. Diagram (ASCII or mermaid) |
| `docs/adr/0000-template.md` | Template with fields: Status, Context, Decision, Consequences, Alternatives considered, References. Numbering rule (zero-padded, 4 digits) |
| `docs/adr/0001-record-architecture-decisions.md` | Meta-ADR: we adopt ADRs from now on; rule for when an ADR is required (changing a crate boundary, adding an external dependency used across multiple crates, deviating from ErsatzTV behavior) |
| `docs/adr/0002-ui-framework-leptos.md` | Back-fill: choice of Leptos (SSR + islands) over Dioxus/Yew/server-only. Context: master plan |
| `docs/adr/0003-ffmpeg-cli-spawn.md` | Back-fill: `tokio::process::Command` instead of libav bindings. Rationale: parity with ErsatzTV patterns, freedom to upgrade FFmpeg |
| `docs/adr/0004-collapsed-initial-schema.md` | Back-fill: a single sqlx schema instead of porting 393 EF migrations |
| `docs/adr/0005-drawvg-graphics-engine.md` | Back-fill: `drawvg` + SVG templates replace ErsatzTV’s `ScriptGraphicsElement` pipe |
| `docs/adr/0006-sqlx-over-sea-orm.md` | Back-fill: `sqlx` + compile-time SQL checks, MySQL behind a feature flag |
| `docs/adr/0007-conventional-commits.md` | Back-fill: Conventional Commits mandatory, CI-enforced |

### D. Testing contract

| File | Content |
|---|---|
| `docs/how-to/testing.md` | Unit (in-module `#[cfg(test)]`), integration (per-crate `tests/`), snapshot (`insta` for FFmpeg argv, YAML scheduling output, SVG template rendering), e2e (cross-crate under `nastv-web/tests/`); naming convention; fixture policy |
| `docs/how-to/snapshot-tests.md` | When `insta` is mandatory (every FFmpeg argv generator, every filter-complex builder, every YAML instruction handler); review flow (`cargo insta review`); never commit `.snap.new` |
| `docs/explanation/test-strategy.md` | Why these layers, why no coverage gate, why property tests (`proptest`) stay optional except for scheduling engines |
| Test-DB strategy (inside `testing.md`) | `sqlx::SqlitePool` in-memory per test; MySQL via `testcontainers` only at integration tier, behind `--features mysql-integration` |

### E. Documentation structure (Diataxis)

```
docs/
├── README.md              # Diataxis explainer + signpost
├── CONTRIBUTING.md        # Docs style, markdown rules, link convention, rustdoc requirements
├── adr/                   # (section C)
├── tutorials/             # learn-by-doing
│   └── .gitkeep
├── how-to/                # task-oriented
│   ├── testing.md
│   ├── snapshot-tests.md
│   ├── add-migration.md
│   ├── add-ffmpeg-pipeline.md
│   └── logging.md
├── reference/             # lookup
│   ├── configuration.md
│   ├── parity-matrix.md   # (section F)
│   └── .gitkeep
└── explanation/           # why / background
    ├── coding-style.md    # (may be STYLE.md + link)
    ├── architecture.md    # (may be ARCHITECTURE.md + link)
    ├── test-strategy.md
    └── deviations.md      # (section F)
```

- `docs/README.md` explains the four Diataxis quadrants and states explicitly: never put *how* in an explanation page, never put *why* in a how-to.
- `docs/CONTRIBUTING.md` defines: EN, sentence-case headings, relative links, fenced code blocks with language tags, no HTML unless required, rustdoc must include an example for every public item in `nastv-core`/`nastv-ffmpeg`/`nastv-app`.

### F. Porting contract (NasTv-specific, highest value)

| File | Content |
|---|---|
| `PORTING.md` | C# → Rust idiom table (`LanguageExt.Option<T>` → `Option<T>`, `Either<L,R>` → `Result<R,L>`, `Task<T>` → `async fn -> T`, DI container → constructor-injected traits, MediatR handler → plain `async fn`, EF `DbSet` → sqlx `Query*` + repository trait, `Validation<Error,T>` → `Result<T, Vec<Error>>` or `thiserror`); the procedure “from an ErsatzTV file to a Rust target” (read C#, map to crate + module, write trait, implement, snapshot-test); rule that deviations from ErsatzTV behavior **require an ADR** |
| `docs/reference/parity-matrix.md` | Table: ErsatzTV feature / ErsatzTV source / NasTv target / status (🔴 not started / 🟡 in progress / 🟢 ported / ⚪ deferred / ⚫ dropped) / linked ADR. Living document; every PR that completes a feature updates this table |
| `docs/explanation/deviations.md` | Deliberate divergences from ErsatzTV with rationale: drawvg instead of `ScriptGraphicsElement`, collapsed schema, `sqlx` + compile-time SQL instead of EF Core, Leptos instead of Blazor, no MediatR. Every bullet links to its ADR |

### G. Operational contracts

| File | Content |
|---|---|
| `docs/how-to/logging.md` | `tracing` span naming (`<crate>::<module>::<op>`), mandatory fields (`channel_id`, `playout_id`, `media_id` where applicable), log levels per module class (HTTP route = info, scheduling = debug, FFmpeg spawn = info stdout / debug stderr), no secrets in logs, no `println!` in library code |
| `docs/reference/configuration.md` | Env-var prefix `NASTV_`, layering via `figment`: defaults → `config.toml` → env → CLI, what is a secret (DB password, Trakt token) and therefore belongs in env not in file |
| `docs/how-to/add-migration.md` | sqlx migration naming (`YYYYMMDDHHMM_<snake_description>.sql`), reversibility policy (up-only for dev + documented manual-down for destructive changes), review criteria (never `DROP COLUMN` without an ADR), how to test MySQL and SQLite compatibility |
| `docs/how-to/add-ffmpeg-pipeline.md` | When a new HW path (QSV/AMF/VT/RKMPP/V4L2M2M) is being added: mandatory snapshot test, mandatory mapping to the corresponding ErsatzTV `*PipelineBuilder.cs` |

### H. Release / CI process

| File | Content |
|---|---|
| `.github/pull_request_template.md` | Checklist: type of change, linked ADR (if required), CHANGELOG updated, tests added, snapshots reviewed, `cargo fmt && cargo clippy && cargo test && cargo deny check` executed |
| `.github/ISSUE_TEMPLATE/bug_report.yml` | Required fields: FFmpeg version, NasTv version, reproduction steps, logs |
| `.github/ISSUE_TEMPLATE/feature_request.yml` | Required fields: use case, relation to ErsatzTV behavior (parity or new feature?) |
| `.github/ISSUE_TEMPLATE/porting_task.yml` | NasTv-specific: which ErsatzTV file/feature is being ported, target crate, expected ADR? |
| `.github/ISSUE_TEMPLATE/config.yml` | Links to Discussions + SECURITY |
| `.github/workflows/ci.yml` | Jobs: `fmt` (`cargo fmt --check`), `clippy` (`--all-targets --all-features -D warnings`), `test` (matrix: stable + MSRV, ubuntu-latest, FFmpeg 8.1 from PPA), `deny` (cargo-deny), `commitlint` (on PR), `docs-build` (mdBook if adopted, else `cargo doc`) |
| `.github/workflows/release.yml` | Tag-triggered; optional `cargo publish`, Docker multi-arch image, GitHub Release populated from CHANGELOG section |
| `.github/dependabot.yml` | cargo + github-actions, weekly |
| `RELEASING.md` (or section in CONTRIBUTING) | Semver policy (0.x: breaking = minor, fix = patch), release steps, CHANGELOG cut-over to a new version, tag format (`vX.Y.Z`) |
| `commitlint.config.js` or `cog.toml` | Conventional Commit scopes list (e.g. `domain`, `core`, `ffmpeg`, `infra`, `app`, `web`, `scanner`, `docs`, `ci`, `deps`) |

### I. Claude Code skills

Location: `/home/bart/.claude/skills/` (user-scoped so they’re available in every NasTv session). Each is its own markdown file with frontmatter `name`, `description`, optional `model`, and body instructions.

| Skill | Purpose |
|---|---|
| `port-ersatztv.md` | Input: path to a C# file under `/home/bart/ErsatzTV/`. Output: proposed Rust target (crate + module path), trait sketch, idiom mapping from `PORTING.md`, reference to any existing ADR. **Writes no code**, produces a plan only |
| `explain-ersatztv.md` | Read-only summarizer for a C# file — public API, dependencies, place in the ErsatzTV architecture |
| `new-crate.md` | Scaffold a workspace crate: `crates/<name>/{Cargo.toml, src/lib.rs, src/error.rs, tests/.gitkeep}`, add to workspace `members`, follow the layer rules from `ARCHITECTURE.md` |
| `ffmpeg-snapshot-test.md` | Scaffold an `insta` snapshot test for an FFmpeg argv generator, including `assert_snapshot!` and `cargo insta accept` instructions |
| `add-migration.md` | New sqlx migration with timestamped name, template header, placed under `migrations/` |
| `new-adr.md` | Detect next free ADR number, copy `docs/adr/0000-template.md`, set Status=Proposed |
| `changelog-entry.md` | Add an entry under `[Unreleased]` in the correct subsection (Added/Changed/Fixed/Removed/Security), Conventional-Commit style |
| `update-parity.md` | Update the parity matrix: add a row or change status for an ErsatzTV feature |
| `verify-fase.md` | Run the verification checks from the master plan `sequential-dancing-valiant.md` for a given phase |

### J. Claude / AI contracts

| File | Content |
|---|---|
| `CLAUDE.md` (translate + extend) | EN; cross-reference every contract file once they exist (STYLE, ARCHITECTURE, PORTING, TESTING, CONTRIBUTING, ADRs, parity matrix, deviations); retains the core of the current version |
| `.claude/settings.json` (hooks) | `PostToolUse` on `Edit`/`Write` matching `*.rs`: `cargo fmt --check` + `cargo clippy -p <changed_crate>` (fail-soft, report only); `pre-commit` git hook running Conventional-Commit lint via `cocogitto`/`commitlint` |
| Language agreement for Claude | Section in `CLAUDE.md`: Claude writes repo-bound output in EN, but may use NL in interactive conversation with the maintainer |

---

## Rollout order (waves)

Each wave leaves the repository in a coherent interim state. Wave N+1 builds on Wave N but does not break it if we pause.

**Wave 0 — Language cut-over** (blocks everything else, must run first)
- Translate `README.md` to EN (expanded)
- Translate existing `[Unreleased]` entries in `CHANGELOG.md` to EN
- Translate `CLAUDE.md` to EN and add the language agreement

**Wave 1 — Code style & tool enforcement (B)**
- `rustfmt.toml`, `clippy.toml`, `deny.toml`, `.editorconfig`, `.gitattributes`, `rust-toolchain.toml`
- `STYLE.md` (the *why*)
- These can be added without a `Cargo.toml`; `[workspace.lints]` is inserted when master plan Phase 0 creates the workspace

**Wave 2 — Governance root (A)**
- `CONTRIBUTING.md` (with Conventional Commits, PR flow, cross-references to STYLE)
- `CODE_OF_CONDUCT.md` (Contributor Covenant v2.1)
- `SECURITY.md`, `SUPPORT.md`, `GOVERNANCE.md`

**Wave 3 — Architecture & ADRs (C)**
- `ARCHITECTURE.md`
- `docs/adr/0000-template.md` + ADRs 0001–0007 (back-filling the decisions from the master plan)

**Wave 4 — Docs structure (Diataxis) (E)**
- `docs/README.md`, `docs/CONTRIBUTING.md`
- Empty `tutorials/`, `how-to/`, `reference/`, `explanation/` directories with `.gitkeep`

**Wave 5 — Porting contract (F)**
- `PORTING.md`
- `docs/reference/parity-matrix.md` (empty skeleton table)
- `docs/explanation/deviations.md` (initial list from master plan: drawvg, collapsed schema, sqlx, Leptos, no MediatR)

**Wave 6 — Testing + operational how-tos (D, G)**
- `docs/how-to/testing.md`, `snapshot-tests.md`, `logging.md`, `add-migration.md`, `add-ffmpeg-pipeline.md`
- `docs/reference/configuration.md`
- `docs/explanation/test-strategy.md`

**Wave 7 — Release / CI process (H)**
- `.github/pull_request_template.md`, issue templates, `dependabot.yml`
- `.github/workflows/ci.yml` (fmt/clippy/test/deny/commitlint) + `release.yml`
- `RELEASING.md`
- `commitlint` / `cog.toml`

**Wave 8 — Claude skills + hooks (I, J)**
- `.claude/skills/*.md` for the nine skills in section I
- `.claude/settings.json` with fmt/clippy hooks + commit lint

---

## Critical files to consult during execution

- `/home/bart/nastv/plans/sequential-dancing-valiant.md` — master plan, source of the architectural decisions back-filled into ADRs 0001–0007
- `/home/bart/ErsatzTV/.editorconfig` — style-philosophy reference (do **not** copy; it is C#-specific)
- `/home/bart/ErsatzTV/.github/workflows/` — reference for CI job layout
- `/home/bart/ErsatzTV/.github/ISSUE_TEMPLATE/issue.yml` — pattern for mandatory-field checks
- `/home/bart/nastv/CLAUDE.md` — current Dutch version, translated in Wave 0
- `/home/bart/nastv/CHANGELOG.md` — current Dutch version already using Keep a Changelog
- [Contributor Covenant v2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/) — source for `CODE_OF_CONDUCT.md`
- [Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) — existing choice
- [Diataxis](https://diataxis.fr/) — docs structure
- [Conventional Commits 1.0.0](https://www.conventionalcommits.org/en/v1.0.0/) — commit style

---

## Verification

After finishing each wave:

**Wave 0:**
- No Dutch sentences remain in `README.md`, `CHANGELOG.md`, `CLAUDE.md` (visual + `rg` spot-check)
- Existing content stays semantically equivalent (CHANGELOG entries keep their subsection)

**Wave 1:**
- `cargo fmt --check` and `cargo clippy` run (once Rust code exists) without config errors
- `cargo deny check` runs on an empty workspace
- `.editorconfig` is recognised by RustRover / VS Code

**Wave 2:**
- Every section/file linked from `CONTRIBUTING.md` either exists or has a `TODO` marker pointing to a later wave
- `CODE_OF_CONDUCT.md` matches Contributor Covenant v2.1 verbatim save for whitespace

**Wave 3:**
- `ARCHITECTURE.md` links 1-to-1 to every crate from the master plan’s “Architectuur: crate-indeling” section
- ADRs 0001–0007 cover the seven back-filled decisions and link to their master-plan section
- No ADR has status `Proposed` (all back-fills = `Accepted`)

**Wave 4:**
- `docs/` tree matches the section-E structure exactly
- `docs/README.md` explains the four Diataxis quadrants + signpost

**Wave 5:**
- `PORTING.md` contains at least the seven idiom mappings from section F + the procedure + the ADR rule
- `parity-matrix.md` has rows for at least: Channel, Playout, Block/YAML/Scripted scheduling, Plex/Jellyfin/Emby sources, IPTV M3U, HDHR, Graphics overlay — all status 🔴
- `deviations.md` has five initial entries with ADR links

**Wave 6:**
- Every how-to has (a) an audience line at the top, (b) numbered steps, (c) “see also” links
- Every reference page is 100% lookup, no narrative

**Wave 7:**
- A PR against an empty test branch triggers all CI jobs; fmt/clippy/test/deny/commitlint all green on the template commit
- A PR with a non-conventional commit fails on `commitlint`
- Dependabot config parses as valid

**Wave 8:**
- Every skill file starts with valid frontmatter (`name`, `description`)
- `.claude/settings.json` is valid JSON, hook patterns are correct globs
- A trial edit on a `.rs` file (once Rust code exists) triggers the PostToolUse hook

---

## Out of scope for this plan

- Writing actual Rust code (that remains the master plan `sequential-dancing-valiant.md`)
- Adopting mdBook — reconsider during Wave 4; if adopted, capture as a separate ADR
- Website / external docs site: in-repo `docs/` is canonical

---

## Plan location convention

All NasTv plan files live under `/home/bart/nastv/plans/` alongside the code they describe. The earlier location `/home/bart/.claude/plans/` was a default of Claude Code’s plan mode and is not used by this project — stale copies there should be removed. New plans are created directly in `/home/bart/nastv/plans/` regardless of what plan mode proposes as a default path.
