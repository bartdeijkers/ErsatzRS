use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
use crate::admin::i18n::{normalize_ui_language, tr, TextKey, UiLanguage};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelectOption {
    pub value: String,
    pub label: String,
}

#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FfmpegSettingsForm {
    pub ffmpeg_path: String,
    pub ffprobe_path: String,
    pub default_ffmpeg_profile_id: i64,
    pub preferred_audio_language_code: String,
    pub preferred_subtitle_language_code: String,
    pub fallback_subtitle_language_code: String,
    pub use_embedded_subtitles: bool,
    pub extract_embedded_subtitles: bool,
    pub probe_for_interlaced_frames: bool,
    pub save_reports: bool,
    pub global_watermark_id: Option<i64>,
    pub global_fallback_filler_id: Option<i64>,
    pub hls_segmenter_idle_timeout: i32,
    pub work_ahead_segmenter_limit: i32,
    pub active_transcode_limit: i32,
    pub initial_segment_count: i32,
    pub hls_direct_output_format: i32,
    pub default_mpeg_ts_script: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct FfmpegSettingsPageData {
    pub form: FfmpegSettingsForm,
    /// Effective tool paths the locator resolves to right now; shown in the
    /// form so an empty (auto-detect) field still surfaces the detected path.
    pub ffmpeg_resolved: String,
    pub ffprobe_resolved: String,
    pub active_transcode_capacity: ActiveTranscodeCapacityStatus,
    pub ffmpeg_profiles: Vec<SelectOption>,
    pub languages: Vec<SelectOption>,
    pub watermarks: Vec<SelectOption>,
    pub fallback_fillers: Vec<SelectOption>,
    pub mpeg_ts_scripts: Vec<SelectOption>,
    pub custom_resolutions: Vec<SelectOption>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ActiveTranscodeCapacityStatus {
    pub active: i32,
    pub limit: i32,
    pub peak: i32,
    pub suggested_limit: i32,
    pub suggestion_reason: ActiveTranscodeSuggestionReason,
}

impl Default for ActiveTranscodeCapacityStatus {
    fn default() -> Self {
        Self {
            active: 0,
            limit: 2,
            peak: 0,
            suggested_limit: 2,
            suggestion_reason: ActiveTranscodeSuggestionReason::Software,
        }
    }
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ActiveTranscodeSuggestionReason {
    Hardware,
    Software,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoggingSettingsForm {
    pub default_minimum_log_level: String,
    pub scanning_minimum_log_level: String,
    pub scheduling_minimum_log_level: String,
    pub searching_minimum_log_level: String,
    pub streaming_minimum_log_level: String,
    pub http_minimum_log_level: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HdhrSettingsForm {
    pub uuid: String,
    pub tuner_count: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HdhrSettingsPageData {
    pub form: HdhrSettingsForm,
    pub disabled_by_jwt: bool,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScannerSettingsForm {
    pub library_refresh_interval: i32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayoutSettingsForm {
    pub days_to_build: i32,
    pub skip_missing_items: bool,
    pub scripted_schedule_timeout_seconds: i32,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiSettingsForm {
    pub is_dark_mode: bool,
    pub language: String,
    /// Date notation: `auto`, `dmy`, `mdy` or `ymd`.
    #[serde(default)]
    pub date_format: String,
    /// Clock notation: `auto`, `24h` or `12h`.
    #[serde(default)]
    pub time_format: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum WindowsStartupStatusForm {
    Unsupported,
    Disabled,
    Enabled,
    NeedsRepair,
    Unavailable,
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_app::WindowsStartupStatus> for WindowsStartupStatusForm {
    fn from(value: ersatzrs_app::WindowsStartupStatus) -> Self {
        match value {
            ersatzrs_app::WindowsStartupStatus::Unsupported => Self::Unsupported,
            ersatzrs_app::WindowsStartupStatus::Disabled => Self::Disabled,
            ersatzrs_app::WindowsStartupStatus::Enabled => Self::Enabled,
            ersatzrs_app::WindowsStartupStatus::NeedsRepair => Self::NeedsRepair,
            ersatzrs_app::WindowsStartupStatus::Unavailable => Self::Unavailable,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiSettingsPageData {
    pub form: UiSettingsForm,
    pub windows_startup: WindowsStartupStatusForm,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct XmltvSettingsForm {
    pub days_to_build: i32,
    pub time_zone: String,
    pub block_behavior: String,
}

#[server(prefix = "/admin-api")]
pub async fn load_ffmpeg_settings() -> Result<FfmpegSettingsPageData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_ffmpeg_settings_with_state(&state).await
}

#[server(prefix = "/admin-api")]
pub async fn save_ffmpeg_settings(form: FfmpegSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_ffmpeg_settings_with_state(&state, form).await
}

#[server(prefix = "/admin-api")]
pub async fn load_logging_settings() -> Result<LoggingSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let settings = state
        .application
        .load_logging_settings()
        .await
        .map_err(ServerFnError::new)?;
    Ok(logging_settings_form(settings))
}

#[server(prefix = "/admin-api")]
pub async fn save_logging_settings(form: LoggingSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_logging_settings_with_state(&state, form).await
}

#[server(prefix = "/admin-api")]
pub async fn load_hdhr_settings() -> Result<HdhrSettingsPageData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let settings = state
        .application
        .load_hdhr_settings()
        .await
        .map_err(ServerFnError::new)?;
    Ok(HdhrSettingsPageData {
        form: hdhr_settings_form(settings),
        disabled_by_jwt: state.iptv_access_token_verifier.is_enabled(),
    })
}

#[server(prefix = "/admin-api")]
pub async fn save_hdhr_settings(form: HdhrSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_hdhr_settings_with_state(&state, form).await
}

#[server(prefix = "/admin-api")]
pub async fn load_scanner_settings() -> Result<ScannerSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let settings = state
        .application
        .load_scanner_settings()
        .await
        .map_err(ServerFnError::new)?;
    Ok(ScannerSettingsForm {
        library_refresh_interval: settings.library_refresh_interval,
    })
}

#[server(prefix = "/admin-api")]
pub async fn save_scanner_settings(form: ScannerSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_scanner_settings_with_state(&state, form).await
}

#[server(prefix = "/admin-api")]
pub async fn load_playout_settings() -> Result<PlayoutSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let settings = state
        .application
        .load_playout_settings()
        .await
        .map_err(ServerFnError::new)?;
    Ok(playout_settings_form(settings))
}

#[server(prefix = "/admin-api")]
pub async fn save_playout_settings(form: PlayoutSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_playout_settings_with_state(&state, form).await
}

#[server(prefix = "/admin-api")]
pub async fn load_ui_settings() -> Result<UiSettingsPageData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let settings = state
        .application
        .load_ui_settings()
        .await
        .map_err(ServerFnError::new)?;
    Ok(UiSettingsPageData {
        form: {
            let notation = crate::admin::datetime::load_date_time_preferences(
                state.application.ports().admin_settings(),
            )
            .await?;
            UiSettingsForm {
                is_dark_mode: settings.is_dark_mode,
                language: normalize_ui_language(&settings.language),
                date_format: notation.date.config_value().to_owned(),
                time_format: notation.clock.config_value().to_owned(),
            }
        },
        windows_startup: state.application.windows_startup_status().into(),
    })
}

#[server(prefix = "/admin-api")]
pub async fn save_ui_settings(form: UiSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_ui_settings_with_state(&state, form).await
}

#[server(prefix = "/admin-api")]
#[allow(clippy::unused_async)] // Leptos server functions require an async signature.
pub async fn save_windows_startup(enabled: bool) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    set_windows_startup_with_state(&state, enabled)
}

#[server(prefix = "/admin-api")]
pub async fn load_xmltv_settings() -> Result<XmltvSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let settings = state
        .application
        .load_xmltv_settings()
        .await
        .map_err(ServerFnError::new)?;
    Ok(xmltv_settings_form(settings))
}

#[server(prefix = "/admin-api")]
pub async fn save_xmltv_settings(form: XmltvSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_xmltv_settings_with_state(&state, form).await
}

#[server(prefix = "/admin-api")]
pub async fn add_custom_resolution(width: i32, height: i32) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    create_custom_resolution_with_state(&state, width, height).await
}

#[server(prefix = "/admin-api")]
pub async fn delete_custom_resolution(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    delete_custom_resolution_with_state(&state, id).await
}

#[allow(clippy::too_many_lines)]
#[cfg(all(test, feature = "ssr"))]
pub async fn load_ffmpeg_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<FfmpegSettingsPageData, ServerFnError> {
    let repository = ersatzrs_infra::repositories::SqliteAdminSettingsRepository(pool.clone());
    load_ffmpeg_settings_with_capabilities(
        &repository,
        &ersatzrs_ffmpeg::FfmpegCapabilityReport::default(),
    )
    .await
}

#[allow(clippy::too_many_lines)]
#[cfg(feature = "ssr")]
pub async fn load_ffmpeg_settings_with_state(
    state: &crate::host::state::AppState,
) -> Result<FfmpegSettingsPageData, ServerFnError> {
    let runtime = state.ffmpeg_runtime.read().await;
    let capabilities = runtime.capabilities.clone();
    let runtime_ffmpeg_path = runtime.ffmpeg.path.clone();
    let runtime_ffprobe_path = runtime.ffprobe.path.clone();
    drop(runtime);

    let mut data = load_ffmpeg_settings_with_capabilities(
        state.application.ports().admin_settings(),
        &capabilities,
    )
    .await?;
    if !runtime_ffmpeg_path.is_empty() {
        data.ffmpeg_resolved = runtime_ffmpeg_path;
    }
    if !runtime_ffprobe_path.is_empty() {
        data.ffprobe_resolved = runtime_ffprobe_path;
    }
    Ok(data)
}

#[allow(clippy::too_many_lines)]
#[cfg(feature = "ssr")]
async fn load_ffmpeg_settings_with_capabilities(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> Result<FfmpegSettingsPageData, ServerFnError> {
    let model = ersatzrs_app::queries::load_ffmpeg_settings(repository)
        .await
        .map_err(ServerFnError::new)?;
    let app_form = model.form;
    let mut form = FfmpegSettingsForm {
        ffmpeg_path: app_form.ffmpeg_path,
        ffprobe_path: app_form.ffprobe_path,
        default_ffmpeg_profile_id: app_form.default_ffmpeg_profile_id,
        preferred_audio_language_code: app_form.preferred_audio_language_code,
        preferred_subtitle_language_code: app_form.preferred_subtitle_language_code,
        fallback_subtitle_language_code: app_form.fallback_subtitle_language_code,
        use_embedded_subtitles: app_form.use_embedded_subtitles,
        extract_embedded_subtitles: app_form.extract_embedded_subtitles,
        probe_for_interlaced_frames: app_form.probe_for_interlaced_frames,
        save_reports: app_form.save_reports,
        global_watermark_id: app_form.global_watermark_id,
        global_fallback_filler_id: app_form.global_fallback_filler_id,
        hls_segmenter_idle_timeout: app_form.hls_segmenter_idle_timeout,
        work_ahead_segmenter_limit: app_form.work_ahead_segmenter_limit,
        active_transcode_limit: app_form.active_transcode_limit,
        initial_segment_count: app_form.initial_segment_count,
        hls_direct_output_format: app_form.hls_direct_output_format,
        default_mpeg_ts_script: app_form.default_mpeg_ts_script,
    };

    let scripts = ersatzrs_infra::mpegts_scripts::discover_in(
        &ersatzrs_infra::file_system_layout::mpegts_scripts_folder(),
    )
    .await
    .map_err(ServerFnError::new)?;
    let mpeg_ts_scripts = scripts
        .into_iter()
        .map(|script| SelectOption {
            value: script.id,
            label: script.definition.name,
        })
        .collect::<Vec<_>>();
    reconcile_mpeg_ts_script_selection(&mut form.default_mpeg_ts_script, &mpeg_ts_scripts);

    let ffmpeg_resolved = ersatzrs_ffmpeg::resolve_ffmpeg_path(Some(&form.ffmpeg_path)).path;
    let ffprobe_resolved = ersatzrs_ffmpeg::resolve_ffprobe_path(Some(&form.ffprobe_path)).path;
    let active_transcode_capacity =
        active_transcode_capacity_status(form.active_transcode_limit, capabilities);

    Ok(FfmpegSettingsPageData {
        form,
        ffmpeg_resolved,
        ffprobe_resolved,
        active_transcode_capacity,
        ffmpeg_profiles: settings_select_options(model.ffmpeg_profiles),
        languages: ersatzrs_core::language_codes::playback_language_options()
            .into_iter()
            .map(|(value, label)| SelectOption { value, label })
            .collect(),
        watermarks: settings_select_options(model.watermarks),
        fallback_fillers: settings_select_options(model.fallback_fillers),
        mpeg_ts_scripts,
        custom_resolutions: settings_select_options(model.custom_resolutions),
    })
}

#[cfg(feature = "ssr")]
fn reconcile_mpeg_ts_script_selection(selected: &mut String, scripts: &[SelectOption]) {
    // Empty explicitly selects the typed built-in wrapper; absence still
    // defaults to "default" in the existing application settings query.
    if selected.is_empty() || scripts.iter().any(|script| script.value == *selected) {
        return;
    }
    if let Some(script) = scripts
        .iter()
        .find(|script| script.value.eq_ignore_ascii_case("default"))
        .or_else(|| scripts.first())
    {
        selected.clone_from(&script.value);
    }
}

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub async fn save_ffmpeg_settings_with_state(
    state: &crate::host::state::AppState,
    form: FfmpegSettingsForm,
) -> Result<(), ServerFnError> {
    state
        .application
        .save_ffmpeg_settings(ersatzrs_app::commands::SaveFfmpegSettings {
            ffmpeg_path: form.ffmpeg_path,
            ffprobe_path: form.ffprobe_path,
            default_ffmpeg_profile_id: form.default_ffmpeg_profile_id,
            preferred_audio_language_code: form.preferred_audio_language_code,
            preferred_subtitle_language_code: form.preferred_subtitle_language_code,
            fallback_subtitle_language_code: form.fallback_subtitle_language_code,
            use_embedded_subtitles: form.use_embedded_subtitles,
            extract_embedded_subtitles: form.extract_embedded_subtitles,
            probe_for_interlaced_frames: form.probe_for_interlaced_frames,
            save_reports: form.save_reports,
            global_watermark_id: form.global_watermark_id,
            global_fallback_filler_id: form.global_fallback_filler_id,
            hls_segmenter_idle_timeout: form.hls_segmenter_idle_timeout,
            work_ahead_segmenter_limit: form.work_ahead_segmenter_limit,
            active_transcode_limit: form.active_transcode_limit,
            initial_segment_count: form.initial_segment_count,
            hls_direct_output_format: form.hls_direct_output_format,
            default_mpeg_ts_script: form.default_mpeg_ts_script,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub(crate) fn active_transcode_capacity_status(
    limit: i32,
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> ActiveTranscodeCapacityStatus {
    let snapshot = ersatzrs_infra::transcode_capacity::active_transcode_snapshot(limit);
    let suggestion = suggest_active_transcode_limit(capabilities, available_parallelism());
    ActiveTranscodeCapacityStatus {
        active: snapshot.active,
        limit: snapshot.limit,
        peak: snapshot.peak,
        suggested_limit: suggestion.0,
        suggestion_reason: suggestion.1,
    }
}

#[cfg(feature = "ssr")]
fn available_parallelism() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}

#[cfg(feature = "ssr")]
fn suggest_active_transcode_limit(
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
    cpu_count: usize,
) -> (i32, ActiveTranscodeSuggestionReason) {
    let cpu_count = cpu_count.max(1);
    let acceleration = ersatzrs_ffmpeg::preferred_default_profile_acceleration(capabilities);
    if acceleration == ersatzrs_domain::HardwareAccelerationKind::None {
        (
            i32::try_from((cpu_count / 2).clamp(1, 4)).unwrap_or(4),
            ActiveTranscodeSuggestionReason::Software,
        )
    } else {
        (
            i32::try_from(cpu_count.clamp(2, 8)).unwrap_or(8),
            ActiveTranscodeSuggestionReason::Hardware,
        )
    }
}

#[cfg(feature = "ssr")]
pub async fn load_logging_settings_with_repository(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
) -> Result<LoggingSettingsForm, ServerFnError> {
    let settings = ersatzrs_app::queries::load_logging_settings(repository)
        .await
        .map_err(ServerFnError::new)?;
    Ok(logging_settings_form(settings))
}

#[cfg(feature = "ssr")]
pub async fn save_logging_settings_with_state(
    state: &crate::host::state::AppState,
    form: LoggingSettingsForm,
) -> Result<(), ServerFnError> {
    state
        .application
        .save_logging_settings(ersatzrs_app::commands::SaveLoggingSettings {
            default_minimum_log_level: form.default_minimum_log_level,
            scanning_minimum_log_level: form.scanning_minimum_log_level,
            scheduling_minimum_log_level: form.scheduling_minimum_log_level,
            searching_minimum_log_level: form.searching_minimum_log_level,
            streaming_minimum_log_level: form.streaming_minimum_log_level,
            http_minimum_log_level: form.http_minimum_log_level,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn load_hdhr_settings_with_repository(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
) -> Result<HdhrSettingsForm, ServerFnError> {
    let settings = ersatzrs_app::queries::load_hdhr_settings(repository)
        .await
        .map_err(ServerFnError::new)?;
    Ok(hdhr_settings_form(settings))
}

#[cfg(feature = "ssr")]
pub async fn save_hdhr_settings_with_state(
    state: &crate::host::state::AppState,
    form: HdhrSettingsForm,
) -> Result<(), ServerFnError> {
    if state.iptv_access_token_verifier.is_enabled() {
        return Err(ServerFnError::new(tr(TextKey::HdhrDisabledByJwt)));
    }
    state
        .application
        .save_hdhr_settings(ersatzrs_app::commands::SaveHdhrSettings {
            tuner_count: form.tuner_count,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn save_scanner_settings_with_state(
    state: &crate::host::state::AppState,
    form: ScannerSettingsForm,
) -> Result<(), ServerFnError> {
    state
        .application
        .save_scanner_settings(ersatzrs_app::commands::SaveScannerSettings {
            library_refresh_interval: form.library_refresh_interval,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn load_playout_settings_with_repository(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
) -> Result<PlayoutSettingsForm, ServerFnError> {
    let settings = ersatzrs_app::queries::load_playout_settings(repository)
        .await
        .map_err(ServerFnError::new)?;
    Ok(playout_settings_form(settings))
}

#[cfg(feature = "ssr")]
pub async fn save_playout_settings_with_state(
    state: &crate::host::state::AppState,
    form: PlayoutSettingsForm,
) -> Result<(), ServerFnError> {
    state
        .application
        .save_playout_settings(ersatzrs_app::commands::SavePlayoutSettings {
            days_to_build: form.days_to_build,
            skip_missing_items: form.skip_missing_items,
            scripted_schedule_timeout_seconds: form.scripted_schedule_timeout_seconds,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn save_ui_settings_with_state(
    state: &crate::host::state::AppState,
    form: UiSettingsForm,
) -> Result<(), ServerFnError> {
    use crate::admin::datetime::{ClockFormat, DateOrder};

    let language = normalize_ui_language(&form.language);
    state
        .application
        .save_ui_settings(ersatzrs_app::commands::SaveUiSettings {
            is_dark_mode: form.is_dark_mode,
            language,
        })
        .await
        .map_err(ServerFnError::new)?;
    let settings = state.application.ports().admin_settings();
    save_config_value(
        settings,
        ersatzrs_domain::CONFIG_KEY_PAGES_DATE_FORMAT,
        DateOrder::from_config(&form.date_format).config_value(),
    )
    .await?;
    save_config_value(
        settings,
        ersatzrs_domain::CONFIG_KEY_PAGES_TIME_FORMAT,
        ClockFormat::from_config(&form.time_format).config_value(),
    )
    .await
}

#[cfg(feature = "ssr")]
pub fn set_windows_startup_with_state(
    state: &crate::host::state::AppState,
    enabled: bool,
) -> Result<(), ServerFnError> {
    state
        .application
        .set_windows_startup(enabled)
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn load_xmltv_settings_with_repository(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
) -> Result<XmltvSettingsForm, ServerFnError> {
    let settings = ersatzrs_app::queries::load_xmltv_settings(repository)
        .await
        .map_err(ServerFnError::new)?;
    Ok(xmltv_settings_form(settings))
}

#[cfg(feature = "ssr")]
pub async fn save_xmltv_settings_with_state(
    state: &crate::host::state::AppState,
    form: XmltvSettingsForm,
) -> Result<(), ServerFnError> {
    state
        .application
        .save_xmltv_settings(ersatzrs_app::commands::SaveXmltvSettings {
            days_to_build: form.days_to_build,
            time_zone: form.time_zone,
            block_behavior: form.block_behavior,
        })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn create_custom_resolution_with_state(
    state: &crate::host::state::AppState,
    width: i32,
    height: i32,
) -> Result<(), ServerFnError> {
    state
        .application
        .create_custom_resolution(ersatzrs_app::commands::CreateCustomResolution { width, height })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn delete_custom_resolution_with_state(
    state: &crate::host::state::AppState,
    id: i64,
) -> Result<(), ServerFnError> {
    state
        .application
        .delete_custom_resolution(ersatzrs_app::commands::DeleteCustomResolution { id })
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
fn settings_select_options(
    rows: Vec<ersatzrs_core::repositories::SettingsSelectOption>,
) -> Vec<SelectOption> {
    rows.into_iter()
        .map(|row| SelectOption {
            value: row.id.to_string(),
            label: row.name,
        })
        .collect()
}

#[cfg(feature = "ssr")]
fn logging_settings_form(settings: ersatzrs_app::queries::LoggingSettings) -> LoggingSettingsForm {
    LoggingSettingsForm {
        default_minimum_log_level: settings.default_minimum_log_level,
        scanning_minimum_log_level: settings.scanning_minimum_log_level,
        scheduling_minimum_log_level: settings.scheduling_minimum_log_level,
        searching_minimum_log_level: settings.searching_minimum_log_level,
        streaming_minimum_log_level: settings.streaming_minimum_log_level,
        http_minimum_log_level: settings.http_minimum_log_level,
    }
}

#[cfg(feature = "ssr")]
fn hdhr_settings_form(settings: ersatzrs_app::queries::HdhrSettings) -> HdhrSettingsForm {
    HdhrSettingsForm {
        uuid: settings.uuid,
        tuner_count: settings.tuner_count,
    }
}

#[cfg(feature = "ssr")]
fn playout_settings_form(settings: ersatzrs_app::queries::PlayoutSettings) -> PlayoutSettingsForm {
    PlayoutSettingsForm {
        days_to_build: settings.days_to_build,
        skip_missing_items: settings.skip_missing_items,
        scripted_schedule_timeout_seconds: settings.scripted_schedule_timeout_seconds,
    }
}

#[cfg(feature = "ssr")]
fn xmltv_settings_form(settings: ersatzrs_app::queries::XmltvSettings) -> XmltvSettingsForm {
    XmltvSettingsForm {
        days_to_build: settings.days_to_build,
        time_zone: settings.time_zone,
        block_behavior: settings.block_behavior,
    }
}

#[cfg(feature = "ssr")]
pub(crate) async fn load_bool_for_api(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
    key: &str,
    default: bool,
) -> Result<bool, ServerFnError> {
    ersatzrs_app::queries::get_bool_setting(key, default, repository)
        .await
        .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn load_ui_language_for_api(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
) -> Result<UiLanguage, ServerFnError> {
    ersatzrs_app::queries::get_string_setting(
        ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE,
        "en-us",
        repository,
    )
    .await
    .map(|language| UiLanguage::from_config(&language))
    .map_err(ServerFnError::new)
}

#[cfg(feature = "ssr")]
pub async fn save_config_value(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
    key: &'static str,
    value: &str,
) -> Result<(), ServerFnError> {
    ersatzrs_app::commands::set_admin_setting(
        ersatzrs_app::commands::SetAdminSetting {
            key,
            value: value.to_owned(),
        },
        repository,
    )
    .await
    .map_err(ServerFnError::new)
}

#[cfg(all(test, feature = "ssr"))]
mod tests {
    use super::*;
    use sqlx::Row;
    use std::collections::BTreeSet;

    async fn fresh_pool() -> ersatzrs_infra::db::DbPool {
        let dir = tempfile::tempdir().expect("temp db dir");
        let database_url = format!("sqlite:{}", dir.path().join("settings.db").display());
        std::mem::forget(dir);
        ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database")
    }

    #[test]
    fn mpegts_script_selection_preserves_valid_and_recovers_missing_without_persistence() {
        let scripts = vec![
            SelectOption {
                value: "custom".into(),
                label: "A custom script".into(),
            },
            SelectOption {
                value: "DeFaUlT".into(),
                label: "Default".into(),
            },
        ];
        let mut selected = "custom".to_owned();
        reconcile_mpeg_ts_script_selection(&mut selected, &scripts);
        assert_eq!(selected, "custom");
        selected = "removed".into();
        reconcile_mpeg_ts_script_selection(&mut selected, &scripts);
        assert_eq!(selected, "DeFaUlT");
        selected = "removed".into();
        reconcile_mpeg_ts_script_selection(&mut selected, &scripts[..1]);
        assert_eq!(selected, "custom");
        selected = "removed".into();
        reconcile_mpeg_ts_script_selection(&mut selected, &[]);
        assert_eq!(selected, "removed");
        selected.clear();
        reconcile_mpeg_ts_script_selection(&mut selected, &scripts);
        assert!(selected.is_empty());
        reconcile_mpeg_ts_script_selection(&mut selected, &[]);
        assert!(selected.is_empty());
    }

    #[tokio::test]
    async fn load_ffmpeg_settings_distinguishes_builtin_from_missing_script_setting() {
        let pool = fresh_pool().await;
        let key = ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT;
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
            .bind(key)
            .execute(&pool)
            .await
            .unwrap();
        let repository = ersatzrs_infra::repositories::SqliteAdminSettingsRepository(pool.clone());
        // Test the absent-setting contract before filesystem-dependent UI
        // reconciliation may select an operator's only available script.
        let data = ersatzrs_app::queries::load_ffmpeg_settings(&repository)
            .await
            .unwrap();
        assert_eq!(data.form.default_mpeg_ts_script, "default");
        save_config_value(&repository, key, "").await.unwrap();
        let data = load_ffmpeg_settings_with_pool(&pool).await.unwrap();
        assert!(data.form.default_mpeg_ts_script.is_empty());
        assert_eq!(
            ersatzrs_app::queries::get_optional_setting(key, &repository)
                .await
                .unwrap(),
            Some(String::new())
        );
        pool.close().await;
    }

    #[tokio::test]
    async fn load_ffmpeg_settings_resolves_auto_detect_paths() {
        let pool = fresh_pool().await;
        let repository = ersatzrs_infra::repositories::SqliteAdminSettingsRepository(pool.clone());
        save_config_value(&repository, ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH, "")
            .await
            .expect("save empty ffmpeg path");

        let data = load_ffmpeg_settings_with_pool(&pool)
            .await
            .expect("load ffmpeg settings");

        // An empty saved value means auto-detect; the locator must still
        // surface the effective path (bare tool name at minimum).
        assert!(data.form.ffmpeg_path.is_empty());
        assert!(!data.ffmpeg_resolved.is_empty());
        assert!(!data.ffprobe_resolved.is_empty());
        assert_eq!(data.active_transcode_capacity.limit, 2);
        assert!(data.active_transcode_capacity.suggested_limit >= 1);
        assert_eq!(data.form.preferred_audio_language_code, "eng");
        assert!(data.form.preferred_subtitle_language_code.is_empty());
        assert!(data.form.fallback_subtitle_language_code.is_empty());
        assert_eq!(data.languages.len(), 184);
        assert!(data.languages.iter().any(|option| option.value == "eng"));
        assert!(!data.languages.iter().any(|option| option.value == "enm"));
        assert!(!data.languages.iter().any(|option| option.value == "und"));
    }

    #[test]
    fn active_transcode_suggestion_prefers_hardware_capacity_when_encoder_is_available() {
        let capabilities = ersatzrs_ffmpeg::FfmpegCapabilityReport {
            hwaccels: BTreeSet::from(["cuda".to_owned()]),
            encoders: BTreeSet::from(["h264_nvenc".to_owned()]),
            ..ersatzrs_ffmpeg::FfmpegCapabilityReport::default()
        };

        assert_eq!(
            suggest_active_transcode_limit(&capabilities, 6),
            (6, ActiveTranscodeSuggestionReason::Hardware)
        );
    }

    #[test]
    fn active_transcode_suggestion_stays_conservative_for_software_encoding() {
        assert_eq!(
            suggest_active_transcode_limit(&ersatzrs_ffmpeg::FfmpegCapabilityReport::default(), 8),
            (4, ActiveTranscodeSuggestionReason::Software)
        );
        assert_eq!(
            suggest_active_transcode_limit(&ersatzrs_ffmpeg::FfmpegCapabilityReport::default(), 1),
            (1, ActiveTranscodeSuggestionReason::Software)
        );
    }

    #[tokio::test]
    async fn create_custom_resolution_persists_custom_resolution() {
        let pool = fresh_pool().await;
        let repository = ersatzrs_infra::repositories::SqliteAdminSettingsRepository(pool.clone());

        ersatzrs_app::commands::create_custom_resolution(
            ersatzrs_app::commands::CreateCustomResolution {
                width: 1234,
                height: 567,
            },
            &repository,
        )
        .await
        .expect("create custom resolution");

        let row = sqlx::query(
            r#"SELECT "Name", "IsCustom", "Width", "Height"
               FROM "Resolution" WHERE "Width" = ? AND "Height" = ?"#,
        )
        .bind(1234)
        .bind(567)
        .fetch_one(&pool)
        .await
        .expect("fetch custom resolution");

        assert_eq!(row.try_get::<String, _>("Name").unwrap(), "1234x567");
        assert_eq!(row.try_get::<i32, _>("IsCustom").unwrap(), 1);
        assert_eq!(row.try_get::<i32, _>("Width").unwrap(), 1234);
        assert_eq!(row.try_get::<i32, _>("Height").unwrap(), 567);
    }

    #[tokio::test]
    async fn create_custom_resolution_rejects_duplicates() {
        let pool = fresh_pool().await;
        let repository = ersatzrs_infra::repositories::SqliteAdminSettingsRepository(pool.clone());

        ersatzrs_app::commands::create_custom_resolution(
            ersatzrs_app::commands::CreateCustomResolution {
                width: 1234,
                height: 567,
            },
            &repository,
        )
        .await
        .expect("create custom resolution");

        let error = ersatzrs_app::commands::create_custom_resolution(
            ersatzrs_app::commands::CreateCustomResolution {
                width: 1234,
                height: 567,
            },
            &repository,
        )
        .await
        .expect_err("duplicate resolution should fail");

        assert!(error
            .to_string()
            .contains("Resolution width and height must be unique"));
    }

    #[test]
    fn normalize_ui_language_falls_back_to_ersatztv_default() {
        for language in crate::admin::languages::LANGUAGES {
            assert_eq!(normalize_ui_language(language.code), language.code);
            for alias in language.aliases {
                assert_eq!(normalize_ui_language(alias), language.code);
            }
        }
        assert_eq!(normalize_ui_language("unknown"), "en-us");
    }
}
