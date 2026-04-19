# Project Status

Last updated: 2026-04-19

## Current Phase

**Phase 6 — Leptos Admin UI** next (channel list and editor first).

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

**Phase 0, Phase 1, and Phase 3 are complete.**

**Phase 2 — complete for the currently targeted hardware paths.** The FFmpeg
foundation is in place: typed inputs, command generation, complex filter
routing, watermark/subtitle/song-progress filters, drawvg SVG template
rendering, ffconcat v1.0 playlist writing, snapshot tests, a development
MPEG-TS streaming route at `/dev/stream/bbb.ts`, and NVENC/VAAPI decode-path
parity for the currently implemented builders (`-hwaccel`,
`-hwaccel_output_format`, `-init_hw_device`, hardware scaling, and
`-noautoscale`). Future accelerators such as QSV/AMF/VideoToolbox/RKMPP remain
Phase 7 work rather than a current remediation blocker.

**Phase 3 — complete.** All scheduling engines and the end-to-end playout
build pipeline are implemented and tested: shared `PlayoutSchedulingEngine`
trait, Block/YAML/Scripted engines, SQL-backed content resolvers, full Lua
API parity, `PlayoutBuilder` dispatch, and `apply_build_result` single-
transaction persistence with Reset/Continue semantics verified by round-trip
integration snapshots against in-memory and real-world databases.

**Phase 4 — complete.** Local scan via ffprobe, path replacement, paged
Plex/Jellyfin/Emby sync, provider collection sync, DB-backed provider
credentials, SHA256 `MediaFile.PathHash`, and the dry-run/test remediations
are landed; `ersatzrs-scanner --provider <local|jellyfin|plex|emby> --library-id
<id>` dispatches correctly.

The Phase 4 remediation list is now closed:

1. ~~**No pagination.** All `sync_*_library` functions fetch only the first
   100 items. Port ErsatzTV's `GetPagedLibraryContents` loop so larger
   libraries are scanned in full.~~
2. ~~**Plex JSON casing is wrong.** `PlexMetadataContainerResponse` uses
   `rename_all = "PascalCase"` but Plex sends `totalSize` (camelCase). Fix
   alongside (1) so pagination exercises the real Plex shape.~~
3. ~~**Plex missing `Accept: application/json` header.** ErsatzTV's
   `IPlexServerApi` sets it on every JSON endpoint; without it, real Plex
   servers fall back to XML on some routes and deserialization fails.~~
4. ~~**Plex collections absent.** `GetCollections`/`GetCollectionItems` are
   not ported. Either implement them or mark the feature out of scope in
   an ADR and `parity-matrix.md`.~~
5. ~~**Emby seasons/episodes use the wrong endpoint.** ErsatzTV calls
   `/Shows/{parentId}/Seasons` and `/Shows/{showId}/Episodes`; ErsatzRS
   routes through generic `/Items?includeItemTypes=Season/Episode&parentId=…`.~~
6. ~~**Emby movie fetch duplicates `recursive=true`.** `emby.rs` lines
   85-86 pair the same key twice — likely a copy-paste error that dropped
   a needed parameter.~~
7. ~~**Provider credentials from env vars.** Scanner reads `PLEX_TOKEN`,
   `JELLYFIN_AUTHORIZATION_HEADER`, and `EMBY_API_KEY` from the process
   environment. This is a temporary shim that violates the "no deviation
   from ErsatzTV storage" rule — read them from the provider tables
   (`PlexServerAuthToken`, Jellyfin session state, Emby API key rows).~~
8. ~~**Provider library bootstrap writes the local id as the provider key.**
   `ensure_plex_library`/`ensure_jellyfin_library`/`ensure_emby_library`
   insert `library_id.to_string()` into the provider `Key`/`ItemId`
   column. Thread the real provider id/key through from the scanner
   before real deployments.~~
9. ~~**`MediaFile.PathHash` must be SHA256.** ErsatzTV stores a SHA256 hex
   digest; ErsatzRS currently uses a 16-char custom hash. Migrate to
   SHA256 so imported ErsatzTV databases stay byte-compatible.~~
10. ~~**`--dry-run` still requires credentials.** Each provider dispatch
   loads its config (which reads the token from env) before checking
   `args.dry_run`. Short-circuit to the dry-run branch first.~~
