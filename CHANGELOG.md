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

- Add permission-reviewed yt-dlp self-update: startup/daily jobs invoke the
  configured `yt-dlp -U`, defer while the executable is busy without interrupting
  playback, and show bounded version results and remediation in Add-ons/Health.
  Uncertain process cleanup after an abrupt crash requires explicit tool-scoped
  offline recovery after stopping all related processes; normal confirmed
  shutdown/restart recovers automatically. Recovery does not repair executable
  bytes; a broken executable may require manual repair. Add-on readiness warns
  when native updating failed and a newer release for the configured channel is
  confirmed, naming the program and installed/latest versions through generic
  localized text with manual-update guidance. Unknown latest versions do
  not count as stale. Replacement backup/rollback, quarantine and new manual
  update/history controls are withdrawn from scope.

- Enable opt-in Adaptive refresh in yt-dlp package 0.10.2 through a bounded,
  provider-neutral normalized-JSON probe. Matching source-bound validators avoid
  imports; changed samples enter the existing refresh workflow. Checks sample
  the first 50 entries, with a 12-second deadline and 1 MiB output caps;
  changes outside that window require a full refresh/reconciliation.

- Add advisory Health warnings for add-on-list disk usage, with editable
  thresholds of 25 GB per list and 250 GB combined by default. Zero disables
  each warning independently. Settings work with and without JavaScript;
  warnings count list-owned file sizes and uploaded artwork without changing
  download admission, deleting files or introducing retention policies.

- Acquire human-authored yt-dlp subtitles only for the configured global primary
  and fallback languages, without automatic captions. Caption artifacts use the
  existing bounded file worker and are bound to publication revisions; streaming
  maps their cues through the frozen cuts without downloading the video, while
  Download ahead uses yt-dlp's native processing with a package-local caption
  timing correction. Captions remain aligned across scheduled joins and cuts
  in MPEG-TS and transcoded HLS playback.

- Add generic list-level Stream, Download ahead, and new-videos-only delivery
  selection. New-videos-only shows its saved cutoff beside the selector;
  re-enabling it establishes a fresh cutoff while current published playback
  remains unchanged until publication. Download ahead honors optional
  SponsorBlock removal using native yt-dlp processing.

- Extend provider-neutral add-on-list option contracts with finite preparation
  requirements and an explicit access-check target while preserving existing
  schema bytes. Add native list/item editors with inherited values, explicit
  bulk preview/apply/undo and undo of first-time schema activation. Versioned
  item-operation contracts, a revision-fenced access worker, and bounded
  processed-file admission are in place. Cached files can be reused when a
  removed item reappears without taking ownership from another item.
  Enrichment checkpoints retain their saved option context across restarts;
  publication journals and retained revisions bind each item to its selected
  processed file without changing legacy snapshot digests.
  Published playback selects that file by list and provider ownership; saved
  options use playback v3 without silently falling back to legacy behavior.
  Scanning preserves the prepared duration, including milliseconds and items
  whose provider duration is missing. Preparation workers retain unfinished
  storage until process cleanup is confirmed, with a localized recovery health
  notice. The existing Prepare publication action queues required processing outside
  the publication lease, shows localized progress with and without JavaScript,
  and preserves pending attempts on repeated clicks. Managed add-on cleanup now
  checks process-group/job membership separately from leader exit before releasing
  preparation resources. AMM-M-01A streaming acceptance is complete;
  preserved full-download groundwork belongs to the unfinished AMM-M-01B.
  ADR 0087 supersedes the mandatory processed-video design with a streaming-first
  proof requiring uninterrupted SponsorBlock cuts. The existing preparation work
  is preserved for the planned generic list-level HTML select, independent of
  SponsorBlock and gated by add-on delivery support: Stream (default),
  Download ahead, or Download ahead only for new videos. The last choice applies
  only to videos first imported after enabling it; existing videos keep streaming.
  Enabled SponsorBlock must be honored in all three modes, including both
  streamed and downloaded videos in the new-videos-only mode.
  Shared timeline/input generation and immutable interval-plan storage are in
  place. Scanning uses the frozen edited duration, including milliseconds, and
  rejects conflicting or corrupt playback selections. The generic delivery-mode
  selector is not implemented yet. Publication-only
  playback selections no longer mark an unchanged saved draft as edited. A small,
  bounded initial startup delay is permitted to establish continuity reserve,
  targeting ten seconds with a twenty-second maximum, including source opening
  and reserve establishment. SponsorBlock cut continuity remains mandatory.
  Metadata-only interval jobs now use the ordinary host coordinator and existing
  durable queue, without file storage or the download worker. They preserve
  source/chapter context, share provider cooldowns and await child cleanup on
  shutdown; unresolved cleanup remains blocked across restart. Normal publication
  now admits and selects timing-bound interval metadata instead of requiring a
  processed file. Automatic refresh waits for this evidence without resetting
  failed attempts or cooldowns, and frozen playback selections no longer cause
  false draft-change holds. The official yt-dlp adapter now provides bounded
  metadata-only intervals without media downloads or an FFmpeg probe, matching
  Download ahead's default removal categories. The unpublished custom CDN Range
  adapter and resolve-only operation are replaced by yt-dlp-owned transfer.
  Native yt-dlp capabilities are preferred across the remaining add-on work.
  Retained sections are decoded to one edited-clock raw-video/PCM stream for
  the host's final encoder; the same producer supplies startup metadata and
  playback. Frozen publication intent, managed cancellation and bounded reserve
  remain host-owned. Native playback metadata keeps caption and format listings
  intact within a dedicated bounded envelope; incomplete audio codec labels do
  not override yt-dlp's selection before the actual media probe. The required
  real-video MPEG-TS/HLS gate passes under delay/jitter after removing repeated
  native NUT identifiers and resolving fractional video timestamp collisions
  with retained-only VFR. It checks decoded/player startup, cut continuity,
  source-content correspondence and concealed host failures.
  Both channel encoders consume owned retained inputs and restart the producer
  for hardware retries, preserving explicit audio choices and redacting
  private diagnostics. Retained playback now has a bounded paced MPEG-TS output
  reserve and completed-segment HLS startup gating, with shared-deadline graphics
  and watermark probing. Selected subtitles with an unmapped edited clock are
  explicitly refused. Bounded buffering and actual-host cancellation are
  verified for scheduled MPEG-TS/HLS; direct MP4 reserve support is not implemented.
  Streaming completion now explicitly requires the maintainer-selected real
  yt-dlp video to pass scheduled MPEG-TS and HLS SponsorBlock cut tests;
  synthetic playback proofs alone are insufficient.

- Add an opt-in automatic publication policy for scheduled add-on-list
  refreshes after the first explicit Publish. Only validated additions use
  the existing exact-revision publication lease; changes to current items,
  removals, conflicts, access restrictions, unavailable items, or failed
  preparation remain review drafts with localized Activity and Health notices.
  Manual Refresh, Reconcile, and Reset and reload still require review.

- Add non-destructive full-source reconciliation for add-on media lists. A
  complete validated scratch import previews additions, provider changes,
  reordering, reappearances, and missing items while retaining operator edits
  and the current publication. Missing items require a second complete
  observation at least 24 hours later or explicit confirmation before Publish
  removes them, and Publish atomically accepts the matching staged archive.

- Add transactionally checkpointed incremental discovery for add-on media
  lists. The host gives providers only an invocation-private archive, commits
  the archive and page together, preserves the last complete checkpoint across
  cancellation and restart, and exposes exact examined, skipped-known, and
  emitted counts. The official yt-dlp package uses this contract on Windows
  and POSIX without unsafe early stop, so unchanged playlists emit no duplicate
  items and reordered playlists cannot hide a later unseen item. A separate
  bounded playlist-header probe refreshes rich provider metadata even when the
  item delta is empty, while preserving operator overrides, conflicts, and
  richer fields omitted by flat extraction.

- Add localized fixed and opt-in adaptive refresh cadences for add-on media
  lists. One restart-safe coordinator persists due, retry, success-anchor,
  conditional-validator, and bounded source-change history state; unchanged
  probes avoid full imports, failures honor bounded backoff and Retry-After,
  and Health reports disabled or capability-lost adaptive configurations and
  terminal refresh failures without exposing provider diagnostics.

- Add durable non-mutating source tests, exact saved-draft and reset-impact
  previews, and digest-validated publication history for add-on media lists.
  Operators can compare retained revisions and restore an older snapshot through
  the normal publication lease as a new monotonic revision; revision-owned files
  remain pinned, list sorting is keyboard accessible, and destructive dialogs
  now contain and restore focus for hydrated and no-JavaScript workflows.

- Give Remote Stream search results canonical detail links and an SSR-first,
  localized detail page showing effective metadata, artwork, tracks, liveness,
  duration, availability, owning add-on lists and their published options,
  playback health, and applicable channel actions. Add-on-list-filtered search
  now indexes only memberships from the committed published snapshot.

- Add a durable typed-confirmation Reset and reload action for add-on media
  lists. It removes derived state before a first-generation full import while
  retaining approved configuration and the empty generated-playlist identity;
  list and item metadata stay visible and editable with hydrated/no-JavaScript
  parity, and absent episode numbering is non-blocking and shown as Unnumbered.

- Add a provider-neutral add-on-list lifecycle persistence foundation for
  versioned declarative options, explicit refresh cadence and publication
  policy, durable reset/reconcile and incremental checkpoint state, retained
  revisions, tombstones, bounded artifact ownership, and redacted tool-update
  history while preserving legacy add-on request contracts.

- Allow administrators to remove an uploaded offline add-on repository bundle
  from Sources without uninstalling package snapshots or deleting their
  settings and data. Checkout sources remain operator-owned and cannot be
  removed through ErsatzRS; both hydrated confirmation and no-JavaScript form
  submission are supported.

- Allow editable playlist items to be reordered with localized Move Up and
  Move Down submit buttons or JavaScript-enabled drag-and-drop. Both paths save
  the same persisted item indexes, while generated playlists remain read-only.

### Changed

- Dependencies updated: quick-xml 0.42, process-wrap 10 and jsonschema 0.52,
  plus minor and patch updates to uuid, mlua, flate2, base64, futures-util,
  minijinja, open and syn. NFO and XMLTV guide parsing adapt to quick-xml's
  string-based element names; Windows add-on job tracking keeps its spawn
  ordering under process-wrap 10.

- Scope this add-on lifecycle closeout's platform verification to Linux x64
  and Windows x64. ARM/macOS verification is deferred; declared platform
  support is unchanged.

- CI runs the Chromium and Firefox Playwright suites one after the other
  instead of side by side. Both self-hosted runners share one machine, so
  overlapping browser suites competed for its CPU and memory. A run takes
  roughly ten minutes longer; the second suite still runs when the first
  fails. Separate concurrent workflow runs can still overlap.
- CI no longer retests a commit it has already proved. A `Preflight` job asks
  the API whether the pushed SHA already has a successful CI run and skips the
  gate when it does, so a direct fast-forward merge to `main` stops rerunning
  the feature branch's green run on an identical tree. Pull requests and manual
  dispatches always run in full, and a PR merge commit is a new SHA, so neither
  is ever skipped. The release-candidate gate in `BUILD.md` now cites the run
  that executed the gate: a skipped push run is recorded as provenance, and the
  tag requires every gate job to report success rather than a green workflow
  alone.
