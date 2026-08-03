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

- Added an opt-in **Settings** -> **UI** control that starts the packaged
  `ErsatzRS.exe` tray launcher when the current Windows user signs in. The UI
  reads the live per-user registry state, can repair a stale release path, and
  reports a missing launcher or inaccessible registration without persisting a
  misleading database flag. Linux remains service-manager owned and now has a
  systemd installation guide modeled on Jellyfin's package behavior.

### Fixed

- Fixed local TV remakes and reboots with the same title being merged into one
  show. Full local-library scans now distinguish folder, NFO provider-ID, and
  year identities; transactionally split existing mixed seasons and extras;
  preserve the largest hierarchy's IDs and references; invalidate affected
  folder ETags; clean empty parents without deleting extras-only parents; and
  report any remaining cross-folder ownership through health checks.
- Fixed Windows release validation so the targeted startup-registration tests
  use the normal test profile, where cross-crate test support is available,
  instead of failing to compile before reaching the Windows registry checks.
- Fixed Classic playout corruption across multiple channels by letting SQLite
  and MySQL allocate globally unique identities for collection and fill-group
  enumerator state. Runtime-state writes can no longer replace another
  playout's rows on a primary-key collision; startup audits and Refreshes only
  affected Classic playouts (including legacy textual schedule-kind rows),
  later Continue builds retain the same guard, and the localized health report
  exposes any state that cannot be repaired safely.
- Fixed music video cards showing the letter-placeholder instead of a
  thumbnail for the common same-basename-image sidecar convention (e.g.
  `Song.jpg` next to `Song.mkv`, with no `-thumb` suffix). Local scanning now
  recognizes the bare same-basename image as Thumbnail artwork, matching
  ErsatzTV's `MusicVideoFolderScanner.LocateThumbnail` behavior.

## [1.0.17-beta] - 2026-08-02

### Added

- Added rich XMLTV guide content: `/iptv/xmltv.xml` now renders plot, genre
  categories, content rating (mapped to `MPAA`/`VCHIP` per ErsatzTV's `us:`
  convention), artwork icons, episode numbering (`onscreen` and `xmltv_ns`
  systems), and per-kind titles for movies, episodes, music videos, songs,
  other videos, and remote streams, matching ErsatzTV's own field set.
  Channel elements gain the three-way `<display-name>`, category tags, and
  the same logo fallback chain as the M3U playlist. Guide templates are
  customizable per media kind via `minijinja`, following the same bundled-
  default/override-folder convention as the existing Music Video Credits
  templates (`<data>/templates/channel-guide/<name>.jinja`); a broken custom
  override falls back to the bundled default for that one kind, and a
  template that renders invalid XML falls back to a minimal element for
  that one item, rather than either failing the whole guide. Movie icons
  deliberately fall back to correctly sized thumbnail artwork when no poster
  exists, while bundled Remote Stream entries emit persisted genre categories
  for better filtering and discovery. Locally-scanned
  library artwork icons are now actually reachable (see Security) — this
  previously 404'd because the artwork route only understood Jellyfin/Emby
  paths and a local-cache hash convention nothing ever populated.
- Added standalone Linux/macOS and Windows Beeld & Geluid Schatkamer Remote
  Stream extractors and ErsatzRS library instructions.
- Added complete Remote Stream YAML scanning and URL/script probing, direct
  producer piping for IPTV and HLS, live/reconnect timing, structural health
  checks, and per-card Add To Playlist support.
- Added Jellyfin-inspired Version, Video, Audio, and Subtitle selection to
  movie details for exact Add To Schedule and Add To Playlist entries. The
  selected media version and stream indexes persist through editors and
  scheduling into MPEG-TS/HLS playback; stale selections block playback and
  appear in health reporting instead of silently falling back.
- Added a top-level `/epg` preview and a muted-autoplay channel preview modal
  plus dedicated `/channels/:id/watch` experience. The shared guide presents
  two hours of history and 24 hours ahead in accessible, scrollable
  multi-channel lanes; browser playback uses an isolated H.264 eight-bit/AAC
  derivative with Hls.js-first selection, localized failures, scoped preview
  tokens, responsive uncropped desktop/mobile sizing, and explicit local-preview
  versus shared-session controls. Normal IPTV profiles and output remain
  unchanged.
- Added an exhaustive production-admin route contract for Playwright coverage,
  with seeded collection-detail interaction proof for custom playback order and
  item removal. The contract excludes only the feature-gated developer gallery
  and fails when the production router and hydration evidence drift apart.
- Added on-demand GitHub-hosted CI through a manual runner selector. Pull
  requests and `main` pushes still use self-hosted Linux by default, while
  hosted Test work is sharded by web target, uses a lower-memory test profile
  plus LLVM's `lld` linker, and caches pinned build tools to stay within the
  hosted job lifetime. Hosted and self-hosted runs use independent concurrency
  groups so either pool remains available as a fallback.
- Added optional default and fallback subtitle languages to `/settings/ffmpeg`.
  Built-in MPEG-TS and HLS playback now applies item and channel preferences
  first, then the global default, and tries the global fallback before
  selecting no subtitle.

### Changed

- FFmpeg playback-language settings now use Kodi's common ISO-639-1-backed
  language list instead of exposing historical ISO-639-3 entries. The `und`
  value is presented as Original for audio and Undefined for subtitles,
  selects missing-language streams consistently in MPEG-TS and HLS, and is
  persisted for local sidecar subtitles without a language suffix. Stored
  language-code case and surrounding whitespace are matched
  case-insensitively and normalized without changing the selected language.
- Windows beta tags may now use either the terminal `-beta` form or a numbered
  `-beta.N` suffix; both publish only the win-x64 prerelease archive and skip
  the normal multi-platform and Docker release jobs.
- Release archives and the Docker image now ship a single merged `site/`
  directory for admin-UI assets instead of duplicating them across separate
  `site/` and `public/` folders. Release packaging explicitly merges the
  generated `public/` assets with the hydration `pkg/` bundle because
  frontend-only `cargo-leptos` builds do not consistently copy `assets-dir`
  content into `site-root` on every platform. `crates/ersatzrs-web/public/`
  remains the source layout; only the packaged/runtime duplication was removed.
- Replaced the EPG Visible window scrubber with a 50–200% Zoom control that
  rescales the horizontal timeline and preserves its temporal center. The
  default desktop scale shows about three hours, and the channel-watch layout
  now left-aligns its bounded video while giving the guide full width.
