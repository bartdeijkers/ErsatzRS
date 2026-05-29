use leptos::prelude::*;
use serde::{Deserialize, Serialize};

use crate::admin::i18n::{normalize_ui_language, UiLanguage};

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
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
    pub use_embedded_subtitles: bool,
    pub extract_embedded_subtitles: bool,
    pub probe_for_interlaced_frames: bool,
    pub save_reports: bool,
    pub global_watermark_id: Option<i64>,
    pub global_fallback_filler_id: Option<i64>,
    pub hls_segmenter_idle_timeout: i32,
    pub work_ahead_segmenter_limit: i32,
    pub initial_segment_count: i32,
    pub hls_direct_output_format: i32,
    pub default_mpeg_ts_script: String,
}

impl Default for FfmpegSettingsForm {
    fn default() -> Self {
        Self {
            ffmpeg_path: String::new(),
            ffprobe_path: String::new(),
            default_ffmpeg_profile_id: 0,
            preferred_audio_language_code: "eng".to_owned(),
            use_embedded_subtitles: true,
            extract_embedded_subtitles: false,
            probe_for_interlaced_frames: false,
            save_reports: false,
            global_watermark_id: None,
            global_fallback_filler_id: None,
            hls_segmenter_idle_timeout: 60,
            work_ahead_segmenter_limit: 1,
            initial_segment_count: 1,
            hls_direct_output_format: 2,
            default_mpeg_ts_script: "default".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct FfmpegSettingsPageData {
    pub form: FfmpegSettingsForm,
    pub ffmpeg_profiles: Vec<SelectOption>,
    pub languages: Vec<SelectOption>,
    pub watermarks: Vec<SelectOption>,
    pub fallback_fillers: Vec<SelectOption>,
    pub mpeg_ts_scripts: Vec<SelectOption>,
    pub custom_resolutions: Vec<SelectOption>,
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

impl Default for LoggingSettingsForm {
    fn default() -> Self {
        Self {
            default_minimum_log_level: "Information".to_owned(),
            scanning_minimum_log_level: "Information".to_owned(),
            scheduling_minimum_log_level: "Information".to_owned(),
            searching_minimum_log_level: "Information".to_owned(),
            streaming_minimum_log_level: "Information".to_owned(),
            http_minimum_log_level: "Information".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct HdhrSettingsForm {
    pub uuid: String,
    pub tuner_count: i32,
}

impl Default for HdhrSettingsForm {
    fn default() -> Self {
        Self {
            uuid: String::new(),
            tuner_count: 2,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ScannerSettingsForm {
    pub library_refresh_interval: i32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlayoutSettingsForm {
    pub days_to_build: i32,
    pub skip_missing_items: bool,
    pub scripted_schedule_timeout_seconds: i32,
}

impl Default for PlayoutSettingsForm {
    fn default() -> Self {
        Self {
            days_to_build: 2,
            skip_missing_items: false,
            scripted_schedule_timeout_seconds: 30,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct UiSettingsForm {
    pub is_dark_mode: bool,
    pub language: String,
}

impl Default for UiSettingsForm {
    fn default() -> Self {
        Self {
            is_dark_mode: true,
            language: "en-us".to_owned(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct XmltvSettingsForm {
    pub days_to_build: i32,
    pub time_zone: String,
    pub block_behavior: String,
}

impl Default for XmltvSettingsForm {
    fn default() -> Self {
        Self {
            days_to_build: 2,
            time_zone: "Local".to_owned(),
            block_behavior: "SplitTimeEvenly".to_owned(),
        }
    }
}

#[server]
pub async fn load_ffmpeg_settings() -> Result<FfmpegSettingsPageData, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_ffmpeg_settings_with_pool(&state.pool).await
}

#[server]
pub async fn save_ffmpeg_settings(form: FfmpegSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_ffmpeg_settings_with_state(&state, form).await
}

#[server]
pub async fn load_logging_settings() -> Result<LoggingSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_logging_settings_with_pool(&state.pool).await
}

#[server]
pub async fn save_logging_settings(form: LoggingSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_logging_settings_with_state(&state, form).await
}

#[server]
pub async fn load_hdhr_settings() -> Result<HdhrSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_hdhr_settings_with_pool(&state.pool).await
}

#[server]
pub async fn save_hdhr_settings(form: HdhrSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_hdhr_settings_with_state(&state, form).await
}

#[server]
pub async fn load_scanner_settings() -> Result<ScannerSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(ScannerSettingsForm {
        library_refresh_interval: load_i32(
            &state.pool,
            ersatzrs_domain::CONFIG_KEY_LIBRARY_REFRESH_INTERVAL,
            0,
        )
        .await?,
    })
}

#[server]
pub async fn save_scanner_settings(form: ScannerSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_scanner_settings_with_pool(&state.pool, form).await
}

#[server]
pub async fn load_playout_settings() -> Result<PlayoutSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_playout_settings_with_pool(&state.pool).await
}

#[server]
pub async fn save_playout_settings(form: PlayoutSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_playout_settings_with_state(&state, form).await
}

#[server]
pub async fn load_ui_settings() -> Result<UiSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(UiSettingsForm {
        is_dark_mode: load_bool(
            &state.pool,
            ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
            true,
        )
        .await?,
        language: load_string(
            &state.pool,
            ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE,
            "en-us",
        )
        .await
        .map(|language| normalize_ui_language(&language))?,
    })
}

#[server]
pub async fn save_ui_settings(form: UiSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_ui_settings_with_pool(&state.pool, form).await
}

#[server]
pub async fn load_xmltv_settings() -> Result<XmltvSettingsForm, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_xmltv_settings_with_pool(&state.pool).await
}

#[server]
pub async fn save_xmltv_settings(form: XmltvSettingsForm) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    save_xmltv_settings_with_state(&state, form).await
}

#[server]
pub async fn add_custom_resolution(width: i32, height: i32) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    create_custom_resolution(&state.pool, width, height).await
}

#[server]
pub async fn delete_custom_resolution(id: i64) -> Result<(), ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    delete_custom_resolution_with_pool(&state.pool, id).await
}

#[allow(clippy::too_many_lines)]
#[cfg(feature = "ssr")]
pub async fn load_ffmpeg_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<FfmpegSettingsPageData, ServerFnError> {
    let form = FfmpegSettingsForm {
        ffmpeg_path: load_string(pool, ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH, "").await?,
        ffprobe_path: load_string(pool, ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH, "").await?,
        default_ffmpeg_profile_id: i64::from(
            load_i32(
                pool,
                ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID,
                0,
            )
            .await?,
        ),
        preferred_audio_language_code: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE,
            "eng",
        )
        .await?,
        use_embedded_subtitles: load_bool(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
            true,
        )
        .await?,
        extract_embedded_subtitles: load_bool(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES,
            false,
        )
        .await?,
        probe_for_interlaced_frames: load_bool(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES,
            false,
        )
        .await?,
        save_reports: load_bool(pool, ersatzrs_domain::CONFIG_KEY_FFMPEG_SAVE_REPORTS, false)
            .await?,
        global_watermark_id: load_optional_i64(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
        )
        .await?,
        global_fallback_filler_id: load_optional_i64(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID,
        )
        .await?,
        hls_segmenter_idle_timeout: load_i32(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
            60,
        )
        .await?,
        work_ahead_segmenter_limit: load_i32(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS,
            1,
        )
        .await?,
        initial_segment_count: load_i32(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT,
            1,
        )
        .await?,
        hls_direct_output_format: load_i32(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_HLS_DIRECT_OUTPUT_FORMAT,
            2,
        )
        .await?,
        default_mpeg_ts_script: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT,
            "default",
        )
        .await?,
    };

    Ok(FfmpegSettingsPageData {
        form,
        ffmpeg_profiles: select_options(
            pool,
            r#"SELECT "Id", "Name" FROM "FFmpegProfile" ORDER BY "Name", "Id""#,
        )
        .await?,
        languages: ersatzrs_core::language_codes::language_options()
            .into_iter()
            .map(|(value, label)| SelectOption { value, label })
            .collect(),
        watermarks: select_options(
            pool,
            r#"SELECT "Id", "Name" FROM "Watermark" ORDER BY "Name", "Id""#,
        )
        .await?,
        fallback_fillers: select_options(
            pool,
            r#"SELECT "Id", "Name" FROM "FillerPreset" WHERE "FillerKind" = 0 ORDER BY "Name", "Id""#,
        )
        .await?,
        mpeg_ts_scripts: vec![SelectOption {
            value: "default".to_owned(),
            label: "Default".to_owned(),
        }],
        custom_resolutions: select_options(
            pool,
            r#"SELECT "Id", "Name" FROM "Resolution" WHERE "IsCustom" = 1 ORDER BY "Height", "Width", "Id""#,
        )
        .await?,
    })
}

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub async fn save_ffmpeg_settings_with_state(
    state: &crate::host::state::AppState,
    mut form: FfmpegSettingsForm,
) -> Result<(), ServerFnError> {
    validate_ffmpeg_settings(&form).await?;
    if !form.use_embedded_subtitles {
        form.extract_embedded_subtitles = false;
    }

    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH,
        &form.ffmpeg_path,
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH,
        &form.ffprobe_path,
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID,
        &form.default_ffmpeg_profile_id.to_string(),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_SAVE_REPORTS,
        bool_value(form.save_reports),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE,
        &form.preferred_audio_language_code,
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_USE_EMBEDDED_SUBTITLES,
        bool_value(form.use_embedded_subtitles),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_EXTRACT_EMBEDDED_SUBTITLES,
        bool_value(form.extract_embedded_subtitles),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_PROBE_FOR_INTERLACED_FRAMES,
        bool_value(form.probe_for_interlaced_frames),
    )
    .await?;
    save_optional_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_WATERMARK_ID,
        form.global_watermark_id,
    )
    .await?;
    save_optional_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_GLOBAL_FALLBACK_FILLER_ID,
        form.global_fallback_filler_id,
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
        &form.hls_segmenter_idle_timeout.to_string(),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_WORK_AHEAD_SEGMENTERS,
        &form.work_ahead_segmenter_limit.to_string(),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT,
        &form.initial_segment_count.to_string(),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_HLS_DIRECT_OUTPUT_FORMAT,
        &form.hls_direct_output_format.to_string(),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT,
        &form.default_mpeg_ts_script,
    )
    .await?;

    if form.save_reports {
        let path = ersatzrs_infra::logging::default_log_dir().join("ffmpeg");
        if let Err(error) = std::fs::create_dir_all(&path) {
            tracing::warn!(%error, path = %path.display(), "failed to create ffmpeg report folder");
        }
    }

    if form.extract_embedded_subtitles {
        let pool = state.pool.clone();
        tokio::spawn(async move {
            match load_playout_ids(&pool).await {
                Ok(playout_ids) => {
                    for playout_id in playout_ids {
                        if let Err(error) =
                            ersatzrs_infra::subtitles::extract_embedded_playout_subtitles(
                                &pool, playout_id,
                            )
                            .await
                        {
                            tracing::warn!(
                                playout_id,
                                %error,
                                "embedded subtitle extraction failed after ffmpeg settings save"
                            );
                        }
                    }
                }
                Err(error) => {
                    tracing::warn!(%error, "failed to load playouts for embedded subtitle extraction");
                }
            }
        });
    }

    refresh_ffmpeg_runtime(state, &form).await?;

    Ok(())
}

#[cfg(feature = "ssr")]
async fn refresh_ffmpeg_runtime(
    state: &crate::host::state::AppState,
    form: &FfmpegSettingsForm,
) -> Result<(), ServerFnError> {
    let mut config = ersatzrs_ffmpeg::FfmpegRuntimeCheckConfig::from_env();
    config.ffmpeg_path.clone_from(&form.ffmpeg_path);
    config.ffprobe_path.clone_from(&form.ffprobe_path);
    let report = ersatzrs_ffmpeg::check_ffmpeg_runtime_with_config(&config)
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    *state.ffmpeg_runtime.write().await = report;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn load_logging_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<LoggingSettingsForm, ServerFnError> {
    Ok(LoggingSettingsForm {
        default_minimum_log_level: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL,
            "Information",
        )
        .await?,
        scanning_minimum_log_level: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING,
            "Information",
        )
        .await?,
        scheduling_minimum_log_level: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING,
            "Information",
        )
        .await?,
        searching_minimum_log_level: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING,
            "Information",
        )
        .await?,
        streaming_minimum_log_level: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING,
            "Information",
        )
        .await?,
        http_minimum_log_level: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP,
            "Information",
        )
        .await?,
    })
}

