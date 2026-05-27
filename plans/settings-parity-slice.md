# Settings Parity Slice

## Source Map

- ErsatzTV UI/pages: `$ERSATZTV_SRC/ErsatzTV/Pages/Settings/FFmpegSettings.razor`, `LoggingSettings.razor`, `HDHRSettings.razor`, `ScannerSettings.razor`, `PlayoutSettings.razor`, `UISettings.razor`, `XMLTVSettings.razor`.
- ErsatzTV components/view models: `FFmpegSettingsViewModel`, `LoggingSettingsViewModel`, `PlayoutSettingsViewModel`, `UiSettingsViewModel`, `XmltvSettingsViewModel`, HDHR query/command records, `ResolutionViewModel`, `MpegTsScript`, language-code view models.
- ErsatzTV validators: page-local MudForm validators for FFmpeg/FFprobe path existence, HLS timeout >= 30, work-ahead >= 0, initial segments >= 1, HDHR tuner count > 0, scanner interval 0..999999, playout days > 0, scripted timeout >= 10, XMLTV days > 0; handler validation also checks FFmpeg/FFprobe `-version` and XMLTV days <= playout days.
- ErsatzTV handlers/services: `GetFFmpegSettingsHandler`, `UpdateFFmpegSettingsHandler`, `GetLoggingSettingsHandler`, `UpdateLoggingSettingsHandler`, `GetHDHRTunerCountHandler`, `GetHDHRUUIDHandler`, `UpdateHDHRTunerCountHandler`, `GetLibraryRefreshIntervalHandler`, `UpdateLibraryRefreshIntervalHandler`, `GetPlayoutSettingsHandler`, `UpdatePlayoutSettingsHandler`, `GetUiSettingsHandler`, `UpdateUiSettingsHandler`, `GetXmltvSettingsHandler`, `UpdateXmltvSettingsHandler`, resolution create/delete handlers, `IMpegTsScriptService`.
- ErsatzTV repository/query paths: `IConfigElementRepository` backed by `ConfigElement`, `FFmpegProfile`, `Resolution`, `Watermark`, fallback `FillerPreset`, `Channel`, and `Playout` queries.
- Side effects: FFmpeg save creates the reports directory when enabled, queues embedded subtitle extraction when extraction is enabled, and queues FFmpeg capability refresh; logging save updates persisted levels and live Serilog switches; playout save queues Continue builds for all playouts; XMLTV save queues channel-data refresh for all channels; HDHR UUID is generated and persisted if missing; UI save refreshes the browser.
- ErsatzRS target modules: `crates/ersatzrs-web/src/admin/pages/settings.rs`, `settings.module.css`, `admin/server_fns/settings.rs`, `api/settings.rs`, route/module exports, `crates/ersatzrs-domain/src/config.rs`, `crates/ersatzrs-core/src/language_codes.rs`, `STATUS.md`, `docs/reference/parity-matrix.md`.
- Intentional deviations: ErsatzRS keeps its existing application shell chrome and FFmpeg 8.1 runtime environment contract from ADR 0008; the page-level settings routes still use ErsatzTV field names, config keys, and side effects.

## Visual Source Map

- ErsatzTV layout containers: every route uses `MudForm` at max-height 100%, a square `MudPaper` action bar at 64px min-height/height across full width, then a `div.d-flex.flex-column` with `height: 100vh; overflow-x: auto`, containing `MudContainer MaxWidth.ExtraLarge pt-8`.
- ErsatzTV action bars/buttons: top-left filled primary `Save Settings` button with save icon and `ml-6`; FFmpeg custom resolutions has a filled primary `Add Custom Resolution` button below its section divider.
- ErsatzTV tables/forms/pagers/dialogs: settings fields are repeated `MudStack Row=true Breakpoint=SmAndDown form-field-stack gap-md-8 mb-5` rows with a left label container and a right input/select/checkbox; FFmpeg custom resolutions uses a dense hover `MudTable` with `max-width: 800px`, one resolution column, and a 60px delete action column.
- Width/alignment/spacing rules: content max width is MudBlazor ExtraLarge (1920px); heading is Typo.h5 with bottom margin 0.5rem, divider bottom margin 1.5rem; form rows keep consistent label/input columns until small breakpoints stack.
- Reference screenshots and viewport sizes: ErsatzRS browser smoke covered the Razor-derived reference contract at 1440x1000 and 390x844.
- ErsatzRS shared primitives/CSS modules to reuse: existing admin CSS variables, icon primitive for save/add/delete glyphs, and the form/table geometry patterns used by recent scheduling/media parity CSS modules.
- Known visual deviations: none accepted; any screenshot mismatch found in browser verification must stay in `ParityDebt`.

## Acceptance

- [x] UI fields/actions/routes/back-links match ErsatzTV.
- [x] Validation and error surfaces match ErsatzTV.
- [x] Persistence and side effects match ErsatzTV.
- [x] Critical save/action paths have SSR or browser-safe fallback.
- [x] UI layout has screenshot/bounding-box evidence against ErsatzTV or the Razor-derived reference contract.
- [x] Repeated defect classes have reusable tests or playbook coverage.
- [x] Parity docs, status, and active plan files are updated.

## ParityDebt

- None.

## Verification

- [x] `cargo xtask parity-check web`
- [x] Focused settings server/API tests.
- [x] Browser smoke for settings save/fallback UI.
- [x] Playwright route DOM checks for `/settings/{ffmpeg,logging,hdhr,scanner,playout,ui,xmltv}` at 1440x1000: expected headings, Save Settings control, populated inputs/selects, no role-alert errors, no horizontal overflow.
- [x] Playwright screenshots: `settings-ffmpeg-1440.png`, `settings-ffmpeg-dialog-1440.png`, `settings-ffmpeg-mobile.png`.