- Self-hosted CI jobs now share a persistent `CARGO_TARGET_DIR` outside the
  auto-cleaned checkout instead of restoring `target/` through a remote cache
  round-trip on every run; `clippy`, `test`, and `hydration` no longer pay
  that round-trip on the self-hosted path (the GitHub-hosted fallback is
  unaffected). Also fixes a `test_hosted` matrix cache-key collision, adds a
  manual `reset_target_dir` dispatch input, and adds idempotency guards and
  npm caching for a few smaller redundant CI steps.
- Split the largest media-browser transport, YAML and scripted scheduling
  registries, and Jellyfin/Emby adapters into enumerable feature modules.
  Exhaustive route, instruction, Lua-global, and media-source gates now prevent
  new variants from silently falling through while preserving public imports,
  server-function endpoints, serialized YAML, scheduler output, provider
  requests and persistence, hydration, and visible admin behavior.
- Search and per-kind media browser cards now preserve ErsatzTV's artwork
  semantics: poster cards request provider height 440, thumbnail cards request
  height 220, and existing provider resize parameters are replaced instead of
  duplicated. Season and episode cards keep valid upstream navigation, music
  videos no longer receive fabricated detail links, and See All routes preserve
  the active query through pagination and letter navigation. The pinned
  upstream routes have no library-filter combobox, so ErsatzRS does not invent
  one.
- Watermark editing now renders the selected custom asset in a truthful
  Full-HD reference preview. Actual Size uses native image dimensions, the
  preview includes a centered 4:3 safe-frame guide without an outer 16:9
  guideline, and localized Previous/Next controls provide deterministic
  comparison navigation with explicit list ends and unsaved-edit protection.
- Split Trakt maintenance into capability-honest local and remote paths. Local
  matching, generated playlists, settings, deletion, and hourly rematching no
  longer require a placeholder API client; interactive and scheduled remote
  refresh require real configuration and retain typed failures without
  preventing independent local matching.
- Completed the admin application-layer extraction. Collection, playlist,
  channel, schedule, settings, Trakt, media, playout, template, deco, filler,
  watermark, troubleshooting, artwork, and layout transports now call
  storage-neutral application commands and queries while infrastructure owns
  SQL, typed row decoding, transaction boundaries, and SQLite/MySQL details.
  Shared localized resource states no longer turn storage or decode failures
  into submittable forms or plausible empty/default data, and architecture
  lint now forbids raw admin pool access alongside direct SQL and silent
  fallback patterns.
- Unsaved block previews now use the production block scheduling engine in
  Reset mode with temporary template, playout, and reference state instead of a
  simplified duration projection. The editor shows the actual enriched rows
  produced from every supported block collection kind, preserves drafts
  without saving, and distinguishes loading, valid empty, and localized
  failure states.
- Updated futures-util to 0.3.33, Tokio to 1.53.1, leptos_axum to 0.8.10,
  async-trait to 0.1.91, and clap to 4.6.4. Scheduled Cargo and GitHub Actions
  updates now arrive in one weekly multi-ecosystem Dependabot rollup instead of
  separate pull requests.
- All playout-build requests now enter through one application command with
  typed accepted, queued, building, unavailable, and rejected outcomes. Admin
  and API side effects share the same FIFO admission, coalescing, and activity
  state while the scheduler retains Refresh/Reset processing and persisted
  build behavior. Stronger modes now upgrade queued work, and mutations that
  arrive during an active build retain one mode-upgradable follow-up instead
  of being silently discarded.
- Library and targeted-show scans now enter through one application-owned FIFO
  command boundary and operation registry. Web and periodic refresh producers
  share persisted provider validation, queued/running/progress/terminal state,
  Local show fallback, provider-specific show synchronization, and retained
  failure diagnostics while the native composition root keeps concrete
  provider, local scanner, and embedded-subtitle execution. Periodic refresh
  coalesces a library that is already queued or running without changing manual
  FIFO repeat behavior, and rejected scan requests now surface as failures
  instead of false queued confirmations.
- The Phase 7B application-service foundation now gives `ersatzrs-app`
  documented command, query, typed-error, port, and operation-coordination
  boundaries. Core no longer exposes SQLx errors, infrastructure maps concrete
  storage failures explicitly, and the native web composition root injects
  concrete repositories into one `ApplicationServices` container shared with
  scheduler tasks.
- Development-only component-gallery and sample-stream routes now require a
  non-default build feature plus explicit runtime opt-in. Structured hydration
  data is now the canonical admin route-verification source, with validation
  against duplicate or mismatched TEST.md route tables and durable browser
  coverage for the detailed playout workflow.
- Admin settings and library storage failures now render a localized, stable
  `data-state="error"` resource instead of a plausible default form or empty
  list. Their reads use typed infrastructure models and the shared repository
  error vocabulary; architecture lint now also ratchets alternate awaited
  result fallbacks, form-rendering Suspense fallbacks, and row-decode defaults.
  Library polling errors clear after the affected load or a complete polling
  cycle recovers instead of leaving the page permanently in an error state.
- Established the storage-neutral application-service boundary and native web
  composition root in ADR 0051. A baseline-aware architecture gate now blocks
  an empty app crate, forbidden adapter dependencies, new direct admin SQL, and
  new server-error-to-default loaders while existing debt is migrated by
  vertical slice; raw Executor SQL, SQLx glob imports, and production-capable
  cfg forms cannot bypass its debt accounting.

- Added dependency-aware structural milestone tracking through
  `cargo xtask progress`, with validated acceptance/evidence records for
  restoring the application layer, closing audited stopgaps, and preparing the
  future "The Box" video-jukebox add-on API.
- Renamed the FFmpeg profile audio/video transcoding controls and database
  fields from the ambiguous `NormalizeAudio`/`NormalizeVideo` to
  `TranscodeAudio`/`TranscodeVideo`. Disabling them still copies the source
  streams; LoudNorm, framerate, and color normalization remain separate.
- Made Dutch (nl) admin UI terminology consistent. "Shuffle" now uses the common
  loanword everywhere instead of the uncommon "schudden", and the "Scripted",
  playout, deco, and multi-collection wording is unified (one rendering each,
  matching the surrounding labels).
- Saving a channel no longer rebuilds its playouts automatically, matching
  ErsatzTV. Manual Refresh/Reset and Reset All requests now use a reliable
  unbounded worker queue, acknowledge queued work immediately, and show
  queued/building activity until the persisted build outcome is available.