#[cfg(feature = "ssr")]
pub async fn save_logging_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: LoggingSettingsForm,
) -> Result<(), ServerFnError> {
    for level in [
        &form.default_minimum_log_level,
        &form.scanning_minimum_log_level,
        &form.scheduling_minimum_log_level,
        &form.searching_minimum_log_level,
        &form.streaming_minimum_log_level,
        &form.http_minimum_log_level,
    ] {
        validate_log_level(level)?;
    }

    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL,
        &form.default_minimum_log_level,
    )
    .await?;
    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SCANNING,
        &form.scanning_minimum_log_level,
    )
    .await?;
    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SCHEDULING,
        &form.scheduling_minimum_log_level,
    )
    .await?;
    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_SEARCHING,
        &form.searching_minimum_log_level,
    )
    .await?;
    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_STREAMING,
        &form.streaming_minimum_log_level,
    )
    .await?;
    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_MINIMUM_LOG_LEVEL_HTTP,
        &form.http_minimum_log_level,
    )
    .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn save_logging_settings_with_state(
    state: &crate::host::state::AppState,
    form: LoggingSettingsForm,
) -> Result<(), ServerFnError> {
    save_logging_settings_with_pool(&state.pool, form).await?;
    crate::host::bootstrap::reload_logging_from_database(&state.pool)
        .await
        .map_err(ServerFnError::new)?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn load_hdhr_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<HdhrSettingsForm, ServerFnError> {
    let uuid = match load_optional_string(pool, ersatzrs_domain::CONFIG_KEY_HDHR_UUID).await? {
        Some(value) if uuid::Uuid::parse_str(&value).is_ok() => value,
        _ => {
            let uuid = uuid::Uuid::new_v4().to_string();
            save_config_value(pool, ersatzrs_domain::CONFIG_KEY_HDHR_UUID, &uuid).await?;
            uuid
        }
    };

    Ok(HdhrSettingsForm {
        uuid,
        tuner_count: load_i32(pool, ersatzrs_domain::CONFIG_KEY_HDHR_TUNER_COUNT, 2).await?,
    })
}

#[cfg(feature = "ssr")]
pub async fn save_hdhr_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: HdhrSettingsForm,
) -> Result<(), ServerFnError> {
    if form.tuner_count <= 0 {
        return Err(ServerFnError::new("Tuner count must be greater than zero"));
    }
    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_HDHR_TUNER_COUNT,
        &form.tuner_count.to_string(),
    )
    .await
}

