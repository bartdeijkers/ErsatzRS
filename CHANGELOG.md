# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Conventional Commits](https://www.conventionalcommits.org/)
for commit messages.

## [Unreleased]

### Added

- Phase 1 playout runtime-state persistence parity for SQLite: playout anchors,
  schedule/fill-group enumerator state rows, gaps, history, program-schedule
  anchors, playout templates, focused in-memory integration coverage, and
  broader env-gated real-world playout decode assertions.
- Real-world ErsatzTV SQLite compatibility coverage gated by the
  `ERSATZRS_REAL_WORLD_SQLITE` environment variable.
- Persistence gap matrix for all 138 ErsatzTV SQLite tables, including current
  Rust repository coverage, read/write coverage, env-gated real-world decode
  coverage, compatibility risk, and recommended implementation order.
- Phase 3 block scheduling parity slice for core effective-template derivation
  data, ErsatzTV-style block keys, collection key/etag scheduling metadata,
  guide windows, change-detection scaffolding, deco block-start/default filler
  behavior, and focused insta snapshots.
- Diataxis reference glossary for project and porting vocabulary.
- Phase 3 SQL-backed search-query parity slice for scheduling: a core-owned
  Lucene-like query parser/evaluator, SQLite search-document projection from
  normalized metadata, fielded smart collection/direct search support for
  represented title, sort/original title, year, release date, genre, tag,
  studio, actor, media kind, show/season/episode metadata, and provider GUIDs,
  plus show/season/artist container expansion and focused in-memory SQLite
  integration tests.
- Phase 3 SQL-backed scheduling content resolver repository for SQLite,
  covering collection expansion, multi-collection union/de-duplication, smart
  collection/search query matching, playlist child metadata (`collection_key`,
  `playback_order`, `play_all`, and `count`), show name/GUID resolution,
  watermark lookup, graphics path lookup, and in-memory SQLite integration
  tests.
- Phase 3 YAML scheduling parity for YAML imports (`import:` key merges
  content/sequence definitions from external files with case-insensitive
  key deduplication), expression-driven count behavior (variables `count`
  and `random`, full arithmetic with `+`, `-`, `*`, `/`, `%`, and
  parentheses), deterministic shuffle-groups for Marathon playout children
  (Fisher-Yates with LCG seeded from playout-id + instruction-index +
  day-of-year), and focused tests covering playlist batch/count and
  play-all child enumeration semantics.
- Phase 3 YAML scheduling parity for show-name/show-GUID content resolution,
  show/season marathon grouping, marathon child history keys/details,
  deterministic `play_all_items` playlist behavior, and empty-source fallback
  coverage.
- Phase 3 scheduling parity foundation for resolver-backed YAML content sources,
  deterministic in-memory scheduling adapters, and history application/cleanup
  snapshots.
- Phase 3 YAML scheduling history parity for exact media-item detail resume,
  structured playlist child/current-child history, reset pruning, and per-key
  cleanup snapshots.
- Phase 2 FFmpeg foundation with typed inputs, command generation, complex
  filter routing, software/NVENC/VAAPI pipeline builders, drawvg SVG template
  rendering, ffconcat v1.0 playlist writing, and snapshot coverage.
- Repository-specific Phase 1 integration test files covering provider media
  sources, relation-loading queries, graphics channel lookups, metadata writes,
  and `NotFound` behavior.
- Phase 1 persistence foundation with repository traits, SQLite migration,
  sqlx-backed repository implementations, offline sqlx metadata, and
  repository integration tests.
- Phase 1 persistence design spec.
- Phase 1 domain models for configuration, channels, playout, scheduling,
  media, collections, libraries, media sources, FFmpeg profiles, filler,
  graphics, and metadata.
- Cargo workspace with initial crate stubs for domain, core, infra, app,
  ffmpeg, web, and scanner crates.
- FFmpeg 8.1 runtime version detection and test coverage.
- Minimal `axum` + Leptos SSR web host with `/healthz`.
- SQLite startup bootstrap with sqlx migrations.
- Multi-stage Docker image with FFmpeg 8.1 and the `ersatzrs-web` binary.
- Initial documentation contracts for long-term solo development and future
  public open-source collaboration.
- Diataxis documentation structure.
- ADR process and initial architecture decisions.
- Porting contract for translating ErsatzTV behavior into Rust.
- GitHub issue, pull request, CI, release, and dependency-update scaffolding.
- Repo-local Claude skills for recurring ErsatzRS workflows.
- Rust-oriented `.gitignore`.
- Project license.

### Changed

- SQLite repository decoding and writes now use ErsatzTV-compatible integer
  enum values, EF-style UTC `DateTime` text, and ErsatzTV `TimeSpan` parsing.
- Env-gated real-world ErsatzTV SQLite compatibility coverage now exercises
  additional repository surfaces for media sources, metadata, collections,
  program schedules, block scheduling, deco, fillers, graphics, and config.
- Project branding, workspace crates, binaries, Docker wiring, docs, and
  snapshots were renamed from NasTv/`nastv-*` to ErsatzRS/`ersatzrs-*`.
- FFmpeg 8.1 runtime detection now lives in `ersatzrs-ffmpeg` and is reused by
  `ersatzrs-web`.
- Phase 1 persistence now uses an expanded normalized SQLite schema derived
  from the final ErsatzTV model instead of the earlier hybrid JSON/scalar
  mapping, and the active infra repositories now map scalar columns and join
  tables directly.
- Phase 1 persistence design now documents the implemented enum mapping helper
  approach rather than orphan-rule-impossible direct `sqlx` enum impls.
- Cargo deny metadata and policy now cover workspace crates and accepted
  transitive license/advisory exceptions.
- Workspace package metadata, lints, binary declarations, and Rust version are
  configured.
- Expanded the README from a placeholder into an English project overview.
- Expanded `.gitignore` for local settings, secrets, runtime databases, logs,
  generated artifacts, and editor files.
- SQLite migration path documentation now clarifies resolution from
  `CARGO_MANIFEST_DIR`.

### Fixed

- Template repository upsert no longer writes placeholder child rows that
  violate normalized foreign keys.
- Docker healthcheck no longer depends on `wget`.
- Web startup tracing, listen address handling, UTF-8 FFmpeg output validation,
  and FFmpeg version test coverage were tightened.

[Unreleased]: https://github.com/bartdeijkers/ErsatzRS
