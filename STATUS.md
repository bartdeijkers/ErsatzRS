# Project Status

Last updated: 2026-04-17

## Current Phase

**Phase 5 — Protocol Endpoints** next

### Phase 0 Checklist

- [x] Documentation contracts (CONTRIBUTING, STYLE, ARCHITECTURE, PORTING)
- [x] ADRs 0001–0007
- [x] Diataxis documentation structure
- [x] Parity matrix skeleton
- [x] GitHub CI, issue templates, PR template, dependabot
- [x] Tooling config (rustfmt, clippy, deny, editorconfig, gitattributes)
- [x] Claude skills for recurring workflows
- [x] Conventional Commits and commitlint
- [x] Cargo workspace and empty crate stubs
- [x] FFmpeg 8.1 runtime detection and fail-fast
- [x] Minimal axum + Leptos SSR page
- [x] SQLite bootstrap (empty DB on first run)
- [x] Docker image with FFmpeg 8.1
- [x] Health endpoint (`/healthz` → 200)

### What an Agent Should Do Next

Phase 0 and Phase 1 are complete. Phase 2 now has a working FFmpeg foundation:
typed inputs, command generation, complex filter routing, software/NVENC/VAAPI
pipeline builders, watermark/subtitle/song-progress filters, drawvg SVG
template rendering, ffconcat v1.0 playlist writing, snapshot tests, and a
development MPEG-TS streaming route at `/dev/stream/bbb.ts` using local Big
Buck Bunny fixtures when present. Phase 3 now has a `ersatzrs-core` scheduling
foundation with a shared `PlayoutSchedulingEngine` trait, deterministic
in-memory scheduling adapters behind content resolver traits, block scheduling,
sequential YAML instruction dispatch, Lua 5.4 scripted scheduling, playout
anchors, gaps, pre-roll/post-roll/mid-roll, watermark and graphics state, filler
kinds, YAML content source resolution for collection, multi_collection,
smart_collection, playlist, search, and single media item references, practical
history application/cleanup, exact chronological history resume by media item
details, structured playlist child/current-child history behavior for the
in-memory adapter, a core-owned Lucene-like search query parser/evaluator, and
SQLite search-document projection for represented metadata fields with
show/season/artist container expansion, YAML parity coverage for SQL-backed
playlist child resolution and expression edge cases, block update/removal
change detection with item-level guide/watermark/graphics flags, and SQL-backed
Lucene fields for director/writer, language/subtitle tags, codec, and
dimensions.

Phase 3 is now **complete**. All scheduling engines and the end-to-end playout
build pipeline are implemented and tested:

1. ~~Compare generated timelines against ErsatzTV fixtures where available~~ —
   done: block scheduling now has ErsatzTV-fixture-based regression coverage for
   basic chronological blocks, default deco filler, and guide-group locking.
2. ~~End-to-end playout build~~ — done: `PlayoutBuilder` orchestrator in
   `ersatzrs-core` dispatches on schedule kind; `apply_build_result` in
   `ersatzrs-infra` persists the full `SchedulingResult` (items, gaps, history,
   anchors, program-schedule anchors, fill-group indices) in a single
   transaction respecting Reset and Continue semantics; round-trip integration
   tests with insta snapshots verify Block and YAML engines against in-memory
   databases.
3. ~~Expand Lua scheduling bindings~~ — done: full ErsatzTV Lua API parity.
4. ~~Close remaining YAML/block/search priority gaps~~ — done.

Phase 4 is now **complete**. The scanner can extract ffprobe metadata into
domain media-version, stream, and chapter rows; scan local `.mp4`, `.mkv`, and
`.avi` files into SQLite as `OtherVideo` items; apply provider/container path
replacement rules; sync Jellyfin movies, shows, seasons, episodes, and
collections; sync Plex movies, shows, seasons, and episodes; sync Emby movies,
shows, seasons, episodes, and collections; and dispatch single-shot scanner
runs through `ersatzrs-scanner --provider <local|jellyfin|plex|emby>
--library-id <id>`.

**Phase 5 next step**: start protocol endpoints with `/iptv/channels.m3u`, then
add HLS video endpoints, HDHomeRun discovery/lineup, UDP discovery on port
65001, and XMLTV guide output.