#[cfg(feature = "ssr")]
pub async fn save_hdhr_settings_with_state(
    state: &crate::host::state::AppState,
    form: HdhrSettingsForm,
) -> Result<(), ServerFnError> {
    save_hdhr_settings_with_pool(&state.pool, form.clone()).await?;
    state.set_tuner_count(form.tuner_count);
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn save_scanner_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: ScannerSettingsForm,
) -> Result<(), ServerFnError> {
    if form.library_refresh_interval < 0 {
        return Err(ServerFnError::new(
            "Library refresh interval must be 0 (do not refresh) or greater than zero",
        ));
    }
    if form.library_refresh_interval >= 1_000_000 {
        return Err(ServerFnError::new(
            "Library refresh interval must be less than 1,000,000. Use 0 to disable automatic refresh",
        ));
    }
    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_LIBRARY_REFRESH_INTERVAL,
        &form.library_refresh_interval.to_string(),
    )
    .await
}

#[cfg(feature = "ssr")]
pub async fn load_playout_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<PlayoutSettingsForm, ServerFnError> {
    Ok(PlayoutSettingsForm {
        days_to_build: load_i32(pool, ersatzrs_domain::CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD, 2).await?,
        skip_missing_items: load_bool(
            pool,
            ersatzrs_domain::CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS,
            false,
        )
        .await?,
        scripted_schedule_timeout_seconds: load_i32(
            pool,
            ersatzrs_domain::CONFIG_KEY_PLAYOUT_SCRIPTED_SCHEDULE_TIMEOUT_SECONDS,
            30,
        )
        .await?,
    })
}

