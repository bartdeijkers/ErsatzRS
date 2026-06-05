#[cfg(feature = "ssr")]
use std::collections::BTreeMap;

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(feature = "ssr")]
const SEQUENTIAL_SCHEDULE_SCHEMA: &str =
    include_str!("../../../resources/sequential-schedule.schema.json");
#[cfg(feature = "ssr")]
const SEQUENTIAL_SCHEDULE_IMPORT_SCHEMA: &str =
    include_str!("../../../resources/sequential-schedule-import.schema.json");

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TroubleshootingReport {
    pub general_json: String,
    pub nvidia_capabilities: String,
    pub qsv_capabilities: String,
    pub vaapi_capabilities: String,
    pub video_toolbox_capabilities: String,
    pub show_nvidia_qsv: bool,
    pub show_vaapi: bool,
    pub show_video_toolbox: bool,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct YamlValidationResult {
    pub exists: bool,
    pub yaml_text: String,
    pub json_text: String,
    pub messages: Vec<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockPlayoutOption {
    pub playout_id: i64,
    pub channel_number: String,
    pub channel_name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TroubleshootingBlockRow {
    pub id: i64,
    pub group_id: i64,
    pub group_name: String,
    pub name: String,
    pub minutes: i32,
    pub stop_scheduling: i32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockPlayoutHistoryRow {
    pub id: i64,
    pub start: String,
    pub finish: String,
    pub key: String,
    pub details: String,
    pub playback_order: String,
    pub collection_type: String,
    pub collection_name: String,
    pub media_item_type: String,
    pub media_item_title: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackTroubleshootingContext {
    pub mode: PlaybackTroubleshootingMode,
    pub media_item_id: i64,
    pub channel_id: i64,
    pub title: String,
    pub kind: String,
    pub duration_seconds: i64,
    pub selected_ffmpeg_profile_id: i64,
    pub ffmpeg_profiles: Vec<PlaybackTroubleshootingOption>,
    pub stream_selectors: Vec<String>,
    pub subtitles: Vec<PlaybackTroubleshootingSubtitle>,
    pub watermarks: Vec<PlaybackTroubleshootingOption>,
    pub graphics_elements: Vec<PlaybackTroubleshootingOption>,
    pub media_info_json: String,
}

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub enum PlaybackTroubleshootingMode {
    #[default]
    Missing,
    MediaItem,
    Channel,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackTroubleshootingOption {
    pub id: i64,
    pub name: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlaybackTroubleshootingSubtitle {
    pub id: i64,
    pub language: String,
    pub title: String,
    pub codec: String,
}

#[server]
pub async fn load_troubleshooting_report() -> Result<TroubleshootingReport, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_troubleshooting_report_with_state(&state).await
}

#[server]
pub async fn validate_sequential_schedule_file(
    yaml_file: String,
    is_import: bool,
) -> Result<YamlValidationResult, ServerFnError> {
    std::future::ready(validate_sequential_schedule_file_impl(
        &yaml_file, is_import,
    ))
    .await
}

#[server]
pub async fn list_block_playout_options() -> Result<Vec<BlockPlayoutOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_block_playout_options_with_pool(&state.pool).await
}

#[server]
pub async fn list_blocks_for_playout(
    playout_id: i64,
) -> Result<Vec<TroubleshootingBlockRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_blocks_for_playout_with_pool(&state.pool, playout_id).await
}

#[server]
pub async fn list_block_playout_history(
    playout_id: i64,
    block_id: i64,
) -> Result<Vec<BlockPlayoutHistoryRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    list_block_playout_history_with_pool(&state.pool, playout_id, block_id).await
}

#[server]
pub async fn load_playback_troubleshooting_context(
    media_item_id: i64,
    channel_id: i64,
) -> Result<PlaybackTroubleshootingContext, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_playback_troubleshooting_context_with_pool(&state.pool, media_item_id, channel_id).await
}

#[cfg(feature = "ssr")]
pub async fn load_troubleshooting_report_with_state(
    state: &crate::host::state::AppState,
) -> Result<TroubleshootingReport, ServerFnError> {
    use ersatzrs_core::health::HealthCheckStatus;
    use ersatzrs_infra::health::make_health_check_service;

    let ffmpeg_settings = load_ffmpeg_settings(&state.pool).await?;
    let health_service = make_health_check_service(
        state.pool.clone(),
        state.ffmpeg_runtime.read().await.clone(),
        state.system_startup.schema_status.clone(),
    );
    let health = health_service
        .perform_health_checks()
        .await
        .into_iter()
        .filter(|result| {
            matches!(
                result.status,
                HealthCheckStatus::Warning | HealthCheckStatus::Fail
            )
        })
        .map(|result| HealthSummary {
            title: result.title,
            message: result.message,
        })
        .collect();

    let runtime = state.ffmpeg_runtime.read().await.clone();
    let video_toolbox_probe_output = video_toolbox_probe_output(&runtime.capabilities);
    let info = GeneralTroubleshootingInfo {
        version: env!("CARGO_PKG_VERSION").to_owned(),
        environment: troubleshooting_environment(),
        runtime: runtime_summary(),
        cpus: cpu_summary(),
        health,
        ffmpeg_settings,
        ffmpeg_profiles: load_ffmpeg_profiles(&state.pool).await?,
        channels: load_channels(&state.pool).await?,
    };

    Ok(TroubleshootingReport {
        general_json: serde_json::to_string_pretty(&info)?,
        nvidia_capabilities: hardware_report(HardwareReportSpec {
            title: "NVIDIA",
            hwaccels: &runtime.capabilities.hwaccels,
            expected_hwaccels: &["cuda", "cuvid"],
            encoders: &runtime.capabilities.encoders,
            expected_encoders: &["h264_nvenc", "hevc_nvenc", "av1_nvenc"],
            decoders: &runtime.capabilities.decoders,
            expected_decoders: &["h264_cuvid", "hevc_cuvid", "mpeg2_cuvid", "mpeg4_cuvid"],
            raw_probe_output: runtime.capabilities.nvidia_gpu_list.as_deref(),
        }),
        qsv_capabilities: hardware_report(HardwareReportSpec {
            title: "QSV",
            hwaccels: &runtime.capabilities.hwaccels,
            expected_hwaccels: &["qsv"],
            encoders: &runtime.capabilities.encoders,
            expected_encoders: &["h264_qsv", "hevc_qsv", "mpeg2_qsv", "av1_qsv"],
            decoders: &runtime.capabilities.decoders,
            expected_decoders: &["h264_qsv", "hevc_qsv", "mpeg2_qsv", "av1_qsv"],
            raw_probe_output: None,
        }),
        vaapi_capabilities: hardware_report(HardwareReportSpec {
            title: "VAAPI",
            hwaccels: &runtime.capabilities.hwaccels,
            expected_hwaccels: &["vaapi"],
            encoders: &runtime.capabilities.encoders,
            expected_encoders: &["h264_vaapi", "hevc_vaapi", "mpeg2_vaapi", "av1_vaapi"],
            decoders: &runtime.capabilities.decoders,
            expected_decoders: &[],
            raw_probe_output: None,
        }),
        video_toolbox_capabilities: hardware_report(HardwareReportSpec {
            title: "VideoToolbox",
            hwaccels: &runtime.capabilities.hwaccels,
            expected_hwaccels: &["videotoolbox"],
            encoders: &runtime.capabilities.encoders,
            expected_encoders: &["h264_videotoolbox", "hevc_videotoolbox"],
            decoders: &runtime.capabilities.decoders,
            expected_decoders: &["h264_videotoolbox", "hevc_videotoolbox"],
            raw_probe_output: video_toolbox_probe_output.as_deref(),
        }),
        show_nvidia_qsv: !cfg!(target_os = "macos"),
        show_vaapi: cfg!(target_os = "linux"),
        show_video_toolbox: cfg!(target_os = "macos"),
    })
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct GeneralTroubleshootingInfo {
    version: String,
    environment: BTreeMap<String, String>,
    runtime: RuntimeSummary,
    cpus: Vec<CpuSummary>,
    health: Vec<HealthSummary>,
    ffmpeg_settings: FfmpegSettingsSummary,
    channels: Vec<ChannelSummary>,
    ffmpeg_profiles: Vec<FfmpegProfileSummary>,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct CpuSummary {
    name: String,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct HealthSummary {
    title: String,
    message: String,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct RuntimeSummary {
    os: &'static str,
    arch: &'static str,
    executable: String,
    current_directory: String,
    data_directory: String,
    transcode_directory: String,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct FfmpegSettingsSummary {
    ffmpeg_path: String,
    ffprobe_path: String,
    default_ffmpeg_profile_id: i32,
    save_reports: bool,
    use_embedded_subtitles: bool,
    extract_embedded_subtitles: bool,
    preferred_audio_language_code: String,
    hls_segmenter_idle_timeout: i32,
    work_ahead_segmenter_limit: i32,
    initial_segment_count: i32,
    hls_direct_output_format: i32,
    default_mpeg_ts_script: String,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct ChannelSummary {
    id: i64,
    number: String,
    name: String,
    ffmpeg_profile_id: i64,
    streaming_mode: i32,
    stream_selector_mode: i32,
    stream_selector: Option<String>,
    is_enabled: bool,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct FfmpegProfileSummary {
    id: i64,
    name: String,
    hardware_acceleration: i32,
    video_format: i32,
    audio_format: i32,
    resolution_id: i64,
}

#[cfg(feature = "ssr")]
async fn load_ffmpeg_settings(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<FfmpegSettingsSummary, ServerFnError> {
    Ok(FfmpegSettingsSummary {
        ffmpeg_path: load_string(pool, ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH, "").await?,
        ffprobe_path: load_string(pool, ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH, "").await?,
        default_ffmpeg_profile_id: load_i32(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_PROFILE_ID,
            0,
        )
        .await?,
        save_reports: load_bool(pool, ersatzrs_domain::CONFIG_KEY_FFMPEG_SAVE_REPORTS, false)
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
        preferred_audio_language_code: load_string(
            pool,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_PREFERRED_LANGUAGE_CODE,
            "eng",
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
            "Default",
        )
        .await?,
    })
}

#[cfg(feature = "ssr")]
async fn load_channels(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<ChannelSummary>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Id", "Number", "Name", "FFmpegProfileId", "StreamingMode",
                  "StreamSelectorMode", "StreamSelector", "IsEnabled"
           FROM "Channel"
           ORDER BY "SortNumber", "Number", "Id""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| ChannelSummary {
            id: row.try_get("Id").unwrap_or_default(),
            number: row.try_get("Number").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
            ffmpeg_profile_id: row.try_get("FFmpegProfileId").unwrap_or_default(),
            streaming_mode: row.try_get("StreamingMode").unwrap_or_default(),
            stream_selector_mode: row.try_get("StreamSelectorMode").unwrap_or_default(),
            stream_selector: row.try_get("StreamSelector").ok(),
            is_enabled: row.try_get("IsEnabled").unwrap_or(false),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_ffmpeg_profiles(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<FfmpegProfileSummary>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT "Id", "Name", "HardwareAcceleration", "VideoFormat",
                  "AudioFormat", "ResolutionId"
           FROM "FFmpegProfile"
           ORDER BY "Name", "Id""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| FfmpegProfileSummary {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
            hardware_acceleration: row.try_get("HardwareAcceleration").unwrap_or_default(),
            video_format: row.try_get("VideoFormat").unwrap_or_default(),
            audio_format: row.try_get("AudioFormat").unwrap_or_default(),
            resolution_id: row.try_get("ResolutionId").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_string(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    default: &str,
) -> Result<String, ServerFnError> {
    let value =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .fetch_optional(pool)
            .await?;
    Ok(value.unwrap_or_else(|| default.to_owned()))
}

#[cfg(feature = "ssr")]
async fn load_i32(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    default: i32,
) -> Result<i32, ServerFnError> {
    Ok(load_string(pool, key, &default.to_string())
        .await?
        .parse::<i32>()
        .unwrap_or(default))
}

#[cfg(feature = "ssr")]
async fn load_bool(
    pool: &ersatzrs_infra::db::DbPool,
    key: &str,
    default: bool,
) -> Result<bool, ServerFnError> {
    let default_text = if default { "true" } else { "false" };
    let value = load_string(pool, key, default_text).await?;
    Ok(matches!(
        value.trim().to_ascii_lowercase().as_str(),
        "1" | "true"
    ))
}

#[cfg(feature = "ssr")]
fn troubleshooting_environment() -> BTreeMap<String, String> {
    troubleshooting_environment_from(std::env::vars())
}

#[cfg(feature = "ssr")]
fn troubleshooting_environment_from(
    variables: impl IntoIterator<Item = (String, String)>,
) -> BTreeMap<String, String> {
    variables
        .into_iter()
        .filter(|(key, _)| is_troubleshooting_environment_key(key))
        .map(|(key, value)| {
            let value = if is_sensitive_environment_key(&key) {
                "<redacted>".to_owned()
            } else {
                value
            };
            (key, value)
        })
        .collect()
}

#[cfg(feature = "ssr")]
fn is_troubleshooting_environment_key(key: &str) -> bool {
    key.starts_with("ETV_")
        || key.starts_with("ErsatzRS_")
        || key.starts_with("ERSATZRS_")
        || key.starts_with("LEPTOS_")
        || matches!(
            key,
            "DATABASE_URL" | "FFMPEG_PATH" | "FFPROBE_PATH" | "RUST_LOG"
        )
}

#[cfg(feature = "ssr")]
fn is_sensitive_environment_key(key: &str) -> bool {
    let key = key.to_ascii_uppercase();
    ["PASSWORD", "PASS", "SECRET", "TOKEN", "KEY", "CREDENTIAL"]
        .iter()
        .any(|needle| key.contains(needle))
}

#[cfg(feature = "ssr")]
fn runtime_summary() -> RuntimeSummary {
    RuntimeSummary {
        os: std::env::consts::OS,
        arch: std::env::consts::ARCH,
        executable: display_path(std::env::current_exe().ok()),
        current_directory: display_path(std::env::current_dir().ok()),
        data_directory: display_path(Some(ersatzrs_infra::file_system_layout::data_path(""))),
        transcode_directory: display_path(Some(
            ersatzrs_infra::file_system_layout::transcode_folder(),
        )),
    }
}

#[cfg(feature = "ssr")]
fn display_path(path: Option<std::path::PathBuf>) -> String {
    path.map_or_else(String::new, |path| path.display().to_string())
}

#[cfg(feature = "ssr")]
fn cpu_summary() -> Vec<CpuSummary> {
    let count = std::thread::available_parallelism().map_or(1, usize::from);
    vec![CpuSummary {
        name: format!("{count} logical CPUs"),
    }]
}

#[cfg(feature = "ssr")]
#[derive(Clone, Copy)]
struct HardwareReportSpec<'a> {
    title: &'a str,
    hwaccels: &'a std::collections::BTreeSet<String>,
    expected_hwaccels: &'a [&'a str],
    encoders: &'a std::collections::BTreeSet<String>,
    expected_encoders: &'a [&'a str],
    decoders: &'a std::collections::BTreeSet<String>,
    expected_decoders: &'a [&'a str],
    raw_probe_output: Option<&'a str>,
}

#[cfg(feature = "ssr")]
fn hardware_report(spec: HardwareReportSpec<'_>) -> String {
    let mut lines = vec![format!("{} FFmpeg capability report", spec.title)];
    lines.push(String::new());
    append_capability_section(
        &mut lines,
        "Hardware accelerators",
        spec.hwaccels,
        spec.expected_hwaccels,
    );
    append_capability_section(
        &mut lines,
        "Encoders",
        spec.encoders,
        spec.expected_encoders,
    );
    append_capability_section(
        &mut lines,
        "Decoders",
        spec.decoders,
        spec.expected_decoders,
    );
    if let Some(output) = spec
        .raw_probe_output
        .filter(|value| !value.trim().is_empty())
    {
        lines.push("Device probe:".to_owned());
        let summary = ersatzrs_ffmpeg::nvidia_gpu_list_summary(output);
        if summary.is_empty() {
            lines.extend(output.lines().map(|line| format!("  {line}")));
        } else {
            lines.extend(summary.into_iter().map(|line| format!("  {line}")));
        }
        lines.push(String::new());
    }
    lines.join("\n")
}

#[cfg(feature = "ssr")]
fn video_toolbox_probe_output(
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> Option<String> {
    if capabilities.videotoolbox_decoders.is_empty()
        && capabilities.videotoolbox_encoders.is_empty()
    {
        return None;
    }

    let mut lines = Vec::new();
    lines.push("framework decoders:".to_owned());
    append_probe_values(&mut lines, &capabilities.videotoolbox_decoders);
    lines.push("framework encoders:".to_owned());
    append_probe_values(&mut lines, &capabilities.videotoolbox_encoders);
    Some(lines.join("\n"))
}

#[cfg(feature = "ssr")]
fn append_probe_values(lines: &mut Vec<String>, values: &std::collections::BTreeSet<String>) {
    if values.is_empty() {
        lines.push("  none".to_owned());
    } else {
        lines.extend(values.iter().map(|value| format!("  {value}")));
    }
}

#[cfg(feature = "ssr")]
fn append_capability_section(
    lines: &mut Vec<String>,
    heading: &str,
    available: &std::collections::BTreeSet<String>,
    expected: &[&str],
) {
    lines.push(format!("{heading}:"));
    if expected.is_empty() {
        lines.push("  No explicit FFmpeg capability probe is required.".to_owned());
    } else {
        for name in expected {
            let status = if available.contains(*name) {
                "available"
            } else {
                "missing"
            };
            lines.push(format!("  {name}: {status}"));
        }
    }
    lines.push(String::new());
}

#[cfg(feature = "ssr")]
fn validate_sequential_schedule_file_impl(
    yaml_file: &str,
    is_import: bool,
) -> Result<YamlValidationResult, ServerFnError> {
    use std::path::Path;

    let trimmed = yaml_file.trim();
    let extension = Path::new(trimmed)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let exists = matches!(extension.as_str(), "yaml" | "yml") && Path::new(trimmed).is_file();
    if !exists {
        return Ok(YamlValidationResult {
            exists: false,
            yaml_text: String::new(),
            json_text: String::new(),
            messages: Vec::new(),
        });
    }

    let yaml_text = std::fs::read_to_string(trimmed)?;
    match sequential_schedule_json_from_yaml(&yaml_text) {
        Ok(json) => {
            let json_text = numbered_pretty_json(&json)?;
            let messages = validate_sequential_schedule_json(&json, is_import)?;
            Ok(YamlValidationResult {
                exists: true,
                yaml_text,
                json_text,
                messages,
            })
        }
        Err(error) => Ok(YamlValidationResult {
            exists: true,
            yaml_text,
            json_text: String::new(),
            messages: vec![error.to_string()],
        }),
    }
}

#[cfg(feature = "ssr")]
fn sequential_schedule_json_from_yaml(yaml: &str) -> Result<serde_json::Value, ServerFnError> {
    let yaml_value = serde_yaml::from_str::<serde_yaml::Value>(yaml)?;
    Ok(serde_json::to_value(yaml_value)?)
}

#[cfg(feature = "ssr")]
fn numbered_pretty_json(value: &serde_json::Value) -> Result<String, serde_json::Error> {
    let formatted = serde_json::to_string_pretty(value)?;
    Ok(formatted
        .lines()
        .enumerate()
        .map(|(index, line)| format!("{:4}: {line}", index + 1))
        .collect::<Vec<_>>()
        .join("\n"))
}

#[cfg(feature = "ssr")]
fn validate_sequential_schedule_json(
    value: &serde_json::Value,
    is_import: bool,
) -> Result<Vec<String>, ServerFnError> {
    let schema_text = if is_import {
        SEQUENTIAL_SCHEDULE_IMPORT_SCHEMA
    } else {
        SEQUENTIAL_SCHEDULE_SCHEMA
    };
    let schema = serde_json::from_str::<serde_json::Value>(schema_text)?;
    let validator = jsonschema::draft202012::new(&schema)
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    Ok(validator
        .iter_errors(value)
        .map(|error| {
            format!(
                "{} at {} (schema {})",
                error,
                jsonschema_instance_location(error.instance_path().as_str()),
                error.schema_path()
            )
        })
        .collect())
}

#[cfg(feature = "ssr")]
fn jsonschema_instance_location(location: &str) -> &str {
    if location.is_empty() {
        "/"
    } else {
        location
    }
}

#[cfg(not(feature = "ssr"))]
fn validate_sequential_schedule_file_impl(
    _yaml_file: &str,
    _is_import: bool,
) -> Result<YamlValidationResult, ServerFnError> {
    Ok(YamlValidationResult::default())
}

#[cfg(feature = "ssr")]
async fn list_block_playout_options_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<BlockPlayoutOption>, ServerFnError> {
    use crate::admin::server_fns::playouts::KIND_BLOCK;
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT p."Id", COALESCE(c."Number", '') AS "ChannelNumber",
                  COALESCE(c."Name", '') AS "ChannelName"
           FROM "Playout" p
           JOIN "Channel" c ON c."Id" = p."ChannelId"
           WHERE p."ScheduleKind" = ?
           ORDER BY c."SortNumber", c."Number", p."Id""#,
    )
    .bind(KIND_BLOCK)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| BlockPlayoutOption {
            playout_id: row.try_get("Id").unwrap_or_default(),
            channel_number: row.try_get("ChannelNumber").unwrap_or_default(),
            channel_name: row.try_get("ChannelName").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn list_blocks_for_playout_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    playout_id: i64,
) -> Result<Vec<TroubleshootingBlockRow>, ServerFnError> {
    use sqlx::Row;

    if playout_id <= 0 {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"SELECT DISTINCT b."Id", b."BlockGroupId",
                  COALESCE(bg."Name", '') AS "GroupName",
                  COALESCE(b."Name", '') AS "Name",
                  b."Minutes", b."StopScheduling"
           FROM "TemplateItem" ti
           INNER JOIN "PlayoutTemplate" pt ON pt."TemplateId" = ti."TemplateId"
           INNER JOIN "Block" b ON b."Id" = ti."BlockId"
           INNER JOIN "BlockGroup" bg ON bg."Id" = b."BlockGroupId"
           WHERE pt."PlayoutId" = ?
           ORDER BY "GroupName", "Name", b."Id""#,
    )
    .bind(playout_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| TroubleshootingBlockRow {
            id: row.try_get("Id").unwrap_or_default(),
            group_id: row.try_get("BlockGroupId").unwrap_or_default(),
            group_name: row.try_get("GroupName").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
            minutes: row.try_get("Minutes").unwrap_or_default(),
            stop_scheduling: row.try_get("StopScheduling").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn list_block_playout_history_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    playout_id: i64,
    block_id: i64,
) -> Result<Vec<BlockPlayoutHistoryRow>, ServerFnError> {
    use serde_json::Value;
    use sqlx::Row;

    if playout_id <= 0 || block_id <= 0 {
        return Ok(Vec::new());
    }

    let rows = sqlx::query(
        r#"SELECT "Id", "When", "Finish", "Key", "Details", "PlaybackOrder"
           FROM "PlayoutHistory"
           WHERE "PlayoutId" = ? AND "BlockId" = ?
           ORDER BY "Id"
           LIMIT 100"#,
    )
    .bind(playout_id)
    .bind(block_id)
    .fetch_all(pool)
    .await?;

    let mut history = Vec::with_capacity(rows.len());
    for row in rows {
        let key: String = row.try_get("Key").unwrap_or_default();
        let details: String = row.try_get("Details").unwrap_or_default();
        let details_json = serde_json::from_str::<Value>(&details).unwrap_or(Value::Null);
        let media_item_id = details_json
            .get("MediaItemId")
            .and_then(Value::as_i64)
            .filter(|id| *id > 0);
        let (media_item_type, media_item_title) = match media_item_id {
            Some(id) => media_item_summary(pool, id).await?,
            None => (String::new(), String::new()),
        };

        history.push(BlockPlayoutHistoryRow {
            id: row.try_get("Id").unwrap_or_default(),
            start: format_datetime(row.try_get("When").unwrap_or_default()),
            finish: row
                .try_get::<Option<String>, _>("Finish")
                .ok()
                .flatten()
                .map(format_datetime)
                .unwrap_or_default(),
            key: key.clone(),
            details,
            playback_order: playback_order_name(row.try_get("PlaybackOrder").unwrap_or_default()),
            collection_type: key_json_from_str(&key)
                .get("CollectionType")
                .map(collection_type_name)
                .unwrap_or_default(),
            collection_name: collection_name_from_key(pool, &key_json_from_str(&key)).await?,
            media_item_type,
            media_item_title,
        });
    }

    Ok(history)
}

#[cfg(feature = "ssr")]
fn format_datetime(value: String) -> String {
    chrono::DateTime::parse_from_rfc3339(&value)
        .map(|date| {
            date.with_timezone(&chrono::Utc)
                .format("%Y-%m-%d %H:%M:%S UTC")
                .to_string()
        })
        .unwrap_or(value)
}

#[cfg(feature = "ssr")]
fn key_json_from_str(key: &str) -> serde_json::Value {
    serde_json::from_str::<serde_json::Value>(key).unwrap_or(serde_json::Value::Null)
}

#[cfg(feature = "ssr")]
fn playback_order_name(value: i32) -> String {
    match value {
        1 => "Shuffle".to_owned(),
        2 => "Chronological".to_owned(),
        3 => "Random".to_owned(),
        4 => "None".to_owned(),
        _ => String::new(),
    }
}

#[cfg(feature = "ssr")]
fn collection_type_name(value: &serde_json::Value) -> String {
    match value
        .as_i64()
        .or_else(|| value.as_str()?.parse::<i64>().ok())
    {
        Some(1) => "Collection".to_owned(),
        Some(2) => "SmartCollection".to_owned(),
        Some(3) => "TelevisionShow".to_owned(),
        Some(4) => "TelevisionSeason".to_owned(),
        Some(5) => "Artist".to_owned(),
        Some(6) => "Playlist".to_owned(),
        Some(7) => "MultiCollection".to_owned(),
        _ => value.as_str().unwrap_or_default().to_owned(),
    }
}

#[cfg(feature = "ssr")]
async fn collection_name_from_key(
    pool: &ersatzrs_infra::db::DbPool,
    key_json: &serde_json::Value,
) -> Result<String, ServerFnError> {
    let collection_id = key_json
        .get("CollectionId")
        .and_then(serde_json::Value::as_i64);
    if let Some(id) = collection_id.filter(|id| *id > 0) {
        return Ok(sqlx::query_scalar::<_, String>(
            r#"SELECT COALESCE("Name", '') FROM "Collection" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .unwrap_or_default());
    }

    let smart_collection_id = key_json
        .get("SmartCollectionId")
        .and_then(serde_json::Value::as_i64);
    if let Some(id) = smart_collection_id.filter(|id| *id > 0) {
        return Ok(sqlx::query_scalar::<_, String>(
            r#"SELECT COALESCE("Name", '') FROM "SmartCollection" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(pool)
        .await?
        .unwrap_or_default());
    }

    Ok(String::new())
}

#[cfg(feature = "ssr")]
async fn media_item_summary(
    pool: &ersatzrs_infra::db::DbPool,
    id: i64,
) -> Result<(String, String), ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT
              CASE
                WHEN EXISTS (SELECT 1 FROM "Movie" WHERE "Id" = mi."Id") THEN 'Movie'
                WHEN EXISTS (SELECT 1 FROM "Episode" WHERE "Id" = mi."Id") THEN 'Episode'
                WHEN EXISTS (SELECT 1 FROM "MusicVideo" WHERE "Id" = mi."Id") THEN 'MusicVideo'
                WHEN EXISTS (SELECT 1 FROM "OtherVideo" WHERE "Id" = mi."Id") THEN 'OtherVideo'
                WHEN EXISTS (SELECT 1 FROM "RemoteStream" WHERE "Id" = mi."Id") THEN 'RemoteStream'
                WHEN EXISTS (SELECT 1 FROM "Song" WHERE "Id" = mi."Id") THEN 'Song'
                WHEN EXISTS (SELECT 1 FROM "Image" WHERE "Id" = mi."Id") THEN 'Image'
                WHEN EXISTS (SELECT 1 FROM "Show" WHERE "Id" = mi."Id") THEN 'Show'
                WHEN EXISTS (SELECT 1 FROM "Season" WHERE "Id" = mi."Id") THEN 'Season'
                WHEN EXISTS (SELECT 1 FROM "Artist" WHERE "Id" = mi."Id") THEN 'Artist'
                ELSE ''
              END AS "Kind",
              COALESCE(
                (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "RemoteStreamMetadata" WHERE "RemoteStreamId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "SongMetadata" WHERE "SongId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "ImageMetadata" WHERE "ImageId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "ShowMetadata" WHERE "ShowId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "SeasonMetadata" WHERE "SeasonId" = mi."Id" LIMIT 1),
                (SELECT "Title" FROM "ArtistMetadata" WHERE "ArtistId" = mi."Id" LIMIT 1),
                ''
              ) AS "Title"
           FROM "MediaItem" mi
           WHERE mi."Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map_or_else(
        || (String::new(), String::new()),
        |row| {
            (
                row.try_get("Kind").unwrap_or_default(),
                row.try_get("Title").unwrap_or_default(),
            )
        },
    ))
}

#[cfg(feature = "ssr")]
pub async fn load_playback_troubleshooting_context_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    media_item_id: i64,
    channel_id: i64,
) -> Result<PlaybackTroubleshootingContext, ServerFnError> {
    let ffmpeg_profiles = load_playback_options(
        pool,
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "FFmpegProfile"
           ORDER BY "Name", "Id""#,
    )
    .await?;
    let watermarks = load_playback_options(
        pool,
        r#"SELECT "Id", COALESCE("Name", '') AS "Name"
           FROM "ChannelWatermark"
           ORDER BY "Name", "Id""#,
    )
    .await?;
    let graphics_elements = load_playback_options(
        pool,
        r#"SELECT "Id", COALESCE("Name", "Path", '') AS "Name"
           FROM "GraphicsElement"
           ORDER BY "Name", "Id""#,
    )
    .await?;
    let default_profile_id = ffmpeg_profiles.first().map_or(0, |profile| profile.id);

    if media_item_id > 0 {
        let Some(row) = load_playback_media_item_row(pool, media_item_id).await? else {
            return Err(ServerFnError::new("Media item does not exist"));
        };
        let media_info_json =
            crate::admin::server_fns::media_browser::get_media_item_info_json_with_pool(
                pool,
                media_item_id,
            )
            .await
            .unwrap_or_default();
        let subtitles = load_playback_subtitles(pool, media_item_id).await?;
        return Ok(PlaybackTroubleshootingContext {
            mode: PlaybackTroubleshootingMode::MediaItem,
            media_item_id,
            channel_id: 0,
            title: row.title,
            kind: split_pascal_case(&row.kind),
            duration_seconds: parse_duration_seconds(&row.duration),
            selected_ffmpeg_profile_id: default_profile_id,
            ffmpeg_profiles,
            stream_selectors: load_stream_selectors().await,
            subtitles,
            watermarks,
            graphics_elements,
            media_info_json,
        });
    }

    if channel_id > 0 {
        let Some(channel) = load_playback_channel_row(pool, channel_id).await? else {
            return Err(ServerFnError::new("Channel does not exist"));
        };
        return Ok(PlaybackTroubleshootingContext {
            mode: PlaybackTroubleshootingMode::Channel,
            media_item_id: 0,
            channel_id,
            title: channel.name,
            kind: "Channel".to_owned(),
            duration_seconds: 0,
            selected_ffmpeg_profile_id: channel.ffmpeg_profile_id,
            ffmpeg_profiles,
            stream_selectors: load_stream_selectors().await,
            subtitles: Vec::new(),
            watermarks,
            graphics_elements,
            media_info_json: String::new(),
        });
    }

    Ok(PlaybackTroubleshootingContext {
        mode: PlaybackTroubleshootingMode::Missing,
        ffmpeg_profiles,
        stream_selectors: load_stream_selectors().await,
        watermarks,
        graphics_elements,
        ..PlaybackTroubleshootingContext::default()
    })
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
struct PlaybackMediaItemRow {
    kind: String,
    title: String,
    duration: String,
}

#[cfg(feature = "ssr")]
#[derive(Debug)]
struct PlaybackChannelRow {
    name: String,
    ffmpeg_profile_id: i64,
}

#[cfg(feature = "ssr")]
async fn load_playback_media_item_row(
    pool: &ersatzrs_infra::db::DbPool,
    media_item_id: i64,
) -> Result<Option<PlaybackMediaItemRow>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT
              CASE
                WHEN movie."Id" IS NOT NULL THEN 'Movie'
                WHEN episode."Id" IS NOT NULL THEN 'Episode'
                WHEN other_video."Id" IS NOT NULL THEN 'OtherVideo'
                WHEN remote_stream."Id" IS NOT NULL THEN 'RemoteStream'
                WHEN song."Id" IS NOT NULL THEN 'Song'
                WHEN image."Id" IS NOT NULL THEN 'Image'
                ELSE 'Playback'
              END AS "Kind",
              COALESCE(
                movie_meta."Title",
                episode_meta."Title",
                other_video_meta."Title",
                remote_stream_meta."Title",
                song_meta."Title",
                image_meta."Title",
                ''
              ) AS "Title",
              COALESCE(mv."Duration", '00:00:00') AS "Duration"
           FROM "MediaItem" mi
           LEFT JOIN "Movie" movie ON movie."Id" = mi."Id"
           LEFT JOIN "MovieMetadata" movie_meta ON movie_meta."MovieId" = movie."Id"
           LEFT JOIN "Episode" episode ON episode."Id" = mi."Id"
           LEFT JOIN "EpisodeMetadata" episode_meta ON episode_meta."EpisodeId" = episode."Id"
           LEFT JOIN "OtherVideo" other_video ON other_video."Id" = mi."Id"
           LEFT JOIN "OtherVideoMetadata" other_video_meta ON other_video_meta."OtherVideoId" = other_video."Id"
           LEFT JOIN "RemoteStream" remote_stream ON remote_stream."Id" = mi."Id"
           LEFT JOIN "RemoteStreamMetadata" remote_stream_meta ON remote_stream_meta."RemoteStreamId" = remote_stream."Id"
           LEFT JOIN "Song" song ON song."Id" = mi."Id"
           LEFT JOIN "SongMetadata" song_meta ON song_meta."SongId" = song."Id"
           LEFT JOIN "Image" image ON image."Id" = mi."Id"
           LEFT JOIN "ImageMetadata" image_meta ON image_meta."ImageId" = image."Id"
           LEFT JOIN "MediaVersion" mv
                  ON mv."MovieId" = movie."Id"
                  OR mv."EpisodeId" = episode."Id"
                  OR mv."OtherVideoId" = other_video."Id"
                  OR mv."RemoteStreamId" = remote_stream."Id"
                  OR mv."SongId" = song."Id"
                  OR mv."ImageId" = image."Id"
           WHERE mi."Id" = ?
           ORDER BY mv."Id"
           LIMIT 1"#,
    )
    .bind(media_item_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| PlaybackMediaItemRow {
        kind: row.try_get("Kind").unwrap_or_default(),
        title: row.try_get("Title").unwrap_or_default(),
        duration: row.try_get("Duration").unwrap_or_default(),
    }))
}

#[cfg(feature = "ssr")]
async fn load_playback_channel_row(
    pool: &ersatzrs_infra::db::DbPool,
    channel_id: i64,
) -> Result<Option<PlaybackChannelRow>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT COALESCE("Name", '') AS "Name", "FFmpegProfileId"
           FROM "Channel"
           WHERE "Id" = ?"#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| PlaybackChannelRow {
        name: row.try_get("Name").unwrap_or_default(),
        ffmpeg_profile_id: row.try_get("FFmpegProfileId").unwrap_or_default(),
    }))
}

#[cfg(feature = "ssr")]
async fn load_playback_options(
    pool: &ersatzrs_infra::db::DbPool,
    sql: &'static str,
) -> Result<Vec<PlaybackTroubleshootingOption>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(sql).fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|row| PlaybackTroubleshootingOption {
            id: row.try_get("Id").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
async fn load_stream_selectors() -> Vec<String> {
    let folder = ersatzrs_infra::file_system_layout::data_path("channel-stream-selectors");
    let mut selectors = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(folder).await else {
        return selectors;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let path = entry.path();
        let is_yaml = path
            .extension()
            .and_then(|extension| extension.to_str())
            .is_some_and(|extension| {
                matches!(extension.to_ascii_lowercase().as_str(), "yml" | "yaml")
            });
        if is_yaml {
            if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
                selectors.push(name.to_owned());
            }
        }
    }
    selectors.sort();
    selectors
}

#[cfg(feature = "ssr")]
async fn load_playback_subtitles(
    pool: &ersatzrs_infra::db::DbPool,
    media_item_id: i64,
) -> Result<Vec<PlaybackTroubleshootingSubtitle>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT sub."Id",
                  COALESCE(sub."Language", '') AS "Language",
                  COALESCE(sub."Title", '') AS "Title",
                  COALESCE(sub."Codec", '') AS "Codec"
           FROM "Subtitle" sub
           WHERE (
                 sub."MovieMetadataId" IN (SELECT "Id" FROM "MovieMetadata" WHERE "MovieId" = ?)
              OR sub."EpisodeMetadataId" IN (SELECT "Id" FROM "EpisodeMetadata" WHERE "EpisodeId" = ?)
              OR sub."OtherVideoMetadataId" IN (SELECT "Id" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = ?)
           )
             AND NOT (sub."SubtitleKind" = 0 AND sub."IsExtracted" = 0 AND COALESCE(sub."Codec", '') NOT IN ('dvd_subtitle', 'hdmv_pgs_subtitle', 'xsub'))
           ORDER BY sub."StreamIndex", sub."Id""#,
    )
    .bind(media_item_id)
    .bind(media_item_id)
    .bind(media_item_id)
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| PlaybackTroubleshootingSubtitle {
            id: row.try_get("Id").unwrap_or_default(),
            language: row.try_get("Language").unwrap_or_default(),
            title: row.try_get("Title").unwrap_or_default(),
            codec: row.try_get("Codec").unwrap_or_default(),
        })
        .collect())
}