- Classic schedule items now present Smart Shuffle as a Playback Order option.
  Selecting it keeps the compatible persisted Shuffle order and enables the
  item policy; its spacing controls appear inline only while selected. Existing
  Smart Shuffle policies attached to other randomized Classic orders normalize
  to Shuffle when next saved.

### Removed

- Removed the Smart Shuffle Overview preview panel from the schedule items page
  (`/schedules/{id}/items`) and the block editor, along with its server
  functions, response types, translations, and the underlying
  `smart_shuffle_candidates` repository query. Building the preview had to
  enumerate every candidate of every item in the draft, which was too slow to
  be useful. The Smart Shuffle item settings themselves are unchanged, as is
  Smart Shuffle behavior during playout building.

### Fixed

- Fixed rich XMLTV guide rendering for metadata and provider URLs containing
  XML metacharacters, localized non-ASCII ratings, custom-title overrides,
  episode poster/still artwork parity, and literal percent sequences in local
  artwork paths. Custom guide templates now receive ErsatzTV's complete
  per-kind context, and guide metadata is loaded in bounded batches instead of
  issuing sequential subtype and relationship queries for every distinct
  scheduled item. Token-complete custom fragments with unclosed elements now
  take the per-item fallback path instead of corrupting the complete guide.
- Fixed the XMLTV `<channel id>`/`<programme channel>` attributes, which used
  the bare channel number and could mismatch the M3U playlist's checksummed
  `tvg-id`, breaking guide-to-channel association for IPTV clients that match
  strictly by id. The `xmltv.days_to_build` and `xmltv.time_zone` settings
  now take effect (previously always rendered a hardcoded 48-hour UTC
  window regardless of the persisted setting). Jellyfin and Emby movie sync
  now persist genres, tags, actors, directors, and writers, matching show
  and episode sync (previously silently dropped for movies only).
- Delayed mirror channels now reuse their source channel's playout across
  MPEG-TS, HLS, browser preview, admin EPG, and XMLTV while keeping the mirror's
  own channel settings. Signed offsets round-trip correctly, source history is
  retained until delayed broadcasts finish, and the admin UI prevents valid
  mirrors from receiving an independent playout.
- Beeld & Geluid Remote Stream extractors now select one video/audio rendition
  pair instead of mapping every HLS rendition into the MPEG-TS pipe, preserving
  the stream order required by channel HLS transcoding.
- Windows Remote Stream batch producers now receive their quoted script path
  and arguments through the exact `cmd.exe` command line, so paths containing
  spaces and URL arguments no longer become literal quoted executable names
  during scanning or IPTV/HLS playback.
- Scanner settings now identify the library refresh interval as hours in every
  supported language, matching the unit used by the scheduler and ErsatzTV's
  field adornment.
- Admin toasts now match ErsatzTV's pinned MudBlazor 8.15 snackbar behavior and
  appearance: filled light/dark severity colors, top-right placement, semantic
  icons, accessible inline controls, five-item oldest-first queueing, duplicate
  suppression, timed entry/visibility/exit, and lifecycle pause during pointer
  or touch interaction.
- Playwright E2E now explicitly regenerates the Stylance CSS bundle and
  synchronizes the authoritative web `public/` assets into the built Leptos
  site root before booting the server, preventing clean checkouts paired with
  persistent Cargo targets from omitting generated CSS or producing broad 404,
  MIME-type, hydration, and geometry failures.
- HLS work-ahead now evaluates Remote Stream liveness at the actual transcode
  cursor, preventing a following live item from being opened before its
  scheduled start. Script producers are cancelled and reaped whenever IPTV,
  HLS, or ffprobe stops consuming them, including stalled producers with no
  stdout, and producer stderr now retains only a bounded rolling tail.
- Exact movie stream selections now preserve an explicitly silent audio choice
  and Subtitle Off instead of falling through to channel selectors, resolve
  pinned subtitles against the selected media version, and fail closed on
  ambiguous subtitle metadata. EPG zoom also keeps the temporal viewport
  center instead of jumping to the start, and disappearing movies report a
  localized error in every supported culture.
- Classic-scheduled randomized playouts no longer freeze on the same order
  after every `Reset`: the operation now regenerates the playout seed,
  matching ErsatzTV's `ResetPlayout()`. `Refresh` now rejects the old
  end-of-horizon schedule state and undated Continue collection anchors,
  rebuilding only from a dated checkpoint aligned with the requested window
  when one exists.
- The Classic engine's shuffle seed arithmetic no longer silently collapses a
  negative seed to a constant `0`, and "Shuffle Schedule Items" reshuffling on
  a cycle wraparound now avoids repeating the same schedule item across the
  reshuffle boundary, matching the equivalent protection Classic collection
  shuffles already had.
- `Random Start Point` now only affects `Chronological`, `Season, Episode`,
  and `Shuffle In Order` schedule items, matching ErsatzTV. Previously it was
  applied to every playback order, including ones that are already
  randomized by construction.