- CI skips docs-only pull requests, matching the non-code `paths-ignore` that
  already applied to `main` pushes. This is safe only while the repository has
  no branch protection; if required status checks are ever enabled, docs-only
  PRs need a sentinel job reporting under the required names instead.

- Run the Playwright admin E2E suite as one CI job per browser project
  (`chromium`, `firefox`) instead of one job for both. The suite runs on a
  single worker because it mutates shared application state, so per-project jobs
  are the only available parallelism: each job boots its own temporary database
  and web host. The two projects are evenly balanced (7.3 and 7.7 minutes of a
  15.3-minute serial run), so the stage should drop from ~17.8 to roughly 10
  minutes once each job's ~2.4 minutes of build and boot is counted. Same tests,
  same projects, no coverage change.

- Run the CI test jobs through nextest with the same target selection as before
  (ADR 0090), under a new `ci` profile that keeps the no-retry,
  run-through-failures and slow-test-after-60s semantics and adds a JUnit
  report. Per-test durations are uploaded as a build artifact, so growth in CI
  test time is attributable to individual tests instead of one wall-clock
  number. Doctest coverage is unchanged: neither selection ran them, and
  `./run.sh test-closeout` remains the only path that adds them.

- Adapt Add-on Lists to the locked task layout with separate item review,
  settings, playback/access, activity and artwork pages. Native forms retain
  unsaved edits and selection across navigation, and task saves use coherent
  transactions with revision and metadata checks. Add 10/25/50/100/All paging,
  exact bulk selection, fixed adjacent-item controls and vertically stacked
  access settings on wide displays. Artwork uploads remain
  attached to unsaved edits, and publishing keeps newer input unsaved.
  Item review separates editable fields from saved-item tools, keeps First aired
  visible, makes numbered/unnumbered choices explicit, and groups optional
  metadata and replacement selection behind restrained disclosures. Shorter
  option guidance, contextual action labels and clearer light-theme field edges
  make the form easier to scan; existing dates and unsaved replacements are
  preserved through native navigation and Save. Publication readiness updates
  after background preparation without reloading the page or losing unsaved input.

- Move Settings to the bottom of the admin navigation drawer and collapse the
  expanded Support group into a single badged `Support` link beside it
  (ADR 0089). Health checks, Logs, and Troubleshooting now render as in-page
  route tabs, reusing the pattern Settings already uses for its seven pages, so
  the health warning/error badge stays visible no matter which drawer groups are
  expanded. Every `/system/*` URL is unchanged; Logs and Troubleshooting cost one
  extra click from the drawer, and the `/system/troubleshooting/*` detail pages
  carry the same tab strip.

- Defer the two preserved download-ahead browser acceptance tests to AMM-M-01B,
  where they must use its explicit delivery workflow; streaming acceptance and
  lower-level preparation tests remain active.

- Reopen the add-on-list atomic-publication contract after Windows dogfood
  showed that a failed post-scan directory rollback can leave an otherwise
  valid recovery journal permanently blocked. AMM-J now owns durable restart
  recovery, Windows/POSIX filesystem retry, localized operator actions, and
  item-scoped continuation for provider-restricted entries. Its integrated
  packaged Windows restart repetition remains part of the AMM-N closeout
  instead of blocking the focused recovery-UI milestone.

- Accept ADR 0085 and broaden the open Add-on Metadata Management roadmap
  through AMM-N: coordinate publication with every scan origin, make blocked
  and reset/reload workflows explicit, retain redacted causal diagnostics,
  preview source/change/downstream impact, compare/restore bounded revisions,
  reconcile without erasing edits, expose inline metadata and Remote Stream
  details, add fixed and opt-in cadence-aware adaptive refresh plus observable
  safe yt-dlp archive early stop, carry inherited/bulk
  browser-cookie/SponsorBlock/subtitle behavior into
  playback under artifact budgets, and run permission-reviewed, validated,
  rollback-safe yt-dlp maintenance at startup and daily.

- Refresh the dependency rollup, including `base64` 0.23 and
  `ed25519-dalek` 3.0 for signed add-on repositories, while preserving the
  lowercase boolean and `none` output of existing XMLTV and graphics templates
  across the MiniJinja 2.23 update.

### Fixed

- Preserve the Windows application-data locations needed by yt-dlp to find
  the selected browser profile, without forwarding unrelated environment
  values or handling cookie contents in ErsatzRS.

- Isolate package verification from inherited database settings and application
  state. Application logs now honor the existing application-data override,
  keeping disposable verification logs with the rest of its test data; normal
  default log locations are unchanged.

- Stop the `ersatzrs-web` build script from failing when a Cargo target
  directory is reused from a different checkout path. It read
  `CARGO_MANIFEST_DIR` through `env!`, which bakes the checkout path into the
  compiled build script; Cargo reuses that binary after the target directory
  moves, so a CI runner whose work folder was relocated failed with
  `read admin language directory: NotFound`. The path is now read when the build
  script runs.

- Run the `ersatzrs-web` integration tests on the manual GitHub-hosted CI
  fallback. Its `web-bins` shard ran only `--bins`, leaving the crate's
  `tests/*.rs` targets unrun on that path while the self-hosted job covered
  them through `--tests`.

- Stop an add-on list from getting stuck in an endless publication recovery.
  The publication verification scan rewrites its own scanner sidecars inside the
  managed folder it is about to verify, so the sealed revision fingerprint no
  longer matched itself and recovery retried the same failure forever, leaving
  the list permanently "Publish blocked". The fingerprint now covers published
  content only, ignoring scanner state files and empty generated folders;
  intact revisions sealed by earlier versions are verified and upgraded before
  the scan rewrites their sidecars.

- Keep a list in `Publishing` once its publication has selected the revision it
  verifies, including after retry exhaustion until its recovery journal is
  finalized. An ordinary edit landing between verification retries could
  overwrite that state and leave recovery unable to converge. A publication that
  never reached verification still releases the draft when its lease expires.

- Give publication recovery a retry budget. A permanent failure now ends as a
  terminal, operator-actionable state with its recovery actions available,
  instead of retrying on a timer indefinitely.

- Keep the add-on preparation worker alive while the host monitors its exit;
  release its liveness pipe only on shutdown so queued preparation can run.

- Add-on list publication verifies artwork in its owning media folder, including
  nested playlist folders, while retaining managed-folder and content-hash checks.
- Saved add-on lists with provider-supplied person ordering or thumbnails no
  longer appear edited just by opening the metadata editor and hide publication
  preparation. Unchanged people fields retain that metadata in both hydrated
  and HTML-only saves.

- Use the retained prepared file for add-on-list items in HLS-backed Hybrid
  playback, matching native TS. Missing or invalid published artifacts fail
  safely instead of invoking raw provider playback.

- Keep unchanged generated playlist sidecars byte-stable during publication
  verification, so a timestamp-only rewrite cannot strand a sealed publication
  in recovery. Normal refreshes still update their synchronization time.

- Render the add-on-list index and Remote Stream details without JavaScript,
  including native sorting and channel links, instead of leaving a loading
  placeholder while their content remains in hidden streamed fragments.
  Keep JavaScript-only revision-restore buttons hidden when scripting is off.

- Use familiar `S01E01` episode labels throughout the admin UI in every culture,
  preserving localized Unnumbered labels, season-zero specials, and media filenames.

- Repair add-on-list CI coverage for refresh controls, source-probe fixture
  isolation, save/reload synchronization, supported option schemas, and retained
  checkpoint invalidation without changing the intended runtime behavior.
  Isolate reset lifecycle jobs, identities, and directories between browser
  scenarios so queued work cannot mutate the next fixture, and preserve the
  expected draft revision after tombstone confirmation in storage fixtures.

- Retain a bounded, ordered, redacted activity history for each add-on-list
  publication and recovery job. The localized editor, Health, and read-only
  runtime audit now distinguish original failures from later recovery failures;
  hydrated Copy and no-JavaScript Download actions expose only typed phases,
  counts, outcomes, and sanitized operating-system error details.

- Show durable add-on-list publication phase, attempts, last progress, and
  redacted failure codes in administration and health, with generation-fenced
  Retry recovery and Restore previous revision actions for hydrated and
  no-JavaScript use. Draft settings and item edits now lock only while a live
  publication lease is executing; expired work is shown as recovery-required
  instead of indefinitely Publishing.

- Add the `media-list.list.v6` typed availability reason contract. Official
  providers can classify authentication and content restrictions without
  exposing cookies or raw provider diagnostics; restricted entries remain in
  review, preserve the current published snapshot through refresh, and are
  excluded item-by-item at the next explicit publication unless a valid Local
  replacement is selected.

- Keep add-on-list Save and Publish actions disabled until item edits have
  refreshed publication readiness, preventing a fast follow-up publication
  from submitting the previous saved revision.

- Run exact-revision add-on-list publication and recovery as durable background
  work behind one shared managed-library lease. Publication now survives
  request cancellation and process restart, deduplicates repeated Publish
  requests and provider refreshes, transactionally fences expired workers,
  waits for startup replay before admitting dependent refreshes, cancels scans
  that lose their heartbeat, preserves concurrent provider changes as a later
  draft, and adopts stranded pre-upgrade publication journals for recovery.

- Make add-on-list filesystem publication resumable across process exits and
  Windows sharing violations with revision-owned integrity markers, immutable
  byte-copy backups, bounded retry, pre-upgrade terminal-state adoption,
  pre-sealed full-tree fingerprints, a post-swap Local verification scan,
  exact logical projection/cardinality checks, active-publication writer
  fencing, and journal-last terminal cleanup after filesystem and SQLite
  convergence.

### Removed

- Remove the remaining hard-coded Trakt assumptions from the core playlist and
  add-on-list admin pages. Provider-specific list discovery and parsing remain
  add-on-owned, with an architecture guard preventing them from leaking back
  into these host pages.

## [1.0.31-beta] - 2026-08-30

### Added

- Add named advanced-search drafts to the smart-collection composer. Queries
  can be saved, recalled later, modified, and updated without creating a smart
  collection or triggering collection and playout refresh side effects.

- Add a repository-local Context7 documentation skill that targets current,
  version-specific external contracts, keeps retrieval to the smallest useful
  query set, and avoids calls when local repository evidence is authoritative.

### Changed

- Separate media discovery from durable smart-collection authoring. Global
  search now stays results-first with a compact common-filter disclosure and
  exact query handoff; the smart-collection route uses localized
  Match all/Match any/Exclude any cards, focused nested-group editing, a live
  result count, and the same canonical query for hydrated and no-JavaScript
  saves. Search result counts and bulk actions now reflow as distinct,
  higher-contrast controls, and the mobile navigation cannot obscure the
  search or composer after a desktop-to-mobile transition. See ADR 0083.

- Move Remote Stream playlist filesystem health inspection onto Tokio's
  blocking pool and persist validated effective liveness in version 4 playlist
  sidecars. Health verifies that metadata against the generated-definition hash
  instead of synchronously YAML-parsing every durationless entry.