#[cfg(feature = "ssr")]
fn split_pascal_case(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 4);
    let mut previous_lowercase = false;
    for ch in value.chars() {
        if ch.is_uppercase() && previous_lowercase {
            out.push(' ');
        }
        previous_lowercase = ch.is_lowercase();
        out.push(ch);
    }
    out
}

#[cfg(feature = "ssr")]
fn parse_duration_seconds(value: &str) -> i64 {
    let main = value.split('.').next().unwrap_or(value);
    let parts = main
        .split(':')
        .filter_map(|part| part.parse::<i64>().ok())
        .collect::<Vec<_>>();
    match parts.as_slice() {
        [hours, minutes, seconds] => hours * 3600 + minutes * 60 + seconds,
        [minutes, seconds] => minutes * 60 + seconds,
        [seconds] => *seconds,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hardware_report_marks_available_and_missing_capabilities() {
        let hwaccels = std::collections::BTreeSet::from(["qsv".to_owned()]);
        let encoders = std::collections::BTreeSet::from(["h264_qsv".to_owned()]);
        let decoders = std::collections::BTreeSet::new();

        let report = hardware_report(HardwareReportSpec {
            title: "QSV",
            hwaccels: &hwaccels,
            expected_hwaccels: &["qsv"],
            encoders: &encoders,
            expected_encoders: &["h264_qsv", "hevc_qsv"],
            decoders: &decoders,
            expected_decoders: &["h264_qsv"],
            raw_probe_output: None,
        });

        assert!(report.contains("qsv: available"));
        assert!(report.contains("h264_qsv: available"));
        assert!(report.contains("hevc_qsv: missing"));
        assert!(report.contains("h264_qsv: missing"));
    }

    #[test]
    fn hardware_report_includes_nvidia_device_probe_summary() {
        let hwaccels = std::collections::BTreeSet::from(["cuda".to_owned()]);
        let encoders = std::collections::BTreeSet::from(["h264_nvenc".to_owned()]);
        let decoders = std::collections::BTreeSet::new();

        let report = hardware_report(HardwareReportSpec {
            title: "NVIDIA",
            hwaccels: &hwaccels,
            expected_hwaccels: &["cuda"],
            encoders: &encoders,
            expected_encoders: &["h264_nvenc"],
            decoders: &decoders,
            expected_decoders: &[],
            raw_probe_output: Some("GPU #0: NVIDIA GeForce RTX 4070\nunrelated line"),
        });

        assert!(report.contains("Device probe:"));
        assert!(report.contains("GPU #0: NVIDIA GeForce RTX 4070"));
        assert!(!report.contains("unrelated line"));
    }

    #[test]
    fn troubleshooting_environment_reports_rust_runtime_keys_and_redacts_secrets() {
        let environment = troubleshooting_environment_from([
            (
                "DOTNET_BUNDLE_EXTRACT_BASE_DIR".to_owned(),
                "/tmp/dotnet".to_owned(),
            ),
            ("ASPNETCORE_URLS".to_owned(), "http://+:8409".to_owned()),
            (
                "ELASTICSEARCH_URL".to_owned(),
                "http://localhost".to_owned(),
            ),
            (
                "ErsatzRS_FFMPEG__PATH".to_owned(),
                "/opt/ffmpeg/bin/ffmpeg".to_owned(),
            ),
            ("ERSATZRS_API_TOKEN".to_owned(), "secret-token".to_owned()),
            (
                "DATABASE_URL".to_owned(),
                "sqlite:/tmp/ErsatzRS.db".to_owned(),
            ),
            ("RUST_LOG".to_owned(), "info".to_owned()),
            ("LEPTOS_SITE_ROOT".to_owned(), "target/site".to_owned()),
        ]);

        assert!(!environment.contains_key("DOTNET_BUNDLE_EXTRACT_BASE_DIR"));
        assert!(!environment.contains_key("ASPNETCORE_URLS"));
        assert!(!environment.contains_key("ELASTICSEARCH_URL"));
        assert_eq!(
            environment.get("ErsatzRS_FFMPEG__PATH").map(String::as_str),
            Some("/opt/ffmpeg/bin/ffmpeg")
        );
        assert_eq!(
            environment.get("ERSATZRS_API_TOKEN").map(String::as_str),
            Some("<redacted>")
        );
        assert_eq!(
            environment.get("DATABASE_URL").map(String::as_str),
            Some("sqlite:/tmp/ErsatzRS.db")
        );
        assert_eq!(
            environment.get("RUST_LOG").map(String::as_str),
            Some("info")
        );
        assert_eq!(
            environment.get("LEPTOS_SITE_ROOT").map(String::as_str),
            Some("target/site")
        );
    }

    #[test]
    fn general_report_replaces_empty_dotnet_carryovers_with_runtime_summary() {
        let info = GeneralTroubleshootingInfo {
            version: "0.1.0".to_owned(),
            environment: BTreeMap::new(),
            runtime: RuntimeSummary {
                os: "linux",
                arch: "x86_64",
                executable: "/usr/bin/ersatzrs-web".to_owned(),
                current_directory: "/srv/ersatzrs".to_owned(),
                data_directory: "/var/lib/ersatzrs/data".to_owned(),
                transcode_directory: "/var/lib/ersatzrs/transcode".to_owned(),
            },
            cpus: Vec::new(),
            health: Vec::new(),
            ffmpeg_settings: FfmpegSettingsSummary {
                ffmpeg_path: String::new(),
                ffprobe_path: String::new(),
                default_ffmpeg_profile_id: 0,
                save_reports: false,
                use_embedded_subtitles: true,
                extract_embedded_subtitles: false,
                preferred_audio_language_code: "eng".to_owned(),
                hls_segmenter_idle_timeout: 60,
                work_ahead_segmenter_limit: 1,
                initial_segment_count: 1,
                hls_direct_output_format: 2,
                default_mpeg_ts_script: "default".to_owned(),
            },
            channels: Vec::new(),
            ffmpeg_profiles: Vec::new(),
        };

        let json = serde_json::to_value(&info).expect("general info serializes");

        assert!(json.get("Runtime").is_some());
        assert!(json.get("VideoControllers").is_none());
        assert!(json.get("AviSynth").is_none());
    }

    #[cfg(feature = "ssr")]
    const VALID_SEQUENTIAL_SCHEDULE: &str = r"
content:
  - show:
    key: sitcoms
    guids:
      - source: plex
        value: tvdb://1234
    order: chronological
playout:
  - all:
    content: sitcoms
";

    #[cfg(feature = "ssr")]
    const INVALID_SEQUENTIAL_SCHEDULE: &str = r"
content:
  - show:
    key: sitcoms
    guids22:
      - source: plex
        value: tvdb://1234
    order: chronological
playout:
  - all:
    content: sitcoms
";

    #[cfg(feature = "ssr")]
    #[test]
    fn sequential_schedule_validation_numbers_json_lines_and_accepts_valid_schedule() {
        let json = sequential_schedule_json_from_yaml(VALID_SEQUENTIAL_SCHEDULE).unwrap();
        let json_text = numbered_pretty_json(&json).unwrap();
        let messages = validate_sequential_schedule_json(&json, false).unwrap();

        assert!(json_text.starts_with("   1: {"));
        assert!(json_text.contains(r#""content""#));
        assert!(json_text.contains(r#""playout""#));
        assert!(messages.is_empty(), "{messages:?}");
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn sequential_schedule_validation_reports_schema_messages_for_invalid_schedule() {
        let json = sequential_schedule_json_from_yaml(INVALID_SEQUENTIAL_SCHEDULE).unwrap();
        let messages = validate_sequential_schedule_json(&json, false).unwrap();

        assert!(!messages.is_empty());
        assert!(
            messages
                .iter()
                .any(|message| message.contains("/content/0") || message.contains("guids22")),
            "{messages:?}"
        );
    }

    #[cfg(feature = "ssr")]
    #[test]
    fn sequential_schedule_import_schema_does_not_require_playout() {
        let import_json = sequential_schedule_json_from_yaml(
            r"
content:
  - collection: Cartoons
    key: cartoons
    order: shuffle
",
        )
        .unwrap();

        let import_messages = validate_sequential_schedule_json(&import_json, true).unwrap();
        let full_messages = validate_sequential_schedule_json(&import_json, false).unwrap();

        assert!(import_messages.is_empty(), "{import_messages:?}");
        assert!(
            full_messages
                .iter()
                .any(|message| message.contains("playout")),
            "{full_messages:?}"
        );
    }
}
