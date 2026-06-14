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

### Added

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
