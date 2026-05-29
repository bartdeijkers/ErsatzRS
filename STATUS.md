# Project Status

Last updated: 2026-05-29 (admin i18n parity slice)

## Current Phase

**Phase 7 — Hardening and Parity Completion** in progress.

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
credentials, SHA256 `MediaFile.PathHash`, the dry-run/test remediations, and
provider etag skip for unchanged normal-state Plex/Jellyfin/Emby movie and
episode rows are landed. The etag skip is scoped to the current library,
deep scans bypass it, and non-normal provider rows still flow through update
logic so found items return to `Normal`; `ersatzrs-scanner --provider
<local|jellyfin|plex|emby> --library-id <id> [--deep-scan]` dispatches
correctly. Slice 18 scanner deepening is closed: `MediaStream` writes carry
`Profile`/color/`BitsPerRawSample`/`AttachedPic`/attachment fields,
Jellyfin/Emby decode `LibraryOptions.PathInfos` into provider `*PathInfo`
rows, `sync_{plex,jellyfin,emby}_libraries` cover add/update/delete cascade
with `ShouldSyncItems` parity, the local scanner writes
`MusicVideo`/`MusicVideoMetadata` (with per-folder `Artist`/`ArtistMetadata`
resolution) and `RemoteStream`/`RemoteStreamMetadata`, and a paginated Plex
collection corpus test exercises unicode and mixed-type member responses.

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
Phase 7 rather than Phase 5; see the Phase 7 slice log below for the current
HLS port progress.

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

Slice 5 ports `ConditionalIptvAuthorizeFilter` parity for the current Rust
surface: all `/iptv/*` routes now sit behind a route-scoped middleware that is
enabled only when `JWT__IssuerSigningKey` / `JWT:IssuerSigningKey` is present,
accepts JWTs from `?access_token=` (plus `Authorization: Bearer` for parity
with the underlying bearer handler behavior), validates HS256 signatures and
token expiry, and preserves the `/iptv/logos` exemption for future logo
endpoints. The propagated access token now gates `/iptv/channels.m3u`,
`/iptv/xmltv.xml`, `/iptv/channel/{n}.{ts|m3u8}`, `/iptv/session/{n}/...`, and
`/iptv/hls-direct/{n}...` instead of being a pass-through-only query suffix.

Slice 6 lands the remaining Phase 7 hardware builder surface in
`ersatzrs-ffmpeg`: QSV, AMF, VideoToolbox, RKMPP, and V4L2M2M now have Rust
pipeline builders, hardware-specific encoder names, QSV/Videotoolbox/RKMPP
global hwaccel setup, QSV-specific `vpp_qsv` scaling plus one-second HLS GOP
parity, and dedicated snapshot coverage for the generated argv. This slice is
approved as a GO for Slice 7. Remaining FFmpeg parity work is narrower: the
current `PipelineRequest` model still lacks source-codec metadata, so the more
granular ErsatzTV decoder dispatch tables (`h264_qsv`, `vp9_v4l2m2m`, etc.)
are not yet expressible in Rust and remain future FFmpeg follow-up rather than
Slice 6 blockers.

Slice 7 lands MySQL backend support behind the `mysql` feature. `db::init`
now detects `mysql://` and `mariadb://` URLs when the feature is enabled,
returns a clear unsupported-backend error otherwise, and runs backend-specific
migrations. `ersatzrs-web` and `ersatzrs-scanner` use the backend-aware pool
and repository construction, while dialect-sensitive SQL paths now avoid
SQLite-only constructs such as `INSERT OR REPLACE`, `ON CONFLICT`,
`last_insert_rowid`, `COLLATE NOCASE`, `strftime`, and `datetime('now')` in
production code. The MySQL migration set under `migrations/mysql/` is ordered
for MySQL execution, preserves EF-style named constraints and indexes, and
uses the ErsatzTV MySQL model's `utf8mb4` / `utf8mb4_general_ci` behavior.
Feature-gated integration coverage verifies migrations, config upsert/read,
channel/profile reads, playout item lookup, and scanner insert-id handling
against MySQL. Review found a test-only no-default-features scanner compile
regression; remediation restores the SQLite-specific scanner test helper type.
Review outcome: GO for the next Phase 7 slice.

Slice 8 lands Trakt list integration parity for the current admin surface.
The core Trakt contracts parse ErsatzTV-compatible list locators, the infra
client calls the Trakt v2 API with ErsatzTV header shape, and the synchronizer
persists `TraktList`, `TraktListItem`, and `TraktListItemGuid` rows, rematches
local media by provider GUID then title/year/season/episode fallback, creates
or removes generated playlists under the system `Trakt Lists` group, and runs
hourly auto-refresh/rematch maintenance. The web UI now routes
`/media/trakt/lists` and `/media/trakt/lists/:id` for Add/Edit/Search/Match/
Delete and toggle editing, and `/search?query=trakt_list:{id}` resolves through
the scheduling search projection. `TRAKT__CLIENTID` is required only for API
refresh/add operations; match/update/delete remain DB-only. Review outcome:
GO for the next Phase 7 slice.

Generated Trakt playlist UI hardening is documented in ADR 0011. ErsatzRS
preserves ErsatzTV scheduling semantics for generated playlist rows, but renders
system playlists as read-only projections instead of disabled editable controls
so Count and Play All behavior is explicit in the editor.

Slice 9 ports the first explicit readiness and downgrade/schema-check surface.
`ersatzrs-infra` now opens the mediaserver database before running migrations,
inspects `_sqlx_migrations` for unknown versions, partially applied rows, and
checksum mismatches, and only runs migrations plus default bootstrap when that
schema state is clean. `ersatzrs-web` now captures that startup state, returns
`503 Service Unavailable` from `/healthz` when the schema is not ready, and
logs downgrade / dirty-migration findings at startup. The web startup path also now reads and
backfills the upstream `hdhr.uuid` / `hdhr.tuner_count` config keys while
remaining backward compatible with the earlier mixed-case keys. Review
outcome: ready for review.

Slice 11 is complete for `ersatzrs-ffmpeg`. `PipelineRequest` now carries
optional source video/audio stream metadata, `decoder.rs` ports ErsatzTV's
known decoder table plus software/QSV/V4L2 M2M/implicit hardware dispatch, and
the pipeline emits explicit input video decoder options when source metadata is
available. Snapshot tests now freeze the changed argv for software, QSV, AMF
with software decode, and V4L2 M2M paths. Slice 12 remains the capability-gated
encoder/decoder fallback layer on top of this metadata and dispatch surface.

Slice 12 is complete for `ersatzrs-ffmpeg`. The new report-backed
`HardwareCapabilitiesFactory` mirrors ErsatzTV's `IHardwareCapabilities`
surface for the currently modeled accelerators, `PipelineRequest` can carry
the Slice 10 `FfmpegCapabilityReport`, and pipeline construction now drops to
the software builder path before emitting hardware globals/filters when the
requested hardware encoder or decoder is unavailable. The health-check
hardware support helper now uses the same capability factory so support
warnings stay aligned with pipeline dispatch.

Slice 16 is complete for the non-web health-check engine. `ersatzrs-core` owns
ErsatzTV-compatible health result/status/summary contracts plus a
`HealthCheckService` runner, while `ersatzrs-infra::health` provides SQL-backed
checks for FFmpeg version/capabilities, FFmpeg troubleshooting reports,
hardware acceleration, VAAPI driver guidance, VAAPI device existence,
provider connectivity (reading `maintenance.provider_connectivity`),
file-not-found/unavailable media state, empty schedules, zero-duration media,
missing movie/episode metadata, and downgrade/schema readiness. Runs persist to
`ConfigElement` as `health.results` and `health.summary`. The admin web layer
exposes `/system/health` rendering the persisted snapshot or, when none exists,
the live health service with `NotApplicable` rows filtered like ErsatzTV, and
the same persisted parity summary feeds the health detail surface. This must be
kept current as porting work changes runtime capabilities or parity-sensitive
persisted state.

Slice 17 is complete. `ersatzrs_scheduler::runner::run_scheduler` runs
runtime-independent maintenance work as an in-process tokio task inside
`ersatzrs-web`, matching ErsatzTV's hosted-service model (`Startup.cs`):
periodic playout builds with channel lock markers, due-library refresh
dispatch using `Scanner.LibraryRefreshInterval`, provider connectivity polling
for Plex/Jellyfin/Emby connections, transcode/image/log cache cleanup,
search-index rebuild dispatch markers, and automatic persisted health runs
after successful FFmpeg runtime probing. Loop timestamps and dispatcher state
are stored in `ConfigElement` under `maintenance.*`; health results continue
to use `health.results` and `health.summary`. The `ersatzrs-scheduler` crate
stays a library to keep `runner.rs` testable; only two binaries
(`ersatzrs-web`, `ersatzrs-scanner`) are produced and shipped.

Slice 19 is complete for the non-web search parity target. The core search
parser/evaluator now lowercases and accent-folds text, consumes Lucene boost
syntax, handles fuzzy terms, phrase slop, numeric exact/range fields, wildcards,
and jump-letter page maps. The SQL-backed search projection now carries
Director, Writer, chapter count, duration, image, and remote-stream documents
alongside the existing metadata/stream/provider GUID fields. ADR 0009 records
the decision to keep search in process and SQL-backed while deliberately
dropping ErsatzTV's experimental Elasticsearch backend.

Slice 10 lands ADR 0008 and the first in-repo FFmpeg 8.1 image delivery pass.
`ersatzrs-ffmpeg` now checks both `ffmpeg` and `ffprobe` at startup, honors the
ErsatzRS-specific path overrides with compatibility aliases, parses version
banners, validates required drawvg/software codec/text-stack capabilities, and
captures filters/encoders/decoders/hwaccels/configuration flags in a runtime
report stored on `AppState`. The FFmpeg profile hardware list now reuses that
startup report instead of spawning `ffmpeg -hwaccels` per request. Docker
delivery now stays in this repository with `docker/Dockerfile`,
`docker-bake.hcl`, GHCR multi-arch release workflow coverage for
`linux/amd64`, `linux/arm64`, and `linux/arm/v7`, and hardware-gated smoke
scripts for NVENC, VAAPI, and Vulkan. Review blockers are closed: CI restored
all-features test coverage, the release workflow now checks static GPU
capability reports, Docker helper tools are pinned, the Bookworm FFmpeg 8.1
build path handles Vulkan/SVT-AV1 correctly, and the Leptos hydration library
target builds into the image. Review outcome: GO for the next Phase 7 slice.