11. ~~**Local-scan test silently skips.** `crates/ersatzrs-scanner/tests/
   local_scan.rs` early-returns `Ok` whenever ffprobe or the Big Buck
   Bunny fixture is missing, so CI reports "1 passed" with zero
   behavior exercised. Gate with `#[ignore]` or commit a tiny fixture.~~

**Phase 5 — complete.** The IPTV/HDHomeRun protocol surface is in place:
`/iptv/channels.m3u` emits the M3U playlist; `/iptv/xmltv.xml` renders a
48-hour XMLTV guide via quick-xml with COALESCEd titles from the metadata
tables; `/discover.json`, `/device.xml`, `/lineup_status.json`, and
`/lineup.json` expose the HDHomeRun HTTP shape with verbatim C# field names
(DeviceID/LineupURL/GuideNumber/…); the HDHomeRun UDP discovery responder
listens on port 65001 with TLV + CRC32 encoding covered by protocol tests;
and `/iptv/channel/{number}.ts` resolves the active PlayoutItem, applies
Plex/Jellyfin/Emby path replacements, and pipes a software FFmpeg MPEG-TS
stream with `kill_on_drop` cleanup. Device UUID is seeded into
`ConfigElement` under `HDHR.DeviceUuid` on first start; tuner count is
read from `HDHR.TunerCount` (default 1).

**Phase 6 in progress.** Phases 6.0–6.5 complete. Phase 6.5 delivers the
block scheduling UI: blocks list page with group management, and a fully
hydrated `#[island]` block editor with Up/Down reorder, per-item collection
autocomplete for all `CollectionType` variants, and a "Preview Playout" modal.
`BlockItemForm` faithfully round-trips `watermark_ids` and `graphics_element_ids`
via `GROUP_CONCAT` load + junction-table reinsert on save, matching ErsatzTV's
`ReplaceBlockItem` shape. HLS multi-variant playlists, the segmenter worker
with work-ahead and discontinuity tracking, hls-direct passthrough, access
tokens, and Kodi #KODIPROP headers are Phase 7 hardening.