- Classic scheduling's `Shuffle`, `Shuffle In Order`, `Random`, and `Random
  Rotation` playback orders are no longer aliases of each other. `Shuffle` is
  now a plain flat shuffle; `Shuffle In Order` (and the `Multi-Episode
  Shuffle` placeholder) keep the group-distance spread that previously
  applied to `Shuffle` too, now generalized to spread by show as well as
  artist; `Random` performs independent deterministic draws with replacement;
  and `Random Rotation` randomizes within show/artist groups while preserving
  group separation across cycle and persisted-build boundaries, including
  collections that repeat one media item with distinct audio or subtitle
  overrides. Rotation uses a direct singleton-group path and heap-backed
  grouped interleave so large movie libraries no longer trigger quadratic
  scheduling work.
- Browser-preview E2E fixtures now preserve the channel-number and integer
  schedule-kind database contracts, so the shared Playwright database cannot
  invalidate later channel-number and playout coverage.
- Manual CI runs on the default branch now lint the selected head commit
  instead of passing an empty range to commitlint, restoring the
  GitHub-hosted fallback when the self-hosted runner is unavailable.
- Hardened the EPG and browser-preview slice after correctness review. Preview
  and normal HLS workers now use distinct admission identities while sharing
  the same global capacity limit; guide labels use local time; aggregate
  selection matches live playback when playouts overlap; large title lookups
  are deduplicated and batched; and modal close, Stop Preview, and channel
  changes invalidate pending asynchronous responses. New HLS workers also
  withhold playlists until their session directory is prepared, preventing a
  restart from briefly publishing stale segments.
- Custom channel stream selectors are now read from one canonical folder,
  `<data>/scripts/channel-stream-selectors`, matching ErsatzTV's
  `ScriptsFolder/channel-stream-selectors`. Playback and playback
  troubleshooting previously looked one level higher than the channel editor
  listed them, so a selector that appeared in the channel dropdown was never
  applied. The folder is created at startup, an absent or unreadable folder is
  reported as "no selectors" instead of an error, and every file in it is
  offered (not only `.yml`/`.yaml`), so the channel editor and the
  troubleshooting page can no longer disagree. This fixes
  `/system/troubleshooting/playback` rendering only an error message on installs
  that never created the folder.
- The Add Smart Collection page no longer breaks admin-UI interactivity. It
  queried for collection id 0, and reporting that failure through the toast
  channel during server rendering left the client unable to hydrate, disabling
  every button and dialog on the page. Toasts are now client-only by
  construction, and `cargo xtask hydration-lint` gained a rule that fails the
  build when a server-side load reaches for the toast channel.
- Admin pages now report a failed initial load instead of quietly showing empty
  or default data. Media browsers and collection member lists render the shared
  error state; the Plex, Jellyfin, and Emby library and path editors show the
  error and disable Save so a failed load cannot overwrite real settings; and
  the collection and smart-collection editors surface it with the form disabled
  while still server-rendering the no-JS fallback.
- Preserved typed failures and consistency across extracted admin mutations.
  Collection media actions now validate referenced records and create a new
  collection with its first item in one transaction. Initial playout creation
  removes the unbuilt record when scheduler admission fails, automatic
  collection, schedule, settings, and playout rebuild failures are surfaced,
  and FFmpeg profile reads no longer mask decode or storage failures as
  missing profiles.
- Pull-request labels no longer trigger full CI runs. Hosted-runner fallback is
  now manual-only, preventing dependency automation from spawning duplicate
  workflows while applying routine labels.
- fMP4 HLS init-segment deduplication now selects the earliest generation
  deterministically instead of depending on filesystem directory order.
- Pull-request commit lint now runs the repository's pinned npm dependencies
  directly, so it no longer requires Docker on the self-hosted WSL runner.
- GitHub Actions and release builds now use the repository's supported Rust
  1.97.1 toolchain again, and Dependabot no longer proposes updates for the
  toolchain action whose tags can name Rust releases that do not exist.
- The global preferred audio language now drives built-in MPEG-TS and HLS
  stream selection when an item and channel do not override it. HLS Direct
  retains all source audio streams only when both the explicit audio language
  and title are blank, matching ErsatzTV.
- Playout rebuild confirmations now use the shared success-toast surface instead
  of rendering an unstyled message between the `/playouts` action bar and page
  content. Queued and active rebuilds remain visible through the row spinner.
- Local `dev`, `build`, and `serve` wrappers now build the scanner companion
  beside the web binary before startup, so scheduled automatic library
  refreshes no longer get skipped when only the Leptos web target existed.
- Playback and HLS data access now distinguishes valid missing metadata or
  configuration from database and decode failures. IPTV, HLS, capacity slates,
  and troubleshooting previews refuse to build altered FFmpeg commands or use
  raw paths, default capacity, or different audio, subtitle, deinterlace,
  tonemap, and frame-rate behavior after an operational failure; correlated
  logs and health reporting expose the cause.

### Security

- Fixed an unauthenticated arbitrary local file read in `/artwork/posters`,
  `/artwork/fanart`, and `/artwork/thumbnails`: a percent-encoded or
  double-slash absolute path in the URL replaced the intended local-cache
  base directory entirely (`PathBuf::join` overrides its base when the
  joined segment is absolute), letting any remote client read any file
  readable by the server process. Local artwork is now only ever served
  when the requested path exactly matches a `Artwork.Path` row the scanner
  already persisted, and the local-cache-hash lookup path now rejects any
  path component that isn't an ordinary filename.
- Preview and normal HLS segment routes now reject unsafe channel path
  components before resolving session files, preventing percent-decoded route
  captures from escaping their session-owned temporary directories when IPTV
  token enforcement is disabled or an unscoped token is used.

## [1.0.15-beta.9] - 2026-07-19

### Changed

- Renamed Similar Video Cooldown to Smart Shuffle and moved its opt-in policy
  from schedule/playout-wide settings onto each Classic schedule item, reusable
  Block item, Sequential YAML content entry, and Scripted content key. Legacy
  policies and anchor JSON remain readable and migrate without changing active
  schedules. Classic and Block editors now provide draft-aware per-source
  summaries plus searchable, tag-filtered, 50-row candidate details.
- Rebuilt the end-user documentation around Diátaxis with a first-channel
  tutorial; goal-oriented Windows, Docker, local-library, Jellyfin, IPTV,
  backup, content-variant, and troubleshooting guides; and current operator
  references for configuration, endpoints, navigation, media-source support,
  system requirements, release bundles, channel architecture, and the managed
  FFmpeg runtime. Contributor, parity, testing, and release-maintainer material
  now has a separate navigation map so it does not obscure the new-user path.
- Raised the required Rust toolchain from 1.85 to the latest stable release,
  Rust 1.97.1, across workspace manifests, local development, CI, release
  builds, and Docker builders.
- Enforced cargo-nextest for local Rust testing through cross-platform
  `run.sh` / `run.cmd` wrappers and Codex/Claude command guards. The closeout
  wrapper runs nextest before Cargo doctests, while CI retains its direct Cargo
  test selection.
- SQLite startup and lazy pools now enforce foreign keys, a 30-second busy
  timeout, WAL journaling, and NORMAL synchronization on every connection;
  the matching migration enables WAL for existing databases, and health
  reporting detects any runtime drift from those defaults.

### Fixed

- Smart Shuffle now isolates Classic policies and filter tags by schedule-item
  identity even when multiple items share one content source or reuse a filter
  rule ID for different searches, preserves explicit YAML and reusable Block
  item Off overrides during legacy-policy migration, hydrates tags from
  multiline Scripted Lua declarations, localizes overview warnings, and
  advances playout-scoped item spacing across intervening randomized-Off and
  chronological sources.
- Schedule item editing now explains that pre-, mid-, and post-roll filler
  applies to every media item selected by that schedule item. Regression
  coverage preserves ordered `Multiple (3)` then `One` schedules through
  save/reload and Classic Refresh rebuilds, protecting three-videos/one-bumper
  channel loops from collapsing into per-video post-roll filler.
- NVENC channels can now downscale HDR sources and composite alpha watermarks
  without feeding an `nv12` main surface into a `yuva420p` CUDA overlay. The
  pipeline tracks the real post-scale CUDA format and inserts the required
  `yuv420p` conversion before `overlay_cuda`, preventing HLS segmenter startup
  failures on affected Windows/NVIDIA systems.
- Unchanged Plex, Jellyfin, and Emby items in `RemoteOnly` state now honor the
  provider ETag short-circuit instead of refetching metadata and overwriting
  persisted media-file data on every scan. Content-variant classification still
  reapplies from provider page data, while `FileNotFound` and `Unavailable`
  items continue through a full rescan so they can recover.

## [1.0.15-beta.8] - 2026-07-17

### Fixed

- Windows beta archives now generate and verify the public Stylance bundle
  before packaging. They stage Leptos JavaScript/WASM under `site/pkg` only,
  avoiding duplicate public fonts and images under `site/`.

## [1.0.15-beta.7] - 2026-07-17

### Fixed

- Windows-only beta releases now select Git Bash before the Rust toolchain
  action, so its temporary scripts execute on the self-hosted Windows runner.

## [1.0.15-beta.6] - 2026-07-17

### Added

- French (France) admin UI localization (ADR 0021): the admin UI can now be
  used entirely in French. Selecting `français (France)` on `/settings/ui`
  persists `fr-fr` (aliases `fr`, `fr-FR`, `fr_FR`), and every admin page,
  dynamic label, and stable workflow/error message renders in French while
  technical identifiers (FFmpeg, Plex, codec names, routes, configuration
  keys) stay verbatim. This includes ADR 0019's Smart Shuffle presets, thresholds,
  rules, and tag-preview states. Covered by i18n coverage tests and the
  settings-i18n browser proof.
- Collection and playlist items can now persist item-level audio/subtitle
  playback overrides, so admins can add a movie with a commentary or alternate
  audio track and see the saved override on collection/playlist inspection.
- The same movie can now be added to one collection several times when each
  row carries a different playback override, so every commentary or alternate
  audio track of a movie can be represented in a single collection. Duplicate
  rows show their own override summary on the collection detail page, can be
  removed or reordered independently, and schedule separately with their own
  audio/subtitle preferences. (`CollectionItem` now uses a surrogate primary
  key in SQLite and MySQL; adding an item that exactly matches an existing
  row, including default adds without an override, remains a no-op.)
- Artist detail pages now support music-video multi-select: each listed music
  video (including alternate versions) has a checkbox, and the shared
  selection toolbar bulk-adds the selected videos to a collection or playlist.
  Per-video Add To Collection keeps working.
- Schedules can now opt into ADR 0019's deterministic Smart Shuffle
  policy across Classic, Block, Sequential YAML, and Scripted playouts. Shared
  count/duration thresholds diversify artist, show, season, parent variant,
  collection, tag, genre, studio, and operator-filter groups, persist through
  Continue builds, and relax impossible constraints without dead air; Off
  remains byte- and behavior-compatible with imported ErsatzTV schedules.
  Localized admin controls include presets, custom thresholds and rules, and a
  deterministic effective-tag preview. This feature was originally named
  Similar Video Cooldown.
- ADR 0020 proposes rebuilding impacted playouts when channels, schedules, or
  referenced library content change, so already-scheduled items pick up new
  settings (e.g. subtitle mode) instead of only being appended past — including
  porting ErsatzTV's item-clearing `Refresh` semantics to the classic engine.

### Changed

- Add To Collection and Add To Playlist now keep you on the current media
  browse, detail, or search page after a successful add instead of navigating
  to the target collection or playlist. The success notification includes an
  explicit, localized "View Collection" / "View Playlist" link for when you do
  want to jump there.
- Updated the locked Leptos package family to `leptos` 0.8.20.
- Updated the locked `leptos_router` dependency to 0.8.14.
- Updated the locked `jsonschema` dependency to 0.46.9.
- Updated the locked `open` dependency to 5.3.6.
- The channel editor's Subtitle Mode dropdown now shows helper text (all
  locales) clarifying that "Forced" only selects forced-narrative tracks and
  that "Any" is the mode to always show subtitles in the preferred language,
  reducing a common misconfiguration where sidecar subtitles never appear.
- Progress tracking now keeps reopened media-browser, ADR 0018 movie-detail, and
  Zero Duration health-check parity debts visible for follow-up work.
- Progress tracking now also records reopened channel-add default group, ADR
  0017 default FFmpeg profile bitrate, and FFmpeg profile editor spacing debts.
- Dutch admin UI now translates "watermark" as "zenderlogo" (station logo)
  instead of the literal "watermerk", matching common Dutch broadcast usage.
- Fresh-database default FFmpeg profiles now seed halved video bitrates
  (4000, 8000, 10000, 22500, and 30000 kbit/s for the 720p through 4K HDR
  profiles) while keeping the previous video buffer sizes, per ADR 0017 v3.
  Existing databases keep their saved profiles unchanged.

### Fixed

- Classic, YAML, and scripted playout `Refresh` builds now clear and rebuild the
  scheduled window instead of appending past stale items; classic refresh also
  preserves same-day checkpoint anchors so enumerator/shuffle continuity
  survives while per-item settings are re-stamped.
- Saving a channel now enqueues a playout `Refresh` for each channel playout
  when playback-impacting fields change, including subtitle/audio preferences,
  watermark, streaming mode, FFmpeg profile, and playout offset.
- Reset Playout and Reset All Playouts now rewrite near-term classic playout
  rows instead of only appending future items, so reset-applied playback
  settings are visible on the next scheduled item.
- Collection detail pages now hydrate cleanly after adding items from media
  pages and following the explicit "View Collection" toast action.
- Creating a channel at `/channels/add` now pre-fills the Group field with
  `ErsatzRS` again instead of `ErsatzTV`; existing channels keep their saved
  group values.
- The FFmpeg profile editor's Video and Audio sections now use the same
  vertical spacing between fields as the General section, in desktop and
  mobile layouts.
- Release Docker image builds now compile the native `ersatzrs-web` crate as an
  `rlib` only, avoiding the unused native web `cdylib` link that caused ARMv7
  QEMU aborts during multi-arch GHCR publication.
- Movie detail pages now render linked extras as TV-style action rows, allow
  those extras to be added to collections and playlists, and expose alternate
  movie editions plus commentary audio/subtitle playback options.
- Songs and images now use the shared media-browser action shell with the
  right-side selection toolbar, item actions, media-info/troubleshooting links,
  and correct playlist item types; artist and show detail pages now hydrate
  reliably with restored artist collection and preview-artwork behavior.
- The Zero Duration health check now ignores ordinary static image media while
  continuing to warn for zero-duration video/audio media and animated-capable
  image paths that still need duration metadata.

### Security

- Updated `quick-xml` to 0.41 to resolve RUSTSEC-2026-0194 (quadratic
  duplicate-attribute checking) and RUSTSEC-2026-0195 (unbounded
  namespace-declaration allocation), both denial-of-service vectors in XML
  parsing.

## [1.0.15-beta.4] - 2026-07-01

### Fixed

- Release Docker image builds now compile only the runtime binaries instead of
  the extra web library target, avoiding ARMv7 QEMU memory exhaustion while
  publishing the multi-arch GHCR image.

## [1.0.15-beta.3] - 2026-07-01

### Fixed

- Release Docker image builds now isolate Cargo target caches per target
  platform, avoiding concurrent multi-arch BuildKit cleanup races that could
  remove active Cargo fingerprints during GHCR publication.

## [1.0.15-beta.2] - 2026-06-30

### Fixed

- Release Docker image builds now reuse the prebuilt Leptos web-assets artifact
  instead of compiling `cargo-leptos` inside every target image, avoiding ARMv7
  QEMU out-of-memory failures during GHCR publication.

## [1.0.15-beta.1] - 2026-06-30

### Added

- In-app MPEG-TS troubleshooting for ADR 0013 at
  `/system/troubleshooting/mpeg`: the Support page can capture bounded
  channel MPEG-TS samples, run optional read-only TSDuck `tsp`/`tsbitrate`
  analysis, write a redacted report archive, and surface TSDuck availability
  as a non-blocking health row.
- A purpose-built `ersatzrs-healthcheck` binary now probes `/healthz` without
  curl or shell tooling, ships in release archives, and backs the Docker image
  `HEALTHCHECK`.
- Project-local Codex and Claude hooks now block maintainer-machine absolute
  paths, private media paths, and likely secrets before repository writes, and
  run `rustfmt` on a single edited Rust file after edit tools complete.
- Repo-local `hydration-islands` skill for Leptos WASM island and SSR HTML
  development, capturing the project-specific boundary rules, fallback
  expectations, and browser smoke gates that protect admin UI hydration.
- Content-variant search foundation for ADR 0018: `MediaItem` can now store
  primary/extra/alternate-version identity, subtype, and parent linkage, and
  smart-collection/search evaluation can query `content_kind`, `sub_kind`,
  `parent_title`, `audio_title`, `subtitle_title`, and `has_commentary` while
  defaulting existing searches to primary items.
- Content-variant UI surfacing for ADR 0018: media browse pages now always show
  special features / versions with localized badges, TV show/season specials
  render as episode-style rows, detail pages expose linked Extras / Special
  Features and Versions sections, and search/smart-collection editing shows the
  new token reference.
- Content-variant local scanner import for ADR 0018: movie/show library extras
  matching ErsatzTV's extra suffix and directory taxonomy are now imported as
  typed `OtherVideo` rows with normalized sub-kinds and parent linkage while
  remaining hidden from default search/scheduling behavior.
- Content-variant alternate-version linking for ADR 0018: local music-video,
  song, and movie scans now classify live, unplugged, acoustic, remix,
  remaster, extended, and director-cut renditions, keep deterministic canonical
  primaries, link same-work alternates through `ParentMediaItemId`, and leave
  orphan alternates typed but unlinked.
- Content-variant commentary render acceptance for ADR 0018: smart collections
  can feed commentary-bearing items into channels whose preferred audio title
  or custom stream selector pins commentary audio and matching subtitles in
  direct IPTV/MPEG-TS and HLS segmenter playback.
- Content-variant scheduling examples for ADR 0018: built-in templates and
  operator docs now cover unplugged rendition smart collections, commentary
  movie channels, and featurette pre-roll filler attached to primary movie
  schedule items without changing default schedules.
- Content-variant provider imports for ADR 0018: Plex, Jellyfin, and Emby movie
  library sync now classify server-reported extras and alternate versions into
  the same hidden-by-default search/scheduling fields used by local libraries.
- Release gate against the recurring "wasm checksum mismatch" that has broken
  several releases (`scripts/release/verify-web-bundle.sh`, wired into the
  release workflow). The Leptos SSR binary and the WASM bundle are built in
  separate jobs; if they disagree on cargo-leptos file hashing the server
  references `/pkg` asset URLs the bundle does not contain and the admin UI
  never hydrates. The gate boots the assembled bundle and fails the release if
  any hydration asset 404s. Hashing is now pinned off deterministically
  (`hash-files = false` in `crates/ersatzrs-web/Cargo.toml`, `LEPTOS_HASH_FILES`
  on both release jobs).
- Whole-app island hydration smoke (`scripts/e2e/hydration-smoke.sh` +
  `tests/e2e/island-hydration.spec.ts`): a browser-level pre-release check that
  boots the host and verifies every route rendering a Leptos `#[island]`
  actually hydrates (sets `window.__ersatzrs_hydrated`, no marker-mismatch
  panics). Documents the build-consistency requirement in BUILD.md.
