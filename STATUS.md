# Project Status

Last updated: 2026-04-15

## Current Phase

**Phase 3 — Scheduling Engines** (foundation started)

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
show/season/artist container expansion.

What remains before Phase 3 can be considered end-to-end complete:

1. Continue search-index parity beyond the current SQL-backed parser/evaluator:
   add a real index/analyzer when needed, cover language/subtitle/codec/dimensions
   and other Lucene fields, add director/writer joins when the schema represents
   them, and broaden provider-specific metadata edge cases.
2. Expand block scheduling to full ErsatzTV effective-template/deco/filler
   behavior, including change detection and collection etags.
3. Complete YAML parity for remaining exact handler semantics (SQL-backed
   playlist item resolution, expression evaluation, imports, shuffle-groups,
   and batch/count playlist child enumeration now have focused coverage).
4. Expand Lua scheduling bindings from the current embedded `mlua` foundation to
   the full ErsatzTV scripted scheduling API.
5. Compare generated timelines against ErsatzTV fixtures where available and
   add longer integration tests with local Big Buck Bunny media.
6. Continue Phase 2 protocol work by replacing `/dev/stream/bbb.ts` with real
   channel playout/IPTV streaming once scheduled playout selection is wired.

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