Persistence parity gaps remain tracked per table in
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
| 2026-04-17 | Claude Code (claude-opus-4-7[1m]) | Phase 1–4 audit: reconciled STATUS.md and `parity-matrix.md` with the code. Phase 4 revised from "complete" to "in progress" with documented gaps — pagination, Plex collections/`Accept` header/JSON casing, Emby `/Shows/{id}/Seasons+Episodes` and duplicate `recursive=true`, DB-backed provider credentials, correct provider library `Key`/`ItemId` writes, SHA256 `PathHash`, `--dry-run` short-circuit, non-skipping local-scan fixture. Phase 2 flagged NVENC/VAAPI as cosmetic encoder swaps pending real hwaccel port. |
| 2026-04-17 | GitHub Copilot (GPT-5.4) | Block A: ported Plex pagination, fixed mixed-case Plex JSON decoding, added `Accept: application/json`, and expanded Plex provider regression tests. |
| 2026-04-17 | GitHub Copilot (GPT-5.4) | Block B: implemented Plex collection sync plus scanner dispatch and added a collection-membership snapshot test. |
| 2026-04-17 | GitHub Copilot (GPT-5.4) | Block C: switched Emby season and episode fetches to `/Shows/{id}` endpoints, restored the movie fields set, and updated Emby provider tests. |
| 2026-04-17 | GitHub Copilot (GPT-5.4) | Block D: moved scanner provider credentials into DB-backed lookups, threaded real provider library keys into Plex/Jellyfin/Emby bootstrap rows, and added focused scanner/provider regression tests. |
| 2026-04-17 | GitHub Copilot (GPT-5.4) | Block E: switched scanner and provider `MediaFile.PathHash` writes to SHA256 hex for ErsatzTV parity; existing databases now require rescans to refresh stored path hashes. |
| 2026-04-17 | GitHub Copilot (GPT-5.4) | Block F: short-circuited provider `--dry-run` before credential loading, marked the ffprobe fixture test ignored with explicit invocation docs, and closed the last Phase 4 remediation items. |
| 2026-04-17 | GitHub Copilot (GPT-5.4) | Block G: ported the NVENC and VAAPI decode-path argv shape from ErsatzTV into the Rust FFmpeg pipeline builders, updated the hardware pipeline snapshots, and cleared the remaining Phase 2 remediation item. |
| 2026-04-17 | Claude Code (claude-opus-4-7[1m]) | Completed Phase 5 protocol endpoints: `/iptv/channels.m3u` + `/iptv/xmltv.xml` via quick-xml (48-hour window, COALESCE titles); HDHomeRun HTTP quartet (`discover.json`, `device.xml`, `lineup_status.json`, `lineup.json`) with verbatim C# field names; UDP discovery responder on port 65001 with TLV + CRC32 round-trip tests; `/iptv/channel/{number}.ts` resolving the active PlayoutItem through `MediaVersion`/`MediaFile`, applying provider path replacements, and piping a software FFmpeg MPEG-TS stream with `kill_on_drop`. Device UUID seeded into `ConfigElement` under `HDHR.DeviceUuid`. Phase 5 ✅. |
| 2026-04-17 | Claude Code (claude-sonnet-4-6) | Phase 6.0 complete: Tailwind v4 CSS pipeline (21 KB), 8 primitive components (button/form/icon/badge/table/dialog/toast/card), AppShell+NavDrawer+TopBar layout, component gallery at `/dev/components`, `leptos_routes_with_context` AppState injection. Phase 6.1 complete: channel list (`/channels`) with paginated table + show-disabled toggle + delete confirm dialog; channel editor (`/channels/add`, `/channels/:id`) with form validation, streaming-mode/FFmpeg-profile/filler selects, and navigate-on-save. All quality gates pass. |
| 2026-04-18 | Claude Code (claude-sonnet-4-6) | Phase 6.2 complete: AppShell CSS-checkbox drawer toggle (no JS/island needed); full NavDrawer nav structure matching ErsatzTV URLs; `PlayoutWarningBadge` and `HealthBadge` islands backed by `#[server]` functions in `server/layout.rs`; `app.rs` and `app.module.css` migrated to Stylance CSS modules; all quality gates pass. |
| 2026-04-18 | Claude Code (claude-sonnet-4-6) | Phase 6.3 complete: media source configuration pages for Local/Jellyfin/Emby/Plex — server functions, 10 page components (list/editor/libraries/paths per provider), shared CSS module, Plex OAuth PIN island, `PathsEditor` shared sub-component; all quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.4 partial: landed `/media/libraries` management page (join across Library + subtype + MediaSource tables) with per-library Scan/Deep Scan firing background `sync_library_by_id`; added `[lib]` target to `ersatzrs-scanner` so the web crate can invoke `scan_local_library` directly. Media browser pages (movies/shows/images) and collection editors are still outstanding. |
| 2026-04-19 | Claude Code (claude-opus-4-7) | Phase 6.4 complete: added `LetterBar` + `FragmentLetterAnchor` primitives (sticky A–Z quick-nav with `#letter-X` anchors, `letter_bucket` helper); `server/media_browser.rs` covering movies/shows/images paginated card-grid queries, manual + smart collection CRUD, `suggest_media_items` autocomplete (Movie ∪ Show), and `add/remove/list_collection_members`; pages `pages/media/{movies,shows,images,collections,smart_collections}.rs` built on a shared `MediaBrowserPage` helper (search input, letter bar, `MediaCardPager`); collection editor uses an `#[island]` autocomplete component with debounced `suggest_media_items` and member add/remove. All quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.5 complete: block scheduling UI — `server/blocks.rs` with CRUD + playout preview server functions; `pages/scheduling/blocks.rs` list page with group management (add/delete groups, add/delete blocks grouped by group_name); `pages/scheduling/block_editor.rs` fully hydrated `#[island]` with `RwSignal<Vec<BlockItemForm>>` local state, Up/Down reorder, `ItemEditor` autocomplete panel for all collection types (Collection/Show/Season/SmartCollection/SearchQuery), and "Preview Playout" modal via `preview_block_playout`. Parity fixes: nav link corrected to `/scheduling/blocks`; col_type Effect skips initial run to preserve loaded references; `BlockItemForm` carries `watermark_ids`/`graphics_element_ids` loaded via `GROUP_CONCAT` and reinserted on save so overlay metadata round-trips faithfully. All quality gates pass. |
