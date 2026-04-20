# Project Status

Last updated: 2026-04-20 (night: Phase 7 Slice 4 — HLS direct passthrough)

## Current Phase

**Phase 7 — Hardening and Parity Completion** next.

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

**Phases 0 through 6 are complete.**

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

**Phase 6 — complete.** Phases 6.0–6.9 complete. Phase 6.9 delivers the
playout timeline visualiser at `/playouts`: a read-only `#[island]` SVG view
for the selected playout with a 24-hour proportional time axis, filler-vs-main
item colouring, hour tick marks, Prev/Next window paging, and click-to-inspect
detail output for title/start/finish/scheduling context. Phase 6.8 delivered
full watermark list and editor parity with ErsatzTV: list at `/watermarks`
with image thumbnail, Mode, Location columns + Edit/Copy/Delete row actions;
SSR wrapper + `#[island]` editor at `/watermarks/add` and `/watermarks/:id`;
`WatermarkForm` (17 fields) with correct ErsatzTV defaults (width=15,
margins=5, frequency=15); full `WatermarkEditViewModelValidator.cs` validation
parity; custom image upload via axum multipart + WASM `gloo_net` FormData
POST; drawvg placement preview (320×180 PNG via FFmpeg, debounced 400 ms).
Phase 6.7 delivered the FFmpeg Profile editor.

**Phase 7 — in progress.** Full HLS parity is intentionally scheduled into
Phase 7 rather than Phase 5; see `plans/defer-hls-parity-to-phase-7.md`.

Phase 7 Slice 1 through Slice 4 are landed. Slice 1 ported the HLS entry surface:
`/iptv/channels.m3u` emits mode-aware URL suffixes (HLS segmenter →
`.m3u8?mode=segmenter`, HLS-direct → `.m3u8?mode=hls-direct`, TS → `.ts`,
TS-legacy → `.ts?mode=ts-legacy`), short unique ids, ErsatzTV-compatible
`tvg-id` via `ChannelIdentifier.FromNumber`, `tvc-stream-{v,a}codec` tags, and
Kodi `#KODIPROP` headers when the `User-Agent` starts with `kodi`. Slice 2
ports the first concrete `HlsSessionWorker` in `ersatzrs-infra`: the segmenter
branch in `/iptv/channel/{n}.m3u8` now starts or touches a worker, the worker
spawns FFmpeg through `tokio::process::Command` into
`$TMPDIR/ersatzrs-hls/{channel}`, `/iptv/session/{n}/hls.m3u8` now serves the
live playlist once FFmpeg has written segments, and `/iptv/session/{n}/{file}`
serves relative `.ts` segment files from disk. `HlsSessionHandle` now exposes
`trim_playlist`, `HlsSessionRegistry` owns `try_get`/`insert`/`remove`, and the
in-memory registry remains in `ersatzrs-infra`. Access tokens still propagate
through the playlist URLs (verification deferred).

The Slice 2 GO blockers are closed: the worker now mirrors the MPEG-TS route's
provider path-replacement resolution before spawning FFmpeg, session startup is
guarded by atomic registry insertion so concurrent requests do not launch
duplicate segmenters, and failed FFmpeg exits stop the worker and clean up the
session instead of looping until idle timeout.

Slice 3 ports the first real playlist-management pass from
`HlsPlaylistFilter.cs` / `HlsSessionWorker.cs`: `/iptv/session/{n}/hls.m3u8`
now trims `live.m3u8` before serving it, preserves `#EXT-X-DISCONTINUITY`
boundaries across retained segments, deletes stale `.ts` / `.m4s` / `.mp4`
files from the worker temp directory, and carries the fMP4 `#EXT-X-MAP`
init-segment path through `segmenter-fmp4` output plus the shared
`/iptv/session/{n}/{file}` file route.

Slice 4 ports the `hls-direct` passthrough branch: `.m3u8?mode=hls-direct`
now returns a single-entry live playlist with ErsatzTV-style
`#EXT-X-MEDIA-SEQUENCE` tracking, `#EXT-X-DISCONTINUITY`, and an
output-format-aware `/iptv/hls-direct/{channel}[.ts|.mp4|.mkv]` target based
on the `ffmpeg.hls_direct.output_format` config value (default `mpegts`). The
direct stream route reuses the current FFmpeg spawn path and emits
`video/mp2t`, `video/mp4`, or `video/x-matroska` as appropriate.