Next up: continue `/channels/add` parity from
[`plans/channel-add-parity.md`](plans/channel-add-parity.md). The full editor
contract, repository-backed load/save validation, logo artwork persistence/
serving, IPTV/list logo preference, subtitle-extraction save side effect, and
focused DB tests are in place. The channel editor renders as a no-JS SSR
multipart form while the current app-wide Leptos island marker mismatch remains;
all generated/mirror and default/custom selector fields stay reachable without
client handlers, and local add/edit POST smoke tests persist expanded parity
fields plus uploaded/external/removed logo artwork. Remaining channel-editor
follow-up is final verification against the plan's full gate list and any
ErsatzTV source-level edge cases discovered during that audit. The remaining
non-web priorities are config/ADR closure, side-by-side benchmarking, and long
soak tests.

The `ersatzrs-web` structure refactor now has the target web-crate boundaries
documented in [`docs/reference/web-crate-structure.md`](docs/reference/web-crate-structure.md).
The crate has a thin `main.rs`; runtime startup, state, config, router
composition, and background tasks live under `src/host/`; non-admin protocols
live under `src/transport/`; plain axum admin APIs live under `src/api/`; and
the Leptos admin app, routes, pages, server functions, layout, UI primitives,
and style modules live under `src/admin/`. Static public asset buckets are
reserved as `public/images/` and `public/js/`, while generated hydration assets
remain served from `/pkg`.

The Channels admin list now has a first-pass ErsatzTV visual parity pass:
Roboto is served locally from `public/fonts`, dark-theme tokens mirror the
MudBlazor `PaletteDark` colors more closely, the channel table uses
ErsatzTV-like row height, divider, pager, switch, logo, language, mode, and
icon-action spacing, and `artwork/ersatzrs_full.png` is available as the new
full-width raster wordmark asset.

UX/UI Slice 1 is closed. Channels and watermarks now share the ErsatzTV-style
top action strip, constrained content area, divider, and icon row actions;
channel rows render generated logo images via `/iptv/logos/gen` and show
disabled/hidden number markers. Local library create/update now normalizes
server filesystem paths before validation, accepts `file://` path input,
deduplicates normalized paths, and writes atomically. FFmpeg profile edit keeps
saved hardware acceleration settings visible even when the current runtime
cannot use them, and the hardware list now considers both reported hwaccels and
hardware encoder names. Vulkan is documented as a runtime/toolchain capability,
not an ErsatzTV profile enum value.

Persistence parity gaps remain tracked per table in
`docs/reference/persistence-gap-matrix.md`. Media version/file/stream/chapter
write coverage improved through ffprobe, local scanning, Jellyfin, Plex, and
Emby sync; the remaining high-risk slices are language and subtitle
normalization, metadata joins, template items, and deco break/template rows.
Slice 14 closed the typed program schedule item and block item repository gap:
program schedule item duration/flood/multiple/one subtype rows, schedule item
watermark/graphics joins, alternates, and block item target/overlay joins now
round-trip through infra repositories.

Slice 15 closed the first Classic scheduling engine gap. `PlayoutBuilder` now
dispatches `PlayoutScheduleKind::Classic` to `ClassicSchedulingEngine`, which
schedules One/Multiple/Duration/Flood program schedule items, applies current
ErsatzTV `Strict`/`Flexible` fixed-start behavior, emits compatible
`ClassicSchedulingContext` JSON, and returns playout anchors plus program
schedule anchor/fill-group state through the existing `SchedulingResult`
persistence path. `SqliteSchedulingContentRepository::classic_reference_data`
loads Classic schedules from imported SQLite databases, including typed
schedule items, collection media, existing playout state, program schedule
anchors, fill-group indices, and date-matched Classic schedule alternates. The
post-review pass also tightened shuffled schedule item order, persisted media
anchor seed reuse, count-expression Multiple sizing, playback-order preparation,
guide-group advancement after multi-item modes, and Classic `CollectionKey`
parity. Remaining deeper Classic parity is limited to schedule-item filler
preset fields and generated fake group schedule items for full fill-with-group
enumerator behavior that are not yet represented on the Rust domain model.

Slice 20 closed the remaining non-web config and ADR cleanup. ErsatzTV
`ConfigElement` keys for global watermark and global fallback filler now feed
playout build post-processing: generated items inherit
`ffmpeg.global_watermark_id` when no item watermark is set, and generated gaps
can be filled from `ffmpeg.global_fallback_filler_id`. The mediaserver
bootstrap seeds `log.minimum_level` plus scanning/scheduling/searching/
streaming/http subsystem keys, and `ersatzrs-web`, `ersatzrs-scanner`, and
the in-process scheduler task load those keys into `tracing` filters on startup. XMLTV
block behavior moved into core/infra (`XmltvBlockBehavior` and
`xmltv.block_behavior`) so the transport layer only renders projected guide
programmes. ADR 0010 records the Classic scheduling engine port decision.

**Platform support — at parity with ErsatzTV across all six RIDs.** Every
PR/push runs a 6-cell `platforms` matrix in `.github/workflows/ci.yml` that
builds the workspace for `linux-x64`, `linux-arm64`, `linux-arm`, `osx-arm64`,
`osx-x64`, and `win-x64`, and runs the test suite on the natively-built cells
(linux-x64 on `ubuntu-latest`, osx-arm64 on `macos-latest`, win-x64 on
`windows-latest`). The two ARM Linux cells use cross-rs via
[`Cross.toml`](Cross.toml); osx-x64 cross-compiles from `macos-latest` because
the Intel `macos-13` runners are scarce and being deprecated. Tag pushes now
attach `ersatzrs-<tag>-<rid>.tar.gz` (or `.zip` for win-x64) per RID through
the new `binaries` matrix in [`release.yml`](.github/workflows/release.yml),
sitting alongside the existing Docker publish path. Windows contributors use
[`run.cmd`](run.cmd) and [`run.ps1`](run.ps1) which mirror the bash
`run.sh` subcommands. Audit results: only one source-level fix was needed —
splitting the unix-only ffmpeg version-check test helpers into a
`#[cfg(all(test, unix))] mod unix_tests` so the parser tests still compile on
Windows. The `platform-parity-fixer` agent in
[`.claude/agents/`](.claude/agents/platform-parity-fixer.md) encodes the audit
procedure and the false-positive catalogue for future regressions.

**TV-show-page parity gaps closed.** The remediation plan in
[`plans/scan-languages-fact-row-sync.md`](plans/scan-languages-fact-row-sync.md)
is now fully landed: Slice 0 (plan scaffolding) → Slice 1 (Languages
fact row: vendored ErsatzTV's ISO-639-2 table, ported
`ersatzrs-core::language_codes`, and wired the `GetLanguagesForShow` query
into the show facts) → Slice 2 (playout-refresh: in-process `mpsc`
consumed inside the scheduler loop, with `BuildPlayout(Refresh)` enqueued from
the schedule/collection mutation paths) → Slices 3–5 (Plex/Jellyfin/Emby
targeted show sync: provider single-show fetch + `sync_one_<p>_show` refactor
+ `sync_<p>_show_by_id` infra entries + `scan_show` dispatch from the show
page; Local intentionally falls back to a full library scan, mirroring
ErsatzTV). `ExtractEmbeddedShowSubtitles` on deep show scan is now ported:
Jellyfin/Emby episode scans populate `Subtitle` rows
(`metadata::update_episode_subtitles`), and a successful deep `scan_show`
calls `subtitles::extract_embedded_show_subtitles` (text-subtitle + font
extraction via ffmpeg), gated by `ffmpeg.extract_embedded_subtitles`
(default off) and `ffmpeg.use_embedded_subtitles` (default on). Remaining
documented deferrals: Plex `Subtitle`-row population (blocked on porting Plex
`EpisodeMetadata`; Plex extraction is a no-op until then), the playout-window
`ExtractEmbeddedSubtitles`, transcode-pipeline consumption of extracted
subtitles, `Subtitle`-row population for movies/music-videos/other-videos, and
a global FFmpeg settings UI — all tracked in parity-matrix.

### Admin UI Route Coverage

The NavDrawer in [`crates/ersatzrs-web/src/admin/layout/nav.rs`](crates/ersatzrs-web/src/admin/layout/nav.rs)
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
`/media/music/artists/:id`,
`/media/music/songs`, `/media/other/videos`, `/media/collections`,
`/media/smart-collections`, `/media/multi-collections`,
`/media/rerun-collections`, `/media/playlists`, `/media/trakt/lists`,
`/media/filler/presets`, `/search`, `/blocks` (alias for `/scheduling/blocks`),
`/templates`, `/decos`, `/deco-templates`, `/schedules`, `/playouts`,
`/playouts/sequential/:id`, `/settings/{ffmpeg,logging,hdhr,scanner,playout,ui,xmltv}`,
`/system/health`, `/system/logs`, and `/system/troubleshooting`.

**Advertised but currently 404** (placeholders pending Phase 7):
`/media/remote/streams`. This should not be read as a Phase 6 regression; it
was intentionally shipped in the navigation tree for layout parity ahead of the
corresponding page.

### Admin UI Hydration Parity

Tracking the per-page conversion from `#[component]` interactive bodies to
`#[island]`. See [TEST.md](TEST.md) hydration rules and the reference patterns
in `crates/ersatzrs-web/src/admin/pages/libraries/mod.rs:11-17` and
`pages/media_sources/plex.rs`.

The first scripted Playwright migration on 2026-05-04 found marker-node
hydration mismatches on the pages marked ❌ below. Those rows have island
wiring present, but the browser tests prove their handlers are not reliable
yet.

