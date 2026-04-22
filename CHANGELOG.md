# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/).
This project uses [Conventional Commits](https://www.conventionalcommits.org/)
for commit messages.

## [Unreleased]

### Added

- Phase 7 FFmpeg 8.1 runtime and Docker multi-arch image support: ErsatzRS now
  checks both `ffmpeg` and `ffprobe` at startup, requires version 8.1 or newer,
  records filter/encoder/decoder/hwaccel/configuration capabilities in
  `AppState`, and reuses that report for FFmpeg profile hardware choices. The
  in-repository Docker build now compiles FFmpeg 8.1 from the verified official
  source tarball, includes redistributable CPU/NVENC/VAAPI/Vulkan/QSV runtime
  capability checks where supported, publishes release scaffolding for
  `linux/amd64`, `linux/arm64`, and CPU-only `linux/arm/v7`, and validates the
  final runtime image with `ldd`, `ffmpeg`, `ffprobe`, `drawvg`, and container
  `/healthz` smoke checks.

- Phase 7 Slice 7 MySQL backend support: `ersatzrs-infra` can initialize
  SQLite or MySQL from `DATABASE_URL` when built with the `mysql` feature,
  `ersatzrs-web` and `ersatzrs-scanner` propagate the backend-aware pool, and
  `mysql://` / `mariadb://` URLs now fail with a clear unsupported-backend
  error when the feature is not compiled. Added ordered ErsatzTV-compatible
  MySQL migrations under `migrations/mysql/` with named constraints and
  `utf8mb4` collation parity, plus a feature-gated MySQL integration test
  covering migrations, config upsert/read, channel/profile reads, playout item
  lookup, and scanner insert-id handling.

- Phase 7 Slice 4 HLS direct passthrough: `.m3u8?mode=hls-direct` now emits
  a single-entry direct playlist that targets `/iptv/hls-direct/{channel}`
  with output-format-aware `.ts` / `.mp4` / `.mkv` suffixes, tracks
  per-channel `#EXT-X-MEDIA-SEQUENCE` in memory, and streams direct FFmpeg
  output using the configured `ffmpeg.hls_direct.output_format` setting
  (default `mpegts`).

- Phase 7 Slice 3 HLS playlist trimming: the HLS session worker now filters
  live playlists before serving them, carries forward `#EXT-X-DISCONTINUITY`
  boundaries, deletes stale `.ts`/`.m4s`/`.mp4` segment files from the temp
  session directory, and can emit fMP4 HLS output (`segmenter-fmp4`) with
  `#EXT-X-MAP` / `init.mp4` support on the shared `/iptv/session/{n}/{file}`
  route.

- Phase 6.5 block scheduling UI: `/scheduling/blocks` list page with block
  group management (add/delete groups, add/delete blocks grouped by group
  name); `/scheduling/blocks/:id` block editor as a fully hydrated Leptos
  `#[island]` — `RwSignal<Vec<BlockItemForm>>` local state, Up/Down index
  reorder, per-item `ItemEditor` panel with autocomplete search for
  Collection/TelevisionShow/TelevisionSeason/SmartCollection/SearchQuery
  collection types, playback-order select, EPG and watermark toggles, and a
  "Preview Playout" modal backed by `preview_block_playout`. `BlockItemForm`
  carries `watermark_ids` and `graphics_element_ids` loaded from
  `BlockItemWatermark`/`BlockItemGraphicsElement` via `GROUP_CONCAT` and
  reinserted on every save, matching ErsatzTV's `ReplaceBlockItem` shape and
  preserving overlay metadata across edits.
- Phase 6.4 media library and browser pages: `/media/libraries` management
  page with Scan/Deep Scan actions; `/media/movies`, `/media/shows`,
  `/media/images` paginated card-grid browsers; `LetterBar` +
  `FragmentLetterAnchor` sticky A–Z quick-nav primitives; manual and smart
  collection editors (`/media/collections`, `/media/smart-collections`) with
  autocomplete-backed member management.
- Phase 6.2 layout and navigation wiring: CSS-checkbox drawer toggle drives
  mobile overlay and desktop hide without JS or island hydration; full
  NavDrawer matching ErsatzTV URL structure across all nav groups (Media
  Sources, Media, Lists, Scheduling, Settings, Support); `PlayoutWarningBadge`
  and `HealthBadge` Leptos islands backed by `#[server]` functions querying
  `PlayoutBuildStatus`; `app.rs` and all layout modules migrated from Tailwind
  strings to Stylance CSS modules.
- Phase 6.1 channel list and editor pages: `/channels` paginated table with
  show-disabled toggle and delete confirm dialog; `/channels/add` and
  `/channels/:id` editor with number/name/enabled fields, streaming-mode
  select, FFmpeg-profile select, filler select, form validation, and
  navigate-on-save; all backed by `#[server]` functions over AppState/sqlx.
- Phase 6.0 Leptos admin UI foundation: eight Stylance-scoped primitive
  components (Button, Form, Icon, Badge, Table, Dialog, Toast, Card);
  ErsatzTV PaletteDark design tokens in `globals.module.css`; stylance-cli
  build pipeline producing `public/bundle.css`; `ServeDir` at `/public`;
  `leptos_routes_with_context` AppState injection; component gallery at
  `/dev/components`.
- Phase 5 IPTV and HDHomeRun protocol surface: `/iptv/channels.m3u` M3U
  playlist; `/iptv/xmltv.xml` 48-hour XMLTV guide via quick-xml with
  COALESCEd titles; HDHomeRun HTTP quartet (`/discover.json`, `/device.xml`,
  `/lineup_status.json`, `/lineup.json`) with verbatim C# field names;
  HDHomeRun UDP discovery responder on port 65001 with TLV + CRC32;
  `/iptv/channel/{number}.ts` resolving the active PlayoutItem through
  `MediaVersion`/`MediaFile`, applying provider path replacements, and piping
  a software FFmpeg MPEG-TS stream with `kill_on_drop`; device UUID seeded
  into `ConfigElement` under `HDHR.DeviceUuid`.
- Phase 4 media source scanning: ffprobe metadata extraction; local filesystem
  scanner; paged Plex/Jellyfin/Emby sync with correct `Accept` headers and
  JSON casing; Plex collections and collection-membership sync; Emby
  `/Shows/{id}/Seasons` and `/Shows/{id}/Episodes` endpoint parity; DB-backed
  provider credentials from `PlexServerAuthToken`/Jellyfin/Emby tables;
  correct provider library `Key`/`ItemId` bootstrap writes; SHA256
  `MediaFile.PathHash` for ErsatzTV database compatibility; `--dry-run`
  short-circuit before credential loading; NVENC and VAAPI decode-path argv
  parity in the FFmpeg pipeline builders.
- Phase 3 end-to-end playout build: `PlayoutBuilder` orchestrator, block/YAML/
  Lua engine dispatch, `apply_build_result` single-transaction persistence,
  and round-trip integration tests with insta snapshots.
- Phase 3 Lua scripted scheduling engine at full ErsatzTV API parity: 20+
  bindings including `add_marathon` (show/season/artist/album/director
  grouping, round-robin cycling, shuffle_groups/play_all/item_order, history
  resume) and `create_playlist` (inline playlist from existing enumerators,
  history resume); Reset-mode history removal; 72 snapshot tests.
- Application artwork: SVG/PNG/ICO/ICNS logos.
- Phase 6 implementation plan document (`plans/phase-6-admin-ui.md`).

### Removed

- `docs/003_ersatztv_sqlite3_schema.sql` — superseded by the normative
  `0001_initial.sql` migration and the persistence gap matrix.

### Added (continued from earlier phases)

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

- Phase 7 Slice 2 HLS segmenter hardening: the temp-backed `HlsSessionWorker`
  now mirrors the MPEG-TS route's provider path-replacement handling before
  invoking FFmpeg, uses atomic session registration to avoid duplicate
  segmenters on concurrent requests, and treats failed FFmpeg child exits as
  terminal cleanup instead of looping until idle timeout.

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