- Skip a Remote Stream whose playback behavior or duration is unknown instead
  of refusing the whole playout build, matching ErsatzTV's `PlayoutBuilder`,
  which drops zero-duration items from a collection and keeps scheduling. A
  single add-on episode that resolved without a duration previously left its
  channel with no playout items and nothing to stream. Dropped items are now
  reported as playout build warnings.

### Fixed

- Recover severely under-delivering live HLS hardware output by trying one
  software H.264 restart before lowering source quality. Temporary software
  decisions are isolated by channel and the complete FFmpeg profile, verified
  720p decisions can still escalate, and startup jitter retains a 20-second
  observation floor. See ADR 0084.

- Recover live add-on HLS playback that cannot sustain 1080p by requesting one
  provider-neutral 720p rendition fallback, verifying it against playlist
  delivery, remembering it for 24 hours, and reporting the temporary quality
  reduction through localized health guidance. See ADR 0084.

- Keep the Windows beta web-bundle gate independent of runner-owned FFmpeg
  drift. Auto-discovered overrides are now accepted only when they report
  FFmpeg major version 9; otherwise the gate uses the normal managed
  ErsatzRS-ffmpeg acquisition path instead of failing on an older PATH tool.

- Preserve canonical search semantics across structured edits, including
  multiword text, wildcard, phrase, and fuzzy values; reject incomplete queries
  during initial projection and durable saves; wait for the current composer
  model before saving; keep the editor locked until projection completes; and
  preserve the exact `/search` return URL across no-JavaScript save failures.
  Composer validation is localized in every supported admin culture.

- Validate managed Remote Stream definitions through the production parser
  before treating them as duration-exempt live entries in health, preventing an
  invalid definition with no playback URL from producing a passing result.

- Preserve an existing Classic playout when Remote Stream filtering leaves a
  required schedule source empty. Reset and Refresh now refuse the destructive
  rebuild instead of clearing the materialized timeline without replacement.

- Detect live HLS segment under-runs even when every received MPEG-TS packet is
  structurally valid, expose recent events through health, and make the manual
  TSDuck gate compare PCR media time with wall time without misclassifying
  variable-rate HLS from packet count and nominal bitrate.

- Stop health checks from reporting durationless live Remote Stream playlist
  entries, excluded add-on detail failures, and unpublished television
  projections as actionable warnings.

- Play sidecar subtitles whose filename contains an apostrophe, comma or
  semicolon. Those characters reached the ffmpeg `subtitles` filter unescaped,
  where an apostrophe opened an unterminated quote that swallowed the rest of
  the filter graph — including the `fontsdir` drive-colon escape behind it — and
  killed the channel with "No option name near ..." at the moment the affected
  episode started. See ADR 0082.

- Keep Classic playouts refreshable after the first build. Checkpoints are now
  captured wholly at the 100-nanosecond tick precision used by persisted
  timestamp columns, including nested Smart Shuffle release times and duration
  boundaries. A checkpoint therefore preserves both its scheduler continuation
  time and cooldown selection after a database round trip. Previously a
  sub-tick seam could either refuse every Refresh after a Reset or resume a
  duration cooldown early and change the future media order.

## [1.0.29-beta] - 2026-08-28

### Changed

- Rework desktop Settings around one global navigation entry, localized route
  tabs, responsive section cards, and a standardized sticky save action. Logs
  now uses the full content width with a compact search toolbar, accessible
  sorting, and the shared localized pagination controls.

- Replace public HTTPS add-on catalogs, signing keys, and automatic refreshes
  with operator-managed local Git checkouts and complete offline repository
  ZIPs. Add-ons are still installed and updated individually as immutable
  snapshots behind the off-by-default Unknown Sources gate; existing catalog
  installations remain runnable as legacy unmanaged packages and can be
  explicitly adopted by a local source without losing settings or data.

- Standardize `/media/*` pagination with a built-in localized `Rows per page`
  option list, range and page status, accessible previous/next actions,
  44-pixel targets, responsive wrapping, and SSR-safe link navigation.

- Require a completed, successful CI run for the exact release-candidate
  commit before creating any alpha, beta, or normal release tag.

### Fixed

- Keep add-on detail pages hydrated while they render the shared Settings
  route tabs inside the island boundary.

## [1.0.28-beta] - 2026-08-27

### Changed

- Rework add-on-list review around the collection: publication readiness now
  uses a non-sticky secondary rail on wide screens and returns to normal flow
  before the review workspace becomes cramped.

- Reject Windows beta publication when the packaged server and hydration
  assets fail the structural web-bundle consistency gate.

### Fixed

- Keep large add-on-list validation sets usable by grouping repeated blockers
  in the publication-readiness rail, showing the blocking reason on affected
  rows, opening episode numbering directly, and refreshing collection metadata
  when overview import becomes ready without overwriting operator edits.

- Keep the Proposal B publication-readiness flow within the add-on-list review
  workspace at intermediate viewport widths.

## [1.0.27-beta] - 2026-08-26

### Added

- Add repository-scoped admin UX design and audit skills with a Tunarr-inspired
  semantic HTML/CSS visual contract, WCAG 2.2 and responsive review, deceptive-
  pattern checks, and an evidence-backed UI component catalog.

- Add undoable direct and bulk add-on-list review controls, cursor-based
  episode numbering, date-gap season detection, and responsive bulk media-kind
  and playback-behavior editing.

### Changed

- Made every canonical repository skill discoverable through the current
  `.agents/skills/<name>/SKILL.md` layout while retaining the existing Codex and
  Claude compatibility pointers, with cross-platform mirror validation to
  prevent skill contracts from drifting.

### Fixed

- Keep the add-on-list publication-readiness heading in normal document flow
  with quieter typography while preserving its information, contrast, and
  accessible section relationship.

- Make add-on-list import, review, and publication self-explanatory with
  automatic Ready materialization, authoritative saved-draft and blocker state,
  faithful no-JavaScript create/save/review/publish forms, actionable readiness,
  explicit per-row save/error feedback, keyboard-safe mobile navigation, and a
  compact responsive review layout proven from 320 CSS pixels through 4K.

- Queue playback-required detail enrichment before publishing finite add-on
  streams with missing durations, clarify item-inclusion controls, and keep
  provider-distinct rows stable while the review table updates.

## [1.0.26-beta] - 2026-08-25

### Added

- Accepted ADR 0077 and added a typed FFmpeg 9 `drawtext` filter/pipeline leg
  beside drawvg, with inline or reloadable file text, explicit font resolution,
  layout and styling controls, cross-platform escaping, z-ordered composition,
  hardware-frame bridging, runtime capability enforcement, and first-party
  package gates for the complete text-shaping stack. Text graphics now expose a
  localized drawvg/drawtext choice while preserving drawvg for existing JSON;
  the bounded minijinja controller atomically refreshes live UTF-8 text in the
  same FFmpeg process, is wired through IPTV/HLS/retry/preview cleanup, reports
  invalid configurations through health, and has hydrated plus no-JavaScript
  editor coverage.

- Added focused repository skills for fast Rust iteration, read-only runtime
  incident analysis, stateful Refresh continuity, SQL cardinality/idempotency,
  durable background work, and Windows dogfood acceptance. New cross-platform
  `affected-checks` and read-only SQLite `runtime-audit` developer commands
  turn changed paths and runtime state into bounded, redacted evidence without
  executing broad gates or mutating an operator database.

### Changed

- Accepted ADR 0080 and standardized Runtime, Cargo, migrations, current
  documentation, repository skills, agent checklists, developer scripts,
  affected-check recommendations, and CI on SQLite as the sole supported
  database backend. Removed MySQL/MariaDB URLs now fail with migration guidance.
  The local closeout exercises one workspace graph plus doctests. The
  maintainer confirmed that the retired optional backend had no user
  deployments, so no operator conversion procedure or release blocker is
  required.

- Reopened the Classic Refresh and add-on television projection parity
  contracts after Windows dogfood exposed two release-correctness defects:
  scan-triggered Refresh can rewind a shuffled Classic playout when no dated
  checkpoint exists, and duplicate thumbnail projection can multiply episode
  rows in the television browser. Corrective milestones now require safe
  existing-database handling, scoped scan invalidation, idempotent artwork,
  and SQLite, MySQL, browser, restart, and health proof.

### Fixed

- Official yt-dlp 0.7.1 and Beeld & Geluid 0.9.2 add-ons now accept the full
  direct-video, playlist, shared-list, dated-search, series, and nested-episode
  URL matrix on Windows and POSIX. Direct yt-dlp objects become one-item lists
  with their finite/live behavior intact, provider query parameters and order
  survive aggregate pagination, and invalid output cannot become a successful
  empty draft. Existing failed or empty drafts expose a localized retry that
  replaces only the incomplete generation while preserving operator state;
  health links directly to that recovery path.

- Beeld & Geluid add-on 0.9.1 now preserves every series, shared-list, and
  saved-search query parameter while replacing only pagination; merges HTML
  cards with embedded provider JSON by stable episode identity; and lets
  explicit `isPlayable=false` evidence override optimistic card defaults. The
  exact eight-item Strip- en cartoontekenen overview now retains “Les 7” as
  `unavailable` / `not_playable` without a detail request on Windows and POSIX,
  so a Local replacement can be selected and published. Reimport also repairs
  affected seven-item drafts without duplicates or provider-order drift.

- Made canonical skill-mirror validation handle both real Git symlinks on
  Linux and flattened pointer files in Windows checkouts, eliminating a
  platform-specific `xtask` CI failure.

- Restored the scoped `cargo-deny` exception for the RSA timing advisory that
  is bundled by `jsonwebtoken`'s RustCrypto provider. ErsatzRS IPTV tokens are
  restricted to HS256 shared secrets and never execute the affected RSA
  private-key operation; the corrected policy replaces the obsolete MySQL
  rationale and keeps dependency CI enforceable.

- Add-on television publication now canonicalizes repeated artwork candidates,
  reads one deterministic thumbnail per projected item, repairs only safe exact
  duplicates in existing SQLite/MySQL databases, and reports aggregate
  projection-cardinality problems through localized health diagnostics.

- Remote Stream membership scans now refresh only Classic playouts whose
  current schedules can consume the changed media or managed playlist. Stable
  membership deltas produce one reasoned, coalesced refresh intent per playout;
  the intent commits atomically with playlist reconciliation, survives restart
  before queue admission, and exposes failed or stale work through health.

- Classic Refresh now restores complete, production-generated day checkpoints
  across process restarts instead of rewinding shuffled media. Checkpoints
  preserve schedule, source, fill-group, Smart Shuffle, Random, and Random
  Rotation state; membership changes reconcile by stable playback identity,
  and legacy continuation-only databases refuse the destructive rebuild with
  an actionable Reset/health diagnostic while leaving the playout window
  untouched.

- Toast lifecycles now use measured elapsed time, so delayed browser timer
  callbacks cannot stretch the configured display and fade durations after a
  hover or touch pause.

### Removed

- Removed the retired MySQL/MariaDB migration tree, live MySQL CI job,
  MySQL-specific check graph, and the obsolete RSA advisory exception carried
  only for SQLx MySQL authentication.

## [1.0.25-beta] - 2026-08-23

### Changed