- CI now runs `cargo xtask hydration-lint -p ersatzrs-web` and the full
  Playwright admin E2E suite by default, booting `ersatzrs-web` against an
  isolated SQLite database with matched Leptos SSR/WASM assets.

### Changed

- ErsatzRS is now a beta-stage project, and the release line publishes Linux
  and Windows archives plus GHCR Docker images while still intentionally
  omitting macOS archives for now.
- Startup database URL resolution now flows through the startup configuration
  helper, preferring `ErsatzRS_DATABASE__URL` / `ERSATZRS_DATABASE__URL` while
  preserving the legacy Docker `DATABASE_URL` override and default SQLite path.
- Infra health checks are split into registry submodules under
  `crates/ersatzrs-infra/src/health/`, with a completeness test covering the
  registered title set and order.

### Fixed

- Existing app-data MPEG-TS wrapper defaults that were seeded with `MuxRate` but
  without `PcrPeriod` now upgrade on startup, so direct `.ts` playback uses the
  same 20 ms PCR cadence as the current bundled script after a restart.
- HLS Direct MPEG-TS output now transcodes TS-incompatible copy sources such as
  WebM/Opus to the channel profile instead of emitting private/data streams,
  and normalizes hardware-only H.264 presets when falling back to software x264.
- Default `.ts` requests for MPEG-TS hybrid channels now use the continuous
  MPEG-TS path unless the hybrid wrapper is requested explicitly, avoiding
  HLS-wrapper segment resets in VLC.