#[cfg(feature = "ssr")]
pub async fn save_playout_settings_with_state(
    state: &crate::host::state::AppState,
    form: PlayoutSettingsForm,
) -> Result<(), ServerFnError> {
    if form.days_to_build <= 0 {
        return Err(ServerFnError::new(
            "Playout days to build must be greater than zero",
        ));
    }
    if form.scripted_schedule_timeout_seconds < 10 {
        return Err(ServerFnError::new(
            "Scripted schedule timeout seconds must be at least 10",
        ));
    }

    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD,
        &form.days_to_build.to_string(),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_PLAYOUT_SKIP_MISSING_ITEMS,
        bool_value(form.skip_missing_items),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_PLAYOUT_SCRIPTED_SCHEDULE_TIMEOUT_SECONDS,
        &form.scripted_schedule_timeout_seconds.to_string(),
    )
    .await?;

    let playout_ids: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "Playout" ORDER BY "Id""#)
        .fetch_all(&state.pool)
        .await?;
    enqueue_playout_builds(
        state,
        playout_ids,
        ersatzrs_core::scheduling::PlayoutBuildMode::Continue,
    );
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn save_ui_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    form: UiSettingsForm,
) -> Result<(), ServerFnError> {
    let language = normalize_ui_language(&form.language);
    save_config_value(
        pool,
        ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE,
        bool_value(form.is_dark_mode),
    )
    .await?;
    save_config_value(pool, ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, &language).await
}

