#![cfg_attr(test, allow(dead_code))]

#[cfg(feature = "ssr")]
use std::collections::BTreeMap;

use leptos::prelude::*;
use serde::{Deserialize, Serialize};

pub const MPEG_DEFAULT_SAMPLE_DURATION_SECONDS: u64 = 20;
pub const MPEG_MAX_SAMPLE_DURATION_SECONDS: u64 = 120;

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
    #[serde(default)]
    pub streaming_engine: ersatzrs_domain::StreamingEngine,
    pub preferred_audio_title: String,
    pub selected_subtitle_id: i64,
    pub title: String,
    pub kind: String,
    pub duration_seconds: i64,
    pub selected_ffmpeg_profile_id: i64,
    pub ffmpeg_profiles: Vec<PlaybackTroubleshootingOption>,
    pub stream_selectors: Vec<String>,
    pub music_video_credits_templates: Vec<String>,
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

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MpegTroubleshootingContext {
    pub channels: Vec<MpegTroubleshootingChannel>,
    pub selected_channel_number: String,
    pub selected_mode: String,
    pub duration_seconds: u64,
    pub retain_raw_sample: bool,
    pub availability: MpegTsduckAvailability,
    pub latest_report: Option<MpegTroubleshootingReport>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MpegTsduckAvailability {
    pub available: bool,
    pub version: Option<String>,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct MpegTroubleshootingChannel {
    pub number: String,
    pub name: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MpegTroubleshootingReport {
    pub run_id: String,
    pub channel_number: String,
    pub mode: String,
    pub status: MpegTroubleshootingStatus,
    pub summary: String,
    pub duration_seconds: u64,
    pub sample_bytes: u64,
    pub retain_raw_sample: bool,
    pub sample_path: Option<String>,
    pub archive_path: String,
    pub sample_url: String,
    pub tsduck_version: Option<String>,
    pub analysis: Option<MpegTsduckAnalysis>,
    pub errors: Vec<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MpegTsduckAnalysis {
    pub tsp_command: MpegTsduckCommandSpec,
    pub tsbitrate_command: MpegTsduckCommandSpec,
    pub status: MpegTsduckAnalysisStatus,
    pub summary: String,
    pub stability_summary: String,
    pub stability_samples: Vec<MpegTsduckStabilitySample>,
    pub analysis_json_excerpt: String,
    pub continuity_findings: Vec<String>,
    pub pcr_findings: Vec<String>,
    pub services: Vec<String>,
    pub pids: Vec<String>,
    pub bitrate_bps: Option<u64>,
    pub stdout_excerpt: String,
    pub stderr_excerpt: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MpegTsduckStabilitySample {
    pub second_start: u64,
    pub second_end: u64,
    pub level: MpegTsduckStabilityLevel,
    pub packet_count: u64,
    pub bitrate_bps: u64,
    pub note: String,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MpegTsduckStabilityLevel {
    Stable,
    Warning,
    Error,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct MpegTsduckCommandSpec {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MpegTsduckAnalysisStatus {
    Completed,
    Failed,
    NotAvailable,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum MpegTroubleshootingStatus {
    NotAvailable,
    NotApplicable,
    Completed,
    Failed,
}

#[server(prefix = "/admin-api")]
pub async fn load_troubleshooting_report() -> Result<TroubleshootingReport, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_troubleshooting_report_with_state(&state).await
}

#[server(prefix = "/admin-api")]
pub async fn validate_sequential_schedule_file(
    yaml_file: String,
    is_import: bool,
) -> Result<YamlValidationResult, ServerFnError> {
    std::future::ready(validate_sequential_schedule_file_impl(
        &yaml_file, is_import,
    ))
    .await
}

#[server(prefix = "/admin-api")]
pub async fn list_block_playout_options() -> Result<Vec<BlockPlayoutOption>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_block_playout_options()
        .await?
        .into_iter()
        .map(|row| BlockPlayoutOption {
            playout_id: row.playout_id,
            channel_number: row.channel_number,
            channel_name: row.channel_name,
        })
        .collect())
}

#[server(prefix = "/admin-api")]
pub async fn list_blocks_for_playout(
    playout_id: i64,
) -> Result<Vec<TroubleshootingBlockRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    Ok(state
        .application
        .list_blocks_for_playout(playout_id)
        .await?
        .into_iter()
        .map(|row| TroubleshootingBlockRow {
            id: row.id,
            group_id: row.group_id,
            group_name: row.group_name,
            name: row.name,
            minutes: row.minutes,
            stop_scheduling: row.stop_scheduling,
        })
        .collect())
}

#[server(prefix = "/admin-api")]
pub async fn list_block_playout_history(
    playout_id: i64,
    block_id: i64,
) -> Result<Vec<BlockPlayoutHistoryRow>, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    state
        .application
        .list_block_playout_history(playout_id, block_id)
        .await?
        .into_iter()
        .map(|row| {
            let key_json =
                serde_json::from_str::<serde_json::Value>(&row.key).map_err(ServerFnError::new)?;
            Ok(BlockPlayoutHistoryRow {
                id: row.id,
                start: format_datetime(row.start),
                finish: row.finish.map(format_datetime).unwrap_or_default(),
                key: row.key,
                details: row.details,
                playback_order: playback_order_name(row.playback_order),
                collection_type: key_json
                    .get("CollectionType")
                    .map(collection_type_name)
                    .unwrap_or_default(),
                collection_name: row.collection_name,
                media_item_type: row.media_item_type,
                media_item_title: row.media_item_title,
            })
        })
        .collect()
}

/// Shared SSR/WASM view of an owned ephemeral preview; no native runtime types cross the island boundary.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct OwnedPreviewView {
    pub id: String,
    pub state: String,
    pub ready: bool,
    pub cleanup_confirmed: bool,
    pub playlist_url: String,
    pub selected_at: String,
    pub error_code: String,
    pub return_url: String,
    pub log: String,
}

#[server(prefix = "/admin-api")]
pub async fn load_owned_preview_status(
    id: String,
) -> Result<Option<OwnedPreviewView>, ServerFnError> {
    use crate::host::state::AppState;
    let state = expect_context::<AppState>();
    let status = state.application.playback_preview_status(id).await?;
    Ok(status.map(|status| OwnedPreviewView {
        id: status.id,
        state: match status.state {
            ersatzrs_app::PlaybackPreviewState::Queued => "queued",
            ersatzrs_app::PlaybackPreviewState::Running => "running",
            ersatzrs_app::PlaybackPreviewState::Completed => "completed",
            ersatzrs_app::PlaybackPreviewState::Failed => "failed",
            ersatzrs_app::PlaybackPreviewState::Cancelling => "cancelling",
            ersatzrs_app::PlaybackPreviewState::Cancelled => "cancelled",
        }
        .into(),
        ready: status.ready,
        cleanup_confirmed: status.cleanup_confirmed,
        playlist_url: status.playlist_url,
        selected_at: status.selected_at.unwrap_or(status.created_at),
        error_code: status.error_code.unwrap_or_default(),
        return_url: status.return_url,
        log: status.log.map(|log| log.text).unwrap_or_default(),
    }))
}

#[server(prefix = "/admin-api")]
pub async fn load_playback_troubleshooting_context(
    media_item_id: i64,
    channel_id: i64,
    preferred_audio_title: String,
    selected_subtitle_id: i64,
) -> Result<PlaybackTroubleshootingContext, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let read = state
        .application
        .load_playback_troubleshooting(media_item_id, channel_id)
        .await?;
    let mut context = playback_context_from_admin(
        read,
        media_item_id,
        channel_id,
        &preferred_audio_title,
        selected_subtitle_id,
    )?;
    if media_item_id > 0 {
        let info = state
            .application
            .get_media_item_info(media_item_id)
            .await?
            .ok_or_else(|| ServerFnError::new("Media item does not exist"))?;
        context.media_info_json =
            crate::admin::server_fns::media_browser::media_item_info_json_from_admin(info)?;
    }
    Ok(context)
}

#[server(prefix = "/admin-api")]
pub async fn load_mpeg_troubleshooting_context(
    channel_number: String,
    mode: String,
    duration_seconds: u64,
    retain_raw_sample: bool,
) -> Result<MpegTroubleshootingContext, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    load_mpeg_troubleshooting_context_with_state(
        &state,
        channel_number,
        mode,
        duration_seconds,
        retain_raw_sample,
    )
    .await
}

#[cfg(feature = "ssr")]
#[allow(clippy::too_many_lines)]
pub async fn load_troubleshooting_report_with_state(
    state: &crate::host::state::AppState,
) -> Result<TroubleshootingReport, ServerFnError> {
    use ersatzrs_core::health::HealthCheckStatus;

    let runtime = state.ffmpeg_runtime.read().await.clone();
    let report_data = state.application.load_troubleshooting_report_data().await?;
    let ffmpeg_settings = ffmpeg_settings_summary_from_application(
        report_data.ffmpeg_settings,
        &runtime.capabilities,
    )?;
    let health = state
        .application
        .run_admin_health_checks(true)
        .await?
        .results
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

    let nvidia_probe_output = nvidia_probe_output(&runtime.capabilities);
    let video_toolbox_probe_output = video_toolbox_probe_output(&runtime.capabilities);
    let info = GeneralTroubleshootingInfo {
        versions: crate::host::troubleshooting::TroubleshootingVersions::detected(
            state.next_version.get().map(String::as_str),
            &runtime.ffmpeg.version,
        ),
        environment: troubleshooting_environment(),
        runtime: runtime_summary(),
        cpus: cpu_summary(),
        health,
        ffmpeg_settings,
        ffmpeg_profiles: report_data
            .ffmpeg_profiles
            .into_iter()
            .map(|profile| FfmpegProfileSummary {
                id: profile.id,
                name: profile.name,
                hardware_acceleration: profile.hardware_acceleration,
                video_format: profile.video_format,
                audio_format: profile.audio_format,
                resolution_id: profile.resolution_id,
            })
            .collect(),
        channels: report_data
            .channels
            .into_iter()
            .map(|channel| ChannelSummary {
                id: channel.id,
                number: channel.number,
                name: channel.name,
                ffmpeg_profile_id: channel.ffmpeg_profile_id,
                streaming_mode: channel.streaming_mode,
                stream_selector_mode: channel.stream_selector_mode,
                stream_selector: channel.stream_selector,
                is_enabled: channel.is_enabled,
            })
            .collect(),
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
            raw_probe_output: nvidia_probe_output.as_deref(),
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
pub async fn load_mpeg_troubleshooting_context_with_state(
    state: &crate::host::state::AppState,
    channel_number: String,
    mode: String,
    duration_seconds: u64,
    retain_raw_sample: bool,
) -> Result<MpegTroubleshootingContext, ServerFnError> {
    let channels = state
        .application
        .list_mpeg_troubleshooting_channels()
        .await?
        .into_iter()
        .map(|channel| MpegTroubleshootingChannel {
            number: channel.number,
            name: channel.name,
        })
        .collect();
    let selected_channel_number = channel_number.trim().to_owned();
    let selected_mode = normalize_mpeg_mode(&mode);
    let duration_seconds =
        ersatzrs_infra::tsduck::normalize_sample_duration_seconds(duration_seconds);
    let tsduck_availability = ersatzrs_infra::tsduck::detect_tsduck().await;

    let latest_report = if selected_channel_number.is_empty() {
        None
    } else if !is_supported_mpeg_mode(&selected_mode) {
        Some(mpeg_report_not_applicable(
            &selected_channel_number,
            &selected_mode,
            duration_seconds,
            retain_raw_sample,
            tsduck_availability.version.clone(),
            "Selected mode is not an MPEG-TS stream.",
        ))
    } else if !tsduck_availability.available {
        Some(mpeg_report_not_available(
            &selected_channel_number,
            &selected_mode,
            duration_seconds,
            retain_raw_sample,
            tsduck_availability.version.clone(),
            tsduck_availability
                .error
                .clone()
                .unwrap_or_else(|| "tsp was not found on PATH".to_owned()),
        ))
    } else {
        Some(
            run_mpeg_troubleshooting(
                state,
                &selected_channel_number,
                &selected_mode,
                duration_seconds,
                retain_raw_sample,
                tsduck_availability.version.clone(),
            )
            .await?,
        )
    };

    Ok(MpegTroubleshootingContext {
        channels,
        selected_channel_number,
        selected_mode,
        duration_seconds,
        retain_raw_sample,
        availability: tsduck_availability.into(),
        latest_report,
    })
}

#[cfg(all(not(feature = "ssr"), not(target_arch = "wasm32")))]
pub async fn load_mpeg_troubleshooting_context_with_state(
    _state: &crate::host::state::AppState,
    _channel_number: String,
    _mode: String,
    _duration_seconds: u64,
    _retain_raw_sample: bool,
) -> Result<MpegTroubleshootingContext, ServerFnError> {
    Ok(MpegTroubleshootingContext::default())
}

#[cfg(all(test, feature = "ssr"))]
async fn load_mpeg_troubleshooting_channels(
    pool: &ersatzrs_infra::db::DbPool,
) -> Result<Vec<MpegTroubleshootingChannel>, ServerFnError> {
    use sqlx::Row;

    let rows = sqlx::query(
        r#"SELECT COALESCE("Number", '') AS "Number", COALESCE("Name", '') AS "Name"
           FROM "Channel"
           WHERE "IsEnabled" = 1
           ORDER BY "SortNumber", "Number", "Id""#,
    )
    .fetch_all(pool)
    .await?;

    Ok(rows
        .into_iter()
        .map(|row| MpegTroubleshootingChannel {
            number: row.try_get("Number").unwrap_or_default(),
            name: row.try_get("Name").unwrap_or_default(),
        })
        .filter(|channel| !channel.number.trim().is_empty())
        .collect())
}

#[cfg(feature = "ssr")]
async fn run_mpeg_troubleshooting(
    state: &crate::host::state::AppState,
    channel_number: &str,
    mode: &str,
    duration_seconds: u64,
    retain_raw_sample: bool,
    tsduck_version: Option<String>,
) -> Result<MpegTroubleshootingReport, ServerFnError> {
    let run_id = uuid::Uuid::new_v4();
    let run_dir = ersatzrs_infra::tsduck::report_archive_dir(run_id);
    tokio::fs::create_dir_all(&run_dir).await?;
    let sample_path = run_dir.join("sample.ts");
    let sample_url = local_mpeg_sample_url(state, channel_number, mode)?;
    let redacted_sample_url = ersatzrs_infra::tsduck::redact_sensitive_text(&sample_url);

    let sample_bytes = match capture_mpeg_sample(
        &sample_url,
        &sample_path,
        std::time::Duration::from_secs(duration_seconds),
        ersatzrs_infra::tsduck::MAX_SAMPLE_BYTES,
    )
    .await
    {
        Ok(bytes) => bytes,
        Err(error) => {
            let report = MpegTroubleshootingReport {
                run_id: run_id.to_string(),
                channel_number: channel_number.to_owned(),
                mode: mode.to_owned(),
                status: MpegTroubleshootingStatus::Failed,
                summary: "MPEG-TS sample capture failed.".to_owned(),
                duration_seconds,
                sample_bytes: 0,
                retain_raw_sample,
                sample_path: None,
                archive_path: run_dir.display().to_string(),
                sample_url: redacted_sample_url,
                tsduck_version,
                analysis: None,
                errors: vec![ersatzrs_infra::tsduck::redact_sensitive_text(
                    &error.to_string(),
                )],
            };
            write_mpeg_report(&run_dir, &report).await?;
            return Ok(report);
        }
    };

    let analysis =
        ersatzrs_infra::tsduck::analyze_sample(&sample_path, &run_dir, duration_seconds).await;
    let (status, summary, analysis, errors) = match analysis {
        Ok(analysis) => {
            let status = match analysis.status {
                ersatzrs_infra::tsduck::TsduckAnalysisStatus::Completed => {
                    MpegTroubleshootingStatus::Completed
                }
                ersatzrs_infra::tsduck::TsduckAnalysisStatus::Failed
                | ersatzrs_infra::tsduck::TsduckAnalysisStatus::NotAvailable => {
                    MpegTroubleshootingStatus::Failed
                }
            };
            (
                status,
                analysis.summary.clone(),
                Some(analysis.into()),
                Vec::new(),
            )
        }
        Err(error) => (
            MpegTroubleshootingStatus::Failed,
            "TSDuck analysis failed.".to_owned(),
            None,
            vec![ersatzrs_infra::tsduck::redact_sensitive_text(
                &error.to_string(),
            )],
        ),
    };

    if !retain_raw_sample {
        let _ = tokio::fs::remove_file(&sample_path).await;
    }

    let report = MpegTroubleshootingReport {
        run_id: run_id.to_string(),
        channel_number: channel_number.to_owned(),
        mode: mode.to_owned(),
        status,
        summary,
        duration_seconds,
        sample_bytes,
        retain_raw_sample,
        sample_path: retain_raw_sample.then(|| sample_path.display().to_string()),
        archive_path: run_dir.display().to_string(),
        sample_url: redacted_sample_url,
        tsduck_version,
        analysis,
        errors,
    };
    write_mpeg_report(&run_dir, &report).await?;
    Ok(report)
}

#[cfg(feature = "ssr")]
fn mpeg_report_not_available(
    channel_number: &str,
    mode: &str,
    duration_seconds: u64,
    retain_raw_sample: bool,
    tsduck_version: Option<String>,
    error: String,
) -> MpegTroubleshootingReport {
    let run_id = uuid::Uuid::new_v4();
    MpegTroubleshootingReport {
        run_id: run_id.to_string(),
        channel_number: channel_number.to_owned(),
        mode: mode.to_owned(),
        status: MpegTroubleshootingStatus::NotAvailable,
        summary: "TSDuck is not available.".to_owned(),
        duration_seconds,
        sample_bytes: 0,
        retain_raw_sample,
        sample_path: None,
        archive_path: String::new(),
        sample_url: String::new(),
        tsduck_version,
        analysis: None,
        errors: vec![error],
    }
}

#[cfg(feature = "ssr")]
fn mpeg_report_not_applicable(
    channel_number: &str,
    mode: &str,
    duration_seconds: u64,
    retain_raw_sample: bool,
    tsduck_version: Option<String>,
    reason: &str,
) -> MpegTroubleshootingReport {
    let run_id = uuid::Uuid::new_v4();
    MpegTroubleshootingReport {
        run_id: run_id.to_string(),
        channel_number: channel_number.to_owned(),
        mode: mode.to_owned(),
        status: MpegTroubleshootingStatus::NotApplicable,
        summary: reason.to_owned(),
        duration_seconds,
        sample_bytes: 0,
        retain_raw_sample,
        sample_path: None,
        archive_path: String::new(),
        sample_url: String::new(),
        tsduck_version,
        analysis: None,
        errors: Vec::new(),
    }
}

#[cfg(feature = "ssr")]
async fn write_mpeg_report(
    run_dir: &std::path::Path,
    report: &MpegTroubleshootingReport,
) -> Result<(), ServerFnError> {
    let json = serde_json::to_vec_pretty(report)?;
    tokio::fs::write(run_dir.join("report.json"), json).await?;
    let summary_json = serde_json::to_vec_pretty(report)?;
    tokio::fs::write(run_dir.join("summary.json"), summary_json).await?;

    if let Some(analysis) = &report.analysis {
        tokio::fs::write(
            run_dir.join("commands.json"),
            serde_json::to_vec_pretty(&[&analysis.tsp_command, &analysis.tsbitrate_command])?,
        )
        .await?;
        tokio::fs::write(
            run_dir.join("continuity.log"),
            analysis.continuity_findings.join("\n"),
        )
        .await?;
        tokio::fs::write(
            run_dir.join("pcrverify.log"),
            analysis.pcr_findings.join("\n"),
        )
        .await?;
        tokio::fs::write(
            run_dir.join("stability.json"),
            serde_json::to_vec_pretty(&analysis.stability_samples)?,
        )
        .await?;
        if let Some(bitrate) = analysis.bitrate_bps {
            tokio::fs::write(run_dir.join("tsbitrate.txt"), bitrate.to_string()).await?;
        }
        tokio::fs::write(run_dir.join("stdout.txt"), &analysis.stdout_excerpt).await?;
        tokio::fs::write(run_dir.join("stderr.txt"), &analysis.stderr_excerpt).await?;
    }

    Ok(())
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_infra::tsduck::TsduckAvailability> for MpegTsduckAvailability {
    fn from(value: ersatzrs_infra::tsduck::TsduckAvailability) -> Self {
        Self {
            available: value.available,
            version: value.version,
            error: value.error,
        }
    }
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_infra::tsduck::TsduckCommandSpec> for MpegTsduckCommandSpec {
    fn from(value: ersatzrs_infra::tsduck::TsduckCommandSpec) -> Self {
        Self {
            program: value.program,
            args: value.args,
        }
    }
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_infra::tsduck::TsduckAnalysisStatus> for MpegTsduckAnalysisStatus {
    fn from(value: ersatzrs_infra::tsduck::TsduckAnalysisStatus) -> Self {
        match value {
            ersatzrs_infra::tsduck::TsduckAnalysisStatus::Completed => Self::Completed,
            ersatzrs_infra::tsduck::TsduckAnalysisStatus::Failed => Self::Failed,
            ersatzrs_infra::tsduck::TsduckAnalysisStatus::NotAvailable => Self::NotAvailable,
        }
    }
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_infra::tsduck::TsduckStabilityLevel> for MpegTsduckStabilityLevel {
    fn from(value: ersatzrs_infra::tsduck::TsduckStabilityLevel) -> Self {
        match value {
            ersatzrs_infra::tsduck::TsduckStabilityLevel::Stable => Self::Stable,
            ersatzrs_infra::tsduck::TsduckStabilityLevel::Warning => Self::Warning,
            ersatzrs_infra::tsduck::TsduckStabilityLevel::Error => Self::Error,
        }
    }
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_infra::tsduck::TsduckStabilitySample> for MpegTsduckStabilitySample {
    fn from(value: ersatzrs_infra::tsduck::TsduckStabilitySample) -> Self {
        Self {
            second_start: value.second_start,
            second_end: value.second_end,
            level: value.level.into(),
            packet_count: value.packet_count,
            bitrate_bps: value.bitrate_bps,
            note: value.note,
        }
    }
}

#[cfg(feature = "ssr")]
impl From<ersatzrs_infra::tsduck::TsduckAnalysis> for MpegTsduckAnalysis {
    fn from(value: ersatzrs_infra::tsduck::TsduckAnalysis) -> Self {
        Self {
            tsp_command: value.tsp_command.into(),
            tsbitrate_command: value.tsbitrate_command.into(),
            status: value.status.into(),
            summary: value.summary,
            stability_summary: value.stability_summary,
            stability_samples: value
                .stability_samples
                .into_iter()
                .map(Into::into)
                .collect(),
            analysis_json_excerpt: value.analysis_json_excerpt,
            continuity_findings: value.continuity_findings,
            pcr_findings: value.pcr_findings,
            services: value.services,
            pids: value.pids,
            bitrate_bps: value.bitrate_bps,
            stdout_excerpt: value.stdout_excerpt,
            stderr_excerpt: value.stderr_excerpt,
        }
    }
}

#[cfg(feature = "ssr")]
async fn capture_mpeg_sample(
    url: &str,
    sample_path: &std::path::Path,
    duration: std::time::Duration,
    max_bytes: u64,
) -> Result<u64, ServerFnError> {
    use futures_util::StreamExt;
    use tokio::io::AsyncWriteExt;

    let response = reqwest::Client::new()
        .get(url)
        .send()
        .await
        .map_err(|error| ServerFnError::new(error.to_string()))?;
    if !response.status().is_success() {
        return Err(ServerFnError::new(format!(
            "sample stream returned {}",
            response.status()
        )));
    }

    let mut stream = response.bytes_stream();
    let mut file = tokio::fs::File::create(sample_path).await?;
    let deadline = tokio::time::Instant::now() + duration;
    let mut sleep = Box::pin(tokio::time::sleep_until(deadline));
    let mut written = 0_u64;

    loop {
        tokio::select! {
            () = &mut sleep => break,
            chunk = stream.next() => {
                let Some(chunk) = chunk else { break };
                let chunk = chunk.map_err(|error| ServerFnError::new(error.to_string()))?;
                if chunk.is_empty() {
                    continue;
                }
                let remaining = max_bytes.saturating_sub(written);
                if remaining == 0 {
                    break;
                }
                let remaining_usize = usize::try_from(remaining).unwrap_or(usize::MAX);
                let take = chunk.len().min(remaining_usize);
                file.write_all(&chunk[..take]).await?;
                written = written.saturating_add(u64::try_from(take).unwrap_or(u64::MAX));
                if written >= max_bytes {
                    break;
                }
            }
        }
    }

    file.flush().await?;
    if written == 0 {
        return Err(ServerFnError::new("sample stream produced no bytes"));
    }
    Ok(written)
}

#[cfg(feature = "ssr")]
fn normalize_mpeg_mode(mode: &str) -> String {
    match mode.trim() {
        "public-ts" => "public-ts".to_owned(),
        "internal-legacy" | "" => "internal-legacy".to_owned(),
        other => other.to_owned(),
    }
}

#[cfg(feature = "ssr")]
fn is_supported_mpeg_mode(mode: &str) -> bool {
    matches!(mode, "internal-legacy" | "public-ts")
}

#[cfg(feature = "ssr")]
fn local_mpeg_sample_url(
    state: &crate::host::state::AppState,
    channel_number: &str,
    mode: &str,
) -> Result<String, ServerFnError> {
    let address = if mode == "public-ts" {
        state.streaming_http_addr.get()
    } else {
        state.internal_http_addr.get()
    }
    .ok_or_else(|| ServerFnError::new("HTTP listener is not configured"))?;
    let channel_number = urlencoding::encode(channel_number);
    Ok(match mode {
        "public-ts" => format!("http://{address}/iptv/channel/{channel_number}.ts"),
        _ => format!("http://{address}/ffmpeg/stream/{channel_number}"),
    })
}

#[cfg(feature = "ssr")]
#[derive(Debug, Serialize)]
#[serde(rename_all = "PascalCase")]
struct GeneralTroubleshootingInfo {
    #[serde(flatten)]
    versions: crate::host::troubleshooting::TroubleshootingVersions,
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
    teletext_decoder: TeletextDecoderSummary,
    default_ffmpeg_profile_id: i32,
    save_reports: bool,
    use_embedded_subtitles: bool,
    extract_embedded_subtitles: bool,
    preferred_audio_language_code: String,
    hls_segmenter_idle_timeout: i32,
    work_ahead_segmenter_limit: i32,
    active_transcode_limit: i32,
    active_transcode_capacity: crate::admin::server_fns::settings::ActiveTranscodeCapacityStatus,
    initial_segment_count: i32,
    hls_direct_output_format: i32,
    default_mpeg_ts_script: String,
}

#[cfg(feature = "ssr")]
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "PascalCase")]
struct TeletextDecoderSummary {
    capability: ersatzrs_ffmpeg::TeletextDecoderCapability,
    runtime_generation: Option<u64>,
    #[serde(rename = "FFmpegSha256")]
    ffmpeg_sha256: Option<String>,
    #[serde(rename = "FFprobeSha256")]
    ffprobe_sha256: Option<String>,
}

#[cfg(feature = "ssr")]
impl From<&ersatzrs_ffmpeg::FfmpegCapabilityReport> for TeletextDecoderSummary {
    fn from(report: &ersatzrs_ffmpeg::FfmpegCapabilityReport) -> Self {
        let identity = report.runtime_identity.as_ref();
        Self {
            capability: report.teletext_decoder.clone(),
            runtime_generation: identity.map(|identity| identity.generation),
            ffmpeg_sha256: identity.and_then(|identity| identity.ffmpeg_sha256.clone()),
            ffprobe_sha256: identity.and_then(|identity| identity.ffprobe_sha256.clone()),
        }
    }
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
fn ffmpeg_settings_summary_from_application(
    form: ersatzrs_app::queries::FfmpegSettings,
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> Result<FfmpegSettingsSummary, ServerFnError> {
    Ok(FfmpegSettingsSummary {
        ffmpeg_path: form.ffmpeg_path,
        ffprobe_path: form.ffprobe_path,
        teletext_decoder: capabilities.into(),
        default_ffmpeg_profile_id: i32::try_from(form.default_ffmpeg_profile_id)
            .map_err(ServerFnError::new)?,
        save_reports: form.save_reports,
        use_embedded_subtitles: form.use_embedded_subtitles,
        extract_embedded_subtitles: form.extract_embedded_subtitles,
        preferred_audio_language_code: form.preferred_audio_language_code,
        hls_segmenter_idle_timeout: form.hls_segmenter_idle_timeout,
        work_ahead_segmenter_limit: form.work_ahead_segmenter_limit,
        active_transcode_limit: form.active_transcode_limit,
        active_transcode_capacity:
            crate::admin::server_fns::settings::active_transcode_capacity_status(
                form.active_transcode_limit,
                capabilities,
            ),
        initial_segment_count: form.initial_segment_count,
        hls_direct_output_format: form.hls_direct_output_format,
        default_mpeg_ts_script: form.default_mpeg_ts_script,
    })
}

#[cfg(all(test, feature = "ssr"))]
async fn load_ffmpeg_settings(
    pool: &ersatzrs_infra::db::DbPool,
    capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> Result<FfmpegSettingsSummary, ServerFnError> {
    let active_transcode_limit = load_i32(
        pool,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT,
        2,
    )
    .await?;
    Ok(FfmpegSettingsSummary {
        ffmpeg_path: load_string(pool, ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH, "").await?,
        ffprobe_path: load_string(pool, ersatzrs_domain::CONFIG_KEY_FFPROBE_PATH, "").await?,
        teletext_decoder: capabilities.into(),
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
        active_transcode_limit,
        active_transcode_capacity:
            crate::admin::server_fns::settings::active_transcode_capacity_status(
                active_transcode_limit,
                capabilities,
            ),
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
    })
}

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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
                redact_environment_uri(value)
            };
            (key, value)
        })
        .collect()
}

#[cfg(feature = "ssr")]
fn redact_environment_uri(value: String) -> String {
    let Ok(mut uri) = reqwest::Url::parse(&value) else {
        return value;
    };
    if !uri.has_host() {
        return value;
    }
    let has_user_info = !uri.username().is_empty() || uri.password().is_some();
    let query: Vec<_> = uri
        .query_pairs()
        .map(|(key, value)| {
            let value = if is_sensitive_environment_key(&key) {
                "<redacted>".into()
            } else {
                value
            };
            (key.into_owned(), value.into_owned())
        })
        .collect();
    let has_sensitive_query = uri
        .query_pairs()
        .any(|(key, _)| is_sensitive_environment_key(&key));
    if !has_user_info && !has_sensitive_query {
        return value;
    }
    let _ = uri.set_username("");
    let _ = uri.set_password(None);
    if has_sensitive_query {
        uri.query_pairs_mut().clear().extend_pairs(query);
    }
    uri.into()
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
        lines.extend(output.lines().map(|line| format!("  {line}")));
        lines.push(String::new());
    }
    lines.join("\n")
}

#[cfg(feature = "ssr")]
fn nvidia_probe_output(capabilities: &ersatzrs_ffmpeg::FfmpegCapabilityReport) -> Option<String> {
    let mut lines = Vec::new();

    if let Some(output) = capabilities
        .nvidia_gpu_list
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        lines.push("gpu list:".to_owned());
        let summary = ersatzrs_ffmpeg::nvidia_gpu_list_summary(output);
        if summary.is_empty() {
            lines.extend(output.lines().map(|line| format!("  {line}")));
        } else {
            lines.extend(summary.into_iter().map(|line| format!("  {line}")));
        }
    }

    if !capabilities.nvidia_decoders.is_empty() {
        lines.push("decoders:".to_owned());
        for decoder in &capabilities.nvidia_decoders {
            lines.push(format!("  {} {}-bit", decoder.codec, decoder.bit_depth));
        }
    }

    let nvidia_denials: Vec<_> = capabilities
        .hardware_decode_denials
        .iter()
        .filter(|denial| denial.acceleration == ersatzrs_domain::HardwareAccelerationKind::Nvenc)
        .collect();
    if !nvidia_denials.is_empty() {
        lines.push("runtime decode denials:".to_owned());
        for denial in nvidia_denials {
            lines.push(format!("  {} {}-bit", denial.codec, denial.bit_depth));
        }
    }

    if !capabilities.nvidia_encoders.is_empty() {
        lines.push("encoders:".to_owned());
        for encoder in &capabilities.nvidia_encoders {
            lines.push(format!("  {}", encoder.encoder));
            lines.push("    pixel formats:".to_owned());
            append_probe_values_indented(&mut lines, &encoder.pixel_formats);
            lines.push("    profiles:".to_owned());
            append_probe_values_indented(&mut lines, &encoder.profiles);
        }
    }

    if lines.is_empty() {
        None
    } else {
        Some(lines.join("\n"))
    }
}

#[cfg(feature = "ssr")]
fn append_probe_values_indented(
    lines: &mut Vec<String>,
    values: &std::collections::BTreeSet<String>,
) {
    if values.is_empty() {
        lines.push("      none".to_owned());
    } else {
        lines.extend(values.iter().map(|value| format!("      {value}")));
    }
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

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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
fn playback_context_from_admin(
    read: ersatzrs_app::queries::PlaybackTroubleshootingReadModel,
    media_item_id: i64,
    channel_id: i64,
    preferred_audio_title: &str,
    selected_subtitle_id: i64,
) -> Result<PlaybackTroubleshootingContext, ServerFnError> {
    let options = |rows: Vec<ersatzrs_core::repositories::TroubleshootingOptionAdmin>| {
        rows.into_iter()
            .map(|row| PlaybackTroubleshootingOption {
                id: row.id,
                name: row.name,
            })
            .collect()
    };
    let default_profile_id = read
        .data
        .ffmpeg_profiles
        .first()
        .map_or(0, |profile| profile.id);
    let ffmpeg_profiles = options(read.data.ffmpeg_profiles);
    let watermarks = options(read.data.watermarks);
    let graphics_elements = options(read.data.graphics_elements);
    if media_item_id > 0 {
        let media = read
            .data
            .media
            .ok_or_else(|| ServerFnError::new("Media item does not exist"))?;
        return Ok(PlaybackTroubleshootingContext {
            mode: PlaybackTroubleshootingMode::MediaItem,
            streaming_engine: ersatzrs_domain::StreamingEngine::Next,
            media_item_id,
            channel_id: 0,
            preferred_audio_title: preferred_audio_title.trim().to_owned(),
            selected_subtitle_id: selected_subtitle_id.max(0),
            title: media.title,
            kind: split_pascal_case(&media.kind),
            duration_seconds: parse_duration_seconds(&media.duration),
            selected_ffmpeg_profile_id: default_profile_id,
            ffmpeg_profiles,
            stream_selectors: read.stream_selectors,
            music_video_credits_templates: read.music_video_credits_templates,
            subtitles: read
                .data
                .subtitles
                .into_iter()
                .map(|subtitle| PlaybackTroubleshootingSubtitle {
                    id: subtitle.id,
                    language: subtitle.language,
                    title: subtitle.title,
                    codec: subtitle.codec,
                })
                .collect(),
            watermarks,
            graphics_elements,
            media_info_json: String::new(),
        });
    }
    if channel_id > 0 {
        let channel = read
            .data
            .channel
            .ok_or_else(|| ServerFnError::new("Channel does not exist"))?;
        return Ok(PlaybackTroubleshootingContext {
            mode: PlaybackTroubleshootingMode::Channel,
            streaming_engine: channel.streaming_engine,
            media_item_id: 0,
            channel_id,
            preferred_audio_title: String::new(),
            selected_subtitle_id: 0,
            title: channel.name,
            kind: "Channel".to_owned(),
            duration_seconds: 0,
            selected_ffmpeg_profile_id: channel.ffmpeg_profile_id,
            ffmpeg_profiles,
            stream_selectors: read.stream_selectors,
            music_video_credits_templates: read.music_video_credits_templates,
            subtitles: Vec::new(),
            watermarks,
            graphics_elements,
            media_info_json: String::new(),
        });
    }
    Ok(PlaybackTroubleshootingContext {
        mode: PlaybackTroubleshootingMode::Missing,
        ffmpeg_profiles,
        stream_selectors: read.stream_selectors,
        music_video_credits_templates: read.music_video_credits_templates,
        watermarks,
        graphics_elements,
        ..PlaybackTroubleshootingContext::default()
    })
}

#[cfg(all(test, feature = "ssr"))]
pub async fn load_playback_troubleshooting_context_with_pool(
    pool: &ersatzrs_infra::db::DbPool,
    media_item_id: i64,
    channel_id: i64,
    preferred_audio_title: String,
    selected_subtitle_id: i64,
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
        let subtitles = load_playback_subtitles(pool, media_item_id).await?;
        return Ok(PlaybackTroubleshootingContext {
            mode: PlaybackTroubleshootingMode::MediaItem,
            streaming_engine: ersatzrs_domain::StreamingEngine::Next,
            media_item_id,
            channel_id: 0,
            preferred_audio_title: preferred_audio_title.trim().to_owned(),
            selected_subtitle_id: selected_subtitle_id.max(0),
            title: row.title,
            kind: split_pascal_case(&row.kind),
            duration_seconds: parse_duration_seconds(&row.duration),
            selected_ffmpeg_profile_id: default_profile_id,
            ffmpeg_profiles,
            stream_selectors: ersatzrs_infra::file_system_layout::list_channel_stream_selectors()
                .await,
            music_video_credits_templates:
                ersatzrs_infra::music_video_credits::list_music_video_credit_template_names()
                    .await?,
            subtitles,
            watermarks,
            graphics_elements,
            media_info_json: String::new(),
        });
    }

    if channel_id > 0 {
        let Some(channel) = load_playback_channel_row(pool, channel_id).await? else {
            return Err(ServerFnError::new("Channel does not exist"));
        };
        return Ok(PlaybackTroubleshootingContext {
            mode: PlaybackTroubleshootingMode::Channel,
            streaming_engine: channel.streaming_engine,
            media_item_id: 0,
            channel_id,
            preferred_audio_title: String::new(),
            selected_subtitle_id: 0,
            title: channel.name,
            kind: "Channel".to_owned(),
            duration_seconds: 0,
            selected_ffmpeg_profile_id: channel.ffmpeg_profile_id,
            ffmpeg_profiles,
            stream_selectors: ersatzrs_infra::file_system_layout::list_channel_stream_selectors()
                .await,
            music_video_credits_templates: Vec::new(),
            subtitles: Vec::new(),
            watermarks,
            graphics_elements,
            media_info_json: String::new(),
        });
    }

    Ok(PlaybackTroubleshootingContext {
        mode: PlaybackTroubleshootingMode::Missing,
        preferred_audio_title: String::new(),
        selected_subtitle_id: 0,
        ffmpeg_profiles,
        stream_selectors: ersatzrs_infra::file_system_layout::list_channel_stream_selectors().await,
        watermarks,
        graphics_elements,
        ..PlaybackTroubleshootingContext::default()
    })
}

#[cfg(all(test, feature = "ssr"))]
#[derive(Debug)]
struct PlaybackMediaItemRow {
    kind: String,
    title: String,
    duration: String,
}

#[cfg(all(test, feature = "ssr"))]
#[derive(Debug)]
struct PlaybackChannelRow {
    name: String,
    streaming_engine: ersatzrs_domain::StreamingEngine,
    ffmpeg_profile_id: i64,
}

#[cfg(all(test, feature = "ssr"))]
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
                WHEN music_video."Id" IS NOT NULL THEN 'MusicVideo'
                WHEN other_video."Id" IS NOT NULL THEN 'OtherVideo'
                WHEN remote_stream."Id" IS NOT NULL THEN 'RemoteStream'
                WHEN song."Id" IS NOT NULL THEN 'Song'
                WHEN image."Id" IS NOT NULL THEN 'Image'
                ELSE 'Playback'
              END AS "Kind",
              COALESCE(
                movie_meta."Title",
                episode_meta."Title",
                music_video_meta."Title",
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
           LEFT JOIN "MusicVideo" music_video ON music_video."Id" = mi."Id"
           LEFT JOIN "MusicVideoMetadata" music_video_meta ON music_video_meta."MusicVideoId" = music_video."Id"
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
                  OR mv."MusicVideoId" = music_video."Id"
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

#[cfg(all(test, feature = "ssr"))]
async fn load_playback_channel_row(
    pool: &ersatzrs_infra::db::DbPool,
    channel_id: i64,
) -> Result<Option<PlaybackChannelRow>, ServerFnError> {
    use sqlx::Row;

    let row = sqlx::query(
        r#"SELECT COALESCE("Name", '') AS "Name", "FFmpegProfileId", "StreamingEngine"
           FROM "Channel"
           WHERE "Id" = ?"#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(PlaybackChannelRow {
            streaming_engine: match row.try_get::<i32, _>("StreamingEngine")? {
                0 => ersatzrs_domain::StreamingEngine::Legacy,
                1 => ersatzrs_domain::StreamingEngine::Next,
                _ => return Err(ServerFnError::new("invalid channel streaming engine")),
            },
            name: row.try_get("Name").unwrap_or_default(),
            ffmpeg_profile_id: row.try_get("FFmpegProfileId").unwrap_or_default(),
        })
    })
    .transpose()
}

#[cfg(all(test, feature = "ssr"))]
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

#[cfg(all(test, feature = "ssr"))]
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

    #[cfg(feature = "ssr")]
    #[test]
    fn teletext_troubleshooting_keeps_probe_status_bound_to_runtime_identity() {
        use ersatzrs_ffmpeg::{
            FfmpegCapabilityReport, FfmpegRuntimeIdentity, TeletextDecoderCapability,
            TeletextDecoderStatus,
        };
        let mut report = FfmpegCapabilityReport {
            runtime_identity: Some(FfmpegRuntimeIdentity {
                generation: 12,
                ffmpeg_path: "ffmpeg".into(),
                ffmpeg_sha256: Some("a".repeat(64)),
                ffprobe_path: "ffprobe".into(),
                ffprobe_sha256: Some("b".repeat(64)),
                selected_nvidia_device: None,
            }),
            teletext_decoder: TeletextDecoderCapability {
                status: TeletextDecoderStatus::MissingOptions,
                missing_options: vec!["txt_page".into()],
                original_pts: false,
            },
            ..FfmpegCapabilityReport::default()
        };
        let json = serde_json::to_value(TeletextDecoderSummary::from(&report)).unwrap();
        assert_eq!(json["RuntimeGeneration"], 12);
        assert_eq!(json["Capability"]["original_pts"], false);
        assert_eq!(json["FFmpegSha256"], "a".repeat(64));
        assert_eq!(json["FFprobeSha256"], "b".repeat(64));
        assert_eq!(json["Capability"]["status"], "missing_options");
        assert_eq!(json["Capability"]["missing_options"][0], "txt_page");

        report.runtime_identity = None;
        report.teletext_decoder = TeletextDecoderCapability::default();
        let json = serde_json::to_value(TeletextDecoderSummary::from(&report)).unwrap();
        assert!(json["RuntimeGeneration"].is_null());
        assert!(json["FFmpegSha256"].is_null());
        assert_eq!(json["Capability"]["status"], "missing_decoder");
    }

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
    fn hardware_report_includes_device_probe_output_verbatim() {
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
            raw_probe_output: Some("gpu list:\n  GPU #0: NVIDIA GeForce RTX 4070"),
        });

        assert!(report.contains("Device probe:"));
        assert!(report.contains("GPU #0: NVIDIA GeForce RTX 4070"));
    }

    #[test]
    fn nvidia_probe_output_summarizes_gpu_list_and_includes_decoder_encoder_details() {
        let capabilities = ersatzrs_ffmpeg::FfmpegCapabilityReport {
            nvidia_gpu_list: Some("GPU #0: NVIDIA GeForce RTX 4070\nunrelated line".to_owned()),
            nvidia_decoders: vec![
                ersatzrs_ffmpeg::NvidiaDecoderCapability::new("h264", 8),
                ersatzrs_ffmpeg::NvidiaDecoderCapability::new("hevc", 10),
            ],
            hardware_decode_denials: Vec::new(),
            nvidia_encoders: vec![ersatzrs_ffmpeg::NvidiaEncoderCapability::new(
                "h264_nvenc",
                std::collections::BTreeSet::from(["yuv420p".to_owned()]),
                std::collections::BTreeSet::from(["high".to_owned()]),
            )],
            ..Default::default()
        };

        let output = nvidia_probe_output(&capabilities).expect("nvidia probe output");

        // GPU list is summarized: noise is filtered out, the device line stays.
        assert!(output.contains("gpu list:"));
        assert!(output.contains("GPU #0: NVIDIA GeForce RTX 4070"));
        assert!(!output.contains("unrelated line"));

        // Decoder detail (codec + bit depth) is now surfaced, mirroring VideoToolbox.
        assert!(output.contains("decoders:"));
        assert!(output.contains("h264 8-bit"));
        assert!(output.contains("hevc 10-bit"));

        // Encoder detail (pixel formats + profiles) is surfaced.
        assert!(output.contains("encoders:"));
        assert!(output.contains("h264_nvenc"));
        assert!(output.contains("yuv420p"));
        assert!(output.contains("high"));
    }

    #[test]
    fn nvidia_probe_output_is_none_without_probe_data() {
        let capabilities = ersatzrs_ffmpeg::FfmpegCapabilityReport::default();
        assert!(nvidia_probe_output(&capabilities).is_none());
    }

    #[test]
    fn mpeg_troubleshooting_mode_mapping_is_narrow() {
        assert_eq!(normalize_mpeg_mode(""), "internal-legacy");
        assert_eq!(normalize_mpeg_mode("public-ts"), "public-ts");
        assert!(is_supported_mpeg_mode("internal-legacy"));
        assert!(is_supported_mpeg_mode("public-ts"));
        assert!(!is_supported_mpeg_mode("hls"));
    }

    #[test]
    fn mpeg_not_available_report_does_not_capture_samples() {
        let report = mpeg_report_not_available(
            "7",
            "internal-legacy",
            20,
            false,
            None,
            "tsp not found".to_owned(),
        );

        assert_eq!(report.status, MpegTroubleshootingStatus::NotAvailable);
        assert_eq!(report.sample_bytes, 0);
        assert!(report.sample_path.is_none());
        assert!(report.analysis.is_none());
        assert_eq!(report.errors, vec!["tsp not found"]);
    }

    #[tokio::test]
    async fn local_mpeg_sample_url_uses_the_bound_listener_for_each_mode() {
        use crate::host::state::{AppState, IptvAccessTokenVerifier};
        let state = AppState::for_tests(IptvAccessTokenVerifier::disabled());
        assert!(local_mpeg_sample_url(&state, "7", "internal-legacy").is_err());
        assert!(local_mpeg_sample_url(&state, "7", "public-ts").is_err());
        state
            .internal_http_addr
            .set("[::1]:19001".parse().unwrap())
            .unwrap();
        state
            .streaming_http_addr
            .set("127.0.0.1:19002".parse().unwrap())
            .unwrap();
        assert_eq!(
            local_mpeg_sample_url(&state, "7", "internal-legacy").unwrap(),
            "http://[::1]:19001/ffmpeg/stream/7"
        );
        assert_eq!(
            local_mpeg_sample_url(&state, "7", "public-ts").unwrap(),
            "http://127.0.0.1:19002/iptv/channel/7.ts"
        );
    }

    #[cfg(feature = "ssr")]
    #[tokio::test]
    async fn mpeg_report_archive_writes_redacted_summary_and_analysis_artifacts() {
        let temp = tempfile::tempdir().expect("tempdir");
        let report = MpegTroubleshootingReport {
            run_id: "run-1".to_owned(),
            channel_number: "7".to_owned(),
            mode: "public-ts".to_owned(),
            status: MpegTroubleshootingStatus::Completed,
            summary: "completed".to_owned(),
            duration_seconds: 20,
            sample_bytes: 1024,
            retain_raw_sample: false,
            sample_path: None,
            archive_path: temp.path().display().to_string(),
            sample_url: "http://127.0.0.1/iptv/channel/7.ts?access_token=<redacted>".to_owned(),
            tsduck_version: Some("tsp version 3.44".to_owned()),
            analysis: Some(MpegTsduckAnalysis {
                tsp_command: MpegTsduckCommandSpec {
                    program: "tsp".to_owned(),
                    args: vec!["--realtime=off".to_owned()],
                },
                tsbitrate_command: MpegTsduckCommandSpec {
                    program: "tsbitrate".to_owned(),
                    args: vec!["sample.ts".to_owned()],
                },
                status: MpegTsduckAnalysisStatus::Completed,
                summary: "analysis completed".to_owned(),
                stability_summary: "Stable for 1/1 sampled seconds.".to_owned(),
                stability_samples: vec![MpegTsduckStabilitySample {
                    second_start: 0,
                    second_end: 1,
                    level: MpegTsduckStabilityLevel::Stable,
                    packet_count: 10,
                    bitrate_bps: 15_040,
                    note: "TSDuck reported no continuity or PCR findings.".to_owned(),
                }],
                analysis_json_excerpt: "{}".to_owned(),
                continuity_findings: vec!["PID 256 discontinuity".to_owned()],
                pcr_findings: vec!["PCR warning".to_owned()],
                services: vec!["1: ErsatzRS".to_owned()],
                pids: vec!["256: video".to_owned()],
                bitrate_bps: Some(123_456),
                stdout_excerpt: "stdout access_token=<redacted>".to_owned(),
                stderr_excerpt: "stderr token=<redacted>".to_owned(),
            }),
            errors: Vec::new(),
        };

        write_mpeg_report(temp.path(), &report)
            .await
            .expect("report archive");

        let summary = tokio::fs::read_to_string(temp.path().join("summary.json"))
            .await
            .expect("summary");
        assert!(summary.contains("\"sample_url\""));
        assert!(summary.contains("access_token=<redacted>"));
        assert!(!summary.contains("access_token=secret"));
        assert_eq!(
            tokio::fs::read_to_string(temp.path().join("continuity.log"))
                .await
                .expect("continuity"),
            "PID 256 discontinuity"
        );
        assert_eq!(
            tokio::fs::read_to_string(temp.path().join("pcrverify.log"))
                .await
                .expect("pcr"),
            "PCR warning"
        );
        assert_eq!(
            tokio::fs::read_to_string(temp.path().join("tsbitrate.txt"))
                .await
                .expect("bitrate"),
            "123456"
        );
        let stability = tokio::fs::read_to_string(temp.path().join("stability.json"))
            .await
            .expect("stability");
        assert!(stability.contains("\"level\": \"Stable\""));
        assert!(stability.contains("\"packet_count\": 10"));
        assert!(temp.path().join("commands.json").exists());
        assert!(!temp.path().join("sample.ts").exists());
    }

    #[test]
    fn troubleshooting_environment_removes_uri_credentials() {
        let environment = troubleshooting_environment_from([
            (
                "ERSATZRS_TEST_URI".into(),
                "https://fixture-user:fixture-password@example.invalid/search?token=fixture-token&index=media".into(),
            ),
            ("ERSATZRS_TEST_PATH".into(), "relative/path with spaces".into()),
        ]);
        let uri = &environment["ERSATZRS_TEST_URI"];
        for credential in ["fixture-user", "fixture-password", "fixture-token"] {
            assert!(!uri.contains(credential), "synthetic credential leaked");
        }
        assert!(uri.contains("example.invalid/search"));
        assert!(uri.contains("index=media"));
        assert_eq!(
            environment["ERSATZRS_TEST_PATH"],
            "relative/path with spaces"
        );
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
            versions: crate::host::troubleshooting::TroubleshootingVersions::detected(
                Some("0.9.4-test"),
                &ersatzrs_ffmpeg::version::FfmpegVersion {
                    major: 9,
                    minor: 2,
                    patch: 3,
                },
            ),
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
                teletext_decoder: TeletextDecoderSummary::default(),
                default_ffmpeg_profile_id: 0,
                save_reports: false,
                use_embedded_subtitles: true,
                extract_embedded_subtitles: false,
                preferred_audio_language_code: "eng".to_owned(),
                hls_segmenter_idle_timeout: 60,
                work_ahead_segmenter_limit: 1,
                active_transcode_limit: 2,
                active_transcode_capacity:
                    crate::admin::server_fns::settings::ActiveTranscodeCapacityStatus::default(),
                initial_segment_count: 1,
                hls_direct_output_format: 2,
                default_mpeg_ts_script: "default".to_owned(),
            },
            channels: Vec::new(),
            ffmpeg_profiles: Vec::new(),
        };

        let json = serde_json::to_value(&info).expect("general info serializes");

        assert_eq!(json["Version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(json["NextVersion"], "0.9.4-test");
        assert_eq!(json["FFmpegVersion"], "9.2.3");
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
    fn sequential_schedule_validation_accepts_date_overrides_and_rejects_missing_fields() {
        let yaml = r#"
content:
  - collection: Cartoons
    key: cartoons
    order: shuffle
playout:
  - count: "1"
    content: cartoons
schedules:
  - name: Holiday
    start_date: 12-25
    end_date: 2026-12-25
    priority: 10
    playout:
      - count: "1"
        content: cartoons
"#;
        let json = sequential_schedule_json_from_yaml(yaml).expect("valid YAML");
        let messages = validate_sequential_schedule_json(&json, false).expect("validate schema");
        assert!(messages.is_empty(), "{messages:?}");

        let mut invalid = json;
        let schedule = invalid["schedules"][0]
            .as_object_mut()
            .expect("schedule object");
        schedule.remove("end_date");
        let messages =
            validate_sequential_schedule_json(&invalid, false).expect("validate missing date");
        assert!(messages.iter().any(|message| message.contains("end_date")));
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

#[cfg(all(test, feature = "ssr"))]
mod default_engine_tests {
    use super::*;

    #[test]
    fn playback_context_uses_next_for_media_and_the_owning_engine_for_channels() {
        use ersatzrs_core::repositories::{PlaybackChannelSummaryAdmin, PlaybackMediaSummaryAdmin};
        for engine in [
            ersatzrs_domain::StreamingEngine::Legacy,
            ersatzrs_domain::StreamingEngine::Next,
        ] {
            let mut read = ersatzrs_app::queries::PlaybackTroubleshootingReadModel {
                data: ersatzrs_core::repositories::PlaybackTroubleshootingAdmin::default(),
                stream_selectors: Vec::new(),
                music_video_credits_templates: Vec::new(),
            };
            read.data.channel = Some(PlaybackChannelSummaryAdmin {
                name: "Channel".to_owned(),
                ffmpeg_profile_id: 7,
                streaming_engine: engine,
            });
            let context = playback_context_from_admin(read, 0, 10, "", 0).unwrap();
            assert_eq!(context.streaming_engine, engine);
            assert_eq!(context.channel_id, 10);
        }
        let mut read = ersatzrs_app::queries::PlaybackTroubleshootingReadModel {
            data: ersatzrs_core::repositories::PlaybackTroubleshootingAdmin::default(),
            stream_selectors: Vec::new(),
            music_video_credits_templates: Vec::new(),
        };
        read.data.media = Some(PlaybackMediaSummaryAdmin {
            title: "Media".to_owned(),
            ..PlaybackMediaSummaryAdmin::default()
        });
        let context = playback_context_from_admin(read, 12, 0, "Dutch", 5).unwrap();
        assert_eq!(
            context.streaming_engine,
            ersatzrs_domain::StreamingEngine::Next
        );
        assert_eq!(context.preferred_audio_title, "Dutch");
        assert_eq!(context.selected_subtitle_id, 5);
    }
}