- Add-on Lists now preserve provider-neutral unknown/finite/live playback
  behavior through review, publication, scheduling, IPTV, HLS, health, and
  yt-dlp. Remote Stream remains the collection type for both live and on-demand
  media, while unsupported or unknown live behavior is refused safely.

- Add-on list imports now publish a verified paginated overview before optional
  item-detail requests and resume both discovery and enrichment from durable
  checkpoints. The fair-use scanner serializes automatic and manual requests
  per provider host using a randomized global range configurable under Add-ons
  (2–16 seconds by default), without shortening an existing cooldown. Lists can
  stay overview-only, every row can fetch details manually, and the editor shows
  durable progress, item number, first-air date, season/episode columns, and
  exact accessible bulk-numbering previews. Beeld & Geluid and yt-dlp share the
  provider-neutral workflow on POSIX and Windows.

## [1.0.24-beta] - 2026-08-22

### Added

- Add-on list imports now open as a reviewable two-step Import/Publish draft
  using one localized metadata editor for Beeld & Geluid, yt-dlp, and future
  providers. The shared `media-list.list.v4` document covers list and item
  titles, dates and numbering, descriptions, genres, large tag sets, credits,
  languages, GUIDs, and role-aware poster/fanart/thumbnail candidates. The
  editor supports per-item inclusion, a filterable review count, inline
  accordion editing, movie primary/extra roles independent of source order,
  original-order playlist creation, and pagination across every collection
  type before explicit publication. The official yt-dlp package keeps the
  frequently updated `yt-dlp` executable in charge of all provider extraction;
  its shared POSIX/PowerShell mapper only translates the complete JSON result.

- The standalone add-on validator can now validate captured media-list NDJSON
  through the exact runtime conformance parser, allowing provider adapters to
  reject invalid metadata types and formats before publication.

- Television add-on drafts can now assign one season and consecutive episode
  numbers to all missing entries in original provider order, optionally in
  bounded batches for multi-season lists. Existing numbering is preserved by
  default, occupied numbers are skipped, and an explicit override remains
  available alongside per-episode editing and pagination.

- New built-in music-video-credits preset `_MusicFactory_ArtistTitle`, a
  "Music Factory" (TMF) styled artist/song-title card alongside the MTV-styled
  `_default.ass`. It renders two equal-width stacked bars bottom-left — a black
  bar with the gold artist name flush on top of a gold bar with the black song
  title — faded in and out over the same 9s–16s window; the TMF brand gold is
  `#EFB832`. The "MUSIC FACTORY" wordmark and TMF channel bug are intended as
  separate always-on image graphics/watermark overlays rather than baked into
  this per-song card.

- Music-video-credits templates can opt into real `minijinja` rendering with a
  leading `{# ersatzrs:minijinja #}` marker, so a template's whole layout —
  sizes, margins and positions derived from the output resolution — lives in
  the template instead of a per-preset Rust renderer (ADR 0076). The
  `_MusicFactory_ArtistTitle` preset uses this, and a render error falls back to
  the built-in default card. Existing token/interval presets are unchanged.

### Changed

- Re-running a Windows beta tag now replaces its existing release archive
  after rebuilding and validating it, while first-time beta tags continue to
  create a prerelease normally.

- Windows release packaging now tests startup registration through a small
  platform adapter instead of compiling the complete web test harness in a
  second Cargo profile. The release still performs the isolated real-HKCU
  registry round trip on its Windows runner and exact source revision, and the
  packaging gate verifies the ZIP contains a non-empty `ErsatzRS.exe` startup
  launcher at the package root.

- Scheduled an additive FFmpeg `drawtext` overlay leg after add-on metadata
  closeout and before Phase 9 teletext. ADR 0077 keeps `drawvg` as the default
  static/vector-rich backend while defining reloadable text files for live
  clocks, countdowns, and changing now/next or video-jukebox presentation.

### Fixed

- Existing add-on lists can now be reclassified after import to any supported
  library type. Save and Publish replay the durable provider snapshot instead
  of invoking the add-on again, while preserving source identity, original
  order, inclusion choices, and operator metadata edits; the list detail page
  shows both the current library type and any pending type change.

- Publishing a reviewed add-on list now bypasses a fresh empty draft cache, so
  the staged item selection is materialized into its managed library on the
  first publish; existing published lists with that stale empty state repair
  themselves on their next scan. Remote Stream health also understands v3
  sidecars and reports successful synchronizations with no active entries.
  Shared artwork downloads now identify ErsatzRS to providers that reject
  anonymous HTTP clients, preventing Beeld & Geluid episode stills from
  failing publication with HTTP 403. Reviewed NFO generation also retains
  list-wide tags and metadata defaults alongside item metadata.

- Replaying a reviewed publication snapshot now treats a legacy nullable extra
  artwork list as empty across both SQLite and MySQL instead of failing the
  publication during row decoding.

- Make the Windows `test-closeout` wrapper run both the default and
  all-features nextest matrices, matching Linux and preserving the
  SQLite-only integration coverage hidden when MySQL is enabled.

- Make the structural web-bundle and Playwright verifiers use native
  drive-letter paths when Git Bash launches Windows processes, and force their
  temporary hosts to loopback, so the hydration gates can open their temporary
  SQLite databases without triggering a Windows Firewall prompt.

- Keep Windows browser-gate artwork and runtime caches inside the throwaway
  test root by explicitly overriding both ErsatzRS state directories; Windows
  known-folder resolution otherwise ignores an isolated `HOME`.

- Generate a managed list-level `tvshow.nfo` or `artist.nfo` from the
  add-on's title and description before applying an automatic library
  presentation, including before the first library scan. An empty untracked
  placeholder is healed instead of being reported as malformed XML, while real
  user-authored NFO files keep their ownership protection.

- Project-local Codex hooks now invoke the real Windows Python installation
  through repository-relative paths, avoiding repeated `PostToolUse hook
  (failed)` reports caused by the inaccessible Windows Store `python3` alias
  and Git's sandbox-user ownership check.

- Workspace test binaries and the native `ersatzrs-web` dev package omit debug
  information, preventing the Windows web test harness from exceeding MSVC's
  program-database limit and its RLIB from exceeding the 4-GiB archive limit.

- Bind the runtime smoke host exclusively to loopback, avoiding an interactive
  Windows Firewall prompt for its ephemeral debug executable.

- Enforce the selected FFmpeg profile's scaling behavior, output frame, and
  audio channel count for managed Remote Stream items whose media properties
  are unavailable until FFmpeg opens the add-on stream, instead of silently
  preserving the source geometry or channel layout.

- Decode escaped Beeld & Geluid descriptions consistently in the official
  Windows add-on package, including embedded quotation marks and newlines,
  instead of rejecting otherwise valid series metadata as malformed JSON.

### Removed

- Dropped the unread `pin_id` field from the Plex sign-in island's polling
  state; the poll loop already used its own cloned copy.

### Security

