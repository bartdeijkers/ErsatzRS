# Support Pages Parity Closeout

## Source Map

- ErsatzTV UI/pages: `$ERSATZTV_SRC/ErsatzTV/Pages/Index.razor`,
  `Logs.razor`, and `Pages/Troubleshooting/{Troubleshooting,YamlValidator,BlockPlayoutTroubleshooting,BlockPlayoutHistory,PlaybackTroubleshooting}.razor`.
- ErsatzTV components/view models: `LogEntryViewModel`,
  `PagedLogEntriesViewModel`, health `HealthCheckResult` /
  `HealthCheckSummary`, troubleshooting report/tab view models, sequential
  schedule validation models, block playout history rows, and playback
  troubleshooting option/context models.
- ErsatzTV validators: sequential schedule YAML/JSON schema validation,
  stream-selector YAML rule validation, media-item/channel playback
  troubleshooting input checks, and table state validation through MudTable
  page/sort/page-size state.
- ErsatzTV handlers/services: `GetAllHealthCheckResultsHandler`,
  `GetRecentLogEntriesHandler`, `HealthCheckService`, `SystemStartup`
  health-summary notifications, troubleshooting report/archive/sample
  handlers, sequential schedule validators, block playout history queries, and
  playback troubleshooting HLS/report generation.
- ErsatzTV repository/query paths: latest app-data log file read with
  search/sort/page-size paging, health rows filtered to exclude
  `NotApplicable`, persisted log page size via `ConfigElementKey.LogsPageSize`,
  playout warning counts, health warning/error counts, block playout history,
  media item/channel playback lookup, FFmpeg profiles/settings/channels, and
  hardware capability probes.
- Side effects: health summary badges update the Support nav group and Health
  Checks link; logs page saves rows-per-page as config; troubleshooting archive
  and sample endpoints create downloadable artifacts; playback troubleshooting
  starts `.troubleshooting` HLS transcodes and writes FFmpeg/report/log files.
- ErsatzRS target modules:
  `crates/ersatzrs-web/src/admin/pages/system/{health,logs,troubleshooting}.rs`,
  `crates/ersatzrs-web/src/admin/server_fns/{layout,logs,troubleshooting}.rs`,
  `crates/ersatzrs-web/src/api/troubleshooting.rs`,
  `crates/ersatzrs-web/src/admin/layout/{badges,nav}.rs`,
  `crates/ersatzrs-web/src/admin/languages/`, and
  `tests/e2e/support-pages.spec.ts`.
- Intentional deviations: ErsatzRS reports Rust/runtime-specific environment
  and FFmpeg 8.1 hardware capability data in troubleshooting reports; this is
  repo architecture parity rather than an ErsatzTV UI omission.

## Visual Source Map

- ErsatzTV layout containers: support pages render inside full-height
  `MudForm`, `MudContainer MaxWidth.ExtraLarge`, h5 headings, dividers, dense
  table surfaces, and support nav group badges in `MainLayout.razor`.
- ErsatzTV action bars/buttons: logs search uses a label plus search-adorned
  input; logs Timestamp and Level headers are sort labels; troubleshooting
  tools are direct links/download controls; playback troubleshooting exposes
  preview/log/download controls.
- ErsatzTV tables/forms/pagers/dialogs: health is a two-column dense table with
  status icons and message links; logs is a fixed-header dense table with
  search, Timestamp/Level sortable headers, no-records/loading text, and
  `MudTablePager`; troubleshooting uses tabbed report tables and form-backed
  validation routes.
- Width/alignment/spacing rules: ExtraLarge-width content, top padding, 64rem
  health content, 80rem logs content, dense row heights, right-aligned pager,
  no clipped nav badge counts, and mobile-safe table overflow.
- Reference screenshots and viewport sizes: visual proof will be captured by
  `tests/e2e/support-pages.spec.ts` at desktop and mobile widths; source-derived
  geometry above is the reference because no live ErsatzTV screenshots are
  checked in for this route family.
- ErsatzRS shared primitives/CSS modules to reuse:
  `layout.module.css`, existing system page inline table styles,
  `troubleshooting.module.css`, and the shared i18n text keys.
- Known visual deviations: none for health/logs/nav badge behavior after this
  slice; troubleshooting keeps the existing Rust-specific report data called
  out above.

## Acceptance

- [x] Health rows filter `NotApplicable`, render status/message/link parity,
      and nav badges show current health warning/error counts.
- [x] Logs search, Timestamp/Level sort, page navigation, and rows-per-page
      state behave like ErsatzTV's server-backed `MudTable`.
- [x] Troubleshooting report, sequential schedule, block playout, and playback
      routes remain routed with existing download/direct-link side effects.
- [x] Browser coverage proves the high-risk support workflows and visible
      geometry with no console errors.
- [x] Parity tracker, changelog, and top-three prompt reflect the final proven
      scope.

## ParityDebt

- None.

## Verification

- [x] `cargo test -p ersatzrs-web --all-targets logs`.
- [x] `cargo xtask hydration-lint -p ersatzrs-web`.
- [x] `./run.sh serve`.
- [x] `E2E_BASE_URL=http://127.0.0.1:8409 npx playwright test tests/e2e/support-pages.spec.ts --project=chromium`.
- [ ] `cargo xtask parity-check web`.
- [ ] `cargo xtask progress check`.
