# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
this project follows [Semantic Versioning](https://semver.org/spec/v2.0.0.html),
and commit messages use
[Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/).

Entries are curated for people, not generated from raw git logs. Keep
`[Unreleased]` current during development, then move its entries into a dated
release section when tagging a release.

## [Unreleased]

### Fixed

- Media collection route-family parity is complete for multi-collections,
  rerun collections, playlists, and filler presets. Multi-collections now have
  HTML POST fallback save/delete routes matching the other media collection
  editors, and durable browser coverage proves the route family renders the
  localized list/editor workflows without console errors.
- Scanner parity is complete: local music-video NFO parsing now accepts
  ErsatzTV-style XML fragment streams while preserving fallback behavior for
  invalid sidecars, local scans retry SQLite busy/locked contention instead of
  aborting immediately, and the media-server `Unavailable` state branch has
  regression coverage.
- Admin media scan results now refresh hydrated views without a manual reload:
  `/system/health` recomputes current health checks, `/media/trash`,
  `/media/libraries`, and the media browser lists refetch when scanner-updated
  media state changes. `/media/trash` now includes the same `FileNotFound`
  media families as ErsatzTV, including missing TV episodes that previously
  appeared in health checks but not Trash. The non-ErsatzTV green scan-queued
  snackbar on `/media/libraries` was removed.
- Release builds now serve every Leptos server function again, fixing the
  Windows release-build failures where **Scan Library** (`/media/libraries`) and
  **Show Media Info** for TV episodes reported
  `error deserializing server function results: Invalid format: missing delimiter in ""`.
  Root cause: `server_fn` derives each `/api/<fn><hash>` route from
  `xxh64(CARGO_MANIFEST_DIR + ":" + module_path)`, so the absolute build path
  leaked into the URL. The release pipeline builds the hydration WASM on a Linux
  runner and the per-RID server binary on its own runner, so the two halves
  computed different hashes; the hydrated client POSTed to routes the server
  never registered and every server-function call returned an empty `404`. The
  build now pins `SERVER_FN_OVERRIDE_KEY` (in `.cargo/config.toml`) so paths are
  identical regardless of where each half is compiled. This supersedes the
  1.0.11-alpha attempt, which changed the scan return type but did not address
  the path mismatch.

### Changed

- Revised the default FFmpeg profile decision (ADR 0017). New installations will
  seed **five** ready-to-use profiles instead of one — 720p, 1080p, 1080p HDR,
  4K, and 4K HDR, all with AC-3 6-channel 640 kbps audio — and **auto-enable
  hardware acceleration when a usable encoder is detected**. Bitrates are tuned
  for 25 fps content over 1 Gbps HTTP unicast with two concurrent streams. (ADR
  only; the seeding code is not yet implemented.)
- ADRs now track decision state and implementation state as two independent
  sections. `## Status` records only the decision lifecycle
  (Proposed/Accepted/Superseded); a new `## Implementation` section records
  whether the code exists (`Not implemented` / `Partial` / `Implemented` /
  `N/A`), so an `Accepted {date}` ADR can no longer be mistaken for shipped
  code. `cargo xtask progress adr` surfaces a "Built" column, and
  `cargo xtask progress check` fails if an ADR is missing the section or claims
  `Implemented` while a parity debt linked to it (`adr = "NNNN"`) is still open.

## [1.0.11-alpha] - 2026-06-17

### Fixed

- Library and TV show scan actions now return a non-empty server-function
  payload, fixing the release-build scan toast failure
  `error deserializing server function results: Invalid format: missing delimiter in ""`.

## [1.0.10-alpha] - 2026-06-17

### Fixed

- Windows release UI interactions are restored for the `/media/libraries` scan
  button, `/system/logs` pager controls, and TV season episode **Show Media
  Info** actions. The libraries page now renders its toast container inside the
  hydrated island, and the Logs pager no longer leaks Leptos macro text into the
  DOM or loops log-page reloads after loading the saved page size.

## [0.0.9-alpha] - 2026-06-17

### Fixed

- Missing library items are now detected during scans. Previously nothing ever
  flagged moved/deleted media, so such items stayed `Normal` forever and never
  appeared in `/media/trash`, the **File Not Found** / **Unavailable** health
  checks, or anywhere else. Local scans now flag items whose file disappeared as
  `FileNotFound`; media-server scans (Plex/Jellyfin/Emby, movies and TV) trash
  items no longer present on the server and flag items whose local file is
  missing as `RemoteOnly`. The `Skip Missing Items` setting
  (`playout.skip_missing_items`) is now honored: when enabled, `FileNotFound`
  and `Unavailable` items are excluded from playout scheduling, matching
  ErsatzTV's `PlayoutBuilder`.

### Changed

- HLS Segmenter parity tracking now documents the open calibration playback
  deviation debt: structurally clean HLS captures can still drift at decoded
  content boundaries, so TSDuck transport checks alone are no longer treated as
  sufficient proof of playout continuity.

### Added

- Admin UI languages are now registered from the language files themselves, so
  adding a supported admin language only requires adding a new module under
  `crates/ersatzrs-web/src/admin/languages/` with the standard metadata block
  and translations.
- Remote Streams now has an ErsatzTV-style admin browser at
  `/media/remote/streams`, including search-index filtering, card paging,
  letter jumps, collection/playlist add actions, no-JS list HTML, and
  Playwright coverage.
- Playback calibration and verification tooling for streaming work.
  `scripts/calibration/` generates three 1080p60 clips (per-clip background
  colour, large clip number, millisecond ABS/per-clip timecodes, frame
  counters, a per-second sweep bar, a centred geometry square, and a per-second
  audio pip) as MKV sources plus MPEG-TS, for visually tracking playout order
  and spotting jitter/skips. `scripts/e2e/` adds a TSDuck end-to-end playback
  gate that validates served streams — continuity-counter/PCR/PTS integrity,
  PSI structure, and audio/video PIDs — across the MPEG-TS (legacy and hybrid)
  and HLS (segmenter and direct) pipelines. The gate is a required manual check,
  not part of CI; the generated clips are gitignored and regenerated on demand.

### Fixed

- Support pages parity is now closed for Health, Logs, and Troubleshooting:
  dynamic Support health badges render hydrated warning/error counts, Logs
  supports ErsatzTV-style search, Timestamp/Level sorting, persisted page size,
  and pager navigation, and durable browser coverage exercises the support
  route-family fallbacks.
- Scheduling admin route-family parity is now closed for schedules, blocks,
  templates, decos, and deco templates, with durable browser coverage for the
  hydrated route workflows, editor fallback forms, localized labels, and
  console-clean visual geometry.
- Classic scheduling now preserves imported schedule-item filler preset IDs and
  schedules ErsatzTV-style per-item pre-roll, post-roll, tail, and fallback
  filler around Classic playout items while keeping fill-group state intact.
- Opacity-expression watermarks now loop still image inputs so FFmpeg evaluates
  ErsatzTV clock variables over live direct `.ts` and HLS segment timelines
  instead of freezing the expression at the first image frame.
- HLS Segmenter now enforces a configurable active transcode limit
  (`ffmpeg.segmenter.active_transcode_limit`, default 2) in addition to the
  existing work-ahead limit, so rapid channel switching does not leave many old
  idle sessions spawning FFmpeg processes while the current viewer keeps
  priority.
- HLS Segmenter FFmpeg seeks now preserve sub-second playout and in-point
  offsets through the worker, command builders, direct IPTV stream path, and
  troubleshooting previews instead of flooring to whole seconds before `-ss`.
- The TSDuck playback gate now converts shell paths to host-native paths when
  needed, isolates the server temp directory per run across platforms, waits
  for transient SQLite bootstrap locks, and checks HLS Segmenter audio-pip
  cadence in the decoded capture.
- The Windows launcher build script now consumes the `embed_resource`
  compilation result so the workspace clippy gate stays clean with
  `-D warnings`.
- FFmpeg playback now uses persisted media stream indexes for video and audio
  inputs, fixing watermarked channels whose files store audio before video,
  such as the MTV channel stream failing with a filtergraph stream-specifier
  error.
- HLS Segmenter `StopOnDisconnect` sessions now stop after the configured idle
  timeout even while FFmpeg is still running, segment-file requests refresh the
  session like ErsatzTV, and the MPEG-TS hybrid wrapper waits for initial HLS
  segments before wrapping a restarted session.
- Settings route-family i18n parity is now browser-proven across `en-us`, `pl`,
  `pt-br`, and `nl` for `/settings/ffmpeg`, `/settings/logging`,
  `/settings/hdhr`, `/settings/scanner`, `/settings/playout`, `/settings/ui`,
  and `/settings/xmltv`, with translated headings, labels, save actions, and
  no English fallback leakage outside intentional technical values.
- FFmpeg profile editing now exposes ErsatzTV-style H.264 profile choices,
  persists NVENC `high444p`, and shows the unsupported-format warning when
  NVENC is paired with MPEG-2.
- Legacy MPEG-TS concat streams now stop each internal stream at the active
  `PlayoutItem` boundary, honoring short `InPoint`/`OutPoint` durations so
  `mode=ts-legacy` advances through cyclic playout items instead of continuing
  the same media file past the seam.
- Classic scheduling now avoids immediate repeats across shuffled collection
  reshuffle boundaries and persists continue-build anchors at the last
  generated item finish when a media item overshoots a build hard stop.
- Channel music-video credit subtitles now extract the bundled ErsatzTV fonts
  before playback, so `_default.ass` renders with the matching title style, and
  the `/channels` Troubleshoot action now only appears for enabled channels
  with playouts.
- Windows release builds now discover bundled Leptos hydration assets from the
  `site/pkg` directory next to the server binary even when `LEPTOS_*` runtime
  variables are absent, restoring hydrated admin actions such as watermark
  saves and `/channels` stop-session visibility.
- Watermark placement previews now avoid Windows drive-letter escaping in the
  FFmpeg `drawvg` filter, fixing preview refreshes in the Windows admin build.

## [0.0.8-alpha] - 2026-06-13

### Added

- AV1 hardware-decode fallback now learns unsupported CUDA/NVDEC paths at
  runtime and falls back to software `libdav1d` decode instead of repeatedly
  attempting a known-bad hardware path.
- Dynamic watermark overlays now route through the graphics overlay plan,
  including loop options and NCalc opacity-expression translation for
  ErsatzTV-style watermark timing.
- Music video credit templates now render in the admin UI.
- Workflow review gates now validate parity progress, runtime smoke coverage,
  and completed-prompt hygiene before status work is considered closed.
- ADR 0014 now records the hardware capability fallback contract for FFmpeg
  decode behavior.

### Changed

- Top-gap automation was hardened, then removed with its agent documentation
  once the workflow moved back to explicit curated gap prompts.
- Dependabot updates for `chrono`, `stylance`, `uuid`, and
  `download-artifact` were batched into one dependency-maintenance slice.
- Progress and status documents now narrow completed work to the exact proven
  parity scope and keep unfinished hardware-decode debt visible.

### Fixed

- Smart-collection search now covers the documented ErsatzTV Lucene fields,
  relative date helpers, and nested `smart_collection` expansion used by
  smart collections, fixing the `Music Videos` profile query from 699 to the
  expected 377 results.
- Windows release packaging now prefers Git Bash on the self-hosted runner so
  bash-based release steps receive valid GitHub Actions temp script paths.
- Collections and smart collections now restore the pinned ErsatzTV admin
  layout contract, including the manual add-page save path, no-JS collection
  form fallback, detail-page custom ordering, and browser parity coverage.
- HLS segmenter playlists now keep program-date-time and MPEG-TS continuity
  counters stable across appended FFmpeg process boundaries, fixing the
  parallel live HLS playback freeze debt.
- The four ADR 0014 hardware-decode parity debts are closed in the FFmpeg
  capability and fallback path.
- FFmpeg fake-tool snapshot tests serialize script execution to avoid
  `ETXTBSY` flakes.
- Multiple parity documents were corrected from completed-looking status back
  to `In progress` where open debt remained.

## [0.0.7-alpha] - 2026-06-10

### Changed

- Release packaging documentation now describes the actual web asset and
  archive paths used by the release workflow.

### Fixed

- Docker release builds now use the workspace context and no longer leak host
  target-directory paths into build stages.
- Release jobs run on the intended self-hosted runners, with Linux checks kept
  on Linux hosts.
- Windows target-directory configuration was removed from committed config so
  release builders can use their own platform paths.
- FFmpeg subtitle filter paths are escaped correctly for both parser levels,
  including Windows paths.
- Subtitle burn-in is skipped when a seeked subtitle has no active cues.
- Scanner writes `VideoScanKind` values with ErsatzTV-compatible numbering.
- NVENC and CUDA FFmpeg paths keep still images and CUDA surface tracking on
  the correct software/hardware boundary.
- Platform-dependent FFmpeg snapshots were made Windows-neutral.

## [0.0.6-alpha] - 2026-06-10

### Fixed

- Windows run configuration now reads `.cargo/config.toml`, escapes
  target-directory backslashes correctly, and strips verbatim path prefixes
  from normalized library paths.
- The Windows launcher sets `LEPTOS_OUTPUT_NAME` for the bundled server.

## [0.0.5-alpha] - 2026-06-09

### Added

- FFmpeg and FFprobe path resolution now goes through the `FFmpegLocator` port.
- FFmpeg command generation parity was extended across runtime filter
  requirements, NVIDIA decoder probing, VideoToolbox probing, playback metadata
  options, encoder capability probes, QSV extra frames, HLS PTS offsets, MP4
  flags, realtime still-image filters, HDR/zscale smoke coverage, and
  zscale/libzimg runtime requirements.
- Channels gained a `Stop Transcode Session` action.
- The HLS, IPTV, and streaming runtime paths gained transport-parity coverage
  for direct stream cancellation, HLS segment lifecycle handling, playout
  updates on live time, and FFmpeg stream-failure logging.
- Bundled template presets were added as application assets.
- Release packaging gained web asset reuse for Docker publishing, frontend-only
  web asset builds, lower-memory release settings, and Docker disk-pressure
  reductions.
- CI gained path filtering, run cancellation, and fail-fast behavior to reduce
  wasted runs.
- Daily progress routines, pinned parity baselines, and gap prompt refresh
  documents were added to the progress workflow.
- Cargo workspace with the initial domain, core, infra, app, FFmpeg, web, and
  scanner crates.
- Initial documentation contracts for long-term solo development and future
  public open-source collaboration, including Diataxis docs, ADRs, porting
  contracts, GitHub templates, CI, release scaffolding, dependency-update
  scaffolding, and repo-local agent skills.
- Application artwork in SVG, PNG, ICO, and ICNS formats.
- FFmpeg 8.1 runtime version detection and the initial `axum` plus Leptos SSR
  web host with `/healthz`.
- SQLite startup bootstrap, the collapsed initial schema, repository traits,
  SQLx repository implementations, offline SQLx metadata, and integration
  tests.
- Domain models for configuration, channels, playout, scheduling, media,
  collections, libraries, media sources, FFmpeg profiles, filler, graphics,
  and metadata.
- Persistence design documentation and a persistence gap matrix covering the
  ErsatzTV SQLite tables.
- Real-world ErsatzTV SQLite compatibility coverage gated by
  `ERSATZRS_REAL_WORLD_SQLITE`.
- FFmpeg foundation with typed inputs, command generation, complex filter
  routing, software/NVENC/VAAPI pipeline builders, drawvg SVG template
  rendering, ffconcat playlist writing, and snapshot coverage.
- Scheduling engines and persistence through Phase 3, including block, YAML,
  Lua scripted scheduling, SQL-backed content resolution, smart collection
  search, history parity, and playout build persistence.
- Media source scanning for local files, Plex, Jellyfin, and Emby, including
  ffprobe metadata, provider credentials, provider library bootstrap writes,
  Plex collections, Emby shows/seasons/episodes, and ErsatzTV-compatible media
  file path hashing.
- IPTV and HDHomeRun protocol surfaces, including M3U, XMLTV, HDHomeRun HTTP
  endpoints, UDP discovery, MPEG-TS streaming, and persisted device UUIDs.
- Leptos admin UI foundation, layout/navigation, channel editor, media library
  and browser pages, collection editors, block scheduling UI, and Channels
  visual parity work.
- HLS segmenter hardening, playlist trimming, fMP4 support, direct HLS
  passthrough, and runtime FFmpeg/Docker multi-arch support.
- SQLite or MySQL initialization from `DATABASE_URL` when built with the
  `mysql` feature, ErsatzTV-compatible MySQL migrations, and MySQL integration
  coverage.
- Dependency and CI hardening for `cargo deny`, including CSS and test HTTP
  tooling updates, lockfile refreshes, license policy updates, and
  duplicate-version policy alignment.

### Changed

- The AGPL `evalexpr` dependency was replaced with an in-tree condition
  evaluator.
- Cargo dependency maintenance updated `windows-sys`, `embed-resource`, and
  related Dependabot-managed packages.
- Snapshot review documentation now records that `cargo-insta` is already
  installed.
- Project branding, crates, binaries, Docker wiring, documentation, and
  snapshots were renamed from NasTv/`nastv-*` to ErsatzRS/`ersatzrs-*`.
- Persistence moved to an expanded normalized SQLite schema derived from the
  final ErsatzTV model instead of the earlier hybrid JSON/scalar mapping.
- SQLite repository decoding and writes now use ErsatzTV-compatible integer
  enum values, EF-style UTC `DateTime` text, and ErsatzTV `TimeSpan` parsing.
- FFmpeg runtime detection moved into `ersatzrs-ffmpeg` and is reused by
  `ersatzrs-web`.
- Cargo deny metadata and policy now cover workspace crates and accepted
  transitive license/advisory exceptions.
- Workspace package metadata, lints, binary declarations, Rust version,
  README, `.gitignore`, and SQLite migration path documentation were expanded
  into the current project contracts.

### Removed

- `docs/003_ersatztv_sqlite3_schema.sql` was removed after the normative
  `0001_initial.sql` migration and persistence gap matrix superseded it.

### Fixed

- Release packaging now includes web assets for all archives, repairs packaged
  build targets, and reuses built web assets in Docker publish jobs.
- FFmpeg runtime parity was hardened across NVENC CUDA initialization, CUDA
  overlay filters, required runtime filters, zscale/libzimg requirements,
  VideoToolbox probing, NVIDIA encoder/decoder probing, HEVC options, bitrate
  and frame-rate options, audio output options, GOP spelling, and SD
  colorspace normalization.
- HLS segmenter transport parity now preserves fMP4 init segments and closes
  segmenter transport gaps.
- Streaming now keeps HLS playout updates on live time.
- Scheduling persists continue-build anchors.
- Cargo deny policy was updated for `RUSTSEC-2026-0173`.
- Web stream-seek test assertions were corrected.
- Admin libraries and Plex source buttons are hydrated islands with stable DOM
  structure so click handlers do not fall back to inert SSR markup.
- Admin shell sidebar visual parity was tightened around logo placement, top
  app bar width, sidebar surface color, and default collapsed groups.
- Template repository upserts no longer write placeholder child rows that
  violate normalized foreign keys.
- Docker health checks no longer depend on `wget`.
- Web startup tracing, listen address handling, UTF-8 FFmpeg output validation,
  and FFmpeg version test coverage were tightened.

[Unreleased]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.8-alpha...HEAD
[0.0.8-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.7-alpha...v0.0.8-alpha
[0.0.7-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.6-alpha...v0.0.7-alpha
[0.0.6-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.5-alpha...v0.0.6-alpha
[0.0.5-alpha]: https://github.com/bartdeijkers/ErsatzRS/releases/tag/v0.0.5-alpha