Next up (Slice 5): close the remaining HLS parity gaps around
`ConditionalIptvAuthorizeFilter` behavior, access-token verification on the
new direct/segmenter URLs, and any remaining fMP4 init-segment caching nuance
that still matters once direct mode is in place.
After HLS is complete, continue Phase 7 items from the master plan: additional
hardware paths, MySQL backend support, Trakt integration, Docker multi-arch
images, side-by-side benchmarking, and long soak tests.

Persistence parity gaps remain tracked per table in
`docs/reference/persistence-gap-matrix.md`. Media version/file/stream/chapter
write coverage improved through ffprobe, local scanning, Jellyfin, Plex, and
Emby sync; the remaining high-risk slices are language and subtitle
normalization, metadata joins, program schedule item subtypes, template items,
block item targets, and deco break/template rows.

### Admin UI Route Coverage

The NavDrawer in [`crates/ersatzrs-web/src/ui/layout.rs`](crates/ersatzrs-web/src/ui/layout.rs)
advertises the full ErsatzTV navigation tree, but several entries are
intentional placeholders pending Phase 7. Authoritative per-link mapping lives
in [`docs/reference/parity-matrix.md`](docs/reference/parity-matrix.md#admin-ui-route-coverage);
the summary below is the readiness view.

**Routed today** (work end-to-end against the current binary): `/channels`,
`/ffmpeg` (alias for `/ffmpeg/profiles`), `/watermarks`,
`/media/sources/{local,jellyfin,emby,plex}` and their per-source
`libraries`/`paths` sub-pages, `/media/libraries`, `/media/trash`,
`/media/movies`, `/media/tv/shows` (alias `/media/shows`),
`/media/browser/images` (alias `/media/images`), `/media/music/artists`,
`/media/music/songs`, `/media/other/videos`, `/media/collections`,
`/media/smart-collections`, `/blocks` (alias for `/scheduling/blocks`),
`/playouts` and `/playouts/sequential/:id`.

**Advertised but currently 404** (placeholders pending Phase 7):
`/media/remote/streams`, `/media/multi-collections`, `/media/rerun-collections`,
`/media/playlists`, `/media/trakt/lists`, `/media/filler/presets`,
`/schedules`, `/templates`, `/decos`, `/deco-templates`,
`/settings/{ffmpeg,logging,hdhr,scanner,playout,ui,xmltv}`, and
`/system/{health,logs,troubleshooting}`. These should not be read as Phase 6
regressions; they were intentionally shipped in the navigation tree for layout
parity ahead of the corresponding pages.

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
| 2026-04-19 | GitHub Copilot (Claude Sonnet 4.6) | Phase 6.6 complete: `server/playouts.rs` with `list_playouts`, `get_scripted_playout`, `save_scripted_playout`, `erase_playout_items_and_history`, and `validate_yaml_playout` server functions; `pages/scheduling/playouts.rs` SSR list page at `/playouts`; `pages/scheduling/yaml_editor.rs` with `YamlEditorPage` SSR wrapper and `YamlEditorIsland` — 500 ms debounced YAML validation via `YamlPlayoutEngine::definition_from_str`, save action navigating to `/playouts` on success, erase action with `Modal` confirmation dialog. Raw YAML stored in `ScheduleFile` (ErsatzRS divergence from ErsatzTV file-path storage). All quality gates pass. |
| 2026-04-19 | GitHub Copilot (Claude Sonnet 4.6) | Phase 6.6 parity fixes: (1) `save_scripted_playout` now spawns a background `tokio::task::spawn` reset-build after UPDATE — loads playout via `get_with_anchor`, populates `SchedulingReferenceData` from `yaml_reference_data` (new infra helper), runs `YamlPlayoutEngine::schedule_definition`, persists via `apply_build_result`; (2) `erase_playout_items_and_history` expanded to mirror `ErasePlayoutHistoryHandler.cs`: randomises `Seed` via SQLite `random()`, deletes `PlayoutAnchor`, nulls `OnDemandCheckpoint`, deletes `PlayoutProgramScheduleAnchor`/`RerunHistory`/`PlayoutGap`/`PlayoutBuildStatus`; (3) save button disabled when `validation_error` is `Some`; save action pre-validates before persisting to guard the debounce race. `YamlContentItem::source()` made `pub`. All quality gates pass. |
| 2026-04-19 | GitHub Copilot (Claude Sonnet 4.6) | Phase 6.6 scheduler/parity fixes: (1) SQL `ScheduleKind` filter changed from 4 (Scripted/Lua → `ScriptedSchedulingEngine`) to 3 (Sequential/YAML → `YamlPlayoutEngine`) in `get_scripted_playout` and `save_scripted_playout`; (2) `YamlPlayoutEngine::definition_for_playout` updated to detect inline YAML content (ErsatzRS divergence: `ScheduleFile` stores raw YAML, not a filesystem path) — newline-in-value is the heuristic; (3) `rebuild_yaml_playout` now uses `PlayoutBuilder::default().build()` as the canonical dispatcher instead of calling `YamlPlayoutEngine::schedule_definition` directly; (4) `yaml_reference_data` gains a `playout_id` arg, loads `existing_items` and `existing_history`, and propagates `media_items_for_source` errors instead of swallowing with `unwrap_or_default`. All quality gates pass. |
| 2026-04-19 | GitHub Copilot (Claude Sonnet 4.6) | Phase 6.6 naming/routing/detection fixes: (1) playout list `SCRIPTED=4` constant corrected to `SEQUENTIAL=3` so edit links appear for Sequential/YAML playouts (not Scripted/Lua); edit URL changed to `/playouts/sequential/{id}`; (2) full rename `get_scripted_playout`→`get_sequential_playout`, `save_scripted_playout`→`save_sequential_playout`, `ScriptedPlayoutForm`→`SequentialPlayoutForm`, route segment `"scripted"`→`"sequential"`, user-facing label "Scripted Schedule"→"Sequential Schedule"; (3) inline YAML detection replaced with path-prefix heuristic (`!starts_with('/')  && !starts_with('~')`) — robust even for single-line YAML. All quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.7 complete: `server/ffmpeg_profiles.rs` with `list_ffmpeg_profile_rows`, `count_ffmpeg_profiles`, `get_ffmpeg_profile_form`, `save_ffmpeg_profile` (INSERT seeds a default 1080p resolution if none exists), `delete_ffmpeg_profile`, and `copy_ffmpeg_profile` (SELECT→INSERT preserving all columns); `pages/ffmpeg_profiles/list.rs` SSR list at `/ffmpeg/profiles` with `PagedTable` + `DeleteDialog`; `pages/ffmpeg_profiles/editor.rs` with SSR wrapper `FfmpegProfileEditorPage` and `#[island]` `FfmpegProfileEditorIsland` — `RwSignal<i32>` for `video_format` drives hardware accordion visibility (H264/H265 → show, other → hide), inner `hardware_acceleration` signal drives device/driver fields, "Copy Profile" button opens `Modal` with `TextField` for name wired to `copy_ffmpeg_profile`. Routes `/ffmpeg/profiles`, `/ffmpeg/profiles/add`, `/ffmpeg/profiles/:id` registered in `app.rs`. All quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.7 NO GO remediation: (1) nav link `/ffmpeg` → `/ffmpeg/profiles`; (2) VideoFormat.Copy and AudioFormat.Copy corrected to 99 (C# parity); (3) `FfmpegProfileForm` expanded to 31 fields with `FFmpegProfile.New` defaults (AudioBufferSize=384, QsvExtraHardwareFrames=64, VideoProfile="high", NormalizeVideo/Audio/Colors/Deinterlace=true); (4) full server-side validation + handler-mirroring derivation (NormalizeVideo=false→Copy/None; NormalizeAudio=false→Copy; PadMode/BitDepth derived); (5) editor island rewritten — HW accordion driven by `normalize_video` signal, VAAPI sub-rows when hw==3, QSV frames when hw==1, all 31 fields present; (6) copy Effect navigates to `/ffmpeg/profiles/{new_id}`; list updated with Resolution + AudioFormat columns. All quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.7 second NO GO remediation: (1) `deny.toml` — added ISC + CDLA-Permissive-2.0 licenses (TLS stack: ring/rustls-webpki/untrusted/webpki-roots), added RUSTSEC-2025-0052 ignore (async-std discontinued, dev-dep only via httpmock); (2) video preset parity — ported `AvailablePresets.ForAccelAndFormat` as `presets_for_profile(hw, vf, bd)`, added server-side preset clearing in `save_ffmpeg_profile`, replaced free-text VideoPreset `TextField` with reactive `SelectField<String>` driven by hw/vf/bd signals, added Effect to clear stale preset when combination changes; (3) LoudNorm target parity — added Effect mirroring `FFmpegProfileEditViewModel.NormalizeLoudnessMode` setter: enabling LoudNorm sets target to −16, disabling clears it; fixed DB load default; (4) HW acceleration capability filtering — new `list_supported_hw_accelerations` server fn runs `ffmpeg -hwaccels`, maps to `HardwareAccelerationKind` integers, SSR wrapper passes capability-filtered list to island via `initial_hw_accels` prop. All quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.8 complete: `server/watermarks.rs` with `list_watermark_rows`, `count_watermarks`, `get_watermark_form`, `save_watermark` (name validation, duplicate check, OpacityExpression/image field clearing), `delete_watermark`; `pages/watermarks/list.rs` paged table at `/watermarks` with Mode and Location columns + DeleteDialog; `pages/watermarks/editor.rs` SSR wrapper + island — Placement section hidden when mode=None, Intermittent timing section when mode=Intermittent, OpacityExpression section when mode=OpacityExpression; location/size option Vecs wrapped in `StoredValue` to satisfy `Fn` inside `Show`. Routes wired at `/watermarks`, `/watermarks/add`, `/watermarks/:id`. All quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.8 NO GO remediation: (1) `preview_watermark_placement` server fn — generates 320×180 PNG via FFmpeg drawvg filter from an SVG template written to `tempfile::NamedTempFile`, returns `data:image/png;base64,…`; editor debounces 400 ms via `gloo_timers::future::TimeoutFuture` (`#[cfg(target_arch = "wasm32")]` guard) then fires `preview_resource` showing `<img>` in Placement Preview section; (2) custom image handling fixed — `image_path`/`image_content_type` signals loaded from form and preserved on save; `watermark_upload.rs` axum multipart upload endpoint (`/api/watermarks/image`) + static serve endpoint (`/artwork/watermarks/:filename`); WASM `upload_file` fn in editor POSTs `FormData` via `gloo_net`; (3) full `WatermarkEditViewModelValidator.cs` validation parity — custom image required, width (0,100], margins [0,50], duration < freq×60, opacity (0,100]; (4) defaults corrected to width=15, h_margin=5, v_margin=5, frequency_minutes=15; (5) list parity — image thumbnail column (custom path or "[channel logo]") and copy action with `Modal` + `TextField`, row actions Edit | Copy | Delete. All quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.8 second NO GO remediation: (1) drawvg filter option corrected from `filename=` to `file=` (matches FFmpeg docs and existing `ersatzrs-ffmpeg` helper at `graphics/mod.rs:31`); (2) upload size guard added — `max_upload_bytes()` reads `ETV_MAXIMUM_UPLOAD_MB` env var (default 10 MB, parity with `SystemEnvironment.MaximumUploadMb`), returns HTTP 413 when exceeded. All quality gates pass. |
| 2026-04-19 | Claude Code (claude-sonnet-4-6) | Phase 6.8 third NO GO remediation: axum's `Multipart` extractor enforces a 2 MB default body limit independently of handler logic; applied `DefaultBodyLimit::max(max_upload_bytes())` as a per-route layer on `/api/watermarks/image` in `main.rs` so the actual transport cap matches `ETV_MAXIMUM_UPLOAD_MB`; handler byte-length check retained as secondary guard; `max_upload_bytes()` made `pub`. All quality gates pass. |
| 2026-04-19 | GitHub Copilot (GPT-5.4) | Phase 6.9 complete: added `list_playout_items` server data, embedded playout selection on `/playouts`, and shipped a read-only SVG timeline island with 24-hour paging, proportional bar placement, filler colouring, hour ticks, and click-to-inspect item details. All quality gates pass. |
| 2026-04-19 | GitHub Copilot (GPT-5.4) | Updated STATUS.md to mark Phase 6 complete, advance the repo to Phase 7, and record the playout timeline SQL dedupe follow-up. |
| 2026-04-19 | GitHub Copilot (GPT-5.4) | Restored ErsatzTV route parity for already-ported web pages by wiring canonical aliases for `/ffmpeg`, `/blocks`, `/media/tv/shows`, and `/media/browser/images`, while keeping the previously shipped `/ffmpeg/profiles`, `/scheduling/blocks`, `/media/shows`, and `/media/images` paths as temporary aliases; updated nav and internal links to prefer the ErsatzTV URLs. `cargo check -p ersatzrs-web` passes. |
| 2026-04-19 | GitHub Copilot (GPT-5.4) | Ported the next non-placeholder media pages: added artist/song/other-video browsers at `/media/music/artists`, `/media/music/songs`, and `/media/other/videos` backed by new `server/media_browser.rs` SQL loaders with ErsatzTV-style subtitle/artwork mapping; added a real `/media/trash` page for the currently implemented browser types (movies, shows, artists, other videos, songs, images) with grouped file-not-found sections plus direct delete-selected and empty-trash actions via `MediaItem` deletes. `cargo check -p ersatzrs-web`, `cargo build -p ersatzrs-web`, and HTTP probes against all four new routes pass. |
| 2026-04-20 | GitHub Copilot (GPT-5.4) | Amended the master plan and added a planning note under `plans/` so full HLS parity is intentionally scheduled in Phase 7, aligning the plan with current status and implementation sequencing. |
| 2026-04-20 | Claude Code (claude-opus-4-7) | Reconciled STATUS.md, parity-matrix.md, and the actual code: rewrote parity matrix to mark Phase 4 Scanner / Phase 5 IPTV+XMLTV+HDHomeRun HTTP+UDP+MPEG-TS as Ported, HLS as Deferred (Phase 7), drawvg engine as Ported, and added Phase 6 admin-UI feature rows (channels/ffmpeg-profile/watermark/media-source/library/media-browser/collection/block/sequential-playout/timeline). Added an "Admin UI Route Coverage" table to the matrix and STATUS.md listing the 19 routed nav links and the 20 nav entries that are intentional 404 placeholders pending Phase 7 (remote streams, multi/rerun collections, playlists, trakt lists, filler presets, schedules/templates/decos/deco-templates, all `/settings/*`, all `/system/*`). |
| 2026-04-20 | Claude Code (claude-opus-4-7[1m]) | Phase 7 Slice 1 — HLS entry routes + multi-variant playlist + session-registry seam. Ported `ChannelPlaylist.cs` mode-aware URL suffix, Kodi `#KODIPROP` headers, `ChannelIdentifier.FromNumber`, short unique id, and `tvc-stream-{v,a}codec` tags on `/iptv/channels.m3u`. Added `/iptv/channel/{n}.m3u8` dispatcher (segmenter → multi-variant playlist targeting `/iptv/session/{n}/hls.m3u8`; TS modes redirect to `.ts`; `hls-direct` returns 501 until a later slice), the `build_multi_variant_playlist` builder with ErsatzTV's `BANDWIDTH = (VideoBitrate + AudioBitrate) * 1000 * 1.2` formula, and an `/iptv/session/{n}/hls.m3u8` endpoint that 503s until Slice 2 lands the worker. Introduced `HlsSessionRegistry` + `HlsSessionHandle` traits in `ersatzrs-core` with `InMemoryHlsSessionRegistry` in `ersatzrs-infra`, wired into `AppState`. Access tokens propagate through all URLs (verification deferred to Slice 5). Six new insta snapshots cover m3u mode-aware/Kodi/access-token and three variant-playlist shapes. Gates: fmt, clippy -D warnings, tests (workspace), deny check all green. |
| 2026-04-20 | GitHub Copilot (GPT-5.4) | Phase 7 Slice 2 — HLS segmenter worker. Extended the core HLS registry/handle traits with async insert/remove and playlist access, added `ersatzrs-infra::hls_worker::HlsSessionWorker` with FFmpeg CLI spawn into temp-backed HLS output plus an argv snapshot test, wired `/iptv/channel/{n}.m3u8` to start/touch workers, and added `/iptv/session/{n}/hls.m3u8` + `/iptv/session/{n}/{file}` serving for live playlists and segment files. |
| 2026-04-20 | GitHub Copilot (GPT-5.4) | Hardened Phase 7 Slice 2 after review: reused provider path replacements for HLS worker media paths, made session worker registration atomic to avoid duplicate FFmpeg segmenters under concurrent requests, and stopped failed FFmpeg workers from restart-looping until idle timeout. |
| 2026-04-20 | GitHub Copilot (GPT-5.4) | Phase 7 Slice 3 — HLS playlist trimming and cleanup. Ported the first real `HlsPlaylistFilter` behavior into `ersatzrs-infra::hls_worker`: session playlists now trim around recent segments, preserve discontinuities, delete stale temp-backed segment files, and support fMP4 segmenter output with `#EXT-X-MAP` / `init.mp4` serving on `/iptv/session/{n}/{file}`. Review outcome: GO for Slice 4. |
| 2026-04-20 | GitHub Copilot (GPT-5.4) | Phase 7 Slice 4 — HLS direct passthrough. Ported the direct-HLS branch in `iptv/hls.rs`: `.m3u8?mode=hls-direct` now emits a single-entry playlist with per-channel media-sequence tracking and format-aware `.ts` / `.mp4` / `.mkv` targets, `/iptv/hls-direct/{channel}` reuses the shared FFmpeg stream path with config-driven output format selection, and focused tests/snapshot coverage landed for the playlist shape and output-format helpers. Review outcome: GO for Slice 5. |