- HLS segmenter startup now waits for a stable media-segment floor and serves
  normalized MPEG-TS playlists without ordinary item-boundary discontinuity
  tags, so VLC no longer turns item changes into PCR resets or rebuffer stalls.
- HLS Segmenter MPEG-TS appends now advance from the worker's successful append
  duration instead of a lagging PCR probe, preventing overlapping 4-second
  timestamp ranges that made VLC, Kodi, and MPC-HC freeze until pause/resume.
- The startup FFmpeg runtime downloaded/updated toast now appears once per
  browser tab for a specific runtime event instead of reappearing on every admin
  page switch.
- Foreground MPEG-TS viewers now reclaim stale HLS transcode slots before
  emitting the all-streams-in-use slate, preventing a recently closed HLS
  session from injecting a slate into direct channel playback.
- MPEG-TS troubleshooting reports now include a per-second stability meter and
  persist the same samples as `stability.json`, making the 20-second TSDuck
  analysis usable for spotting empty captures, continuity failures, and PCR
  warnings without reading raw logs first.
- Multi-collection editor hydration now keeps the save button DOM stable while
  preserving the ErsatzTV-style action bar icon in both hydrated and no-JS
  fallback rendering.
- Pending admin hydration gaps for Plex/Jellyfin/Emby source pages, Movies,
  Trash, Trakt lists, Filler Presets, and Rerun Collections now have durable
  seeded Playwright coverage; the affected islands resolve initial SSR data
  before hydration and provider path replacement tables stay within mobile
  layouts.