Persistence parity gaps are now tracked per table in
`docs/reference/persistence-gap-matrix.md`. Media version/file/stream/chapter
write coverage improved through ffprobe, local scanning, Jellyfin, Plex, and
Emby sync; the remaining high-risk slices are language and subtitle
normalization, metadata joins, program schedule item subtypes, template items,
block item targets, and deco break/template rows.

### Known TODOs from Phase 0

- Docker dependency layer caching (copy manifests first, build deps, then copy src) — improves CI build times.
- Replace bash `/dev/tcp` HEALTHCHECK with a purpose-built `ersatzrs-healthcheck` binary.
- `axum = "0.7"` pinned to match `leptos_axum 0.7` — bump both when leptos_axum upgrades to axum 0.8.
- `std::env::var("DATABASE_URL")` in `main.rs` uses `#[allow(clippy::disallowed_methods)]` — replace with a config crate in Phase 1.
- `ffmpeg` module in `ersatzrs-web` migrated to `ersatzrs-ffmpeg` in Phase 2.

### Blocking Decisions

None. All Phase 0 decisions are recorded in ADRs 0001–0007.

## Session Log

| Date | Agent | Summary |
| --- | --- | --- |
| 2026-04-15 | Copilot (Claude) | Audited repo readiness; added CODEX.md, STATUS.md, copilot-instructions.md, Commands section in AGENTS.md. |
| 2026-04-15 | Claude Code (claude-sonnet-4-6) | Completed Phase 0: Cargo workspace, FFmpeg 8.1 detection, axum+Leptos SSR, SQLite bootstrap, /healthz, Dockerfile. Branch: phase/0-bootstrap. |
| 2026-04-15 | Codex | Started Phase 1 by porting channel, playout, schedule, media, collection, playlist, block, library, media-source, FFmpeg profile, watermark, graphics, artwork, and filler domain entities into `ersatzrs-domain`. |
| 2026-04-15 | Codex | Added Phase 1 persistence foundation: repository traits, SQLite migration, sqlx repositories, offline metadata, tests, and cargo-deny cleanup. |
| 2026-04-15 | GitHub Copilot (GPT-5.4) | Replaced the hybrid JSON/scalar persistence layer with an expanded normalized SQLite schema, normalized repository mappings, and updated integration tests/sqlx metadata. |
| 2026-04-15 | Codex | Closed remaining Phase 1 persistence gaps: split repository integration tests, added missing relation coverage, fixed template child-row writes, and reconciled enum mapping docs. |
| 2026-04-15 | Codex | Implemented Phase 2 FFmpeg foundation: command generator, complex filter routing, software/NVENC/VAAPI builders, drawvg templates, ffconcat writing, and snapshot coverage. |
| 2026-04-15 | Codex | Wired the Phase 2 FFmpeg foundation into `/dev/stream/bbb.ts`, streaming local Big Buck Bunny MP4 fixtures as MPEG-TS for development verification. |
| 2026-04-15 | Codex | Started Phase 3 scheduling: added shared scheduling trait, block/YAML/Lua engines in `ersatzrs-core`, YAML handler snapshots, Lua scripted scheduling tests, and block timeline snapshots. |
| 2026-04-15 | Codex | Continued Phase 3 scheduling parity: added core content resolver traits/adapters, YAML content source resolution, practical history application/cleanup, and new snapshots. |
| 2026-04-15 | Codex | Continued Phase 3 YAML parity: added detail/index history resume, structured playlist child history behavior, reset/cleanup snapshots, and persistence-compatible history fields. |
| 2026-04-15 | GitHub Copilot (GPT-5.4) | Continued Phase 3 YAML parity with show/GUID resolution, show/season marathon grouping, playlist-child marathon history, deterministic `play_all_items`, fallback coverage, and new snapshots. |
| 2026-04-15 | Claude Code (claude-sonnet-4-6) | Continued Phase 3 YAML parity: YAML imports (`merge_import` + file-loading), expression-driven count (`count`/`random` variables + arithmetic), deterministic marathon shuffle-groups (LCG Fisher-Yates), and playlist batch/count child semantics; 42 tests all green. |
| 2026-04-15 | Codex | Added SQL-backed Phase 3 scheduling content resolver coverage in `ersatzrs-infra` for collection, multi-collection, smart collection, playlist, search, show, watermark, and graphics lookups with SQLite integration tests. |
| 2026-04-15 | Codex | Continued Phase 3 search parity with a core-owned Lucene-like query evaluator, SQLite metadata search documents, fielded smart/direct search tests, and show/season/artist search-hit expansion. |
| 2026-04-15 | Codex | Committed the project rename from NasTv/`nastv-*` to ErsatzRS/`ersatzrs-*`, including crate directories, package names, docs, Docker references, and snapshot names. |
| 2026-04-15 | Codex | Added a Diataxis reference glossary and linked it from the documentation index. |
| 2026-04-15 | Codex | Continued Phase 3 block scheduling parity with core effective-template derivation structs, block keys, collection etags, guide windows, change-detection scaffolding, deco block-start/default filler behavior, and focused snapshots. |
| 2026-04-15 | Codex | Wired block scheduling reference-data preparation to SQLite for playout templates, blocks, block items, deco break content, existing playout items/history, and collection-etag comparison coverage. |
| 2026-04-15 | GitHub Copilot (GPT-5.4) | Continued Phase 3 block scheduling parity with persisted block/template/playout-template update fields, alternate template and deco-template selection, SQLite-backed default filler/deco windows, and focused core/infra tests. |
| 2026-04-15 | Codex | Folded the Phase 3 block scheduling migration into the collapsed initial SQLite schema using the ErsatzTV SQLite schema as the reference source. |
| 2026-04-16 | Codex | Verified `0001_initial.sql` against the ErsatzTV SQLite schema reference and updated remaining infra fixtures to the real table names and FK requirements; full `ersatzrs-infra --tests` passes. |
| 2026-04-16 | Codex | Added real-world ErsatzTV SQLite compatibility coverage and updated infra decoders for EF integer enums, DateTime, TimeSpan, and date-only values. |
| 2026-04-16 | Codex | Added a 138-table persistence gap matrix and expanded env-gated real-world SQLite decode coverage across existing infra repositories. |
| 2026-04-16 | GitHub Copilot (GPT-5.4) | Added SQLite playout runtime-state write parity for anchors, enumerator state rows, gaps, history, program-schedule anchors, and playout templates with focused infra tests. |
| 2026-04-17 | Claude Code (claude-sonnet-4-6) | Expanded Lua scripted scheduling engine to full ErsatzTV API parity: 20+ bindings (add_count/all/duration/pad/skip/rewind/pre_roll/guide_group/is_done/watermark/graphics + content registration), Reset-mode history removal, history resume on registration; 18 new snapshot tests, 0 failures. |
| 2026-04-17 | Claude Code (claude-sonnet-4-6) | Closed remaining Lua scheduling gaps: add_marathon (show/season/artist/album/director grouping, round-robin cycling, shuffle_groups/play_all/item_order, history resume) and create_playlist (inline playlist from existing enumerators, history resume); shared PlaylistEnumerator types promoted to mod.rs; 4 new snapshot tests, 72 total passing. |
| 2026-04-17 | Codex | Closed the requested Phase 3 scheduling parity gaps in order: YAML playlist/expression semantics, block item removal/change detection, and SQL-backed Lucene fields for credits, languages, codecs, and dimensions; full workspace tests passed before each feature commit. |
| 2026-04-17 | Codex | Added ErsatzTV-fixture-based block scheduling regression coverage and documented serialized parity contracts. |
| 2026-04-17 | Codex | Refactored infra SQL query text to quote aliases and projected identifiers consistently while preserving ErsatzTV schema parity. |
| 2026-04-17 | Codex | Resolved all Clippy warnings blocking `-D warnings`: renamed `fake_collection_key` field, replaced `.map().unwrap_or()` with `.map_or()`, merged identical match arms, and fixed pre-existing infra/test lints. |
| 2026-04-17 | Claude Code (claude-sonnet-4-6) | Completed Phase 3 end-to-end playout build: `PlayoutBuilder` orchestrator, `apply_build_result` single-transaction persistence, and round-trip integration tests with insta snapshots. Phase 3 ✅. |
| 2026-04-17 | Codex | Started Phase 4 media source integrations: ffprobe extraction, local filesystem scanner, path replacement, Jellyfin movie sync, persistence matrix updates, and full workspace gates. |
| 2026-04-17 | Codex | Completed Phase 4 provider scanning: Jellyfin shows/collections, Plex and Emby movie/show sync, scanner CLI dispatch, persistence matrix updates, and full workspace gates. |