#[cfg(feature = "ssr")]
pub async fn load_xmltv_settings_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<XmltvSettingsForm, ServerFnError> {
    Ok(XmltvSettingsForm {
        days_to_build: load_i32(pool, ersatzrs_domain::CONFIG_KEY_XMLTV_DAYS_TO_BUILD, 2).await?,
        time_zone: load_string(pool, ersatzrs_domain::CONFIG_KEY_XMLTV_TIME_ZONE, "Local").await?,
        block_behavior: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR,
            "SplitTimeEvenly",
        )
        .await?,
    })
}

#[cfg(feature = "ssr")]
pub async fn save_xmltv_settings_with_state(
    state: &crate::host::state::AppState,
    form: XmltvSettingsForm,
) -> Result<(), ServerFnError> {
    if form.days_to_build <= 0 {
        return Err(ServerFnError::new(
            "XMLTV days to build must be greater than zero",
        ));
    }
    let playout_days = load_i32(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_PLAYOUT_DAYS_TO_BUILD,
        2,
    )
    .await?;
    if playout_days < form.days_to_build {
        return Err(ServerFnError::new(format!(
            "XMLTV days to build ({}) cannot be greater than Playout days to build ({playout_days})",
            form.days_to_build
        )));
    }
    validate_one_of(&form.time_zone, &["Local", "Utc"])?;
    validate_one_of(&form.block_behavior, &["SplitTimeEvenly", "UseActualTimes"])?;

    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_XMLTV_TIME_ZONE,
        &form.time_zone,
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_XMLTV_DAYS_TO_BUILD,
        &form.days_to_build.to_string(),
    )
    .await?;
    save_config_value(
        &state.pool,
        ersatzrs_domain::CONFIG_KEY_XMLTV_BLOCK_BEHAVIOR,
        &form.block_behavior,
    )
    .await?;

    let playout_ids: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "Playout" ORDER BY "Id""#)
        .fetch_all(&state.pool)
        .await?;
    enqueue_playout_builds(
        state,
        playout_ids,
        ersatzrs_core::scheduling::PlayoutBuildMode::Refresh,
    );
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn create_custom_resolution(
    pool: &ersatzrs_infra::db::DbPool,
    width: i32,
    height: i32,
) -> Result<(), ServerFnError> {
    if width <= 0 || height <= 0 {
        return Err(ServerFnError::new("Resolution width or height is invalid"));
    }
    let existing: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "Resolution" WHERE "Width" = ? AND "Height" = ?"#)
            .bind(width)
            .bind(height)
            .fetch_optional(pool)
            .await?;
    if existing.is_some() {
        return Err(ServerFnError::new(
            "Resolution width and height must be unique",
        ));
    }
    let name = format!("{width}x{height}");
    sqlx::query(
        r#"INSERT INTO "Resolution" ("Name", "IsCustom", "Width", "Height") VALUES (?, 1, ?, ?)"#,
    )
    .bind(name)
    .bind(width)
    .bind(height)
    .execute(pool)
    .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
pub async fn delete_custom_resolution_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    id: i64,
) -> Result<(), ServerFnError> {
    let existing: Option<i64> =
        sqlx::query_scalar(r#"SELECT "Id" FROM "Resolution" WHERE "Id" = ? AND "IsCustom" = 1"#)
            .bind(id)
            .fetch_optional(pool)
            .await?;
    if existing.is_none() {
        return Err(ServerFnError::new(format!(
            "Resolution {id} does not exist."
        )));
    }
    let replacement: Option<i64> = sqlx::query_scalar(
        r#"SELECT "Id" FROM "Resolution" WHERE "IsCustom" = 0 ORDER BY "Height" DESC, "Width" DESC, "Id" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?;
    if let Some(replacement_id) = replacement {
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "ResolutionId" = ? WHERE "ResolutionId" = ?"#)
            .bind(replacement_id)
            .bind(id)
            .execute(pool)
            .await?;
    }
    sqlx::query(r#"DELETE FROM "Resolution" WHERE "Id" = ? AND "IsCustom" = 1"#)
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

#[cfg(feature = "ssr")]
async fn validate_ffmpeg_settings(form: &FfmpegSettingsForm) -> Result<(), ServerFnError> {
    validate_tool_path(&form.ffmpeg_path, "ffmpeg").await?;
    validate_tool_path(&form.ffprobe_path, "ffprobe").await?;
    if form.hls_segmenter_idle_timeout < 30 {
        return Err(ServerFnError::new(
            "HLS Segmenter idle timeout must be greater than or equal to 30",
        ));
    }
    if form.work_ahead_segmenter_limit < 0 {
        return Err(ServerFnError::new(
            "Work-Ahead HLS Segmenter limit must be greater than or equal to 0",
        ));
    }
    if form.initial_segment_count < 1 {
        return Err(ServerFnError::new(
            "HLS Segmenter initial segment count must be greater than or equal to 1",
        ));
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn load_playout_ids(pool: &ersatzrs_infra::db::DbPool) -> Result<Vec<i64>, sqlx::Error> {
    sqlx::query_scalar(r#"SELECT "Id" FROM "Playout" ORDER BY "Id""#)
        .fetch_all(pool)
        .await
}

#[cfg(feature = "ssr")]
async fn validate_tool_path(path: &str, name: &str) -> Result<(), ServerFnError> {
    if path.trim().is_empty() {
        return Err(ServerFnError::new(format!("{name} path does not exist")));
    }
    let output = tokio::process::Command::new(path)
        .arg("-version")
        .output()
        .await
        .map_err(|_| ServerFnError::new(format!("Unable to verify {name} version")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    if output.status.success() && stdout.contains(&format!("{name} version")) {
        Ok(())
    } else {
        Err(ServerFnError::new(format!(
            "Unable to verify {name} version"
        )))
    }
}

#[cfg(feature = "ssr")]
fn validate_log_level(level: &str) -> Result<(), ServerFnError> {
    validate_one_of(level, &["Debug", "Information", "Warning", "Error"])
}

#[cfg(feature = "ssr")]
fn validate_one_of(value: &str, allowed: &[&str]) -> Result<(), ServerFnError> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(ServerFnError::new(format!("Invalid option: {value}")))
    }
}

#[cfg(feature = "ssr")]
fn bool_value(value: bool) -> &'static str {
    if value {
        "True"
    } else {
        "False"
    }
}

#[cfg(feature = "ssr")]
async fn select_options(
    pool: &ersatzrs_infra::db::DbPool,
    sql: &str,
) -> Result<Vec<SelectOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(sql).fetch_all(pool).await?;
    Ok(rows
        .iter()
        .map(|row| SelectOption {
            value: row.try_get::<i64, _>("Id").unwrap_or(0).to_string(),
            label: row
                .try_get::<Option<String>, _>("Name")
                .unwrap_or_default()
                .unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_optional_string(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
) -> Result<Option<String>, ServerFnError> {
    let value =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(value)
}

#[cfg(feature = "ssr")]
async fn load_string(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    default: &str,
) -> Result<String, ServerFnError> {
    Ok(load_optional_string(pool, key)
        .await?
        .unwrap_or_else(|| default.to_owned()))
}

#[cfg(feature = "ssr")]
async fn load_i32(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    default: i32,
) -> Result<i32, ServerFnError> {
    Ok(load_optional_string(pool, key)
        .await?
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or(default))
}

#[cfg(feature = "ssr")]
async fn load_optional_i64(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
) -> Result<Option<i64>, ServerFnError> {
    Ok(load_optional_string(pool, key)
        .await?
        .and_then(|value| value.parse::<i64>().ok()))
}

#[cfg(feature = "ssr")]
async fn load_bool(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    default: bool,
) -> Result<bool, ServerFnError> {
    Ok(load_optional_string(pool, key)
        .await?
        .map_or(default, |value| {
            matches!(value.trim().to_ascii_lowercase().as_str(), "true" | "1")
        }))
}

#[cfg(feature = "ssr")]
pub(crate) async fn load_bool_for_api(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    default: bool,
) -> Result<bool, ServerFnError> {
    load_bool(pool, key, default).await
}

#[cfg(feature = "ssr")]
pub async fn load_ui_language_for_api(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<UiLanguage, ServerFnError> {
    load_string(pool, ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us")
        .await
        .map(|language| UiLanguage::from_config(&language))
}

#[cfg(feature = "ssr")]
pub async fn save_config_value(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    value: &str,
) -> Result<(), ServerFnError> {
    let updated = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
        .bind(value)
        .bind(key)
        .execute(pool)
        .await?;
    if updated.rows_affected() == 0 {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
            .bind(key)
            .bind(value)
            .execute(pool)
            .await?;
    }
    Ok(())
}

#[cfg(feature = "ssr")]
async fn save_optional_config_value(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    value: Option<i64>,
) -> Result<(), ServerFnError> {
    if let Some(value) = value {
        save_config_value(pool, key, &value.to_string()).await
    } else {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .execute(pool)
            .await?;
        Ok(())
    }
}

#[cfg(feature = "ssr")]
fn enqueue_playout_builds(
    state: &crate::host::state::AppState,
    playout_ids: Vec<i64>,
    mode: ersatzrs_core::scheduling::PlayoutBuildMode,
) {
    for playout_id in playout_ids {
        let request = ersatzrs_core::scheduling::PlayoutBuildRequest { playout_id, mode };
        if let Err(error) = state.playout_build_tx.try_send(request) {
            tracing::warn!(
                playout_id,
                %error,
                "dropping requested settings-triggered playout build; periodic scheduler will retry later"
            );
        }
    }
}

#[cfg(all(test, feature = "ssr", not(feature = "mysql")))]
mod tests {
    use super::*;
    use sqlx::Row;

    async fn fresh_pool() -> ersatzrs_infra::db::DbPool {
        let dir = tempfile::tempdir().expect("temp db dir");
        let database_url = format!("sqlite://{}", dir.path().join("settings.db").display());
        std::mem::forget(dir);
        ersatzrs_infra::db::init(&database_url)
            .await
            .expect("initialize test database")
    }

    #[tokio::test]
    async fn create_custom_resolution_persists_custom_resolution() {
        let pool = fresh_pool().await;

        create_custom_resolution(&pool, 1234, 567)
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

        create_custom_resolution(&pool, 1234, 567)
            .await
            .expect("create custom resolution");

        let error = create_custom_resolution(&pool, 1234, 567)
            .await
            .expect_err("duplicate resolution should fail");

        assert!(error
            .to_string()
            .contains("Resolution width and height must be unique"));
    }

    #[test]
    fn normalize_ui_language_falls_back_to_ersatztv_default() {
        assert_eq!(normalize_ui_language("en"), "en-us");
        assert_eq!(normalize_ui_language("en-us"), "en-us");
        assert_eq!(normalize_ui_language("pl"), "pl");
        assert_eq!(normalize_ui_language("pt-br"), "pt-br");
        assert_eq!(normalize_ui_language("nl-NL"), "nl-nl");
    }
}