| Page                          | Status | Notes                                                          |
| ----------------------------- | ------ | -------------------------------------------------------------- |
| `LibrariesPage`               | ❌     | Playwright hydration mismatch; lock-state polling present.      |
| `SearchPage`                  | ❌     | Playwright hydration mismatch.                                  |
| `PlexSourcesPage`             | ✅     | OAuth PIN island.                                              |
| `WatermarksEditorPage`        | ✅     |                                                                |
| `BlockEditorPage`             | ✅     |                                                                |
| `YamlEditorPage`              | ✅     |                                                                |
| `TimelinePage` (`/playouts`)  | ✅     | Read-only SVG island; selection is the gap (Slice F).          |
| `CollectionsPage`             | ✅     | Autocomplete island.                                           |
| `MoviesPage`                  | TODO   | Item overflow menu wired; Playwright smoke pending.             |
| `MoviePage` (`/media/movies/:id`) | ✅ | SSR detail page with JSON-backed Add To and Media Info actions; Playwright smoke clean. |
| `ArtistPage` (`/media/music/artists/:id`) | ✅ | Detail page island wired; `/media/music/artists/2355` modal-open smoke clean. |
| `ChannelsListPage`            | ❌     | Playwright hydration mismatch.                                  |
| `FfmpegProfilesListPage`      | ❌     | Playwright hydration mismatch.                                  |
| `FfmpegProfileEditorPage`     | ❌     | Playwright hydration mismatch.                                  |
| `ChannelEditorPage`           | ✅     | SSR multipart form; `/channels/add` and `/channels/:id` smoke clean. |
| `LocalLibrariesPage`          | ❌     | Playwright hydration mismatch; static HTML add-form unchanged.  |
| `JellyfinSourcesPage` etc.    | ⏳     | Awaits Plex pattern + jellyfin/emby mock helper (Slice E).     |
| `EmbySourcesPage` etc.        | ⏳     | Awaits Plex pattern + jellyfin/emby mock helper (Slice E).     |
| `PlayoutsPage` row selection  | ⏳     | Slice F — lift selection to URL query param.                   |
| `BlocksPage`                  | ⏳     | Group add/delete buttons.                                      |
| `TemplatesPage`               | TODO   | Hydrated list actions, delete confirmations, visual smoke, and console check covered in `plans/templates-parity-slice.md`; add durable E2E spec when automated admin coverage is broadened. |
| `TemplateEditorPage`          | TODO   | Hydrated add/remove/drag, browser-navigation edit link, save POST fallback, visual smoke, and console check covered in `plans/templates-parity-slice.md`; add durable E2E spec when automated admin coverage is broadened. |
| `WatermarksListPage`          | ⏳     | DeleteDialog action.                                           |
| `TraktListsPage`              | TODO   | Browser smoke covered Add dialog, Search/Match/Delete row actions, pager geometry, and console check; add durable E2E spec when automated admin coverage is broadened. |
| `TraktListEditorPage`         | TODO   | Browser smoke covered editor layout, checkbox state, save POST fallback form, and console check; add durable E2E spec when automated admin coverage is broadened. |
| `FillerPresetsPage`           | ⏳     | List/editor islands with POST fallback save/delete paths.       |
| `RerunCollectionsPage`        | ⏳     | List/editor islands with POST fallback save/delete paths.       |
| `SmartCollectionsPage`        | ⏳     |                                                                |
| `TrashPage`                   | ⏳     |                                                                |
| `PlexLibrariesPage`           | ⏳     |                                                                |
| `PlexPathsPage`               | ⏳     |                                                                |
| `JellyfinConnectionPage`      | ⏳     |                                                                |
| `JellyfinLibrariesPage`       | ⏳     |                                                                |
| `JellyfinPathsPage`           | ⏳     |                                                                |
| `EmbyConnectionPage`          | ⏳     |                                                                |
| `EmbyLibrariesPage`           | ⏳     |                                                                |
| `EmbyPathsPage`               | ⏳     |                                                                |

Tick a row when both the wiring change and the matching Playwright test in
[TEST.md](TEST.md) pass.

### Known TODOs from Phase 0

- **Admin pages with dead `on:*` handlers (islands hydration gap).** Tracking
  is now in the "Admin UI Hydration Parity" table above. The fix pattern is
  documented in [TEST.md](TEST.md) and proven on `LibrariesPage` /
  `PlexSourcesPage`. Each row is one slice; ship wiring + playbook together.
- Docker dependency layer caching (copy manifests first, build deps, then copy src) — improves CI build times.
- Replace bash `/dev/tcp` HEALTHCHECK with a purpose-built `ersatzrs-healthcheck` binary.
- Keep `axum` and `leptos_axum` on matching major versions; both are currently on 0.8.
- `std::env::var("DATABASE_URL")` in `main.rs` uses `#[allow(clippy::disallowed_methods)]` — replace with a config crate in Phase 1.
- `ffmpeg` module in `ersatzrs-web` migrated to `ersatzrs-ffmpeg` in Phase 2.

### Blocking Decisions

None. All Phase 0 decisions are recorded in ADRs 0001–0007.

## Session Log

