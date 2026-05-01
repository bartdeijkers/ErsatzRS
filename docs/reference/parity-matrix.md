# Parity Matrix

This is the living map from ErsatzTV behavior to ErsatzRS implementation.

Phase-level progress is tracked in [STATUS.md](../../STATUS.md). This matrix
records per-feature parity and links to the authoritative ErsatzTV source.
Persistence-level table coverage is tracked separately in
[persistence-gap-matrix.md](persistence-gap-matrix.md). Admin UI route coverage
is tracked in the "Admin UI Route Coverage" table at the bottom of this file
and mirrored in STATUS.md.

| ErsatzTV feature | ErsatzTV source | ErsatzRS target | Status | ADR |
| --- | --- | --- | --- | --- |
| Domain model | `/home/bart/ErsatzTV/ErsatzTV.Core/Domain/` | `crates/ersatzrs-domain` | Ported | |
| Initial schema / database backends | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Data/TvContext.cs` and `/home/bart/ErsatzTV/ErsatzTV.Infrastructure.MySql/Migrations/` | `migrations/`, `migrations/mysql/`, `crates/ersatzrs-infra` | Ported: the initial SQLite schema matches the ErsatzTV table/column/foreign-key shape and infra repositories now read/write ErsatzTV integer enums, EF-style UTC `DateTime` text, ErsatzTV `TimeSpan` values, and playout runtime state tables (`PlayoutAnchor`, `ScheduleItemsEnumeratorState`, `PlayoutGap`, `PlayoutHistory`, `PlayoutProgramScheduleAnchor`, `CollectionEnumeratorState`, `PlayoutScheduleItemFillGroupIndex`, `FillGroupEnumeratorState`, and `PlayoutTemplate`) for the covered persistence surfaces. Phase 7 Slice 14 adds typed `ProgramScheduleItem` subtype repositories (`Duration`/`Flood`/`Multiple`/`One`), schedule item watermark/graphics joins, EF-compatible `ProgramScheduleAlternate` converters, and typed `BlockItem` target/overlay writes. Phase 7 Slice 7 adds the MySQL backend behind the `mysql` feature: URL detection for `mysql://` / `mariadb://`, unsupported-backend errors when the feature is absent, ErsatzTV-style MySQL DDL with named constraints and `utf8mb4`/`utf8mb4_general_ci` parity, runtime backend-aware SQL helpers, and feature-gated MySQL migration/read-write integration coverage. Remaining table-level read/write and real-world decode gaps are tracked in [persistence-gap-matrix.md](persistence-gap-matrix.md). | [ADR 0004](../adr/0004-collapsed-initial-schema.md) |
| FFmpeg command generation | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/CommandGenerator.cs` | `crates/ersatzrs-ffmpeg` | In progress: software pipeline emits a faithful argv; NVENC and VAAPI keep the upstream hardware decode-path shape (`-hwaccel`, `-hwaccel_output_format`, `-init_hw_device`, hardware scaling, and `-noautoscale`); and Phase 7 Slice 6 adds QSV/AMF/VideoToolbox/RKMPP/V4L2M2M builder parity for the command layer with dedicated snapshot coverage. Phase 7 Slice 11 adds source video/audio stream metadata to `PipelineRequest`, ports the known decoder table, and selects input video decoders from source codec + hardware acceleration (`h264`, `h264_qsv`, `h264_v4l2m2m`, etc.) with snapshot coverage for the changed argv. Phase 7 Slice 12 adds report-backed hardware capability checks and capability-gated encoder/decoder fallback before hardware globals and filters are emitted. Remaining work is narrower: broader filter coverage and deeper per-device hardware probes. | [ADR 0003](../adr/0003-ffmpeg-cli-spawn.md) |
| FFmpeg filter graph | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/Filter/ComplexFilter.cs` | `crates/ersatzrs-ffmpeg` | In progress: complex-filter label routing, watermark/subtitle/song-progress filters, and ffconcat v1.0 playlist writing are in place with snapshot coverage. Remaining work tracks the broader filter catalogue beyond the currently exercised paths. | [ADR 0003](../adr/0003-ffmpeg-cli-spawn.md) |
| Drawvg graphics engine | ErsatzTV external graphics renderer flow | `crates/ersatzrs-ffmpeg`, `assets/` | Ported: drawvg SVG template rendering replaces ErsatzTV's external graphics-renderer pipe. Used by the Phase 6.8 watermark editor for the in-browser placement preview (PNG via `file=` drawvg option). | [ADR 0005](../adr/0005-drawvg-graphics-engine.md) |
| Block scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/BlockScheduling/` | `crates/ersatzrs-core`, `crates/ersatzrs-infra` | Ported: shared `PlayoutSchedulingEngine` trait, effective-template derivation, alternate template and deco-template selection, SQL-backed template/block/deco loading, change-detection (removal/update), persisted collection-etag comparison, deco block-start/default filler windows, and ErsatzTV-fixture-based regression coverage. End-to-end build wired through `PlayoutBuilder`. | |
| YAML scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/YamlScheduling/` | `crates/ersatzrs-core`, `crates/ersatzrs-infra` | Ported: sequential dispatch, resolver-backed content sources, SQL-backed collection/multi-collection/smart collection/playlist/search/show/watermark/graphics lookups, show-name/show-GUID resolution, detail-based history resume, structured playlist and marathon child history, show/season marathon grouping, `play_all_items`, cleanup/reset, YAML imports (content/sequence merge with case-insensitive deduplication), expression-driven count (`count`/`random` variables and arithmetic), deterministic marathon shuffle-groups, and playlist batch/count child semantics. Driven through `PlayoutBuilder` for end-to-end builds. | |
| Scripted scheduling | `/home/bart/ErsatzTV/ErsatzTV.Core/Scheduling/ScriptedScheduling/` | `crates/ersatzrs-core` | Ported: full Lua API parity (20+ bindings: add_count/all/duration/pad/skip/rewind/pre_roll/guide_group/is_done/watermark/graphics, content registration, add_marathon, create_playlist), Reset-mode history removal, history resume on registration, deterministic ordering. Engine selection + persistence wired through `PlayoutBuilder`. | |
| Search index | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Search/` | `crates/ersatzrs-core`, `crates/ersatzrs-infra`, `crates/ersatzrs-web/src/admin/pages/search.rs` | In progress: scheduling has a core-owned Lucene-like parser/evaluator and SQLite search documents for the represented fields (`title`, `sort_title`, `original_title`, `year`, `release_date`, `genre`, `tag`, `studio`, `actor`, `type`, `show_title`, `show_genre`, `show_tag`, `show_studio`, `season_number`, `episode_number`, `metadata_kind`, provider GUID source fields, languages, codecs, dimensions, `trakt_list`). Show, season, and artist hits expand to playable children. `/search?query=...` now resolves the same scheduling search source used by Trakt list Search actions. Remaining gaps: no persistent inverted index, no analyzer/tokenization parity, no fuzzy/proximity/boost/page-map behavior, no chapter fields, and director/writer fields are placeholders until the normalized schema has metadata join tables. | |
| Scanner | `/home/bart/ErsatzTV/ErsatzTV.Scanner/` | `crates/ersatzrs-scanner` | Ported (Phase 4 closed): local filesystem scan via ffprobe dispatches by `Library.MediaKind` for movies, other videos, songs, and images; paged Plex/Jellyfin/Emby sync for movies/shows/seasons/episodes; Plex/Jellyfin/Emby collection sync; CLI dispatch via `--provider`/`--library-id`/`--dry-run`; Plex requests send `Accept: application/json` and decode the real `totalSize` casing; Emby uses `/Shows/{id}/Seasons` and `/Shows/{id}/Episodes`; scanner credential lookup is DB-backed; provider library bootstrap preserves real provider `Key`/`ItemId`; `MediaFile.PathHash` is SHA256 hex; provider dry runs short-circuit before credential loading. Phase 4 remediation list closed; further coverage tracked per provider in the persistence gap matrix. | |
| IPTV M3U playlist | `/home/bart/ErsatzTV/ErsatzTV/Controllers/IptvController.cs` | `crates/ersatzrs-web/src/transport/iptv/playlist.rs` | Ported: `/iptv/channels.m3u` emits the M3U playlist. | |
| XMLTV guide | `/home/bart/ErsatzTV/ErsatzTV/Controllers/IptvController.cs` (xmltv branch) | `crates/ersatzrs-web/src/transport/iptv/guide.rs` | Ported: `/iptv/xmltv.xml` renders a 48-hour XMLTV guide via quick-xml with COALESCEd titles from the metadata tables. | |
| MPEG-TS channel streaming | `/home/bart/ErsatzTV/ErsatzTV/Controllers/IptvController.cs` (ts branch) | `crates/ersatzrs-web/src/transport/iptv/stream.rs` | Ported: `/iptv/channel/{number}.ts` resolves the active PlayoutItem through `MediaVersion`/`MediaFile`, applies provider path replacements, and pipes a software FFmpeg MPEG-TS stream with `kill_on_drop` cleanup. | |
| HLS variant playlists / segmenter | `/home/bart/ErsatzTV/ErsatzTV.FFmpeg/OutputFormat/OutputFormatHls.cs`, `/home/bart/ErsatzTV/ErsatzTV/Controllers/IptvController.cs`, `/home/bart/ErsatzTV/ErsatzTV.Application/Streaming/HlsSessionWorker.cs` | `crates/ersatzrs-web/src/transport/iptv/{hls,variant,streaming_specs,access_token,playlist}.rs`, `crates/ersatzrs-core/src/hls.rs`, `crates/ersatzrs-infra/src/{hls,hls_worker}.rs` | In progress (Phase 7). Slice 1 landed the HLS entry surface: `/iptv/channels.m3u` emits the mode-aware URL suffix (`.m3u8?mode={segmenter,hls-direct}` / `.ts[?mode=ts-legacy]`), Kodi `#KODIPROP` headers on `kodi*` User-Agents, short base64 unique ids, `ChannelIdentifier.FromNumber` tvg-id, and `tvc-stream-{v,a}codec` tags. `/iptv/channel/{n}.m3u8` resolves mode (segmenter → multi-variant playlist, TS modes redirect to `.ts`, `hls-direct` no longer 501s), and `build_multi_variant_playlist` emits `#EXTM3U` + `#EXT-X-STREAM-INF:BANDWIDTH=…,RESOLUTION=…,CODECS="…"` using the ErsatzTV `(VideoBitrate + AudioBitrate) * 1000 * 1.2` formula. Slice 2 landed the first concrete worker: `HlsSessionRegistry` now owns `try_get`/`try_insert`/`insert`/`remove`, `HlsSessionHandle` exposes async `trim_playlist`, `HlsSessionWorker` in infra spawns FFmpeg via `tokio::process::Command` into temp-backed HLS output, mirrors the MPEG-TS route's provider path-replacement resolution before invoking FFmpeg, uses atomic registration to prevent duplicate session workers under concurrent requests, `/iptv/channel/{n}.m3u8?mode=segmenter` starts or touches the worker, and `/iptv/session/{n}/hls.m3u8` plus `/iptv/session/{n}/{file}` serve the live playlist and relative segment files from disk. Slice 3 trims live playlists before serving them, preserves `#EXT-X-DISCONTINUITY` boundaries across retained segments, deletes stale temp-backed `.ts` / `.m4s` / `.mp4` files, and supports fMP4 worker output (`segmenter-fmp4`) with `#EXT-X-MAP` / `init.mp4` delivery through the shared session file route. Slice 4 ports the direct branch: `.m3u8?mode=hls-direct` now emits a single-entry playlist with per-channel media-sequence tracking, `#EXT-X-DISCONTINUITY`, and `/iptv/hls-direct/{n}[.ts\|.mp4\|.mkv]` targets driven by `ffmpeg.hls_direct.output_format` (default `mpegts`); the direct stream route reuses the shared FFmpeg process streaming path and emits output-format-aware content types. Slice 5 ports `ConditionalIptvAuthorizeFilter` parity: `/iptv/*` routes are protected by a route-scoped middleware that activates only when `JWT__IssuerSigningKey` / `JWT:IssuerSigningKey` is configured, extracts the token from `?access_token=` or `Authorization: Bearer`, validates HS256 signatures plus `exp`, and preserves the `/iptv/logos` exemption. Remaining: any fMP4 init-segment cache parity that still matters after direct mode, then the broader Phase 7 items after HLS completion. See [`plans/defer-hls-parity-to-phase-7.md`](../../plans/defer-hls-parity-to-phase-7.md). | |
| HDHomeRun HTTP endpoints | `/home/bart/ErsatzTV/hdhr.md` | `crates/ersatzrs-web/src/transport/hdhr/http.rs` | Ported: `/discover.json`, `/device.xml`, `/lineup_status.json`, and `/lineup.json` expose the HDHomeRun HTTP shape with verbatim C# field names (DeviceID/LineupURL/GuideNumber/...). Device UUID seeded into `ConfigElement` under `HDHR.DeviceUuid`; tuner count read from `HDHR.TunerCount`. | |
| HDHomeRun UDP discovery | `/home/bart/ErsatzTV/hdhr.md` | `crates/ersatzrs-web/src/transport/hdhr/udp.rs` | Ported: UDP discovery responder on port 65001 with TLV + CRC32 encoding, covered by protocol round-trip tests. | |
| Channels admin UI | `ErsatzTV/Pages/Channel*.razor` | `crates/ersatzrs-web/src/admin/pages/channels/` | Ported: `/channels` paged table + delete/show-disabled toggle; `/channels/add` and `/channels/:id` editor with form validation, streaming-mode/FFmpeg-profile/filler selects, navigate-on-save. | |
| FFmpeg Profile editor | `ErsatzTV/Pages/FFmpegProfile*.razor` | `crates/ersatzrs-web/src/admin/pages/ffmpeg_profiles/` | Ported: list at `/ffmpeg/profiles` (alias `/ffmpeg`), editor `#[island]` at `/ffmpeg/profiles/add` and `/ffmpeg/profiles/:id` with all 31 fields, hardware accordion driven by `normalize_video`, video-preset parity via `presets_for_profile`, LoudNorm parity, capability-filtered HW acceleration list (`ffmpeg -hwaccels`), Copy Profile via `Modal`. | |
| Watermark editor | `ErsatzTV/Pages/Watermark*.razor` | `crates/ersatzrs-web/src/admin/pages/watermarks/` | Ported: list at `/watermarks` with image thumbnail + Mode/Location columns + Edit/Copy/Delete; SSR wrapper + `#[island]` editor at `/watermarks/add` and `/watermarks/:id`; 17-field form with full `WatermarkEditViewModelValidator.cs` validation parity; custom image upload via axum multipart (`/api/watermarks/image`) + WASM `gloo_net` FormData POST + size guard via `ETV_MAXIMUM_UPLOAD_MB`; drawvg placement preview (320×180 PNG, debounced 400 ms). | |
| Media source configuration | `ErsatzTV/Pages/MediaSources*.razor` | `crates/ersatzrs-web/src/admin/pages/media_sources/` | Ported: list/editor/libraries/paths pages for Local/Jellyfin/Emby/Plex; Plex OAuth PIN island; shared `PathsEditor` sub-component. | |
| Library management | `ErsatzTV/Pages/Libraries.razor` | `crates/ersatzrs-web/src/admin/pages/libraries/` | Ported: `/media/libraries` joins Library + subtype + MediaSource tables; per-library Scan/Deep Scan fires a background `sync_library_by_id`. The web crate invokes `scan_local_library` directly via the `[lib]` target on `ersatzrs-scanner`. | |
| Media browsers (movies, shows, images, music, other videos, songs) | `ErsatzTV/Pages/Movies.razor`, etc. | `crates/ersatzrs-web/src/admin/pages/media/` | Ported for the listed types: `/media/movies`, `/media/tv/shows`, `/media/browser/images`, `/media/music/artists`, `/media/music/songs`, `/media/other/videos`. Shared `MediaBrowserPage` with search input, `LetterBar` quick-nav, and `MediaCardPager`. Trash view at `/media/trash` covers movies/shows/artists/other-videos/songs/images. | |
| Collections / Smart Collections | `ErsatzTV/Pages/Collections*.razor` | `crates/ersatzrs-web/src/admin/pages/media/{collections,smart_collections}.rs` | Ported: list + editor with `#[island]` autocomplete (debounced `suggest_media_items` over Movie ∪ Show) and member add/remove. | |
| Block scheduling UI | `ErsatzTV/Pages/Blocks*.razor` | `crates/ersatzrs-web/src/admin/pages/scheduling/blocks.rs`, `block_editor.rs` | Ported: `/scheduling/blocks` (alias `/blocks`) with group management; `#[island]` editor with `RwSignal<Vec<BlockItemForm>>` local state, Up/Down reorder, `ItemEditor` autocomplete for all collection types, and "Preview Playout" via `preview_block_playout`. Watermark/graphics IDs round-trip via `GROUP_CONCAT`. | |
| Sequential (YAML) playout editor | `ErsatzTV/Pages/Playouts*.razor` (sequential) | `crates/ersatzrs-web/src/admin/pages/scheduling/yaml_editor.rs` | Ported: `/playouts/sequential/:id` with 500 ms debounced YAML validation via `YamlPlayoutEngine::definition_from_str`, save action, erase action with `Modal` confirm. Save spawns a background reset-build through `PlayoutBuilder::default().build()`. Raw YAML stored in `ScheduleFile` (ErsatzRS divergence from ErsatzTV's filesystem-path storage). | |
| Playout list + timeline visualiser | `ErsatzTV/Pages/Playouts.razor`, timeline view | `crates/ersatzrs-web/src/admin/pages/scheduling/{playouts,timeline}.rs` | Ported: `/playouts` SSR list with embedded selection; read-only `#[island]` SVG timeline with 24-hour proportional time axis, filler-vs-main colouring, hour ticks, Prev/Next paging, click-to-inspect detail. | |
| Multi-collections / Rerun collections / Playlists / Filler presets | `ErsatzTV/Pages/MultiCollections*.razor`, etc. | `crates/ersatzrs-web/src/admin/pages/media/` | Not started: nav links advertised in the drawer 404 today (see "Admin UI Route Coverage" below). | |
| Trakt lists | `/home/bart/ErsatzTV/ErsatzTV.Infrastructure/Trakt/`, `/home/bart/ErsatzTV/ErsatzTV.Application/MediaCollections/Commands/*TraktList*`, `ErsatzTV/Pages/TraktLists.razor`, `ErsatzTV/Pages/TraktListEditor.razor`, `ErsatzTV/Shared/AddTraktListDialog.razor` | `crates/ersatzrs-core/src/trakt.rs`, `crates/ersatzrs-infra/src/{trakt,trakt_sync}.rs`, `crates/ersatzrs-web/src/admin/{server_fns/trakt,pages/media/trakt_lists}.rs` | Ported: ErsatzTV-compatible locator parsing, Trakt v2 API client headers and list/item projection, `TraktList`/`TraktListItem`/`TraktListItemGuid` persistence, GUID-first matching with title/year/season/episode fallback, generated playlists under the system `Trakt Lists` group, auto-refresh/rematch maintenance, Add/Edit/Search/Match/Delete UI, and `trakt_list:{id}` search integration. `TRAKT__CLIENTID` is required for add/refresh only. | |
| Schedules / Templates / Decorations / Deco templates | `ErsatzTV/Pages/Schedules*.razor`, etc. | `crates/ersatzrs-web/src/admin/pages/scheduling/` | Not started: nav links advertised in the drawer 404 today. Block scheduling UI is the only scheduling editor currently shipped. | |
| Settings (FFmpeg / Logging / HDHomeRun / Scanner / Playout / UI / XMLTV) | `ErsatzTV/Pages/Settings*.razor` | `crates/ersatzrs-web/src/admin/pages/settings/` | Not started: nav links advertised in the drawer 404 today. Settings live in `ConfigElement`; no UI surface yet. | |
| Support pages (Health / Logs / Troubleshooting) | `ErsatzTV/Pages/Health*.razor`, etc. | `crates/ersatzrs-web/src/admin/pages/system/` | Not started: nav links advertised in the drawer 404 today. The `HealthBadge` island already counts issues via `count_health_issues`, but no detail page exists yet. | |
| Remote streams browser | `ErsatzTV/Pages/RemoteStreams*.razor` | `crates/ersatzrs-web/src/admin/pages/media/` | Not started: nav link advertised in the drawer 404s today. Remote-stream domain entities exist; no UI yet. | |

Statuses:

- Not started
- In progress
- Ported
- Deferred
- Dropped

## Admin UI Route Coverage

The NavDrawer in [`crates/ersatzrs-web/src/admin/layout/nav.rs`](../../crates/ersatzrs-web/src/admin/layout/nav.rs)
advertises the full ErsatzTV navigation tree, but several entries are
intentional placeholders pending Phase 7. This table is the authoritative
mapping from advertised nav link to backing route in
[`crates/ersatzrs-web/src/admin/routes.rs`](../../crates/ersatzrs-web/src/admin/routes.rs).

### Routed (work today)

| Nav link | Backing route(s) |
| --- | --- |
| `/channels` | `ChannelsListPage` + add/`:id` editor |
| `/ffmpeg` | aliased to `/ffmpeg/profiles`; list + add/`:id` editor |
| `/watermarks` | list + add/`:id` editor |
| `/media/sources/local` | list + add/`:id/edit` |
| `/media/sources/jellyfin` | list + edit + `:id/libraries` + `:id/paths` |
| `/media/sources/emby` | list + edit + `:id/libraries` + `:id/paths` |
| `/media/sources/plex` | list + `:id/libraries` + `:id/paths` (OAuth PIN island) |
| `/media/libraries` | management + per-library Scan/Deep Scan |
| `/media/trash` | trash view across implemented browsers |
| `/media/tv/shows` | shows browser (alias `/media/shows`) |
| `/media/movies` | movies browser |
| `/media/music/artists` | artists browser |
| `/media/other/videos` | other-video browser |
| `/media/music/songs` | songs browser |
| `/media/browser/images` | images browser (alias `/media/images`) |
| `/media/collections` | list + add/`:id`/`:id/edit` editor |
| `/media/smart-collections` | list + add/`:id`/`:id/edit` editor |
| `/media/trakt/lists` | list + `:id` editor; add/search/match/delete actions |
| `/search` | scheduling search result page, including `trakt_list:{id}` queries |
| `/blocks` | aliased to `/scheduling/blocks`; list + `:id` editor |
| `/playouts` | list + embedded timeline + `/playouts/sequential/:id` editor |

### Advertised but 404 (placeholders pending Phase 7)

| Nav link | Notes |
| --- | --- |
| `/media/remote/streams` | Remote-stream entities exist; no UI yet. |
| `/media/multi-collections` | No server fns or page yet. |
| `/media/rerun-collections` | No server fns or page yet. |
| `/media/playlists` | No server fns or page yet. |
| `/media/filler/presets` | Filler entities exist; no UI yet. |
| `/schedules` | Only block scheduling UI is shipped. |
| `/templates` | Only block scheduling UI is shipped. |
| `/decos` | Deco data is loaded by block scheduling but has no editor. |
| `/deco-templates` | Same as above. |
| `/settings/ffmpeg` | All settings persist to `ConfigElement` with no UI. |
| `/settings/logging` | Same as above. |
| `/settings/hdhr` | Same as above. |
| `/settings/scanner` | Same as above. |
| `/settings/playout` | Same as above. |
| `/settings/ui` | Same as above. |
| `/settings/xmltv` | Same as above. |
| `/system/health` | `HealthBadge` island counts issues but has no detail page. |
| `/system/logs` | No log surface yet. |
| `/system/troubleshooting` | No troubleshooting surface yet. |

When closing one of these gaps, move the row from the lower table to the
upper one and add a corresponding row to the main feature matrix above.