- Updated `h2` to 0.4.16 for
  [RUSTSEC-2026-0258](https://rustsec.org/advisories/RUSTSEC-2026-0258): the
  version reached through `hyper`/`axum` accepted and queued empty HTTP/2 DATA
  frames without limit, so a client that did not drain its streams could drive
  unbounded memory use, or a panic on length overflow, against the admin UI and
  the IPTV/HLS endpoints. Low severity.

### Added

- Managed add-on-list Remote Stream entries now fetch their first provider
  image beside the generated definition. The bounded, manifest-host-validated
  download accepts only JPEG, PNG, or WebP; it keeps operator-replaced files,
  skips unchanged generated artwork on refresh, and records per-item failures
  without aborting the rest of a list.

- Add-on-list library projections now carry their NFO genres, tags, studios,
  cast, credits, identifiers, and artwork into the projected metadata rows for
  TV, Movie, Music Video, Other Video, Song, and Image presentations. Refreshes
  replace those facets instead of accumulating duplicate rows.

- Each add-on list entry now owns its media kind. The classification the add-ons
  already derived from provider metadata was stored but never read, so it was
  invisible and the list-wide value decided each entry's NFO root — a mixed list
  wrote every entry under one wrong root. The list-wide control remains as an
  explicit **Apply to all items** bulk default that confirms before overwriting.

- The add-on list collection type now offers every library kind — Auto, TV Shows,
  Movies, Music, Other Videos, Songs, Images, and Remote Streams — and every one
  of them is projected. **Auto** derives the presentation from the entries and
  only enters a projection every entry agrees on; a mixed list stays on Remote
  Streams rather than being guessed into a hierarchy.

- An add-on list can be projected as **Movies**, **Other Videos**, **Songs**,
  **Images**, or **Music Videos**. Music Videos gets one managed artist above the
  list, named by `artist.nfo`, because a music video cannot exist without an
  artist; switching away removes that artist too. For the parentless kinds each
  entry becomes a normal media item over the same Remote Stream version, with its
  metadata from its own NFO. Switching between presentations detaches the
  previous one, reverting detaches the projection instead of deleting the
  provider items, and a revert is refused while a projected item is still
  referenced by a collection, another playlist, or a playout.

- The add-on list editor groups entries by availability into counted
  **non-playable**, **unknown** and **playable** sections, each paged on its own
  (10/25/50/100/all) and each offering local replacement. Non-playable is
  expanded by default; unknown is kept separate because "could not determine"
  is not the same as "cannot play".

- Add-on packages can declare `default_source_hosts` (manifest schema v4): the
  hosts they offer to process, as a statement separate from the hosts they are
  permitted to contact. ErsatzRS uses it to select an add-on for an
  operator-supplied source URL automatically, preferring the most specific claim
  and asking the operator when two packages claim a host equally well.

- **Settings > Add-ons > Sources** gained a **Default remote stream processor**.
  It supplies the add-on for HTTP and HTTPS source URLs that no installed
  package claims, and Health Checks now report a configured default that is
  missing, disabled, or unable to play remote streams. Remote Stream definitions
  without `addon:` are still played directly by FFmpeg; the default does not
  change playback.

- The add-on of a saved add-on list can now be changed from the list editor.
  ErsatzRS enumerates through the newly selected package first, refuses a move
  that would collide with an existing list, and confirms with the operator that
  item metadata, replacements, and chapter corrections are derived again.

- Streaming add-ons can now supply bounded chapter markers through
  `remote-stream.item.v2`. ErsatzRS keeps provider and operator text separate,
  gives manual corrections precedence, offers **Restore provider chapters**,
  and derives stable provider-neutral fragment items from the existing
  enrichment call instead of invoking the add-on once per chaptered item.

- Health Checks now report an enabled add-on whose most recent readiness check
  did not succeed. The reported readiness state is persisted alongside its
  diagnostic code, so an add-on that stopped being usable after it was enabled —
  for example because a required external tool was removed — is reported instead
  of appearing healthy until playback fails.

- Add-on media lists can now be presented as a normal television show with
  Show/Season/Episode identities backed by the existing Remote Stream playback
  versions. Every list item has a dedicated metadata and chapter editor;
  descriptive edits are written to optimistic-concurrency-protected Kodi NFO
  files and then projected into the database. The list-wide chapters control
  has been removed, externally edited NFO files are preserved as user-owned,
  and failed television projections are reported by health diagnostics.

- The official Beeld & Geluid integration now recognizes personal lists,
  series and programme pages, individual episodes, and filtered Schatkamer
  searches through one normalized source grammar. It traverses every result
  page without a fixed page ceiling, preserves provider order, deduplicates by
  stable episode ID, imports editorial introductions and artwork, and reports
  incomplete synchronization clearly. Individual episodes accept validated
  YouTube-style chapter markers: the full episode remains available while each
  chapter becomes a structured, independently selectable Remote Stream fragment
  with exact `start` and optional `end` bounds. The new v3 add-on list contracts
  carry fragment parentage, duration, and additional artwork without changing
  the frozen v1/v2 wire shapes.
- Storage-backed add-on lists can now use v2 availability and content-kind
  contracts. Their rich metadata is written to deterministic Kodi-style `.nfo`
  sidecars beside operational Remote Stream YAML, and the scanner applies the
  same sidecar precedence as other Local media. Add-on Lists offers a per-link
  television/movie/music-video/other-video override and lets an explicitly
  unavailable provider item pin one exact Local media version as its fallback;
  unknown availability never triggers it, and provider recovery wins
  automatically. Interrupted YAML/NFO publication is detected through managed
  hashes and healed, while a provider item omitted from a later snapshot is
  retained as an unavailable tombstone so its replacement intent survives.
  An invalidated Local target is now shown explicitly as an invalid
  replacement in Add-on Lists instead of looking like an unconfigured item.
  Remote Stream cards and XMLTV now expose the descriptive content kind without
  changing Remote Stream playback identity. Provider classification rules,
  localized package resources, icons, and fixtures remain owned by each add-on
  rather than the host.
- Importing an add-on list now takes an optional storage folder name next to
  the URL. Leaving it empty keeps the current behaviour — the folder is derived
  from the list name the provider reports — while a supplied name is sanitized
  the same way and refused when another list of the same add-on already owns
  it. The check runs before the provider is invoked, so a rejected name costs
  no provider call and leaves no half-imported list behind.

### Changed

- **Settings > Add-ons > Sources** was rebuilt. Four blocks of equal weight —
  a risk toggle, a routing preference, an unlabelled upload row, and the
  repository list — became three labelled sections: **Unknown sources**,
  **Add-on repositories**, and **Install from file**. The repository list is now
  a table in the same house style as the media-source pages, and it finally
  shows what an operator opens this page for: how many add-ons each source
  offers and when its catalogue was last refreshed (both were already loaded and
  discarded). Actions that cannot run explain themselves on hover instead of
  being silently greyed out, the add/edit form moved into a dialog that names
  which flow you are in and keeps an existing repository's ID read-only, the
  delete confirmation names the repository instead of its ID, the file pickers
  show the chosen file in the UI language rather than the browser's own chrome,
  and the unknown-sources button is red only for the click that accepts risk.
  The **Default remote stream processor** moved to the **Installed add-ons**
  tab, with the add-ons it chooses between.

- Updating an add-on no longer takes it out of service. A package update landed
  disabled with its readiness result cleared, so a working add-on stopped
  serving until an operator noticed and ran **Check** and **Enable** by hand.
  The update now runs that readiness check itself and returns the add-on to
  service when it passes. It stays disabled, as before, when the check fails —
  and when the new version declares a network host, external command, media read
  access, or managed FFmpeg access the previous version did not, because
  granting a package new privileges stays the operator's decision. First
  installs still start disabled. See
  [ADR 0074](docs/adr/0074-keep-updated-add-ons-in-service.md).

- The Settings > Add-ons page dropped its four-card summary band. It spent the
  top of the viewport restating what the tab strip already names, so the counts
  moved into the tab titles: `Installed add-ons (2/3)` reads as enabled over
  installed, and `Updates` and `Sources` carry their own totals.


- The add-on list editor leads with the source URL and the add-on, followed by
  the fields the resolved add-on populates.

- Adding an add-on list now starts with the URL. The add-on selection moved
  below it and became optional: leave it on *Determine from the URL* and
  ErsatzRS fills it in from the URL host, visibly and overridably. The plain
  HTML form path resolves the same way and reports an unroutable URL with its
  own message instead of a generic failure.

- `plans/` now holds only work that still needs doing. Sixteen fully
  implemented plans were pruned once their substance lived in
  `docs/progress/`, and the two remaining blockers of the add-on NFO closeout
  plan are recorded as open parity debts instead of only in a plan header. A
  milestone that is still open must name a live plan; a completed milestone may
  omit the pointer, and `cargo xtask progress check` enforces both halves.
  Contributor-facing TSDuck install steps moved from the pruned MPEG-TS plan to
  [the troubleshooting how-to](docs/how-to/troubleshoot-startup-and-playback.md).

- The official yt-dlp add-on now requires an operator-installed JavaScript
  runtime (deno) alongside yt-dlp, and reports its absence as unavailable rather
  than failing later during playback. Without one, some providers return a media
  URL bound to a restricted player client that the managed FFmpeg runtime is
  refused when it fetches that URL. The add-on also prefers adaptive manifests
  over progressive renditions for the same reason. Both programs must be on the
  `PATH` of the account that runs ErsatzRS, which is not necessarily the same
  environment as an interactive login shell.

- Add-on package manifests, repository-package manifests, and generated
  repository catalogs can now be checked through one shared Rust validation
  path exposed by `cargo xtask validate-addon-manifest` and the thin
  `ersatzrs-addon-validator` binary. Context-independent catalog validation now
  lives in `ersatzrs-addon-contract`, while repository identity, rollback,
  current-time expiry, and signature checks remain host trust concerns.
- The official add-on repository now constructs deterministic packages, icons,
  and unsigned catalogs with a repository-owned Rust publisher. It invokes the
  pinned host validator instead of duplicating manifest rules, publishes from
  a rollback-safe staging tree, and keeps detached signing and deployment as
  separate downstream CI steps.
- Add-on media-list and Remote Stream list output now share one strict parsing
  policy: unknown fields reject the complete enumeration with a bounded error
  naming the field, while omitted and `null` optional arrays both mean absent.
  Existing capability versions are frozen; new fields require a capability
  version bump instead of speculative emission.
- The independently versioned `ersatzrs-addon-contract` crate now owns the
  media-list and Remote Stream NDJSON records, their wire enums, validation,
  and reusable stdout conformance checks. A runnable reference add-on ships as
  a Cargo example. Registry publication is disabled and guarded; the crate
  remains private to the ErsatzRS GitHub repository.
- Failed add-on `list` and `play` operations now carry a stable, bounded JSON
  diagnostic code on stderr. Host-detected missing settings and secrets use the
  same vocabulary, the admin UI localizes recognized codes in every supported
  culture, and unknown third-party codes retain a generic localized fallback.
  Provider-specific error prose and setting keys no longer live in the host;
  environment secrets are redacted and file-secret diagnostics stay suppressed.
- Availability vocabulary is now owned by each add-on package. The official
  yt-dlp package normalizes provider results to `available`, `unavailable`, or
  `unknown` on POSIX and Windows; the host rejects every other explicit value
  atomically with a bounded provider-neutral error instead of silently
  coercing it. Architecture lint prevents provider tokens from returning to
  host crates.

### Removed

- The official Trakt add-on and its native cross-platform build matrix have
  been retired. The public add-ons repository no longer needs credentials for
  the private ErsatzRS repository and publishes only Beeld & Geluid and yt-dlp.
  Provider-neutral Add-on Lists remain available, but Trakt list parity is
  deliberately dropped and the legacy built-in Trakt implementation is not
  restored.

### Fixed

- Installing an add-on failed on Windows with "installed package path escaped
  its root". The package content hash walked the staged package from its plain
  `C:\...` path but re-resolved every file with `std::fs::canonicalize`, which
  returns a `\\?\`-prefixed verbatim path on Windows, so each file looked like
  an escape from its own package root and no add-on could be installed. The
  add-on package paths now resolve through `dunce`, which keeps the ordinary
  path form Windows and `cmd.exe` batch entrypoints accept, and the hasher
  canonicalizes its root the same way it canonicalizes the files under it. The
  same defect made every install fail on macOS, where `/var` resolves to
  `/private/var`. The add-on media storage root also no longer persists a
  verbatim path. `dunce` keeps the compatible form only where Windows accepts
  it and falls back to the verbatim form per path, so every containment check
  now compares raw canonical paths, which cannot disagree on form, and only the
  paths handed to a child process stay on `dunce`. A rejected path is logged
  with the path and root that failed the check, because one message covered
  three different rejections and none of them said which path was at fault.
- `./run.sh test-closeout` ran only the `--all-features` configuration, which
  enables `mysql` and compiles away every `#![cfg(not(feature = "mysql"))]`
  integration test. That pass was not a stricter superset but a smaller set:
  322 tests never ran, and two had been failing on a fixture that seeded a
  `MediaStreamKind` of `0` where the enum is 1-based, leaving the generated
  playlist empty. The closeout now runs both configurations.
- The per-item add-on media list metadata editor is now covered by the admin
  route contract. Its route shipped with the television projection without a
  hydration entry, so the page had no recorded browser evidence and the route
  contract check failed; the entry is recorded as outstanding rather than
  claimed, and browser proof for the editor is still to come.
- MySQL databases can be migrated past the add-on television projection again.
  Its `AddonMediaListItemEpisode` foreign key was named with 68 characters,
  four over MySQL's 64-character identifier limit, so the migration aborted and
  every MySQL install stopped at that point. The name now carries the same
  EF-style truncation the rest of the MySQL schema uses; the SQLite spelling is
  unchanged.
- Choosing a Local replacement for an add-on list item now finds candidates.
  The search required a video stream recorded with a kind the scanner never
  writes, so it matched nothing in any library and the feature could not be
  used at all.
- Presenting an add-on list as a television show no longer fails with a
  database lock. The projection read before it wrote inside a deferred
  transaction, which SQLite refuses to upgrade — immediately, and regardless of
  the configured busy timeout — whenever another connection has written in the
  meantime. Renaming a projected show failed for the same reason.
- Every other write path is now protected against the same failure. All 98
  writing transactions across the repositories, the scanners, and the Plex,
  Jellyfin, and Emby persistence layers now take the write lock up front, so
  concurrent writers queue for the configured busy timeout instead of failing
  outright. This removes a class of intermittent `database is locked` errors
  that could surface anywhere in the admin UI while a library scan or another
  editor was writing. Genuinely read-only transactions deliberately keep the
  concurrent deferred begin, and a new `cargo xtask architecture-lint` rule
  rejects a newly added transaction that writes without it.
- Seasons projected from an add-on list are now listed on the show page.
  They were created without the metadata row every other season source writes,
  and the page requires it, so a projected show appeared to have no seasons or
  episodes at all. Existing projections repair themselves on the next
  synchronization.
- The Local replacement picker is no longer laid out as a bare inline block.
  It is now a dialog that names the item being replaced, sizes itself to the
  viewport instead of a fixed form width, keeps its search box and buttons
  reachable while only the result list scrolls, and reports an empty result
  instead of showing nothing. Its row actions match the icon buttons used
  elsewhere in the table.
- Synchronizing a Remote Stream playlist now refreshes its panel. **Sync Now**,
  and removing an entry (which triggers a synchronization), previously left the
  last-synchronized time and item count stale until the page was reloaded.
- Failed add-on media list operations are now recorded in the log. Importing,
  matching, updating, or deleting a list previously surfaced only a generic HTTP
  failure line, so the bounded provider diagnostic naming the offending record
  never reached **Support > Logs**.
- Finite add-on streams now keep one HLS Segmenter producer alive for the
  complete play item. A bounded startup burst fills the normal work-ahead
  buffer before the producer continues at realtime speed, preventing
  keyframe-aligned source seeking from replaying video at the former 44-second
  process boundary. The TSDuck URL gate now also recognizes public HLS master
  playlists instead of falsely requiring media segments in the top-level
  variant list.
- Channels configured for Transport Stream Hybrid, and explicit `mode=ts`
  requests, now dispatch the modern HLS segmenter wrapper promised by the
  streaming parity contract instead of silently falling through to the legacy
  concat branch. This prevents the wrong pipeline from terminating at Remote
  Stream programme boundaries.
- Beeld & Geluid chapter input now requires the matching v3 list capability,
  preventing older add-on packages from exposing chapter titles that play the
  complete episode. Remote Stream playlist scans validate that every selected
  fragment retains its full episode and link all chapter parents only after
  the complete source-ordered batch has been materialized.
- GitHub-hosted Playwright fallback runs now reuse the FFmpeg 9 runtime that
  server startup acquires for their calibration-media fixtures, instead of
  requiring an unrelated `ffmpeg` executable on the runner's `PATH`.
- Cargo package metadata now points to the canonical ErsatzRS repository and
  declares the same Zlib license as the repository `LICENSE`, so packaged
  add-on contract metadata carries accurate source and licensing links.
- Operator-supplied Add-on List and Remote Stream playlist URLs are now
  restricted to absolute HTTP(S) URLs without embedded credentials. Before a
  provider call, the host also enforces the installed add-on's declared exact,
  suffix, or wildcard `network_hosts`; rejected saved lists remain visible
  with a localized failure instead of disappearing from administration. The
  add-on receives the same canonical URL serialization used for validation,
  preventing authority parser differences from bypassing the host restriction.
- Add-on list failures reached the operator wrapped in transport plumbing —
  "error running server function: conflict: …" — because the localized
  messages match the repository wording and nothing stripped the server
  function's own prefix, so every message dispatched through a server function
  fell through to the raw string. The prefix is now removed before matching,
  which also lands the storage-folder rejections (taken, or unusable as a
  folder name) as ordinary sentences in every supported culture.
- No add-on that declares managed FFmpeg could serve as a media-list provider.
  Resolving an add-on's environment requires both managed paths whenever its
  manifest declares `managed_ffmpeg`, but list enumeration was the one
  invocation path that passed an empty runtime context — so adding a Beeld &
  Geluid list failed with "managed FFmpeg path is unavailable" before a single
  request left the machine. List invocations now supply the managed FFmpeg and
  ffprobe paths, the way every other invocation path already did.
- Local Libraries rendered the storage root each add-on manages as a loose
  block of text above the library table, with its own Settings link, so
  add-on-owned storage read as something other than a media source. Those
  roots are now ordinary rows in the table, sorted in with the libraries and
  labelled "Add-on" as their media kind. Their edit action opens the owning
  add-on's settings page rather than the library editor, and their delete
  action stays visible but disabled, because the add-on — not the operator —
  owns the registration; add-on-managed list libraries now show the same
  disabled delete instead of no delete action at all.
- The Add-ons Updates view reported "No add-ons match these filters." when
  nothing was filtered at all, which reads like the view is broken rather than
  like good news. With no search term and no state filter it now says
  everything is up to date; with either applied it still reports the filter
  miss, because a filter can hide a pending update.
- Two rendering defects in the Settings > Add-ons manager. An "Add-on
  repositories" caption sat in the page header, where it labelled all four
  views instead of the repository list it describes; it now heads that list in
  the Sources view. And on an add-on's detail page the link to its public
  source-code URL had no styling of its own, so it inherited body text colour
  and read as a label rather than a link — directly below a "Source" fact
  naming the repository the add-on came from, leaving two different things
  wearing the same word. The link is now styled as one and reads "View source
  code", translated in every supported culture.
- Connecting to MySQL 8 failed outright with "RSA auth backend disabled".
  MySQL 8 defaults to the `caching_sha2_password` plugin, whose first
  password exchange on a connection needs either TLS or an RSA public-key
  exchange, and sqlx 0.9 moved its RSA backend behind an opt-in feature that
  the `mysql` build did not enable. The feature is now enabled, so a plaintext
  MySQL 8 connection authenticates instead of being refused at startup.
  MariaDB and MySQL 5.7, which default to `mysql_native_password`, were
  unaffected.
- Saving almost anything could silently delete related data. Repository writes
  used SQL `REPLACE INTO`, which is not an update: on both SQLite and MySQL it
  deletes the conflicting row before re-inserting it, and that delete fires
  `ON DELETE CASCADE`. Seventeen statements wrote to a table other rows cascade
  off, so editing an FFmpeg profile destroyed every channel using it (and their
  playouts), saving a channel destroyed its playout and artwork, and — worst —
  every library rescan erased the child rows of each media item it touched,
  because 21 tables cascade off `MediaItem`. All seventeen now use a real
  upsert, which never deletes the row. Statements still using `REPLACE INTO`
  write only to tables nothing cascades off.
- A Plex rescan deleted and recreated a media file row whenever its path hash
  already existed, taking the linked Plex file record with it. The write now
  updates the existing row in place.
- Saving a library discarded every item scanned into it and blanked its stored
  paths. The save deleted all of the library's path rows before re-inserting
  them with an empty path, and media items cascade off those rows. Library save
  now reconciles paths instead: a path that stays keeps its row, its stored
  path, and its scanned items, while a path genuinely removed from the library
  is still deleted along with its items.
- A new database migration did not run until the crate was rebuilt for some
  other reason. `sqlx::migrate!` embeds migrations at compile time but cannot
  register the directory as a build input, so adding a `.sql` file left the
  cached build — and its stale migration set — in place, and restarting the
  server changed nothing. A build script now tracks both migration directories.
- The official add-on repository could never refresh, reporting "the repository
  has no Ed25519 public key". Its production signing key is now pinned, so a
  fresh database verifies the signed catalog instead of failing every refresh.
- Four Italian admin UI error branches never fired, because the English text
  they match against had itself been translated. Overlapping block and deco
  reports, the pinned-stream-selection error, and the movie-not-found error
  now match the message the backend actually emits and render in Italian.

### Added

- Schema-v3 add-ons can now declare file-backed media-list storage. Enabling
  Beeld & Geluid or yt-dlp creates its profile-relative media root and managed
  local source; each imported list becomes a separately named Remote Streams
  library backed by the existing safe playlist synchronizer. Add-on Lists now
  appears under Media Sources immediately below Local, and configured storage
  paths remain portable across Linux, Windows, and macOS.
- Repository-installed add-ons can now expose bounded provider lists through
  `media-list.list.v1`. The new `/media/addon-lists` manager imports and
  refreshes those links, matches movies/shows/seasons/episodes to local media,
  supports `addon_list:<id>` search, and can maintain a playlist in provider
  order. The official Trakt add-on owns Trakt API access and configures its
  client ID only through an environment/file secret reference; Beeld & Geluid
  supports the same workflow for Schatkamer series, shared-list, and saved
  `/zoeken?...` links.

- The Beeld & Geluid streaming add-on can now synchronize public, user-made
  Schatkamer `/lijst/<uuid>` links as ordered Remote Stream playlists alongside
  series links. Duplicate programmes keep their first position, unavailable
  programmes are skipped with a bounded diagnostic, and the standalone POSIX
  and Windows implementations preserve the existing rich metadata projection.
- German for Germany (`de-de`) is now a complete, selectable admin UI language
  (ADR 0022). Every admin text key and every stable workflow error message is
  translated, terminology is unified on one German term per ErsatzTV concept
  (Sendeplan, Sendeplanelement, Playout, Block, Deko, Filler, Sammlung,
  Element), and product nouns, provider/codec/protocol identifiers, routes, and
  configuration keys stay verbatim. The settings-route browser case passes in
  Chromium against a live host with no English fallback leakage.
- Italian for Italy (`it-it`) is now a complete, selectable admin UI language
  (ADR 0025). Terminology is unified on one Italian term per ErsatzTV concept —
  Playout keeps the product noun so it no longer collides with Playback
  ("riproduzione"), Collection settles on "raccolta", Playlist on "playlist",
  and Watermark on "filigrana" — the remaining English passthrough messages are
  translated, and the settings-route browser case passes in Chromium against a
  live host with no English fallback leakage.
- Spanish for Latin America (`es-419`) and Spain (`es-es`) are now complete,
  selectable admin UI languages (ADRs 0028 and 0029). Playout keeps the product
  noun so it no longer collides with Playback ("reproducción"), the remaining
  English passthrough messages are translated, episode subtitles read
  "temporada X, episodio Y", and both settings-route browser cases pass in
  Chromium against a live host with no English fallback leakage. The regional
  split between the two ("vídeo"/"video", "ajustes"/"configuración") is
  preserved.
- A signed, repository-backed add-on platform now provides trusted-source
  replacement, immutable package install/update, manifest-rendered permissions,
  settings and panels, readiness checks, content-integrity health, automatic
  catalog refresh, dependency guards, and localized management under
  **Settings > Add-ons**. The separate official-repository source builds
  independent Beeld & Geluid and yt-dlp Remote Stream packages for all six
  release RIDs; publishing it and pinning its production public key remain
  release work.
- Remote Stream playlist manifests can now enumerate an upstream series through
  an installed add-on, synchronize stable generated definitions, report health
  and bounded troubleshooting, expose localized management and tag facets, and
  optionally maintain a read-only playlist in upstream order with affected
  playout refresh.
- The FFmpeg profile editor now offers the full encoder preset ladders FFmpeg 9
  accepts instead of the single `veryfast` value inherited from ErsatzTV:
  `ultrafast` through `veryslow` for software `libx264`/`libx265`, and the
  complete `veryfast` through `veryslow` target-usage ladder for QSV.
  `placebo` is deliberately not offered, since it cannot sustain realtime
  encoding at any resolution.
- NVENC profiles for HEVC and AV1 can now select FFmpeg 9's `uhq` ("ultra high
  quality") tune. H.264 NVENC does not advertise it, so the option is gated by
  codec.
- Dolby Vision profile 7 sources with a complete BL/EL/RPU record can now
  transcode from their HDR10-compatible base layer to SDR or HDR10 MPEG-TS and
  HLS output. The production path directly software-decodes only the exact
  characterized HEVC Main 10/yuv420p10le, compatibility-id-6 tuple;
  `dovi_split` remains a fixture oracle rather than an optional runtime
  dependency. Dolby Vision signalling is stripped, and incomplete or
  imprecisely identified sources remain rejected. Existing items whose
  base-layer state predates the additive SQLite/MySQL field require a rescan.
- Dolby Vision Profile 5 can now convert through two exact, capability-gated
  NVIDIA operations: SDR/H.264 and HDR10/HEVC. Both use the selected CUDA
  device, RPU-aware Vulkan/libplacebo processing, a software colour-contract
  conversion, and NVENC. ErsatzRS selects the path only after a positive
  functional proof for the current runtime identity; missing proof or a
  learned live failure rejects the source instead of spawning a guessed-colour
  fallback.
- Jellyfin, Emby, and Plex scans now retain rotation, mastering-display,
  MaxCLL/MaxFALL, and Dolby Vision configuration fields. Complete provider
  tuples round-trip to playback, while positive partial metadata is persisted
  as explicitly incomplete with a provider hint so rescans and restarts cannot
  silently downgrade known Dolby Vision to ordinary HEVC/PQ.

### Changed

- Trakt lists are no longer a built-in ErsatzRS feature (ADR 0067). Upgrading
  deliberately deletes legacy Trakt list/item/GUID rows, their generated
  system playlists and page-size preference, then drops all three Trakt tables;
  no existing Trakt data is imported into the provider-neutral add-on schema.

- The tag facets on the remote-streams browser no longer push the card grid off
  the screen. On a tag-heavy library the wrapped chip cloud grew without limit
  and filled most of the viewport before a single stream was visible. Tags now
  render through a shared facet rail: one fixed-height row of chips that
  scrolls sideways, with a CSS-only disclosure that expands the same chips into
  a height-capped cloud. The rail also shows how many tags exist, marks the tag
  currently filtered on, and offers an "All" chip to clear the filter — which
  previously required editing the URL. No hydration is involved, so it behaves
  identically in the `<noscript>` fallback. Facet links on the media detail
  pages (tags, genres, studios, languages, people) now share the same chip
  palette and focus ring, and pick up the light theme instead of a hard-coded
  dark-theme green.

- Add-on trust is now tiered rather than all-or-nothing (ADR 0066). Verified
  repositories keep detached Ed25519 catalog signatures; unverified HTTPS
  repositories and sideloaded add-on or repository ZIPs require one
  off-by-default Unknown Sources opt-in. The permanent trust tier is visible in
  the manager and health report, while the official source still fails closed
  without its pinned key. The localized Settings > Add-ons manager now provides
  Installed, Browse, Updates, and Sources views plus a dedicated detail page for
  every add-on, guarded lifecycle actions, source offers, permissions,
  configuration, readiness, and optional bounded package artwork. Updates and
  source ownership remain manual by design.
- Documentation now has explicit Diátaxis navigation, a complete reference
  index, evidence-backed current System Context and Container views, and
  separate add-on package, repository, runtime, trust, and operator guidance.
  Add-on pages distinguish the implemented development host from the still
  unpublished official repository and production key. Goal-oriented how-to
  guides now also cover building and refreshing a Classic playout, verifying
  local and Remote Stream ingestion, scheduling content variants, managing an
  add-on lifecycle, building manual collections from media results, keeping
  channel content updated with smart collections, and confirming IPTV
  client imports. Release how-to guides now separate normal multi-platform
  tags from Windows-only beta tags and use unmistakable version placeholders.
  Backup and troubleshooting procedures now also cover external databases and
  state, the documented systemd service home, IPTV JWT probes, and redacted
  diagnostic downloads; Jellyfin guidance reflects its Movies/TV-only,
  one-way synchronization contract.
- Remote Stream YAML now uses `addon:` identities for provider playback and
  playlist enumeration. The legacy raw `script:`, `extractor:`, and
  `settings.remote_stream_extractors` contracts are intentionally rejected so
  media files cannot bypass repository trust with arbitrary commands.
- The remaining FFmpeg multi-vendor hardware and Dolby Vision fixture/client
  evidence gaps are explicitly deferred beyond Phase 8 and no longer block
  Phase 9; their unresolved acceptance evidence remains tracked for later
  opportunistic closure.
- CI now executes the Output Dynamic Range health classification against an
  isolated MySQL 8 service instead of allowing that backend regression to skip
  when no external test URL is configured.
- Release guidance now distinguishes Windows-only beta artifacts from the
  normal Linux/Windows/GHCR release matrix, and the remaining pre-publication
  Windows candidate, workflow-hardening, and live-acceptance work is tracked in
  one redacted plan instead of an uncommitted beta test diary.
- FFmpeg settings now publish as one validated runtime generation. ErsatzRS
  stages binaries and graph probes before one transactional settings/cache
  commit, then atomically swaps the shared capability report and paired
  FFmpeg/ffprobe paths; a failed candidate leaves the previous runtime and side
  effects untouched. Cache reuse and learned denials are bound to both binary
  digests plus the selected CUDA device UUID/model and driver, and late child
  failures from an older generation cannot poison its replacement.
- NVIDIA native capability probing and generated CUDA/Vulkan graphs now bind to
  device index zero instead of unioning encoder/decoder support across all
  installed GPUs. Dolby Vision admission is also stricter: only characterized
  Profile 8.1 and Profile 7 tuples are accepted, and Dolby Vision stream copy
  remains rejected until a source/container/output/client matrix is proven.
- The FFmpeg profiles seeded into a **new** database are retuned for FFmpeg 9.
  NVENC seeds now use preset `p5` with tune `hq` instead of `p7` with `ll`, and
  QSV seeds use `medium` (target usage 4) instead of `veryfast` (target usage
  7). The old NVENC pair came from the legacy `llhq` compatibility mapping,
  which pairs the slowest preset with a low-latency tune that a 4-second-segment
  HLS pipeline never benefits from. Software seeds keep `veryfast`. Existing
  databases are not modified, and profiles migrated from `llhp`/`llhq` still
  keep their exact original values.

### Fixed

- Signed add-on catalog refresh now uses verified `ETag`/`Last-Modified`
  revalidation and compare-and-swap trust/catalog revisions, preventing stale
  refreshes, installs, configuration changes, or readiness checks from
  overwriting a newer repository or installation state.
- Add-on execution now gates managed FFmpeg paths on the declared permission,
  owns and terminates managed Unix process groups or Windows Job Objects,
  bounds probe output and drain time, strips unsafe Unix package write modes,
  redacts or suppresses secret-bearing diagnostics, and isolates retained data
  by repository ID.

- Beeld & Geluid playlist-generated episode definitions now retain plot,
  original broadcast timestamp, Kijkwijzer advice, genre and subject labels,
  collection, role-aware people, producer, original broadcasters, and current
  broadcasters. Remote Stream scans persist and index those facets, while
  XMLTV/Kodi output now includes the compatible date, categories, rating, and
  credits instead of dropping the metadata after enumeration.

- Channel mirror source and playout offset controls are now available only
  when the editor's Playout Source is Mirror; server-side normalization still
  discards forged mirror-only values from Generated channels.
- A runtime hardware-encoder demotion no longer reuses the hardware profile's
  encoder preset for the software encode. The QSV and `libx264` ladders share
  several names (`medium`, `slow`, `veryfast`) while meaning entirely different
  cost points, so a demoted QSV profile could hand `libx264` a far more
  expensive preset than the hardware encoder was using and lose realtime.
- Remote Stream producer failures now include the playback surface, channel,
  producer name, effective seek, failure class, exit status, stderr byte count,
  and a bounded diagnostic tail. URLs, credentials, cookies, authorization
  values, and local paths are sanitized before logging, so add-on failures
  are actionable without exposing signed media URLs or private configuration.

## [1.0.21-beta.1] - 2026-08-09

### Fixed

- Runtime-learned hardware graph and decode denials now merge through one
  mandatory process-wide FFmpeg report before the complete snapshot is
  persisted. Long-lived HLS and preview workers read that shared report before
  every FFmpeg command build, and immediate IPTV retries receive the complete
  merged denial set, so concurrent workers neither erase restart state nor
  carry another worker's already-rejected graph into a later spawn or retry.
- Finite Remote Stream producers now treat a bounded consumer closing its input
  as normal debug-level teardown while preserving warnings for producer
  failures and unrelated pipe errors.
- The optional Windows YouTube wrapper keeps successful `yt-dlp` extraction
  quiet while retaining source-side `{seek}` through the managed FFmpeg
  runtime, preventing temporary signed media URLs from reaching normal console
  or captured-log output.

## [1.0.21-beta] - 2026-08-08

### Added

- Added a **Source Orientation** health check. Rotated video — most phone
  recordings — carries its orientation as a container display matrix, and
  MPEG-TS has no field for one, so a channel that *copies* video delivers
  portrait content sideways. The check names the affected items and channels and
  points at the two real fixes: transcode the channel, or use the fMP4 HLS
  variant. Transcoding channels are unaffected, because FFmpeg autorotates and
  the rotation ends up in the pixels. ErsatzRS reports rather than silently
  forcing a transcode, since copy is an explicit choice with a large CPU cost
  attached. The measured behavior of every output format is documented in the
  troubleshooting guide.

- FFmpeg profiles set for HDR10 output can now carry their own **mastering
  display** and **MaxCLL/MaxFALL** values (ADR 0064). ErsatzRS has always had to
  signal something when an HDR10 encode has no source metadata to carry
  forward — converting HLG, or mapping SDR in — and until now that was a fixed
  documented constant. Leaving the fields blank keeps that constant, which the
  editor shows as the placeholder so the effective value is visible rather than
  implied. The override never displaces metadata a source carried itself: a
  graded source still wins, because it describes the display the content was
  actually mastered on. Values are validated at save time, so a typo is refused
  instead of failing every stream later, and the Output Dynamic Range health
  check reports profiles whose stored values are being ignored.

- Added an **Output Dynamic Range** choice to FFmpeg profiles (ADR 0064), so
  SDR and HDR10 output is now an explicit operator decision instead of being
  inferred from a profile name, codec, or bit depth. `SDR (BT.709)` keeps the
  existing behavior of tone-mapping HDR sources to BT.709. `HDR10 (BT.2020 PQ)`
  produces a stable BT.2020/PQ stream: PQ sources are passed through with their
  own mastering-display and MaxCLL/MaxFALL metadata intact, HLG sources are
  normalized to PQ, and SDR sources are mapped in at ITU-R BT.2408 diffuse white
  (203 cd/m²) without inventing highlight detail. The signalling is identical
  for every item, so a schedule that mixes SDR and HDR content does not change
  the stream's colour signalling at a programme boundary.

  Existing profiles are migrated to SDR and keep streaming exactly what they
  streamed before; only a freshly created database seeds its two `hdr` profiles
  as HDR10. HDR10 requires hevc or av1 at 10-bit and is rejected at save time
  rather than silently downgraded, a new **Output Dynamic Range** health check
  reports profiles that cannot produce what they promise, and the control is
  localized in every supported culture. Profiles that copy video are unaffected
  and say so.

  NVENC produces HDR10 too, keeping a PQ source entirely on the GPU. Backends
  without a characterized HDR10 graph — QSV, VAAPI, AMF, VideoToolbox, RKMPP,
  and V4L2M2M — encode HDR10 profiles in software instead of quietly emitting
  SDR, and the health check warns when that happens.

  Scanning now records each source's mastering-display and MaxCLL/MaxFALL
  values, so an HLG or SDR item converted to HDR10 carries the metadata its own
  grade implies rather than a generic default.

- Added uniform content-rating/Kijkwijzer, studio, music-label, language, genre, and
  tag metadata for movies, television, music videos, other videos, songs, and
  images across local/Plex scanning, SQLite/MySQL persistence, search and smart
  collections, Smart Shuffle grouping, and localized detail pages. Music
  videos, songs, and images now have read-only detail routes with linked facet
  searches and the established media actions.

- Added an opt-in **Settings** -> **UI** control that starts the packaged
  `ErsatzRS.exe` tray launcher when the current Windows user signs in. The UI
  reads the live per-user registry state, can repair a stale release path, and
  reports a missing launcher or inaccessible registration without persisting a
  misleading database flag. Linux remains service-manager owned and now has a
  systemd installation guide modeled on Jellyfin's package behavior.

- Added Dolby Vision input support. Profile 8.1 sources — the common case for
  streaming-service 4K releases — now play from their HDR10-compatible base
  layer to both SDR and HDR10 channels, verified against a real file. Profile 5
  and dual-layer profile 7 are rejected with a health entry naming the reason
  rather than played with visibly wrong colour, because neither has a
  characterized conversion. Scanning records each source's Dolby Vision
  configuration. Dolby Vision output is not supported.

### Changed

- Re-sequenced the long-term roadmap: FFmpeg 9 HDR/Dolby Vision, Vulkan, and
  pipeline hardening is Phase 8; teletext capability, persistence, delivery,
  management, and runtime proof is Phase 9; and the "The Box" video-jukebox
  extension moves to Phase 10. This changes planning contracts only, not
  current runtime behavior.

- Defined the future FFmpeg profile dynamic-range contract: transcoding will
  explicitly target stable BT.709 SDR or BT.2020/PQ HDR10 instead of inferring
  HDR from HEVC Main 10 or a profile name. The accepted plan also establishes
  source-derived HDR metadata, fixture-gated Dolby Vision input handling,
  operation-scoped Vulkan/libplacebo fallback, safe long-filtergraph transport,
  and benchmark gates for AMF, VAAPI, libplacebo, and possible Vulkan encoding.
  This is an architecture and delivery contract; the runtime implementation is
  still tracked as not started/partial in `docs/progress/`.

- FFmpeg 9.x is now the explicit runtime contract. Managed startup defaults to
  `ErsatzRS-ffmpeg` v2, rejects FFmpeg 8 and unverified future majors, and keeps
  downloaded paths updateable while preserving administrator-owned overrides.
  NVENC profiles now store and edit presets (`p1`-`p7`) separately from tunes
  (`hq`, `ll`, `ull`, `lossless`), with automatic `llhp`/`llhq` migration. The
  migration is verified to be bit-exact: a profile that used `llhq` or `llhp`
  before the upgrade produces pixel-identical output afterwards.
  One upstream behavior change comes with FFmpeg 9 and is intentionally not
  overridden: `h264_nvenc` now defaults to the **High** profile instead of
  **Main**. This affects only NVENC H.264 profiles that leave the video profile
  unset; set it explicitly on the FFmpeg profile if a client device requires
  Main.

- The nightly Clippy-exemption cleanup now batches individually verified fixes
  behind one push and self-tunes its batch cap from durable run trailers. Its
  fixed 40-minute budget now terminates each verification command using the
  remaining time, rolls back a timed-out target, and records all-skipped or
  pre-success time-box outcomes in an empty state commit so later runs reduce
  the cap instead of retrying the same workload unchanged.

### Removed

- Removed the unused `.empty-cell` CSS rule from the rerun collections admin
  page's module stylesheet (dead-code cleanup, no visible change).
- Removed the unused `.empty` CSS rule from the blocks admin page's module
  stylesheet (dead-code cleanup, no visible change).

### Fixed

- Fixed hardware-graph fallback state and unsupported Dolby Vision handling in
  live streams. A graph probe that ran but produced no classifiable result now
  remains non-selectable instead of being mistaken for an older, never-probed
  runtime; live failures are learned only from operation-specific evidence and
  are published to future HLS and IPTV sessions immediately. Hardware-decode
  failures now use the same process-wide publication path. Concurrent HLS and
  IPTV failures persist merged graph and decode denial snapshots, so one worker
  cannot erase another worker's learned fallback on restart. The one-retry
  limit no longer suppresses learning a failed CUDA fallback. Unsupported
  Dolby Vision HLS items now emit an error slate and advance instead of looping
  on the same item, and early HLS/IPTV refusal removes any rendered graphics.

- Fixed NVENC playback turning anamorphic sources into a solid-green frame on
  FFmpeg 9 when CUDA first shrank the video by its sample-aspect ratio and then
  scaled it to the channel. ErsatzRS now performs one CUDA scale directly to
  the computed square-pixel size, preserving the intended aspect ratio without
  the broken intermediate surface.

- Fixed late tune-in for script-backed Remote Streams by adding opt-in
  source-side seeking. Extractors that accept a standalone `{seek}` argument now
  receive the current playout offset instead of forcing FFmpeg to drain a
  non-seekable pipe from the start; unmarked scripts retain their existing
  behavior. The bundled Beeld & Geluid wrappers use this contract, so tuning
  into a long Schatkamer episode starts promptly even well after its beginning.
  A canonical Windows YouTube wrapper and YAML template now apply the same
  contract while keeping `yt-dlp` an optional manual install, preserving its
  request headers, and forcing downloads through ErsatzRS's managed FFmpeg.

- Fixed HDR content keeping its HDR metadata after being converted to SDR. The
  mastering-display and MaxCLL/MaxFALL values survived tone-mapping and were
  written onto the BT.709 output, so a display was told to expect a 1000-nit
  HDR grade for a picture that had already been converted away from it. This
  affected every HDR source and every encoder.

- Fixed portrait and other rotated recordings being scaled as though they were
  landscape. FFmpeg rotates such sources from their display matrix before any
  filter runs, so ErsatzRS now sizes them from their displayed shape — a
  portrait item letterboxes into a 1080p channel instead of filling it. Scanning
  records the rotation, stream copy is untouched, and no rotation filter is
  added, so the picture is never rotated twice.

- Fixed a graphics element pointing at an animated WebP or GIF taking its whole
  channel down. The loop option differs per demuxer, and the wrong one makes
  FFmpeg fail to open the input at all; it is now chosen from the file's own
  header.

- Fixed NVIDIA HDR playback selecting a Vulkan/libplacebo graph that cannot run.
  ErsatzRS chose that path whenever `ffmpeg -hwaccels` listed `vulkan`, which is
  not evidence that the graph works — on a WSL2 host with an RTX 3080 Ti the
  only Vulkan device is a software rasterizer, so every HDR NVENC transcode died
  with `Could not derive CUDA device`. A bounded functional probe now runs the
  real graph at capability refresh, classifies the failure, remembers the denial
  per FFmpeg binary and GPU, and falls back once to a proven path that still
  encodes on the GPU. Platforms without Vulkan stay healthy, and a new
  **Hardware Graph Capability** health row reports when a fallback is in effect.

  Two related runtime failures were fixed while proving the fallback: the
  previous "non-Vulkan" NVENC HDR chain still ran libplacebo and therefore still
  needed a Vulkan device, and downloading a 10-bit HDR CUDA surface as nv12 is
  rejected by FFmpeg.

- Removed the redundant **Apply** button from the Windows startup section in
  **Settings** -> **UI**. The existing **Save Settings** action now saves the
  theme, language, and Windows startup choice together.

- Fixed the programme guide on the channel watch route never scrolling. Its
  section is a plain grid item, so the page's implicit `auto` column sized
  itself to the guide's full zoomed timeline (~9700px) and left the guide's own
  scroller as wide as its content — the **Now**, **-2h**/**-1h** and
  **+2h**/**+4h** controls had nothing to move. The standalone EPG preview was
  already correct and is unchanged.

- Fixed FFmpeg 9 playback failures by replacing removed `-vsync` arguments with
  `-fps_mode`, translating legacy NVENC low-latency presets at the playback
  boundary, and validating the required output/NVENC options at startup. The
  shared fix also covers script-backed Remote Streams, including the Beeld &
  Geluid extractor.

- Fixed the uniform metadata-facet slice so new detail-page actions hydrate,
  existing local-TV seasons are forced through their first facet backfill,
  Plex movie and episode facets are persisted, Plex artwork uses its owning
  server and token, television episodes use field-local embedded fallbacks,
  and embedded-only scans retain `Embedded` source provenance.
- Fixed script-backed Remote Streams failing when a producer emitted a
  streamable MP4, WebM, or another FFmpeg-supported container instead of
  MPEG-TS. Playback and scanner probing now let FFmpeg detect the producer
  container from its bytes, matching ErsatzTV while preserving MPEG-TS script
  support.
- Fixed the cross-browser channel-watch E2E proof measuring EPG zoom geometry
  before the asynchronous preview failure UI had settled, which made slower CI
  runs report a false center-preservation regression.
- Fixed intermittent Linux `ETXTBSY` failures in FFmpeg capability tests by
  serializing every test that spawns a tool while another test may be writing
  a temporary fake executable.
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
- Fixed local artist scans creating a duplicate artist (with its own
  duplicated artwork) on rescans once an `artist.nfo` name diverged from the
  disk folder name or its disambiguation changed. Artist re-identification now
  matches on the freshly parsed title and disambiguation, and the trailing
  scan pass removes artists that no longer own any music videos, matching
  ErsatzTV's `GetArtistByMetadata` and `DeleteEmptyArtists` behavior.
- Fixed a remastered or upscaled music video (e.g. an "Official Music Video
  (Remastered)" reupload) being classified as an alternate version instead of
  the primary video. Local scanning treats `upscaled` as a `remaster` alias and
  prefers an available enhanced rendition as the canonical music video: a lone
  rendition is promoted outright, and one found alongside a plain/official
  original moves that original into the artist's Versions section. Missing
  enhanced renditions are excluded from canonical selection so a remaining
  playable original becomes primary again, while movie and song renditions
  retain their existing alternate-version behavior.
- Fixed burned-in text subtitle (SRT/ASS/WebVTT) lines silently disappearing
  partway through HLS Segmenter playback whenever a work-ahead chunk restart
  (roughly every 44 seconds) landed in the middle of a subtitle line already
  on screen. The per-chunk subtitle rebase now rewrites cue timestamps
  directly instead of an ffmpeg seek-and-copy pass, and clamps a straddling
  cue to keep showing instead of dropping it. HTTP(S), UTF-8/UTF-16,
  dot-separated SubRip, tab-separated WebVTT timing lines, padded ASS, and
  extensionless provider ASS inputs retain their prior FFmpeg compatibility,
  including a fallback for unsupported syntax. Remote fetch failures also
  redact credential-bearing subtitle URLs from logs (ADR 0061).

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

[Unreleased]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.28-beta...HEAD
[1.0.28-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.27-beta...v1.0.28-beta
[1.0.27-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.26-beta...v1.0.27-beta
[1.0.26-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.25-beta...v1.0.26-beta
[1.0.25-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.24-beta...v1.0.25-beta
[1.0.24-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.23-beta...v1.0.24-beta
[1.0.21-beta.1]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.21-beta...v1.0.21-beta.1
[1.0.21-beta]: https://github.com/bartdeijkers/ErsatzRS/compare/v1.0.17-beta...v1.0.21-beta
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
