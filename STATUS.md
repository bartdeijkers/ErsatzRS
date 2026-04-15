# Project Status

Last updated: 2026-04-15

## Current Phase

**Phase 2 — FFmpeg Pipeline and drawvg Graphics** (foundation wired to a demo stream)

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
Buck Bunny fixtures when present.

What remains before Phase 2 can be considered end-to-end complete:

1. Wire generated pipelines into a real streaming route once channel playout
   selection exists, replacing the development Big Buck Bunny route.
2. Expand builder coverage from the initial software/NVENC/VAAPI paths toward
   the full ErsatzTV filter/encoder matrix.
3. Add VLC/manual streaming verification for the future channel/IPTV routes,
   including longer-lived client disconnect and process-lifecycle checks.

### Known TODOs from Phase 0

- Docker dependency layer caching (copy manifests first, build deps, then copy src) — improves CI build times.
- Replace bash `/dev/tcp` HEALTHCHECK with a purpose-built `nastv-healthcheck` binary.
- `axum = "0.7"` pinned to match `leptos_axum 0.7` — bump both when leptos_axum upgrades to axum 0.8.
- `std::env::var("DATABASE_URL")` in `main.rs` uses `#[allow(clippy::disallowed_methods)]` — replace with a config crate in Phase 1.
- `ffmpeg` module in `nastv-web` migrated to `nastv-ffmpeg` in Phase 2.

### Blocking Decisions

None. All Phase 0 decisions are recorded in ADRs 0001–0007.

## Session Log

| Date | Agent | Summary |
| --- | --- | --- |
| 2026-04-15 | Copilot (Claude) | Audited repo readiness; added CODEX.md, STATUS.md, copilot-instructions.md, Commands section in AGENTS.md. |
| 2026-04-15 | Claude Code (claude-sonnet-4-6) | Completed Phase 0: Cargo workspace, FFmpeg 8.1 detection, axum+Leptos SSR, SQLite bootstrap, /healthz, Dockerfile. Branch: phase/0-bootstrap. |
| 2026-04-15 | Codex | Started Phase 1 by porting channel, playout, schedule, media, collection, playlist, block, library, media-source, FFmpeg profile, watermark, graphics, artwork, and filler domain entities into `nastv-domain`. |
| 2026-04-15 | Codex | Added Phase 1 persistence foundation: repository traits, SQLite migration, sqlx repositories, offline metadata, tests, and cargo-deny cleanup. |
| 2026-04-15 | GitHub Copilot (GPT-5.4) | Replaced the hybrid JSON/scalar persistence layer with an expanded normalized SQLite schema, normalized repository mappings, and updated integration tests/sqlx metadata. |
| 2026-04-15 | Codex | Closed remaining Phase 1 persistence gaps: split repository integration tests, added missing relation coverage, fixed template child-row writes, and reconciled enum mapping docs. |
| 2026-04-15 | Codex | Implemented Phase 2 FFmpeg foundation: command generator, complex filter routing, software/NVENC/VAAPI builders, drawvg templates, ffconcat writing, and snapshot coverage. |
| 2026-04-15 | Codex | Wired the Phase 2 FFmpeg foundation into `/dev/stream/bbb.ts`, streaming local Big Buck Bunny MP4 fixtures as MPEG-TS for development verification. |