| Date | Agent | Summary |
| --- | --- | --- |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for decoration save/delete errors: read ErsatzTV `Decos.razor`, `DecoEditor.razor`, `DecoTemplates.razor`, and `DecoTemplateEditor.razor`, then mapped stable deco/deco-template validation, existence, duplicate, reference, and overlap errors through per-culture `admin/languages/` translations at hydrated toast boundaries while preserving unknown diagnostics verbatim. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for media-source save errors: read ErsatzTV remote media source editor and validator sources, then mapped stable address/API-key validation and Jellyfin reachability errors through per-culture `admin/languages/` translations at the hydrated media-source save boundaries while preserving provider/backend diagnostics verbatim. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for media-browser editor errors: read ErsatzTV collection, smart-collection, multi-collection, rerun-collection, playlist list/editor sources and mapped stable media collection/playlist validation errors through per-culture `admin/languages/` translations at the hydrated toast/preview boundaries. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for filler preset save/delete errors: read ErsatzTV `FillerPresetEditor.razor`, `FillerPresetEditViewModelValidator`, and filler create/update/delete handlers, then moved stable hydrated filler validation/server errors through per-culture `admin/languages/` mappings while preserving unknown diagnostics verbatim. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for media action API errors: read ErsatzTV `AddToCollectionDialog`, `AddToPlaylistDialog`, `MultiSelectBase`, and movie/collection action sources, then localized stable add-to-collection/add-to-playlist/media-info JSON errors at the API boundary through per-culture `admin/languages/` mappings while preserving unknown diagnostics verbatim. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for settings save fallbacks: read ErsatzTV settings Razor validation/snackbar behavior, then localized the ErsatzRS SSR/no-JS settings error page and stable settings validation messages through per-culture `admin/languages/` mappings while preserving unknown diagnostics verbatim. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/dev/components`: treated the routed component gallery as admin UX despite its dev purpose, moved visible gallery labels/buttons/dialog/toast sample text through per-culture `admin/languages/` translations, and preserved HLS/MPEG-TS as technical identifiers. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Updated `AGENTS.md` so future admin UX/UI work must apply i18n at build time across the supported per-culture language files and follow ADR 0012's translation boundary. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Documented the admin UI i18n translation boundary in ADR 0012 after the FFmpeg-profile slice assessment: translate product UX text and stable workflow errors, but preserve technical identifiers, provider/product names, diagnostics, routes, config keys, and user-authored data. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for FFmpeg profile list/error surfaces: read ErsatzTV `FFmpeg.razor`, `FFmpegEditor.razor`, `FFmpegProfileEditViewModel`, and FFmpeg profile create/update/copy handlers, then moved visible `None`/`Copy` profile-format labels and save/copy server errors through per-culture `admin/languages/` translations without changing persisted enum values. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for watermark save/copy errors: read ErsatzTV `WatermarkEditor.razor`, `WatermarkEditViewModelValidator`, and watermark create/update/copy handlers, then moved watermark validation/server errors through per-culture `admin/languages/` translations and removed unused English enum labels from the watermark list server payload. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for FFmpeg profile enum option labels: read ErsatzTV `FFmpeg.razor` and `FFmpegEditor.razor`, then moved tonemap algorithm and loudness-mode option labels through per-culture `admin/languages/` translations without changing persisted enum values. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for media-source and libraries media-kind labels: read ErsatzTV local/remote media-source list, library editor, and path-replacement Razor sources, then moved visible library kind/media kind labels through per-culture `admin/languages/` translations while preserving provider identifiers for scan actions. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for block/template validation errors: read ErsatzTV `Blocks.razor`, `BlockEditor.razor`, `Templates.razor`, and `TemplateEditor.razor`, then moved hydrated block/template server errors through per-culture `admin/languages/` translations while preserving unknown server errors. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for schedule validation errors: read ErsatzTV schedule item view-model/mapper sources and moved hydrated schedule copy/save/item-save server errors through per-culture `admin/languages/` translations while preserving unknown server errors. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for FFmpeg option labels: read ErsatzTV `Settings/FFmpegSettings.razor` and `FFmpegEditor.razor`, then localized the built-in MPEG-TS script label and hardware-acceleration availability suffix from structured option data without changing persisted values. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for channel table/editor values: read ErsatzTV `Channels.razor` and `ChannelEditor.razor`, then localized visible streaming-mode and preferred-audio language labels from raw mode/code values without changing persisted channel data. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for logging levels: read ErsatzTV `Settings/LoggingSettings.razor` and `Logs.razor`, then localized visible log-level option/table labels while preserving canonical persisted/log values. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for scheduling fallback text: read ErsatzTV `BlockEditor.razor` duration adornments, localized the block duration hour label, and moved visible schedule fallback collection labels through per-culture `admin/languages/` helpers without changing persisted item values. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for media season display names: read ErsatzTV season card/search mappers, added per-culture `Specials` / `Season n` display-name formatting, and applied it at search, shared season browser, show detail, and season detail render boundaries without changing persisted/search data. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/blocks`: read ErsatzTV `Blocks.razor` and `BlockEditor.razor`, confirmed the search-query placeholder is an ErsatzRS helper, and moved it to typed per-culture translations while keeping query syntax tokens unlocalized. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for artist detail accessibility text: read ErsatzTV `Artist.razor` and `Shared/MediaCard.razor`, refreshed the artist-detail source/visual map, and moved music-video thumbnail `alt` suffix text to the persisted-language `Thumbnail` translation. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for copy-dialog defaults: read ErsatzTV copy dialog/list sources for FFmpeg profiles, schedules, blocks, and templates, added a per-culture copy-name suffix key, and replaced generated English `Copy` defaults in those copy flows with typed translations. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for optional `(none)` select labels: checked ErsatzTV `FFmpegSettings.razor` and `ScheduleItemsEditor.razor`, then moved settings select empty options and schedule language override empty options to `TextKey::NoneOption` through per-culture `admin/languages/` modules. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for shared media browser surfaces: read ErsatzTV `SongList.razor`, `ImageList.razor`, `MovieList.razor`, `MultiSelectBase.cs`, `MediaCardPager.razor`, and `LetterBar.razor`, refreshed the shared browser source/visual map, and moved simple song/image browser titles, search placeholders, loading/empty states, multi-select route titles, search placeholders, and card select aria labels to typed translations. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/media/collections/:id`: read ErsatzTV `CollectionItems.razor`, refreshed the manual-collection visual/source map, and moved collection detail loading/not-found/back, inline rename, selection/removal, custom playback order, grouped section headings, empty-state, move buttons, validation, and toast labels to typed translations in per-culture `admin/languages/` modules. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/media/other/videos/:id`: read ErsatzTV `OtherVideoList.razor`, `OtherVideoCardViewModel`, media-card mapper, and `GetMediaItemInfoHandler`, added the other-video detail extension to the source/visual map, and moved loading/not-found/action/media-info/file-state/fact-row/actor labels to typed translations in per-culture `admin/languages/` modules. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/media/tv/shows/:id`: read ErsatzTV `TelevisionSeasonList.razor` and show-detail handlers/view models, added show detail to the source/visual map, moved loading/not-found/action/scan/file-state/fact-row/season/actor labels to typed translations in per-culture `admin/languages/` modules, and kept ADR 0012 aligned to `nl`. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/media/music/artists/:id`: read ErsatzTV `Artist.razor`, added artist detail to the source/visual map, moved loading/not-found/back/action/fact-row/music-video warning and album labels to typed translations in per-culture `admin/languages/` modules, and verified the SSR web crate check. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/decos` and `/deco-templates`: read ErsatzTV's Deco/DecoTemplate list/editor Razor sources, added the route family to the source/visual map, moved list/editor labels, option labels, validation/toast text, and no-JS fallback labels to typed translations in per-culture `admin/languages/` modules, and verified the web crate compile path. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-28 | Codex (GPT-5) | Continued the admin UI i18n parity slice: read ErsatzTV's persisted `pages.language` culture provider/resource setup, added ADR 0012 for the intentional `nl` / Nederlands extension, introduced a typed ErsatzRS i18n lookup with per-culture modules under `admin/languages/`, wired persisted language into SSR `<html lang>`, and translated the app shell/navigation/home/not-found, settings route labels/help/actions, `/ffmpeg` profile list/editor, `/search` including bulk/action fallback messages, `/media/libraries`, `/media/sources/{local,jellyfin,emby,plex}`, Plex sign-in status text, the shared media-source path editor, `/watermarks` list/editor labels/options/actions, `/system/{health,logs,troubleshooting}` visible labels/statuses, `/channels` list/editor labels/actions/options, `/schedules` list/editor/item-editor labels/actions/options including no-JS item fallback text, `/blocks` list/editor labels/actions/dialogs/toasts/no-JS fallback labels, `/templates` list/editor labels/actions/dialogs/toasts, `/playouts` list/timeline labels/actions/help text, `/playouts/sequential/:id` YAML editor labels/actions/confirm/toasts, `/media/rerun-collections` list/editor labels/actions/options/pager text, `/media/smart-collections` list/editor labels/actions/options/toasts, `/media/filler/presets` list/editor labels/actions/options/help text, `/media/playlists` list/editor labels/actions/options/toasts/preview text, `/media/multi-collections` list/editor labels/actions/options/validation/toasts/pager text, `/media/collections` list/editor/member labels/actions/toasts, and `/media/tv/seasons/:id` loading/not-found/actions/media-info/file-state/metadata labels. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-28 | Codex (GPT-5) | Continued the admin UI i18n parity slice for movie/shared media action surfaces: moved `/media/movies/:id` loading/not-found/action/menu/modal/media-info/file-state/fact-row labels and shared media browser selection/action/modal/pager labels to typed translations in per-culture `admin/languages/` modules, removed CSS pseudo-content labels from `movie_detail.module.css`, and verified the i18n registry plus web parity checks. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/media/trakt/lists`: read the ErsatzTV Trakt list/editor/Add dialog sources, added the route to the source/visual map, moved list/editor/dialog/action/toast/pager labels and match-count wording to typed translations in per-culture `admin/languages/` modules, and verified the i18n registry plus web parity checks. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-29 | Codex (GPT-5) | Continued the admin UI i18n parity slice for `/media/trash`: read ErsatzTV `Trash.razor`, added trash to the source/visual map, moved headings, subtitle, section links, selection/delete actions, empty-trash confirmation, and success/error toasts to typed translations in per-culture `admin/languages/` modules, and verified the i18n registry plus web parity checks. Full route/component text coverage and four-language browser proof remain active `ParityDebt`. |
| 2026-05-27 | Codex (GPT-5) | Resolved settings review P2s: the app shell now SSR-renders the persisted Light/Dark theme instead of relying only on the client theme fetch, and `/settings/ui` exposes only ErsatzTV's supported cultures (`en-us`, `pl`, `pt-br`) with `en-us` as the default/normalization fallback. |
| 2026-05-27 | Codex (GPT-5) | Fixed `/settings/ffmpeg` Add Custom Resolution so the visible control is a normal POST form instead of a hydration-only dialog; focused web tests now cover custom resolution persistence and duplicate rejection, and browser smoke verified a custom resolution add/redirect on the rebuilt server. |
| 2026-05-27 | Codex (GPT-5) | Corrected the settings default MPEG-TS script parity follow-up: ErsatzRS now stores/selects the built-in script id `default`, matching ErsatzTV's default script folder id, while keeping the visible label `Default`. |
| 2026-05-27 | Codex (GPT-5) | Started the settings route-family parity slice: added source/visual maps in `plans/settings-parity-slice.md`, routed `/settings/{ffmpeg,logging,hdhr,scanner,playout,ui,xmltv}`, added ErsatzTV-shaped forms with raw POST fallbacks, persisted the relevant `ConfigElement` keys, wired playout/XMLTV build enqueue side effects, and moved settings route coverage out of the 404 placeholder list. Remaining work: browser visual verification, hydrated/error-surface smoke, and closing the active settings `ParityDebt`. |
| 2026-05-27 | Codex (GPT-5) | Closed the follow-up settings default gap: mediaserver bootstrap now seeds ErsatzTV's default FFmpeg/logging/scanner/playout/UI/XMLTV settings rows, `/settings/ui` exposes ErsatzTV's English/Polski/Português (Brasil) choices, and the saved Light theme applies through the app shell palette. Evidence so far: focused bootstrap test, `cargo check -p ersatzrs-web --features ssr`, `cargo fmt --all -- --check`, and headless browser smoke at 1440x1000 saving Light + `pt-br` with zero console errors and screenshot `plans/references/settings-ui-light-2026-05-27.png`. |
| 2026-05-27 | Codex (GPT-5) | Completed `/decos` and `/deco-templates` admin parity: added list/editor routes, MudTreeView-derived row surfaces, deco and deco-template save POST fallbacks, server-side validation/persistence for deco watermarks/graphics/break content/filler/dead-air fields, deco-template item replacement, `/decos/:id` break-content table behavior, and a `/deco-templates/:id` day timeline surface with hydrated/no-JS browser evidence. Gates: `cargo fmt --all -- --check`, `cargo check -p ersatzrs-web --all-features`, `cargo test -p ersatzrs-web decos --all-features`, and `cargo xtask parity-check web`. No `ParityDebt` remains in `plans/decos-parity-slice.md`. |
| 2026-05-27 | Codex (GPT-5) | Added ADR 0011 for generated playlist read-only UI and updated `/media/playlists/:id` so system/generated playlist rows show Count as `1 per pass`, Play All/Show In EPG as read-only Yes/No values, and omit disabled row action buttons while keeping preview behavior unchanged. |
| 2026-05-27 | Codex (GPT-5) | Completed `/media/trakt/lists` admin parity: added source/visual map, route-specific MudTable-style layout, Add dialog, row icon actions, rows-per-page persistence, editor checkbox captions, HTML POST fallbacks for add/match/save/delete, focused parser and synchronizer coverage, live route visual/browser smoke, and verified the supplied Trakt list URL responds without storing it in the repo. Gates: `cargo test -p ersatzrs-web trakt --lib`, `cargo test -p ersatzrs-infra --test trakt_sync`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo xtask parity-check web`, and `./run.sh serve` browser checks. |
| 2026-05-26 | Codex (GPT-5) | Ported `/templates` and `/templates/:id` from ErsatzTV with template group/template add/copy/delete, grouped search table, delete/copy dialogs, template editor block add/remove/drag, item replacement persistence, HTML POST fallbacks, validation/parser tests, and source/visual maps plus screenshots in `plans/templates-parity-slice.md`. |
| 2026-05-26 | Codex (GPT-5) | Completed the `/blocks` parity slice: added source/visual maps, fixed block create default duration, surfaced empty block groups, added block copy, multi-collection block items, watermark/graphics selectors, unsaved-form preview input, SSR/no-JS item save fallback, hydrated browser coverage for copy/add-item/preview/save, and 3840x1918 visual evidence against the ErsatzTV reference screenshot. No open `/blocks` `ParityDebt` remains in `plans/blocks-parity-slice.md`. Gates: `cargo test -p ersatzrs-web blocks -- --nocapture`; `cargo xtask parity-check web`. |
| 2026-05-25 | Codex (GPT-5) | Ported the `/schedules` route family from ErsatzTV with list/search/page-size persistence, add/edit save, copy/delete dialogs plus HTML fallbacks, hydrated schedule item add/copy/reorder/delete editing, media-code-backed language override selectors, no-JS item save/action fallback, full item-field replacement persistence, validation coverage, playout refresh side effects, and Playwright visual proof. No `/schedules` `ParityDebt` remains in the active slice. |
| 2026-05-25 | Codex (GPT-5) | Ported `/media/rerun-collections` list/add/edit/delete parity from ErsatzTV with persisted `RerunCollection` target and playback-order fields, HTML POST fallbacks, page-size config persistence, update-triggered playout refresh, source/visual maps, and focused web parser/refresh coverage. |
| 2026-05-25 | Codex (GPT-5) | Cleared the remaining `/media/playlists` `ParityDebt`: playlist preview now builds unsaved playlist reference data and dispatches through `PlayoutBuilder`, with Classic scheduling using `PlaylistEnumerator` for playlist children. Gates: focused core Classic playlist scheduler test and focused web playlist server-function tests. |
| 2026-05-25 | Codex (GPT-5) | Completed the `/media/playlists` ErsatzTV parity slice: playlist group/tree and editor routes now support add/delete groups and playlists, item add/copy/move/delete, transactional item replacement, smart-collection-aware preview expansion, HTML POST create/save/delete fallbacks, focused validation/API parser tests, and browser smoke for add/edit/save/preview/delete with a clean final console. Gate: `cargo xtask parity-check web`. |
| 2026-05-25 | Codex (GPT-5) | Ported `/media/filler/presets` list/add/edit/delete parity from ErsatzTV with persisted `FillerPreset` field coverage, HTML POST fallbacks, page-size config persistence, focused web normalization tests, and infra round-trip coverage for filler kind/mode/expression/chapter fields. |
| 2026-05-24 | Codex (GPT-5) | Hardened the parity workflow from the recent review: added a repo-local `parity-slice` Codex skill, updated the Leptos/ErsatzTV porter and port-planning skill to require source maps, `ParityDebt`, no-stopgap acceptance, and reusable regression thinking, expanded the PR parity rubric, documented the fast parity ladder in `PORTING.md`/`TEST.md`, and added `cargo xtask parity-check` for fmt + hydration lint + focused web tests/clippy. Gates: `cargo check -p xtask` and `cargo xtask parity-check workflow --skip-tests --skip-clippy`. |
| 2026-05-23 | Codex (GPT-5) | Fixed `/media/multi-collections/add` hydration by moving the list and editor into Leptos islands, replacing router-context navigation inside the editor island with a browser location redirect, and moving pager total calculation inside the list Suspense boundary. Gates: `cargo leptos build`, `./run.sh serve`, Playwright add-page smoke adding both a collection and smart collection then saving, SQLite persistence check, and refreshed list-page console check with zero warnings/errors. |
| 2026-05-23 | Codex (GPT-5) | Ported `/media/multi-collections` from ErsatzTV with list/search/page-size persistence, add/edit/delete routes, manual and smart collection item selectors, Schedule As Group and playback-order persistence, transactional item replacement, and playout refresh enqueue for edited multi-collections. Per user instruction, no intermediate quality-gate testing was run. |
| 2026-05-23 | Codex (GPT-5) | Removed the admin sidebar state flicker by applying persisted drawer state from a head bootstrap before stylesheet paint and replaying persisted nav-group `open` state immediately after the drawer markup, while keeping the existing `sessionStorage` keys and listener wiring. Gates: `cargo fmt --all -- --check`, `cargo check -p ersatzrs-web`, `cargo xtask hydration-lint -p ersatzrs-web`, `./run.sh serve`, and Playwright `/channels` -> stored Media/drawer state -> `/media/movies` -> `/channels` persistence smoke with zero console warnings/errors. |
| 2026-05-22 | Codex (GPT-5) | Wired per-card `/search` result menu actions for Add To Collection and Add To Playlist through the existing JSON media-action APIs, with stable card button data attributes and an SSR-rendered fallback modal so the actions work even when search-result island hydration does not attach. Gates: `cargo fmt --all -- --check`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo clippy -p ersatzrs-web --all-targets --all-features -- -D warnings`, `cargo test -p ersatzrs-web --features ssr search`, `(cd crates/ersatzrs-web && cargo leptos build)`, and Playwright `/search?query=library_id%3A1` collection/playlist modal-open smoke. |
| 2026-05-22 | Codex (GPT-5) | Fixed `/search?query=library_id:{id}` result cards: search cards now render native three-dot disclosure menus that open on SSR-rendered results, the shared `MediaCard` no longer nests menu controls inside card links, other videos link to `/media/other/videos/:id`, music videos link to `/media/music/artists/:artist_id#music-video-:id`, and the href helper records the future songs/images/remote-stream detail-route wiring point. Completed three feedback-loop reviews before gates. Gates: `cargo fmt --all -- --check`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo clippy -p ersatzrs-web --all-targets --all-features -- -D warnings`, `cargo test -p ersatzrs-web --features ssr search`, `(cd crates/ersatzrs-web && cargo leptos build)`, Playwright `/search?query=library_id%3A1` menu-open/link smoke, and curl checks for library ids 1-5 menu counts plus movie/show/other-video/music-video link targets. |
| 2026-05-22 | Codex (GPT-5) | Implemented `/media/other/videos/:id` as an SSR other-video detail route: ErsatzTV `OtherVideoCardViewModel` thumbnail-before-poster artwork parity, original-title subtitle, state/file warnings, linked metadata facts, actor cards, Add To Collection, Show Media Info, Troubleshoot Playback, collection-detail links, and `GetMediaItemInfo` JSON support for `OtherVideo` streams/subtitles/chapters. Completed three parity feedback loops before gates: `OtherVideoList.razor`, `MediaCards/Mapper.cs` + `OtherVideoCardViewModel.cs`, and `GetMediaItemInfoHandler.cs` / `OtherVideoRepository.cs`. Gates: `cargo fmt --all -- --check`, `cargo test -p ersatzrs-web --features ssr other_video`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo clippy -p ersatzrs-web --all-targets --all-features -- -D warnings`, `cargo test -p ersatzrs-web --features ssr`, and `(cd crates/ersatzrs-web && cargo leptos build)`. |
| 2026-05-22 | Codex (GPT-5) | Confirmed `/media/movies/:id` already renders ErsatzTV-order linked Studios/Genres/Tags fact rows and added focused regressions for the MovieMetadata-backed Studio/Genre/Tag projection plus lowercase ErsatzTV search-link queries. Completed three ErsatzTV parity feedback loops before gates: `Movie.razor` fact-row surface, `Movies/Mapper.cs` view-model mapping, and `MovieRepository.GetMovie` include/source loading. Gates: `cargo fmt --all -- --check`, `cargo test -p ersatzrs-web --features ssr movie_`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo clippy -p ersatzrs-web --all-targets --all-features -- -D warnings`, and `cargo test -p ersatzrs-web --features ssr`. |
| 2026-05-22 | Codex (GPT-5) | Added `/media/music/artists/:id` artist detail parity for the current media DB surface: ErsatzTV-style artist fanart/thumbnail header, disambiguation, 400-character biography excerpt, linked Languages/Genres/Styles/Moods, artist Add To Collection/Schedule actions with artist-level post-add navigation, and the first 100 sorted music videos with album links, state warnings, and per-video Add To Collection. Gates so far: `cargo fmt --all -- --check`, `cargo check -p ersatzrs-web`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo test -p ersatzrs-web admin::server_fns::media_browser::tests::load_artist_languages_resolves_music_video_audio_stream_codes --features ssr`, `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`, and Playwright `/media/music/artists/2355` render + artist/music-video Add To modal-open smoke with zero console warnings/errors. |
| 2026-05-22 | Codex (GPT-5) | Restored ErsatzTV-style admin sidebar state across SSR page navigations: the drawer toggle and collapsible nav groups now keep their per-tab state in browser session storage while preserving ErsatzRS defaults on a fresh session. Gates: `cargo fmt --all -- --check`, `cargo check -p ersatzrs-web`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo clippy -p ersatzrs-web --all-targets -- -D warnings`, and Playwright `/channels` → open Media group → `/media/movies` plus drawer-toggle persistence back to `/channels` via `./run.sh serve`. |
| 2026-05-22 | Codex (GPT-5) | Completed `/media/movies/:id` movie detail parity for 2001: stable SSR detail rendering, JSON-backed Add To Collection/Add To Playlist/Media Info actions, collection playout-refresh side effects, playlist success without redirecting to a missing route, and a hydration/UI bug-containment playbook in `TEST.md`. Gates: `cargo clean`, `cargo fmt --all -- --check`, `cargo check -p ersatzrs-web --bin ersatzrs-web --no-default-features --features ssr`, `cargo test -p ersatzrs-web admin::server_fns::media_browser::tests::load_movie_languages_resolves_head_version_audio_stream_codes --features ssr`, `cargo clippy -p ersatzrs-web --bin ersatzrs-web --no-default-features --features ssr -- -D warnings`, `(cd crates/ersatzrs-web && cargo leptos build)`, and Playwright `/media/movies/7` with zero current console errors. |
| 2026-05-22 | Codex (GPT-5) | Fixed the shared media-card action menu positioning so TV show season cards render the MoreVert/Add To Collection menu inside the poster instead of overlapping the season title caption. Gates: `cargo fmt --all`, `cargo check -p ersatzrs-web`, `cargo xtask hydration-lint -p ersatzrs-web`. |
| 2026-05-22 | Codex (GPT-5) | Finished the SSR channel editor follow-up and source-level audit: `/channels/add` and `/channels/:id` now post as multipart forms with local logo upload, external logo replacement, stored-logo removal, visible mode-specific fields that do not depend on hydration, form-level validation error redirects, ErsatzTV-order logo fields, absolute stored-logo preview URLs, `http://localhost/...` external-logo validation parity, and explicit missing-edit-channel validation. Updated the channel parity plan/index, parity matrix, hydration notes, and status handoff. Gates: `cargo fmt --all -- --check`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo test -p ersatzrs-web api::channels::tests -- --nocapture`, `cargo test -p ersatzrs-infra --test channel_repository -- --nocapture`, `cargo test -p ersatzrs-infra --test extract_show_subtitles -- --nocapture`, `cargo test -p ersatzrs-web build_m3u -- --nocapture`, `cargo test -p ersatzrs-infra --all-features channel`, `cargo test -p ersatzrs-web --all-features channel`, `cargo clippy -p ersatzrs-core -p ersatzrs-infra -p ersatzrs-web --all-targets -- -D warnings`, `(cd crates/ersatzrs-web && cargo leptos build)`, and Playwright `/channels/add` + `/channels/3` with zero console warnings/errors. |
| 2026-05-22 | Codex (GPT-5) | Removed the `/channels/add` runtime hydration blocker by rendering the channel editor as a server-rendered form while the broader Leptos island marker issue remains. Verified Playwright `/channels/add` with zero console errors, confirmed `/system/logs` still reproduces the app-wide island `failed_to_cast_marker_node`, and smoke-tested add/edit POSTs against SQLite for expanded channel fields and logo artwork persistence. Gates: `cargo fmt --all`, `cargo check -p ersatzrs-web`, `(cd crates/ersatzrs-web && cargo leptos build)`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo test -p ersatzrs-infra --test channel_repository -- --nocapture`, `cargo test -p ersatzrs-infra --test extract_show_subtitles -- --nocapture`, `cargo test -p ersatzrs-web build_m3u -- --nocapture`, `cargo clippy -p ersatzrs-core -p ersatzrs-infra -p ersatzrs-web --all-targets -- -D warnings`. |
| 2026-05-22 | Codex (GPT-5) | Expanded `/channels/add` toward full ErsatzTV editor parity: visible full-field editor, channel editor repository tests, channel logo upload/serving and playlist/list logo preference, state-aware save side effects for embedded subtitle extraction, and Leptos asset builds. Gates: `cargo fmt --all -- --check`, `cargo check -p ersatzrs-web`, `cargo clippy -p ersatzrs-core -p ersatzrs-infra -p ersatzrs-web --all-targets -- -D warnings`, `cargo xtask hydration-lint -p ersatzrs-web`, `cargo test -p ersatzrs-infra --test channel_repository -- --nocapture`, `cargo test -p ersatzrs-infra --test extract_show_subtitles -- --nocapture`, `cargo test -p ersatzrs-web build_m3u -- --nocapture`, `(cd crates/ersatzrs-web && cargo leptos build)`. Browser smoke is not complete: Playwright still finds a runtime `failed_to_cast_marker_node` hydration error in the channel editor. |
| 2026-05-22 | Codex (GPT-5) | Added `plans/channel-add-parity.md` for full ErsatzTV `/channels/add` parity and started the implementation by adding a core/infra channel-editor contract with full-field load/save validation, logo artwork persistence, default channel values, and web server/API wiring that preserves hidden full-form state while the visible editor is expanded. Gates: `cargo check -p ersatzrs-web`, focused clippy for core/infra/web, `cargo xtask hydration-lint -p ersatzrs-web`. |
| 2026-05-22 | Codex (GPT-5) | Restored the show-detail season-card MoreVert action menu so seasons can be added directly to a collection from the TV show page. |
| 2026-05-22 | Codex (GPT-5) | Fixed `/media/tv/seasons/:id` review findings: episode-card pagination now stays on unique `EpisodeMetadata` rows by selecting the head media file via subquery, and remote episode `local_path` now applies library-scoped Plex/Jellyfin/Emby path replacements while preserving the raw provider `file_path`. Added infra regressions for multi-version/multi-file episodes and Plex path replacement display. |
| 2026-05-22 | Codex (GPT-5) | Ported `/media/tv/seasons/:id` from `TelevisionEpisodeList.razor`: added the core/infra `TelevisionRepository` season/episode read surface with DB-backed coverage, `get_season_details`, episode media-info JSON support, generic Add-to-Schedule for media items, shared schedule modal reuse, `SeasonPage` with fanart/poster header and vertical episode cards, routes `/media/tv/seasons/:id` + `/media/seasons/:id`, and parity/persistence matrix updates. |
| 2026-05-22 | Claude Code (claude-opus-4-7) | Ported `ExtractEmbeddedShowSubtitles` (close last `scan-languages-fact-row-sync` deferral). Slice 1: `metadata::update_episode_subtitles` ports `Subtitle.FromMediaStream` + `MetadataRepository.UpdateSubtitles` (add/remove/update keyed on `(EpisodeMetadataId, StreamIndex)`, `IsExtracted` reset on kind/codec/language/flag/path change), wired after `replace_streams` in jellyfin.rs + emby.rs (episode metadata fns now return the id); Plex deferred (no episode metadata yet) with a call-site comment. Slice 2: `ffmpeg_config` (`ffmpeg.use_embedded_subtitles`=true, `ffmpeg.extract_embedded_subtitles`=false defaults, "True"/"False"/"1"/"0" parse) + `file_system_layout` (`cache/streams/{subtitles,fonts}` under `data_local_dir()/ErsatzRS`). Slice 3: `subtitles::extract_embedded_show_subtitles` ports the engine (MD5 upper-hex output paths, codec→ext map, image-codec filter, `-map 0:idx -c:s copy|text`, `-dump_attachment` fonts; input via `MediaFile.Path`+path-replacements, ffmpeg via `ffmpeg_path_from_env` — both divergences commented); added `md-5` dep. Slice 4: wired into deep `scan_show`, removed deferral notes. Tests: subtitle diff logic, jellyfin/emby Subtitle-row assertions, config defaults, argv/MD5/extension unit tests, gate no-op integration tests. |
| 2026-05-20 | Claude Code (claude-opus-4-7) | Implemented Slice 5 of `scan-languages-fact-row-sync`: extracted Emby per-show sync into `sync_one_emby_show`, added a `fetch_single_show` inherent method on `EmbyService` (mirrors ETV `EmbyApiClient.GetSingleShow`'s `/Items?parentId=…&ids=…&includeItemTypes=Series&recursive=false&limit=1`), added `sync_emby_show_by_id` (resolves connection/api-key, maps local `Show.Id` → provider item id + title via `EmbyShow.ItemId` + `ShowMetadata.Title`, then calls the per-show helper), and wired the Emby branch of `scan_show` in `libraries.rs`. Added an httpmock integration test asserting targeted Emby sync, episode-etag skip on non-deep re-run (no extra playback hit), and deep re-sync. Closes Slices 3–5 of `plans/scan-languages-fact-row-sync.md`; updated parity-matrix.md Library management row with the per-show targeted scan description for all three providers. |
| 2026-05-20 | Claude Code (claude-opus-4-7) | Implemented Slice 4 of `scan-languages-fact-row-sync`: extracted Jellyfin per-show sync into `sync_one_jellyfin_show`, added a `fetch_single_show` inherent method on `JellyfinService` (mirrors ETV `JellyfinApiClient.GetSingleShow`'s `/Items?ids=…&parentId=…&recursive=false&limit=1`), added `sync_jellyfin_show_by_id` (resolves connection/api-key, maps local `Show.Id` → provider item id + title, then calls the per-show helper), and wired the Jellyfin branch of `scan_show` in `libraries.rs`. Added an httpmock integration test asserting targeted Jellyfin sync, episode-etag skip on non-deep re-run, and deep re-sync. |
| 2026-05-20 | Codex (GPT-5) | Implemented Slice 3 of `scan-languages-fact-row-sync`: extracted Plex per-show sync into `sync_one_plex_show`, added Plex single-show fetch plus `sync_plex_show_by_id`, wired the show page Scan buttons through the new `scan_show` server function with Jellyfin/Emby/Local fallback, and added httpmock coverage for targeted Plex sync, etag skip, and deep refresh. |
| 2026-05-18 | Codex (GPT-5) | Implemented Slice 2 of `scan-languages-fact-row-sync`: added `PlayoutBuildRequest`, threaded a bounded in-process build channel from `ersatzrs-web` into `run_scheduler`, made the scheduler consume requested builds through the existing playout builder and `EntityLocker`, and enqueue `Refresh` builds after Add-to-Schedule plus collection add/remove/custom-order mutations. Added focused requested-build and playout-lookup tests. |
| 2026-05-18 | Codex (GPT-5) | Implemented Slice 1 of `scan-languages-fact-row-sync`: vendored ErsatzTV's `ISO-639-2_utf-8.txt`, added `ersatzrs-core::language_codes` with code-group expansion and English-name lookup, wired `get_show_details` to resolve distinct episode audio stream languages through the ErsatzTV `GetLanguagesForShow` join, rendered the `Languages` fact row on `/media/tv/shows/:id`, and added focused core + DB-backed web tests. |
| 2026-05-18 | Claude Code (claude-opus-4-7) | Ported the TV show details page (`Pages/TelevisionSeasonList.razor`) to `/media/tv/shows/:id`. Added `get_show_details` (Show+ShowMetadata+MediaItem, poster/fanart, genres/studios, Plex-network tag split via `Tag.ExternalTypeId == "319"`, "/"-split content ratings, ordered actors, season cards), `list_program_schedules`, and `add_show_to_schedule` (ports `AddProgramScheduleItem.ForMediaItem` → `ProgramScheduleItem` + `ProgramScheduleOneItem` TPT row; playout-refresh side-effect noted as deferred). Added a `Thumbnail` `ArtworkUrlKind` + `/artwork/thumbnails/{*path}` route for actor art. New `ShowPage`/`ShowDetailIsland` (page-wrapper→island hydration shape) with seasons + cast grids reusing the `MediaCard` primitive and the now-`pub(crate)` `AddToCollectionModal`/`AddToPlaylistModal`, plus a new `AddToScheduleModal`; Scan (Quick/Deep) actions gated to remote sources reuse `scan_library` (targeted single-show scan deferred — ErsatzRS scanning is library-level; languages fact row deferred like the movies page). Routes `/media/tv/shows/:id` + `/media/shows/:id` registered. fmt + clippy (default + `--features mysql`) + `cargo leptos build` green; SSR-verified live: show 340 renders title, Seasons 1–9, Cast, Content Ratings/Genres facts, fanart, and the Add-to/Scan actions. |
| 2026-05-15 | Claude Code (claude-opus-4-7) | Closed the Local-side of the metadata parity sweep: `upsert_artist_for_folder` now parses `artist.nfo` and writes full `ArtistMetadata` (Title/SortTitle/Disambiguation/Biography/Year/Formed/MetadataKind) with Genres/Styles/Moods + thumb/profile/fanart/logo artwork; `upsert_local_music_video` parses `{video}.nfo` and writes full `MusicVideoMetadata` (Title/SortTitle/Album/Plot/Year/ReleaseDate/Track) with Genres/Tags/Studios/Directors via `replace_strings`/`replace_people`, MusicVideoArtist rows, and ETV-order artwork discovery (per-video suffix → `.tbn` exact-ext → generic in video dir → parent artist folder). Added two `#[ignore]`-gated integration tests (`local_television.rs`, `local_music_video.rs`) that build a tempdir tree with NFOs and assert metadata + child rows + artwork land via `scan_local_library`. |
| 2026-05-15 | Codex (GPT-5) | Reached the metadata parity midpoint: generic metadata helpers are in place, Jellyfin/Emby Show + Season + Episode scans now write rich metadata child rows, and the scanner has the new serde/quick-xml NFO parser plus SortTitle helper for upcoming Local TV, Artist, and Music Video slices. |
| 2026-05-14 | Codex (GPT-5) | Implemented Emby Show + Season + Episode metadata parity slice: show scans now write `ShowMetadata` with child rows and artwork; season scans write `SeasonMetadata`, poster/fanart artwork, and provider GUIDs; episode scans write `EpisodeMetadata`, thumbnail artwork, GUIDs, genres/tags, actors, directors, and writers with focused Emby provider test coverage. |
| 2026-05-14 | Codex (GPT-5) | Implemented Jellyfin Season + Episode metadata parity slice: season scans now write `SeasonMetadata`, poster/fanart artwork, and provider GUIDs; episode scans now write `EpisodeMetadata`, thumbnail artwork, provider GUIDs, genres/tags, actors, directors, and writers with focused Jellyfin provider test coverage. |
| 2026-05-13 | Codex (GPT-5) | Added ErsatzTV-style `/media/movies` item overflow menu with Add To Collection, Show Media Info, and Troubleshoot Playback actions; ported movie media-info JSON projection and added the playback troubleshooting entry route. |
| 2026-05-11 | Claude Code (claude-opus-4-7) | Embedded the maintenance scheduler into `ersatzrs-web` (issue #20). Added `ersatzrs-scheduler` as a dependency of `ersatzrs-web`, spawn `run_scheduler` from `host/tasks.rs` alongside HDHR UDP discovery and Trakt maintenance, dropped the `[[bin]]` section and `src/main.rs` from `ersatzrs-scheduler` (crate remains a library), and trimmed the now-unused `clap`/`tracing-subscriber` deps and tokio features. Updated STATUS.md and parity-matrix.md to reflect the two-binary shape that the Docker image and per-RID release archives already produce. |
| 2026-05-10 | Claude Code (claude-opus-4-7) | Closed the platform-parity slice. Added the `platform-parity-fixer` project agent and Supported Platforms section in AGENTS.md, split unix-only ffmpeg test helpers into `#[cfg(all(test, unix))] mod unix_tests`, added `Cross.toml` for cross-rs ARM/Windows builds, made `ersatzrs-web/build.rs` warn-and-skip when stylance-cli is unavailable so cross builds succeed, expanded the CI matrix with a 6-cell `platforms` job (linux-x64/arm64/arm via cross, osx-arm64 native, osx-x64 cross from Apple Silicon, win-x64 MSVC, all green on real runners), added a `binaries` matrix to release.yml that attaches `ersatzrs-<tag>-<rid>.tar.gz`/`.zip` to GitHub releases per RID, ported `run.sh` to `run.ps1` plus a `run.cmd` shim for Windows contributors, ignored RUSTSEC-2023-0071 with rationale, and updated parity-matrix.md to mark platform support at parity with ErsatzTV. |
| 2026-05-08 | Codex (GPT-5) | Added the `.claude/agents/support-parity-fixer.md` project agent to drive the Support Playwright parity failures to green while following the repo contracts, ErsatzTV source-first workflow, and Leptos hydration rules. |
| 2026-05-08 | Codex (GPT-5) | Captured the live ErsatzTV Legacy Support page criteria and added scripted Playwright coverage for `/system/health`, `/system/logs`, and `/system/troubleshooting`, including the visible health-check row set and troubleshooting report tabs. |
| 2026-05-08 | Codex (GPT-5) | Fixed `/system/health` Support parity to render the ErsatzTV-style health table from persisted or live check results, filter `NotApplicable` rows, and keep the Support nav group expanded by default. |
| 2026-05-09 | Codex (GPT-5) | Added the `/system/logs` Support route with the ErsatzTV-style heading, Search field, Timestamp/Level/Message table, and pager surface used by the scripted Support parity criteria. |
| 2026-05-09 | Codex (GPT-5) | Added the `/system/troubleshooting` Support route with ErsatzTV-style Tools, General, NVIDIA, and QSV tabs plus copyable diagnostic report placeholders for the scripted Support parity criteria. |
| 2026-05-09 | Codex (GPT-5) | Made `/system/logs` read the newest OS-local app-data log file and render recent runtime entries at the configured tracing level, matching ErsatzTV's newest-log-file Support page behavior. |
| 2026-05-06 | Claude Code (claude-opus-4-7) | Brought the `/ffmpeg/:id` editor to behavior parity with `Pages/FFmpegEditor.razor`: reactive Video Profile select with `main`/`high`/`high444p`-vs-`high10` options that disable when not H264 / HW unsupported; AV1 video format option gated to NVENC/VAAPI/QSV; Tonemap hidden under QSV; QSV Extra Hardware Frames shown only under QSV; Pad Mode visible with `NormalizeVideo` and disabled unless VAAPI; Linux-only VAAPI driver/display sub-rows and shared VAAPI/QSV device dropdown with reactive label; new `load_editor_preflight` server fn enumerates `/dev/dri/render*` and the OS gate; added a `ReactiveSelectField` primitive in `admin/ui/primitives/form.rs` so dynamic option lists no longer recreate the SelectField subtree. Replaced `<Show>` blocks with `style:display` per `TEST.md` rule 2 for stable DOM shape. Hydration mismatch on admin `:id` pages remains a project-wide systemic issue (see TEST.md per-page index — `/media/libraries`, `/channels/:id`, `/ffmpeg/profiles/:id` all ❌). |
| 2026-05-04 | Codex (GPT-5) | Migrated the admin hydration playbooks from `TEST.md` to `@playwright/test` under `tests/e2e/`, added the SQLite E2E seed fixture, fixed `/ffmpeg/profiles` route precedence, and marked the scripted hydration baseline red because Playwright now catches marker-node mismatches on the migrated pages. |
| 2026-05-03 | Claude Code (claude-opus-4-7) | Fixed the dead `Scan` / `Deep Scan` buttons on `/media/libraries` (and the `Sign out of Plex` button on `/media/sources/plex`) by wrapping each page's interactive body in a `#[island]`, since `hydrate_islands()` never wires `on:click` declared at the top level of plain `#[component]` pages. Also stabilized the per-row libraries DOM (always-rendered Deep Scan/Scan/spinner toggled via reactive `style:display` and `disabled`) so hydration cannot fall back to inert SSR markup. Documented the broader latent gap in Phase 0 TODOs. |
| 2026-05-03 | Codex (GPT-5) | Fixed the admin hydration crash by removing the prop-bearing channel editor island from the island hydrator path and keeping the local library media-kind hidden input in a stable SSR/CSR DOM shape. |
| 2026-05-03 | Codex (GPT-5) | Closed UX/UI Slice 1: aligned channel/watermark list layout with ErsatzTV, added generated IPTV logo rendering, fixed local library path normalization/atomic persistence, preserved saved FFmpeg hardware values in the profile editor, and updated UX/parity/deviation docs. |
| 2026-05-03 | Claude Code (claude-opus-4-7) | Closed Slice 16 health-check engine: added `ProviderConnectivityHealthCheck` reading the maintenance-loop persisted `maintenance.provider_connectivity` poll (with friendly Plex/Jellyfin/Emby `ServerName` lookup), `VaapiDeviceHealthCheck` validating the configured profile `VaapiDevice` exists on disk, exposed `ProviderConnectivityResult`/`PROVIDER_CONNECTIVITY_KEY` for cross-crate use, added a `/system/health` admin page rendering the persisted `health.results` snapshot, and made the navigation health badge combine the persisted parity summary with the existing startup/runtime counts; new infra unit tests for both checks. |
| 2026-05-02 | Codex (GPT-5) | Closed Slice 20 config and ADR cleanup: wired global watermark/fallback filler config into playout build post-processing, seeded and loaded subsystem logging levels, moved XMLTV block behavior into core/infra, added ADR 0010 for Classic scheduling, and updated parity docs. |
| 2026-05-02 | Codex (GPT-5) | Closed Slice 19 search hardening: added Lucene-like normalization/modifier behavior, fuzzy/proximity/boost parsing, numeric chapter/duration fields, SQL-backed page maps, image/remote-stream search documents, ADR 0009 dropping Elasticsearch, and updated parity/status docs. |
| 2026-05-02 | Claude Code (claude-opus-4-7) | Closed Slice 18 scanner deepening: extended `MediaStream` writes (and the ffprobe / Plex / Jellyfin / Emby projections) with `Profile`, `PixelFormat`, `ColorRange`/`ColorSpace`/`ColorTransfer`/`ColorPrimaries`, `BitsPerRawSample`, `AttachedPic`, and attachment `FileName`/`MimeType`; decoded `LibraryOptions.PathInfos` from Jellyfin/Emby into new `JellyfinPathInfo`/`EmbyPathInfo` writers (`PlexPathInfo` does not exist in ErsatzTV); added `sync_plex_libraries`/`sync_jellyfin_libraries`/`sync_emby_libraries` for full add/update/delete cascade with `ShouldSyncItems` parity; ported per-folder `Artist` resolution to write `MusicVideo`/`MusicVideoMetadata`, and added `.yml`/`.yaml` walking to write `RemoteStream`/`RemoteStreamMetadata` from local scans; added a multi-page Plex collection corpus test exercising unicode names and mixed item types. Updated plan/persistence-gap-matrix/parity-matrix. |
| 2026-05-02 | Codex (GPT-5) | Remediated Slice 18 scanner review findings: added deep-scan propagation for CLI/admin provider scans, scoped provider etag skip/upsert lookups to the current library, restored found provider rows to `Normal` on non-skip updates, and added focused provider regression tests. |
| 2026-05-02 | Codex (GPT-5) | Started Slice 18 scanner deepening: added etag-based skip for unchanged normal-state Plex/Jellyfin/Emby movie and episode sync paths, avoiding Jellyfin/Emby PlaybackInfo lookups and preserving existing media rows when etags match; updated the slice plan and added focused provider tests. |
| 2026-05-02 | Claude Code (claude-opus-4-7) | Closed remaining Slice 17 review findings: library refresh dispatcher now spawns `ersatzrs-scanner` as a subprocess (provider per library inferred via media-source subtype joins, with sibling-binary discovery and an explicit override for tests/operators); failed/missing-binary cases counted in `MaintenancePassReport.libraries_failed`. Channel-lock `ConfigElement` invention removed; ported ErsatzTV's `IEntityLocker.LockPlayout` to a process-local `ersatzrs-core::locking::EntityLocker` shared by `run_maintenance_once`, plus an RAII `PlayoutLockGuard`. Dropped the `_assert_playout_send_sync` dead code. New tests cover scanner spawn (success + non-zero exit) and the in-memory playout-lock skip. |
| 2026-05-02 | Claude Code (claude-opus-4-7) | Remediated Slice 17 review findings: replaced the broken local datetime encoder with the canonical EF-format helper so playout `LastBuild` and library `LastScan` cutoff comparisons work; switched `LibraryRefreshInterval` to ErsatzTV's `scanner.library_refresh_interval` key, hour units, and `0`-disabled semantics; gated each maintenance loop in `run_maintenance_once` on its own cadence (collapsed `*_loop_ran` bools to `MaintenanceLoopFlags`); honoured `playout.days_to_build`; added a `DailyRebuildTime`-aware `PlayoutBuildMode::Reset` path with channel-number ordering; replaced the invented image/log retention with ErsatzTV's parity-correct channel-guide XML pruning. |
| 2026-05-02 | Codex (GPT-5) | Completed Slice 17 background maintenance loops: added `ersatzrs-scheduler`, core loop interval planning, SQL-backed playout/library/provider/search dispatch, cache cleanup, automatic health persistence, and focused infra tests. |
| 2026-05-02 | Codex (GPT-5) | Remediated Slice 15 Classic scheduler review findings: fixed schedule/media enumerator state drift, alternate schedule selection, fill-group index advancement, Multiple count expressions, guide-group advancement, and Classic collection-key parity; reran workspace clippy/tests. |
| 2026-05-02 | Codex (GPT-5) | Completed Slice 15 classic scheduling parity: added `ClassicSchedulingEngine` for One/Multiple/Duration/Flood, fixed-start handling, `PlayoutBuilder` Classic dispatch, SQL-backed Classic reference-data loading, focused core snapshot tests, and SQLite integration coverage. |
| 2026-05-02 | Codex (GPT-5) | Completed Slice 14 persistence parity: added typed program schedule item and block item repository methods, wrote schedule item subtype/overlay rows plus EF-compatible alternates, wrote block item target/overlay joins, updated the persistence gap matrix and slice plan, and added focused SQLite repository tests. |
| 2026-05-01 | Claude Code (claude-opus-4-7) | Slice 13 FFmpeg filter parity: ported `NormalizeLoudnessFilter`, `FrameRateFilter`, `ColorspaceFilter`, hardware deinterlace (yadif/yadif_cuda/deinterlace_qsv/deinterlace_vaapi), tonemap (sw + cuda/qsv/vaapi), and typed `HardwareUploadFilter`/`HardwareDownloadFilter`. Split `filter/` into per-category modules (`audio`, `colorspace`, `deinterlace`, `framerate`, `hardware`, `tonemap`), extended `FrameState` with `is_hdr`/`should_deinterlace`, and made `build_transcode_pipeline` auto-emit deinterlace/tonemap/fps based on `SourceVideoStream`. 28 new insta snapshots cover per-filter argv plus end-to-end interlaced/HDR/fps-mismatch pipelines. |
| 2026-05-01 | Claude Code (claude-opus-4-7) | Slice 16 health-check parity: ported ErsatzTV's `FFmpegReportsHealthCheck` (warns when `ffmpeg.save_reports` is enabled) into `ersatzrs-infra::health`, wired it into `make_health_check_service`, and added a unit test covering the default/disabled/enabled states. |
| 2026-05-01 | Codex (GPT-5) | Completed Slice 12 FFmpeg capability gating: added report-backed hardware capability checks, wired runtime capabilities into pipeline encoder/decoder selection with software fallback snapshots, and aligned health hardware support detection. |
| 2026-05-01 | Codex (GPT-5) | Completed Slice 11 FFmpeg source decoder dispatch: added source stream metadata to `PipelineRequest`, ported the known decoder table/selection logic, updated pipeline argv snapshots, and documented the parity/status change. |
| 2026-05-01 | Codex (GPT-5) | Started Slice 16 health-check engine parity: added core health contracts/runner, infra SQL-backed checks for FFmpeg/runtime/schema/media/schedule conditions, `ConfigElement` persistence for `health.results`/`health.summary`, and updated AGENTS.md to keep health checks current with port progress. |
| 2026-04-25 | Codex (GPT-5) | Closed a focused non-web parity slice: ExternalJson playout builds now succeed as the ErsatzTV no-op, ProgramSchedule/Block upserts preserve existing item target fields, Jellyfin/Emby sync pages through movies/shows/seasons/episodes/collections, local scanning dispatches by library media kind for movies/other videos/songs/images, and focused core/infra/scanner tests plus clippy were run. |
| 2026-04-24 | Codex (GPT-5) | Fixed the immediate admin hydration gap by moving Search, Channel list, and Channel editor browser-driven actions into Leptos islands, improving the missing `/pkg` asset warning, and documenting the required `cargo leptos build` workflow. Gates: fmt, web native/hydrate checks, workspace clippy, and `cargo test -p ersatzrs-web --all-features`. |
| 2026-04-23 | Codex (GPT-5) | Fixed the post-merge CI regressions by installing `stylance-cli` in the Clippy/Test jobs, ignoring the transitive `rsa` advisory for MySQL support, and verifying the workflow with local fmt/clippy/test/deny checks. |
| 2026-04-22 | Codex (GPT-5) | Adjusted the Channels admin list toward the ErsatzTV screenshot with Roboto fonts, closer PaletteDark tokens, MudTable-like spacing, Show Disabled switch styling, logo/language/mode columns, icon-only actions, and the new `artwork/ersatzrs_full.png` asset. Gates: fmt, `cargo check -p ersatzrs-web --all-features`, and `cargo clippy -p ersatzrs-web --all-targets --all-features -- -D warnings`; live smoke was blocked by local FFmpeg 6.1.1 being below the required 8.1. |
| 2026-04-22 | Codex (GPT-5) | Fixed admin shell sidebar/header parity with ErsatzTV: added the ErsatzRS logo beside the hamburger menu, moved the app bar above the sidebar so it spans the full viewport, switched the sidebar to the gray surface background, and made sidebar groups native collapsed-by-default details controls. Gates: fmt and `cargo check -p ersatzrs-web --all-features`; browser smoke checked with local FFmpeg 8.1. |
| 2026-04-22 | Codex (GPT-5) | Completed the `ersatzrs-web` structure refactor: restored production Leptos config fallback, documented the target `host`/`transport`/`api`/`admin` layout with `public/images` and `public/js`, moved admin pages/server functions/UI/style under `src/admin`, moved IPTV/HDHR/dev streaming under `src/transport`, moved watermark upload endpoints under `src/api`, and split admin routes plus layout modules. |
| 2026-04-22 | GitHub Copilot (GPT-5.4) | Continued the `ersatzrs-web` structure refactor by creating an explicit `src/admin/` boundary, moving the admin shell and home page out of `src/ui/`, splitting the home-page CSS into its own module, and broadening the stylance build watch to track future admin moves. |
| 2026-04-22 | GitHub Copilot (GPT-5.4) | Started the `ersatzrs-web` structure refactor with a compile-safe host extraction: added a real library root, reduced `main.rs` to a thin wrapper, and moved startup, router composition, asset lookup, and background tasks into `src/host/` without changing route behavior. |
| 2026-04-22 | GitHub Copilot (GPT-5.4) | Updated the ErsatzRS SVG wordmarks so the artwork branding reads `ErsatzRS` instead of `ErsatzTV` across the main logo variants. |
| 2026-04-22 | Codex (GPT-5) | Fixed Docker startup after the FFmpeg packaging fix by making `ersatzrs-web` fall back to environment-based Leptos configuration when the build-stage Cargo manifest is absent, setting production `LEPTOS_*` image defaults, and smoke-testing the rebuilt container to healthy `/healthz`. |
| 2026-04-22 | Codex (GPT-5) | Fixed the Docker runtime FFmpeg linkage gap by adding the missing `libwebpmux3` and `libva-x11-2` runtime libraries and final-image `ldd` plus `ffmpeg`/`ffprobe`/`drawvg` checks so unresolved FFmpeg shared libraries fail during image build. |
| 2026-04-22 | Codex (GPT-5) | Phase 7 Slice 10 follow-up — fixed review blockers by restoring all-features CI coverage, adding static GPU capability assertions, pinning Docker helper tools, repairing the Bookworm FFmpeg 8.1 Vulkan/SVT-AV1 build path, adding the Leptos hydration library target, and validating the amd64 Docker check. |
| 2026-04-23 | Codex (GPT-5) | Fixed the web crate's Axum/Leptos dependency alignment so Multipart uploads and Leptos router integration compile again with Axum 0.8, regenerating the lockfile and restoring green `clippy`/workspace test gates for the PR #5 merge path. |
| 2026-04-22 | Codex (GPT-5) | Phase 7 Slice 10 — added ADR 0008, structured FFmpeg/ffprobe 8.1 runtime checks with capability reports and cached hardware detection, in-repo FFmpeg 8.1 Docker multi-arch release scaffolding, GHCR manifest/capability-report workflow checks, and hardware smoke scripts. |
| 2026-04-21 | GitHub Copilot (GPT-5.4) | Added startup readiness and downgrade/schema checks: `ersatzrs-infra` now inspects `_sqlx_migrations` before migrations run so the web host can stay up and report unknown, dirty, or checksum-mismatched schema rows through `/healthz` and the Support badge, and the HDHR config keys now match ErsatzTV with legacy-key backfill. |
| 2026-04-21 | GitHub Copilot (GPT-5.4) | Added an explicit mediaserver bootstrap in `ersatzrs-infra` that ports the current ErsatzTV one-time defaults for the system Trakt playlist group, stock resolutions, default FFmpeg profile/config keys, and default channel; `ersatzrs-web` now uses that bootstrap and the lazy UI/Trakt seed paths were removed. |
| 2026-04-21 | GitHub Copilot (GPT-5.4) | Started database-startup parity work: centralized database URL resolution in `ersatzrs-infra`, switched the default SQLite location to `~/.ersatzrs/ErsatzRS.db` for both web and scanner, added focused resolver tests, and updated the operator docs/examples. |
| 2026-04-21 | GitHub Copilot (GPT-5.4) | Updated AGENTS.md with concrete commands for running the full mediaserver on SQLite/MySQL, health-check verification, optional IPTV JWT enforcement, and scanner invocation. |
| 2026-04-21 | Codex (GPT-5) | Completed Phase 7 Trakt list integration with API sync, matching, generated playlists, admin UI, search route, docs, and gates. |
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
| 2026-04-20 | GitHub Copilot (GPT-5.4) | Phase 7 Slice 5 — IPTV/HLS token verification. Ported ConditionalIptvAuthorizeFilter behavior with route-scoped JWT verification for propagated `access_token` URLs and focused middleware tests. Review outcome: GO for Slice 6. |
| 2026-04-20 | GitHub Copilot (GPT-5.4) | Phase 7 Slice 6 — additional FFmpeg hardware paths. Added QSV/AMF/VideoToolbox/RKMPP/V4L2M2M pipeline builders, QSV hardware-init and scaling/HLS GOP parity, hardware-specific encoder argv snapshots, and cleared the slice gates. Review outcome: GO for Slice 7. |
| 2026-04-21 | Codex (GPT-5) | Phase 7 Slice 7 — MySQL backend support. Added feature-gated MySQL runtime initialization, ordered ErsatzTV-compatible MySQL migrations, backend-aware web/scanner repository wiring, dialect remediations, and real MySQL migration/integration coverage. Review found and fixed a no-default-features scanner test compile regression; final outcome: GO for the next slice. |
| 2026-04-23 | Codex (GPT-5) | Fixed the `cargo deny` CI failures by allowing the transitive `CC0-1.0` license, upgrading the Stylance/httpmock toolchain (`stylance`/`stylance-cli` 0.8, `httpmock` 0.8), and aligning duplicate-version skips with the current Leptos/Stylance dependency graph so `cargo deny check` is clean again. |
| 2026-05-16 | Claude Code (claude-opus-4-7) | Search-functionality plan step 9 — See-All list routes. Extended `MediaBrowserKind` with `Season`/`Episode`/`MusicVideo`/`RemoteStream` (+ `tables()`/`collection_type()` verified against `CollectionType.cs`: 2/20/30/70), added `list_seasons`/`list_episodes`/`list_music_videos`/`list_remote_streams` server fns mirroring `list_movies` with the search page's subtitle/href/artwork projections, wired `SEASONS`/`EPISODES`/`MUSIC_VIDEOS`/`REMOTE_STREAMS` `BrowserKindConfig` constants + island dispatch, added `see_all_lists.rs` page components and the four routes (`/media/tv/seasons`, `/media/tv/episodes`, `/media/music/videos`, `/media/remote/streams`). Also committed the previously-uncommitted plan steps 7–8 (section-grid search page + top-bar search field). Fixed a pre-existing broken test (`scheduling_repository_loads_block_reference_data_from_sql`, broken since `ded20dc`): its `media_by_key["block-item:711"]` expectation `[20]` contradicted ErsatzTV's `MediaCollectionRepository.GetShowItems` show→episode expansion; corrected to the parity-faithful `[12, 13, 20]` matching the sibling Collection-1 assertions. fmt + clippy (default and `--features mysql`) + `cargo test --workspace` (default and mysql) all green. Plan step 10 (Playwright playbook + final passes) remains. |