- CI Playwright E2E now runs on its own port, and `ersatzrs-web` honors the
  configured Leptos bind address so the self-hosted runner can coexist with a
  live maintainer admin server on the default `8409` port.
- Schedule item HTML fallback saves now parse repeated raw row fields before
  reconstructing schedule items, so no-JS `/api/schedules/items/save` submits
  preserve multiple rows and their filler, watermark, graphics, language, and
  playback fields instead of rejecting repeated `item_index` values.
- Admin-triggered local library scans now keep retrying SQLite busy/locked
  contention at each import/link/finalization write, so a long Music Videos
  scan can finish ADR 0018 alternate-version classification instead of replaying
  the whole scan and leaving documented smart-collection selectors empty.
- Classic Duration schedule items with count-based post-roll filler now advance
  through the filler before scheduling the next primary item, preventing
  overlapping playout rows in generated schedules.
- Classic schedules with dynamic programming before a future fixed block now
  start at the dynamic item, so channels have active playout rows before the
  fixed anchor instead of going offline until that block.
- Classic shuffled music-video scheduling now carries artist IDs through
  reference-data loading and spaces repeated artists when possible, preventing
  same-artist videos from clustering in MTV-style blocks.
- Admin route-family Playwright coverage now has stable hydrated startup state:
  library scan actions expose queued/failure status, media collection editors
  keep their SSR data through hydration, and scheduling editors load their
  initial data before the island boundary instead of panicking on marker-node
  mismatches.
- FFmpeg profile editing now serves its no-JS fallback from already-resolved
  SSR data and keeps `<noscript>` free of hydration resources, so the H.264
  profile select remains available without client JavaScript.
- The Playwright E2E wrapper now runs serially by default unless
  `ERSATZRS_E2E_WORKERS` or an explicit `--workers` argument is supplied,
  preventing shared-database route specs from racing in the default CI suite.
- Docker runtime images now include the admin UI public assets, Leptos
  hydration bundle, and runtime assets so the UI renders and hydrates from the
  image instead of starting with missing `/public` and `/pkg` files.
- Automatic maintenance health runs now reuse the managed FFmpeg paths resolved
  at startup instead of probing bare `ffmpeg` / `ffprobe` from `PATH`.
- Docker containers no longer crash during startup after the NVIDIA native
  hardware probe unloads CUDA/NVENC driver libraries.
- Docker runtime image builds now cache Rust dependency compilation behind a
  manifest/lockfile layer, so source-only edits rebuild workspace crates
  without recompiling the full third-party dependency graph while preserving
  the `ersatzrs-web` and `ersatzrs-healthcheck` runtime layout.
- The `/channels` Edit Numbers action now opens a real `/channels/numbers`
  editor with localized desktop/mobile actions, SSR/no-JS save fallback,
  validation, and atomic channel-number swaps that preserve `SortNumber`.
- Release publishing now restores GHCR Docker images for the beta line while
  keeping the macOS archive omission in place for the active release line.
- Managed FFmpeg startup on Windows now copies downloaded first-party
  ffmpeg/ffprobe files next to `ErsatzRS.exe` and persists those paths so
  `/settings/ffmpeg` shows the acquired runtime instead of a generic PATH tool.
- Fresh databases now seed the first channel name and group as `ErsatzRS`.
- Library scan clicks now queue in FIFO order and run one library scan at a
  time instead of racing concurrent scans or dropping already-locked requests.
- The hydrated Add Schedule Item action on `/schedules/:id/items` now opens a
  new editable row reliably.
- Remote stream placeholders with zero duration no longer trigger Zero Duration
  warnings on `/system/health`.
- MPEG-TS playback timing is stable across direct `.ts`, legacy concat, hybrid
  wrapper, HLS-direct, and MPEG-TS HLS segmenter paths: FFmpeg outputs now share
  profile-derived `-muxrate` / 20 ms PCR-period settings, bundled wrapper
  scripts render the same PCR period, and the HLS segment normalizer preserves
  continuity-counter rules for PCR-only adaptation packets.
- Windows launcher builds made from Linux/WSL now embed the tray icon resources
  correctly, preventing `ErsatzRS.exe` from failing at startup with
  `Error setting icon from resource: 1812`.
- Database schema health reporting now distinguishes real downgrade detection
  from dirty migrations and migration checksum mismatches, so `/system/health`
  no longer labels every schema metadata issue as an ErsatzTV downgrade.
- Admin island hydration is release-green again: the whole-app smoke now passes
  all 56 no-fixture island routes, and browser spot-checks prove `/channels`
  Delete plus `/playouts` row-select/refresh controls attach without marker
  mismatches.
- CI hydration smoke installs Node tools from a committed `package-lock.json`
  so `npm ci` no longer fails on clean runners.
- Linux-hosted clippy now accepts the Windows launcher build script target
  detection without weakening the repository-wide environment-access lint.

## [1.0.14-alpha] - 2026-06-21

### Changed

- Release archives now intentionally skip macOS targets until a future
  macOS-targeted release, preserving GitHub-hosted runner credits while the
  active release line builds Linux and Windows archives on self-hosted runners.

### Fixed

- Windows upgrades from older bundled-FFmpeg archives now clear stale stored
  `ffmpeg.exe` / `ffprobe.exe` paths from the previous release folder when those
  files are no longer present, allowing managed `ErsatzRS-ffmpeg` startup
  acquisition to run instead of failing before download.

## [1.0.13-alpha] - 2026-06-21

### Added

- ErsatzRS now manages its FFmpeg 8.1 runtime at startup: explicit operator
  overrides still win, otherwise the app validates a managed
  `ErsatzRS-ffmpeg` install, downloads the matching first-party release when
  needed, auto-updates managed installs when a newer release exists, verifies
  checksums, runs drawvg/zscale smoke validation, and reports successful
  downloads or failed update attempts through `/system/health` and an admin
  toast.
- Calibration clip generation now includes a `CAL_SIZE_PRESET=cinema-1920x800`
  mode for synthetic scope-ratio proof geometry (`1920x800`, SAR `1:1`, DAR
  `12:5`) plus a live Scale and Pad proof script that boots ErsatzRS, streams
  the generated source through a 1080p FFmpeg profile, and verifies black
  top/bottom padding in the captured output.

### Changed

- Release archives and Docker images no longer package FFmpeg/ffprobe. The
  Windows launcher and Docker runtime now rely on the managed startup
  `ErsatzRS-ffmpeg` download/update path, while offline deployments can still
  use explicit FFmpeg/ffprobe path overrides or a pre-placed managed runtime.

### Fixed

- Over-capacity HLS and MPEG-TS viewers now receive a playable, looping
  "all streams in use" slate that does not consume a transcode slot and switches
  to real content once capacity frees, instead of immediately getting HTTP 503.
- HLS segmenter active transcode limits now enforce a hard ceiling instead of
  letting recently watched channels bypass the cap. Over-capacity HLS sessions
  reclaim work-ahead/cooling/stale watched transcodes first, otherwise wait on
  the slot-free all-streams-in-use slate; the FFmpeg settings copy now describes
  the hard limit.
- FFmpeg command-generation parity is complete: the NVENC encoder-only
  CUDA-filter gate is now live-proven against an FFmpeg 8.1 build with `cuda`
  and `h264_nvenc` but without `scale_cuda`, `yadif_cuda`, or `overlay_cuda`,
  confirming ErsatzRS keeps NVENC encode while demoting decode/filter work away
  from unavailable CUDA filters.
- FFmpeg profile scaling now preserves the selected `Scale and Pad` behavior for
  near-16:9 sources with odd fitted dimensions instead of rounding the fit up
  into a full-frame stretch; `Stretch` and `Crop` command generation now have
  dedicated regression snapshots alongside the NVENC scale-and-pad path.
- FFmpeg profile command generation now honors VAAPI `HardwareIfPossible`
  padding with `pad_vaapi`, and respects the profile `DeinterlaceVideo` flag
  so interlaced sources are only deinterlaced when the selected profile enables
  it.

## [1.0.12-alpha] - 2026-06-19

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

- The Windows (`win-x64`) release now bundles the first-party
  [`ErsatzRS-ffmpeg`](https://github.com/bartdeijkers/ErsatzRS-ffmpeg) **8.1.2**
  package (pinned to release `v1.0.0`) instead of a third-party Gyan build. This
  guarantees the `drawvg` filter (built with `--enable-cairo`) and the rest of
  the ErsatzRS-required capability set, and aligns the bundled runtime with
  ADR 0008 (first-party builds, no `--enable-nonfree`). The drawvg presence
  assertion is retained.
- New installations now seed **five** ready-to-use FFmpeg profiles instead of
  one — 720p, 1080p, 1080p HDR, 4K, and 4K HDR, all with AC-3 6-channel
  640 kbps audio — and apply the best detected hardware encoder to that fresh
  seed at startup with hardware-compatible presets. HLS segmenter output also
  adds `-max_delay 500000` for
  multichannel AC-3 to tighten A/V interleaving at segment boundaries.
- HDR-to-SDR software tone mapping now converts and tags the full BT.709 SDR
  color state instead of leaving BT.2020 primaries/matrix on the output. A new
  manual `scripts/e2e/hdr-tone-map-proof.sh` gate live-proves HDR10 PQ and HLG
  calibration streams against generated SDR references.
- A new manual `scripts/e2e/nvenc-filter-gate-proof.sh` gate codifies the
  remaining NVENC encoder-only proof prerequisite: FFmpeg must report
  `cuda` + `h264_nvenc` while omitting `scale_cuda`, `yadif_cuda`, and
  `overlay_cuda`.
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

[Unreleased]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.17-beta...HEAD
[1.0.17-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.9...v1.0.17-beta
[1.0.15-beta.9]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.8...v1.0.15-beta.9
[1.0.15-beta.8]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.7...v1.0.15-beta.8
[1.0.15-beta.7]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.6...v1.0.15-beta.7
[1.0.15-beta.6]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.15-beta.5...v1.0.15-beta.6
[1.0.15-beta.1]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.14-alpha...v1.0.15-beta.1
[1.0.14-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.13-alpha...v1.0.14-alpha
[1.0.13-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.12-alpha...v1.0.13-alpha
[1.0.12-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.11-alpha...v1.0.12-alpha
[1.0.11-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.10-alpha...v1.0.11-alpha
[1.0.10-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.9-alpha...v1.0.10-alpha
[0.0.9-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.8-alpha...v0.0.9-alpha
[0.0.8-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.7-alpha...v0.0.8-alpha
[0.0.7-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.6-alpha...v0.0.7-alpha
[0.0.6-alpha]: https://github.com/bartdeijkers/ErsatzRS/compare/v0.0.5-alpha...v0.0.6-alpha
[0.0.5-alpha]: https://github.com/bartdeijkers/ErsatzRS/releases/tag/v0.0.5-alpha
