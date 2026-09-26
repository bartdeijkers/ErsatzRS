use async_trait::async_trait;
use chrono::{DateTime, NaiveDateTime, TimeZone, Timelike, Utc};
use ersatzrs_app::{
    AdminTroubleshootingRuntime, AdminTroubleshootingRuntimeError, PlaybackPreviewRequest,
};
use ersatzrs_core::{
    path_replacement::{replace_path, PathPlatform, PathReplacement},
    repositories::{
        AdminSettingsRepository as _, TroubleshootingPreviewMediaAdmin as PreviewMediaItem,
        TroubleshootingRepository as _,
    },
};
use ersatzrs_domain::{Channel, ChannelSubtitleMode, HardwareAccelerationKind, StreamingMode};
use ersatzrs_ffmpeg::{
    FfmpegCommand, FfmpegRuntimeReport, FrameSize, InputFile, OutputFormat, WatermarkOpacityClock,
};
use ersatzrs_infra::{
    db::DbPool,
    dynamic_text::{shutdown_dynamic_text_controllers, DynamicTextController},
    ffmpeg_config::playback_language_preferences,
    file_system_layout::{
        channel_stream_selectors_folder, list_channel_stream_selectors,
        transcode_troubleshooting_folder, TRANSCODE_TROUBLESHOOTING_CHANNEL,
    },
    hls_worker::build_hls_command_with_opacity_clock,
    playback::{
        enrich_source_video_stream_for_playback, load_channel_target_frame_rate,
        load_playback_profile, load_source_audio_streams, load_source_video_stream,
        load_subtitles_for_media_item, render_playback_graphics_overlays,
        resolve_playback_graphics_by_ids, resolve_playback_watermarks_by_ids,
        select_custom_playback_streams, select_playback_audio_stream_index,
        select_playback_graphics, select_playback_subtitle, select_playback_watermarks,
        subtitle_input_for_stream, watermark_overlay_for_stream, CustomStreamSelection,
        PlaybackGraphicsArtifacts, PlaybackGraphicsRenderContext, PlaybackProfile,
        PlaybackSubtitle,
    },
    repositories::Repositories,
    subtitles::rebase_burned_text_subtitle_input,
};
use std::path::{Path as FsPath, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, OnceLock};
use std::time::Duration;
use tokio::process::Command;
use tokio::sync::{Mutex, OwnedMutexGuard, RwLock};

use crate::transport::iptv::stream;

type ApiResult<T> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

#[derive(Clone)]
pub struct NativeAdminTroubleshootingRuntime {
    pool: DbPool,
    repositories: Arc<Repositories>,
    ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
    next_version: Arc<OnceLock<String>>,
    preview_lock: Arc<Mutex<()>>,
}

impl NativeAdminTroubleshootingRuntime {
    pub fn new(
        pool: DbPool,
        repositories: Arc<Repositories>,
        ffmpeg_runtime: Arc<RwLock<FfmpegRuntimeReport>>,
        next_version: Arc<OnceLock<String>>,
    ) -> Self {
        Self {
            pool,
            repositories,
            ffmpeg_runtime,
            next_version,
            preview_lock: Arc::default(),
        }
    }
}

/// Stable diagnostic fields shared by the General report and preview archive.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct TroubleshootingVersions {
    version: &'static str,
    next_version: String,
    #[serde(rename = "FFmpegVersion")]
    ffmpeg_version: String,
}

impl TroubleshootingVersions {
    pub(crate) fn detected(
        next: Option<&str>,
        ffmpeg: &ersatzrs_ffmpeg::version::FfmpegVersion,
    ) -> Self {
        Self {
            version: env!("CARGO_PKG_VERSION"),
            next_version: next.unwrap_or("unknown").to_owned(),
            ffmpeg_version: ffmpeg.to_string(),
        }
    }
}

#[derive(Debug)]
struct PlaybackPreviewQuery {
    media_item: Option<i64>,
    channel: Option<i64>,
    ffmpeg_profile: Option<i64>,
    streaming_mode: Option<String>,
    watermark: Vec<i64>,
    graphics_element: Vec<i64>,
    stream_selector: Option<String>,
    preferred_audio_title: Option<String>,
    subtitle_id: Option<i64>,
    seek_seconds: Option<i64>,
    start: Option<String>,
}

impl From<PlaybackPreviewRequest> for PlaybackPreviewQuery {
    fn from(request: PlaybackPreviewRequest) -> Self {
        Self {
            media_item: request.media_item_id,
            channel: request.channel_id,
            ffmpeg_profile: request.ffmpeg_profile_id,
            streaming_mode: request.streaming_mode,
            watermark: request.watermark_ids,
            graphics_element: request.graphics_element_ids,
            stream_selector: request.stream_selector,
            preferred_audio_title: request.preferred_audio_title,
            subtitle_id: request.subtitle_id,
            seek_seconds: request.seek_seconds,
            start: request.start,
        }
    }
}

#[derive(Debug)]
struct PreviewNotFound(&'static str);

impl std::fmt::Display for PreviewNotFound {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}

impl std::error::Error for PreviewNotFound {}

#[async_trait]
impl AdminTroubleshootingRuntime for NativeAdminTroubleshootingRuntime {
    async fn list_stream_selectors(&self) -> Result<Vec<String>, AdminTroubleshootingRuntimeError> {
        Ok(list_channel_stream_selectors().await)
    }

    async fn start_playback_preview(
        &self,
        request: PlaybackPreviewRequest,
    ) -> Result<(), AdminTroubleshootingRuntimeError> {
        let ownership = Arc::clone(&self.preview_lock)
            .try_lock_owned()
            .map_err(|_| {
                AdminTroubleshootingRuntimeError::NotFound(
                    "troubleshooting playback is already running".to_owned(),
                )
            })?;
        start_playback_preview(self, &request.into(), ownership)
            .await
            .map_err(map_runtime_error)
    }

    async fn build_troubleshooting_archive(
        &self,
    ) -> Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError> {
        build_troubleshooting_archive()
            .await
            .map_err(map_runtime_error)
    }

    async fn build_media_sample_archive(
        &self,
        media_item_id: i64,
    ) -> Result<Option<Vec<u8>>, AdminTroubleshootingRuntimeError> {
        build_media_sample_archive(self, media_item_id)
            .await
            .map_err(map_runtime_error)
    }
}

#[allow(clippy::needless_pass_by_value)]
fn map_runtime_error(
    error: Box<dyn std::error::Error + Send + Sync>,
) -> AdminTroubleshootingRuntimeError {
    if error.downcast_ref::<PreviewNotFound>().is_some() {
        AdminTroubleshootingRuntimeError::NotFound(error.to_string())
    } else {
        AdminTroubleshootingRuntimeError::unavailable(error.to_string())
    }
}

#[allow(clippy::too_many_lines)]
async fn start_playback_preview(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    ownership: OwnedMutexGuard<()>,
) -> ApiResult<()> {
    if query.channel.is_some_and(|id| id > 0) {
        return start_channel_playback_preview(state, query, ownership).await;
    }

    let media_item_id = query
        .media_item
        .filter(|id| *id > 0)
        .ok_or(PreviewNotFound(
            "media item troubleshooting preview requires mediaItem",
        ))?;
    let ffmpeg_profile_id = query
        .ffmpeg_profile
        .filter(|id| *id > 0)
        .ok_or(PreviewNotFound(
            "playback troubleshooting preview requires ffmpegProfile",
        ))?;
    let media = state
        .repositories
        .troubleshooting
        .load_preview_media_admin(media_item_id)
        .await?
        .ok_or(PreviewNotFound("media item does not exist"))?;
    if media.path.trim().is_empty() {
        return Err(PreviewNotFound("media item has no playable path").into());
    }
    let playback_profile = load_playback_profile(&state.pool, ffmpeg_profile_id)
        .await?
        .ok_or(PreviewNotFound("ffmpeg profile does not exist"))?;

    let working_dir = troubleshooting_folder();
    prepare_troubleshooting_folder(&working_dir).await?;
    write_preview_artifacts(state, query, &media, &playback_profile, &working_dir).await?;

    let path_replacements =
        stream::load_path_replacements(&state.repositories.media_source).await?;
    let media_path = apply_unix_path_replacements(&path_replacements, &media.path);
    let output_format = preview_output_format(query.streaming_mode.as_deref());
    let source_stream = load_source_video_stream(&state.pool, media.media_version_id).await?;
    let source_audio_streams =
        load_source_audio_streams(&state.pool, media.media_version_id).await?;
    let source_stream = enrich_source_video_stream_for_playback(
        source_stream,
        playback_profile.profile.hardware_acceleration,
        &media_path,
    )
    .await;
    let seek_seconds = preview_seek_seconds(query.seek_seconds, media.duration_seconds);
    let program_duration = Duration::from_secs_f64(preview_duration_seconds(
        media.duration_seconds,
        seek_seconds,
    ));
    let selector_channel =
        preview_selector_channel(query, StreamingMode::HttpLiveStreamingSegmenter);
    let language_preferences = playback_language_preferences(&state.pool).await?;
    let preferred_audio_language = language_preferences.audio_language(
        None,
        selector_channel.preferred_audio_language_code.as_deref(),
        selector_channel.preferred_audio_title.as_deref(),
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    let (selected_audio_stream_index, subtitle_input) =
        preview_selected_streams(PreviewSelectedStreamsRequest {
            pool: &state.pool,
            channel: &selector_channel,
            media_version_id: media.media_version_id,
            media_item_id,
            media_path: &media_path,
            output_format,
            subtitle_id: query.subtitle_id,
            seek_seconds,
            content_start_time: Utc::now(),
        })
        .await?;
    let selected_audio_stream_index = match selected_audio_stream_index {
        Some(index) => Some(index),
        None => {
            ersatzrs_infra::audio_script::select(
                &state.pool,
                &selector_channel,
                media_item_id,
                media.media_version_id,
                ersatzrs_infra::audio_script::AudioScriptPreferences {
                    language: preferred_audio_language.as_deref(),
                    title: selector_channel.preferred_audio_title.as_deref(),
                    streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
                },
                &source_audio_streams,
            )
            .await
        }
    };
    let selected_audio_stream_index = selected_audio_stream_index.or_else(|| {
        select_playback_audio_stream_index(
            &source_audio_streams,
            preferred_audio_language.as_deref(),
            selector_channel.preferred_audio_title.as_deref(),
        )
    });
    let watermark_inputs =
        preview_watermark_inputs(&state.pool, query.watermark.clone(), program_duration).await;
    let PlaybackGraphicsArtifacts {
        overlays: graphics_overlays,
        dynamic_text_controllers,
    } = preview_graphics_overlays(PreviewGraphicsRequest {
        pool: &state.pool,
        graphics_element_ids: query.graphics_element.clone(),
        media: &media,
        media_path: &media_path,
        playback_profile: &playback_profile,
        source_stream: source_stream.clone(),
        seek_seconds,
        program_duration,
        working_dir: &working_dir,
    })
    .await;
    let runtime_capabilities = state.ffmpeg_runtime.read().await.capabilities.clone();
    let no_subtitle = Option::<PlaybackSubtitle>::None;
    let metadata = stream::playback_output_metadata(
        Some("ErsatzTV"),
        Some(&selector_channel.name),
        preferred_audio_language.as_deref(),
        &source_audio_streams,
        no_subtitle.as_ref(),
        selected_audio_stream_index,
    );
    let spawn_time = Utc::now();
    let opacity_clock = watermark_opacity_clock_at_spawn(
        spawn_time,
        duration_from_seconds(seek_seconds),
        Some(Duration::from_secs_f64(media.duration_seconds.max(0.0))),
        spawn_time,
    );
    let command = build_hls_command_with_opacity_clock(
        &media_path,
        &working_dir,
        output_format,
        true,
        true,
        false,
        duration_from_seconds(seek_seconds),
        Some(program_duration),
        None,
        &playback_profile,
        None,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        selected_audio_stream_index,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        Some(opacity_clock),
    )?;

    write_ffmpeg_arguments(&working_dir, &command).await?;
    spawn_preview_ffmpeg(&working_dir, command, dynamic_text_controllers, ownership)?;
    wait_for_preview_output(&working_dir, output_format).await?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn start_channel_playback_preview(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    ownership: OwnedMutexGuard<()>,
) -> ApiResult<()> {
    let channel_id = query.channel.filter(|id| *id > 0).ok_or(PreviewNotFound(
        "channel troubleshooting preview requires channel",
    ))?;
    let channel = state
        .repositories
        .troubleshooting
        .load_preview_channel_admin(channel_id)
        .await?
        .ok_or(PreviewNotFound("channel does not exist"))?;
    if !channel.is_enabled {
        return Err(PreviewNotFound("channel is disabled").into());
    }
    let start_time = parse_preview_start(query.start.as_deref()).unwrap_or_else(Utc::now);
    let active = stream::load_active_media_path(&state.pool, channel.id, start_time)
        .await?
        .ok_or(PreviewNotFound("channel has no active playout item"))?;
    let ffmpeg_profile_id = query
        .ffmpeg_profile
        .filter(|id| *id > 0)
        .unwrap_or(channel.ffmpeg_profile_id);
    let playback_profile = load_playback_profile(&state.pool, ffmpeg_profile_id)
        .await?
        .ok_or(PreviewNotFound("ffmpeg profile does not exist"))?;

    let media = state
        .repositories
        .troubleshooting
        .load_preview_media_admin(active.media_item_id)
        .await?
        .ok_or(PreviewNotFound("active media item does not exist"))?;
    let working_dir = troubleshooting_folder();
    prepare_troubleshooting_folder(&working_dir).await?;
    write_preview_artifacts(state, query, &media, &playback_profile, &working_dir).await?;

    let path_replacements =
        stream::load_path_replacements(&state.repositories.media_source).await?;
    let media_path = apply_unix_path_replacements(&path_replacements, &active.media_path);
    let output_format = preview_output_format(query.streaming_mode.as_deref());
    let source_stream = load_source_video_stream(&state.pool, active.media_version_id).await?;
    let source_audio_streams =
        load_source_audio_streams(&state.pool, active.media_version_id).await?;
    let source_stream = enrich_source_video_stream_for_playback(
        source_stream,
        playback_profile.profile.hardware_acceleration,
        &media_path,
    )
    .await;
    let target_frame_rate = if playback_profile.profile.normalize_framerate {
        load_channel_target_frame_rate(&state.pool, channel.id).await?
    } else {
        None
    };
    let start_position = active.content_seek_position(start_time);
    let start_seconds = duration_as_i64_seconds(start_position);
    let program_duration = active_program_duration(&active)
        .unwrap_or_else(|| Duration::from_secs_f64(preview_duration_seconds(0.0, 0)));
    let subtitle_mode = active.subtitle_mode.unwrap_or(channel.subtitle_mode);
    let preferred_subtitle_language = active
        .preferred_subtitle_language_code
        .as_deref()
        .or(channel.preferred_subtitle_language_code.as_deref());
    let language_preferences = playback_language_preferences(&state.pool).await?;
    let preferred_audio_title = active
        .preferred_audio_title
        .as_deref()
        .or(channel.preferred_audio_title.as_deref());
    let preferred_audio_language = language_preferences.audio_language(
        active.preferred_audio_language_code.as_deref(),
        channel.preferred_audio_language_code.as_deref(),
        preferred_audio_title,
        StreamingMode::HttpLiveStreamingSegmenter,
    );
    let mut selector_channel = channel.clone();
    if let Some(selector) = query
        .stream_selector
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        selector_channel.stream_selector = Some(selector.to_owned());
    }
    let content_start_time =
        parse_sqlite_utc_timestamp(&active.playout_start).unwrap_or(start_time);
    let (selected_audio_stream_index, subtitle_input) = if selector_channel
        .stream_selector
        .as_deref()
        .is_some_and(|selector| !selector.trim().is_empty())
    {
        match select_custom_playback_streams(
            &state.pool,
            active.media_version_id,
            active.media_item_id,
            &selector_channel,
            content_start_time,
            StreamingMode::HttpLiveStreamingSegmenter,
        )
        .await
        {
            Ok(Some(selection)) => (
                selection.audio_stream_index,
                subtitle_input_from_optional_subtitle(
                    &state.pool,
                    selection.subtitle,
                    &media_path,
                    output_format,
                    start_seconds,
                )
                .await?,
            ),
            Ok(_) => {
                let subtitle_input = fallback_channel_subtitle_input(
                    &state.pool,
                    active.media_item_id,
                    preferred_subtitle_language,
                    subtitle_mode,
                    &media_path,
                    output_format,
                )
                .await?;
                (
                    None,
                    rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await,
                )
            }
            Err(error) => {
                tracing::warn!(%error, media_item_id = active.media_item_id, "failed to execute troubleshooting channel stream selector");
                let subtitle_input = fallback_channel_subtitle_input(
                    &state.pool,
                    active.media_item_id,
                    preferred_subtitle_language,
                    subtitle_mode,
                    &media_path,
                    output_format,
                )
                .await?;
                (
                    None,
                    rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await,
                )
            }
        }
    } else {
        let subtitle_input = fallback_channel_subtitle_input(
            &state.pool,
            active.media_item_id,
            preferred_subtitle_language,
            subtitle_mode,
            &media_path,
            output_format,
        )
        .await?;
        (
            None,
            rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await,
        )
    };
    let selected_audio_stream_index =
        if selected_audio_stream_index.is_none() && !active.has_pinned_stream_selection {
            ersatzrs_infra::audio_script::select(
                &state.pool,
                &channel,
                active.media_item_id,
                active.media_version_id,
                ersatzrs_infra::audio_script::AudioScriptPreferences {
                    language: preferred_audio_language.as_deref(),
                    title: preferred_audio_title,
                    streaming_mode: StreamingMode::HttpLiveStreamingSegmenter,
                },
                &source_audio_streams,
            )
            .await
        } else {
            selected_audio_stream_index
        };
    let selected_audio_stream_index = selected_audio_stream_index.or_else(|| {
        select_playback_audio_stream_index(
            &source_audio_streams,
            preferred_audio_language.as_deref(),
            preferred_audio_title,
        )
    });
    let watermark_inputs = preview_channel_watermark_inputs(
        &state.pool,
        &channel,
        active.playout_item_id,
        active.disable_watermarks,
        start_time,
        program_duration,
    )
    .await;
    let PlaybackGraphicsArtifacts {
        overlays: graphics_overlays,
        dynamic_text_controllers,
    } = preview_channel_graphics_overlays(PreviewChannelGraphicsRequest {
        pool: &state.pool,
        channel: &channel,
        active: &active,
        media_path: &media_path,
        playback_profile: &playback_profile,
        target_frame_rate,
        source_stream: source_stream.clone(),
        start_position,
        program_duration,
        start_time,
        working_dir: &working_dir,
    })
    .await;
    let runtime_capabilities = state.ffmpeg_runtime.read().await.capabilities.clone();
    let no_subtitle = Option::<PlaybackSubtitle>::None;
    let metadata = stream::playback_output_metadata(
        Some("ErsatzTV"),
        Some(&channel.name),
        preferred_audio_language.as_deref(),
        &source_audio_streams,
        no_subtitle.as_ref(),
        selected_audio_stream_index,
    );
    let opacity_clock = watermark_opacity_clock_at_spawn(
        start_time,
        start_position,
        active.content_duration,
        content_start_time,
    );
    let command = build_hls_command_with_opacity_clock(
        &media_path,
        &working_dir,
        output_format,
        true,
        true,
        false,
        start_position,
        Some(program_duration),
        None,
        &playback_profile,
        target_frame_rate,
        metadata,
        runtime_capabilities,
        source_stream,
        source_audio_streams,
        selected_audio_stream_index,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
        Some(opacity_clock),
    )?;

    write_ffmpeg_arguments(&working_dir, &command).await?;
    spawn_preview_ffmpeg(&working_dir, command, dynamic_text_controllers, ownership)?;
    wait_for_preview_output(&working_dir, output_format).await?;
    Ok(())
}

fn parse_preview_start(value: Option<&str>) -> Option<DateTime<Utc>> {
    let value = value?.trim();
    if value.is_empty() {
        return None;
    }
    DateTime::parse_from_rfc3339(value)
        .map(|parsed| parsed.with_timezone(&Utc))
        .ok()
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%dT%H:%M")
                .ok()
                .map(|parsed| Utc.from_utc_datetime(&parsed))
        })
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S")
                .ok()
                .map(|parsed| Utc.from_utc_datetime(&parsed))
        })
}

fn active_program_duration(active: &stream::ActivePlayoutItem) -> Option<Duration> {
    let start = parse_sqlite_utc_timestamp(&active.playout_start)?;
    let finish = parse_sqlite_utc_timestamp(&active.playout_finish)?;
    (finish - start).to_std().ok()
}

fn parse_sqlite_utc_timestamp(value: &str) -> Option<DateTime<Utc>> {
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(Utc.from_utc_datetime(&parsed));
        }
    }
    None
}

fn duration_from_seconds(seconds: i64) -> Duration {
    u64::try_from(seconds).map_or(Duration::ZERO, Duration::from_secs)
}

fn duration_as_i64_seconds(duration: Duration) -> i64 {
    i64::try_from(duration.as_secs()).unwrap_or(i64::MAX)
}

fn watermark_opacity_clock_at_spawn(
    spawn_time: DateTime<Utc>,
    content_seek: Duration,
    content_duration: Option<Duration>,
    channel_start_time: DateTime<Utc>,
) -> WatermarkOpacityClock {
    let time = spawn_time.time();
    let time_of_day_seconds = f64::from(time.num_seconds_from_midnight())
        + f64::from(time.nanosecond()) / 1_000_000_000.0;
    let channel_seconds = (spawn_time - channel_start_time)
        .to_std()
        .map_or(0.0, |duration| duration.as_secs_f64());
    WatermarkOpacityClock::new(
        time_of_day_seconds,
        content_seek.as_secs_f64(),
        content_duration.map(|duration| duration.as_secs_f64()),
        channel_seconds,
    )
}

async fn prepare_troubleshooting_folder(path: &PathBuf) -> ApiResult<()> {
    if tokio::fs::try_exists(path).await? {
        tokio::fs::remove_dir_all(path).await?;
    }
    tokio::fs::create_dir_all(path).await?;
    Ok(())
}

async fn write_preview_artifacts(
    state: &NativeAdminTroubleshootingRuntime,
    query: &PlaybackPreviewQuery,
    media: &PreviewMediaItem,
    playback_profile: &PlaybackProfile,
    working_dir: &FsPath,
) -> ApiResult<()> {
    let media_info = serde_json::json!({
        "Id": media.media_item_id,
        "Title": serde_json::Value::Null,
        "Kind": media.kind,
        "Duration": media.duration_seconds,
        "Path": media.path,
    });
    tokio::fs::write(
        working_dir.join("media_info.json"),
        serde_json::to_vec_pretty(&media_info)?,
    )
    .await?;

    let runtime = state.ffmpeg_runtime.read().await.clone();
    let mut troubleshooting_info = serde_json::to_value(TroubleshootingVersions::detected(
        state.next_version.get().map(String::as_str),
        &runtime.ffmpeg.version,
    ))?;
    let preview_details = serde_json::json!({
        "FFmpegProfile": {
            "Id": playback_profile.profile.id,
            "Name": playback_profile.profile.name,
            "HardwareAcceleration": format!("{:?}", playback_profile.profile.hardware_acceleration),
            "Resolution": {
                "Width": playback_profile.resolution.width,
                "Height": playback_profile.resolution.height,
            }
        },
        "FFmpeg": {
            "Version": runtime.ffmpeg.version.to_string(),
            "Path": runtime.ffmpeg.path,
        },
        "FFprobe": {
            "Version": runtime.ffprobe.version.to_string(),
            "Path": runtime.ffprobe.path,
        },
        "SelectedWatermarks": query.watermark,
    });
    if let (Some(info), Some(details)) = (
        troubleshooting_info.as_object_mut(),
        preview_details.as_object(),
    ) {
        info.extend(details.clone());
    }
    tokio::fs::write(
        working_dir.join("troubleshooting_info.json"),
        serde_json::to_vec_pretty(&troubleshooting_info)?,
    )
    .await?;

    copy_stream_selector(query.stream_selector.as_deref(), working_dir).await?;
    write_capability_files(
        playback_profile.profile.hardware_acceleration,
        &runtime.capabilities,
        working_dir,
    )
    .await?;
    Ok(())
}

async fn copy_stream_selector(selector: Option<&str>, working_dir: &FsPath) -> ApiResult<()> {
    let Some(selector) = selector.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    if selector.contains("..") || selector.contains('/') || selector.contains('\\') {
        return Ok(());
    }
    let source = channel_stream_selectors_folder().join(selector);
    if tokio::fs::try_exists(&source).await? {
        tokio::fs::copy(source, working_dir.join("stream-selector.yml")).await?;
    }
    Ok(())
}

async fn write_capability_files(
    acceleration: HardwareAccelerationKind,
    report: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
    working_dir: &FsPath,
) -> ApiResult<()> {
    match acceleration {
        HardwareAccelerationKind::Qsv => {
            write_capability_file(working_dir, "capabilities_qsv.txt", report).await?;
            if cfg!(target_os = "linux") {
                write_capability_file(working_dir, "capabilities_vaapi.txt", report).await?;
            }
        }
        HardwareAccelerationKind::Vaapi => {
            write_capability_file(working_dir, "capabilities_vaapi.txt", report).await?;
        }
        HardwareAccelerationKind::Nvenc => {
            write_capability_file(working_dir, "capabilities_nvidia.txt", report).await?;
        }
        HardwareAccelerationKind::VideoToolbox => {
            write_capability_file(working_dir, "capabilities_videotoolbox.txt", report).await?;
        }
        HardwareAccelerationKind::Amf
        | HardwareAccelerationKind::V4l2m2m
        | HardwareAccelerationKind::Rkmpp
        | HardwareAccelerationKind::None => {}
    }
    Ok(())
}

async fn write_capability_file(
    working_dir: &FsPath,
    name: &str,
    report: &ersatzrs_ffmpeg::FfmpegCapabilityReport,
) -> ApiResult<()> {
    let mut text = format!(
        "hwaccels:\n{}\n\nencoders:\n{}\n\ndecoders:\n{}\n\nfilters:\n{}\n",
        report
            .hwaccels
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
        report
            .encoders
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
        report
            .decoders
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
        report
            .filters
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
    );
    if let Some(output) = report
        .nvidia_gpu_list
        .as_deref()
        .filter(|value| !value.trim().is_empty())
    {
        text.push_str("\n\nnvidia_gpu_list:\n");
        text.push_str(output);
        text.push('\n');
    }
    if !report.nvidia_encoders.is_empty() {
        text.push_str("\n\nnvidia_encoder_details:\n");
        for encoder in &report.nvidia_encoders {
            text.push_str("  ");
            text.push_str(&encoder.encoder);
            text.push('\n');
            append_probe_set(&mut text, "pixel_formats", &encoder.pixel_formats);
            append_probe_set(&mut text, "profiles", &encoder.profiles);
        }
    }
    if !report.nvidia_decoders.is_empty() {
        text.push_str("\n\nnvidia_decoder_details:\n");
        for decoder in &report.nvidia_decoders {
            text.push_str("  ");
            text.push_str(&decoder.codec);
            text.push(' ');
            text.push_str(&decoder.bit_depth.to_string());
            text.push_str("-bit\n");
        }
    }
    if !report.videotoolbox_decoders.is_empty() || !report.videotoolbox_encoders.is_empty() {
        text.push_str("\n\nvideotoolbox_framework:\n");
        append_probe_set(&mut text, "decoders", &report.videotoolbox_decoders);
        append_probe_set(&mut text, "encoders", &report.videotoolbox_encoders);
    }
    tokio::fs::write(working_dir.join(name), text).await?;
    Ok(())
}

fn append_probe_set(text: &mut String, heading: &str, values: &std::collections::BTreeSet<String>) {
    text.push_str(heading);
    text.push_str(":\n");
    if values.is_empty() {
        text.push_str("  none\n");
    } else {
        for value in values {
            text.push_str("  ");
            text.push_str(value);
            text.push('\n');
        }
    }
}

fn preview_output_format(streaming_mode: Option<&str>) -> OutputFormat {
    match streaming_mode
        .unwrap_or_default()
        .trim()
        .to_ascii_lowercase()
        .as_str()
    {
        "fmp4" | "segmenter-fmp4" | "hlsmp4" | "3" => OutputFormat::HlsMp4,
        _ => OutputFormat::Hls,
    }
}

fn preview_seek_seconds(requested: Option<i64>, duration_seconds: f64) -> i64 {
    let seek = requested.unwrap_or_default().max(0);
    if duration_seconds <= 0.0 {
        return seek;
    }
    let max_seek =
        i64::try_from(Duration::from_secs_f64((duration_seconds - 1.0).max(0.0)).as_secs())
            .unwrap_or(i64::MAX);
    seek.min(max_seek)
}

fn preview_duration_seconds(duration_seconds: f64, seek_seconds: i64) -> f64 {
    if duration_seconds <= 0.0 {
        return 30.0;
    }
    let remaining = (duration_seconds - seconds_i64_as_f64(seek_seconds)).max(0.0);
    remaining.clamp(0.1, 30.0)
}

fn seconds_i64_as_f64(seconds: i64) -> f64 {
    u32::try_from(seconds).map_or_else(|_| f64::from(u32::MAX), f64::from)
}

fn apply_unix_path_replacements(replacements: &[PathReplacement], path: &str) -> String {
    if replacements.is_empty() {
        path.to_owned()
    } else {
        replace_path(replacements, path, PathPlatform::Unix)
    }
}

fn preview_selector_channel(
    query: &PlaybackPreviewQuery,
    streaming_mode: StreamingMode,
) -> Channel {
    Channel {
        number: TRANSCODE_TROUBLESHOOTING_CHANNEL.to_owned(),
        name: "ETV".to_owned(),
        streaming_mode,
        subtitle_mode: ChannelSubtitleMode::Any,
        stream_selector: query
            .stream_selector
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        preferred_audio_title: query
            .preferred_audio_title
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned),
        ..Channel::default()
    }
}

struct PreviewSelectedStreamsRequest<'a> {
    pool: &'a ersatzrs_infra::db::DbPool,
    channel: &'a Channel,
    media_version_id: i64,
    media_item_id: i64,
    media_path: &'a str,
    output_format: OutputFormat,
    subtitle_id: Option<i64>,
    seek_seconds: i64,
    content_start_time: DateTime<Utc>,
}

async fn preview_selected_streams(
    request: PreviewSelectedStreamsRequest<'_>,
) -> ApiResult<(Option<usize>, Option<InputFile>)> {
    let PreviewSelectedStreamsRequest {
        pool,
        channel,
        media_version_id,
        media_item_id,
        media_path,
        output_format,
        subtitle_id,
        seek_seconds,
        content_start_time,
    } = request;
    if channel
        .stream_selector
        .as_deref()
        .is_some_and(|selector| !selector.trim().is_empty())
    {
        match select_custom_playback_streams(
            pool,
            media_version_id,
            media_item_id,
            channel,
            content_start_time,
            StreamingMode::HttpLiveStreamingSegmenter,
        )
        .await
        {
            Ok(Some(selection)) => {
                let explicit_subtitle = if let Some(id) = subtitle_id {
                    load_preview_subtitle(pool, media_item_id, id).await
                } else {
                    None
                };
                let (audio_stream_index, subtitle) =
                    resolve_custom_media_selection(selection, explicit_subtitle);
                return Ok((
                    audio_stream_index,
                    subtitle_input_from_optional_subtitle(
                        pool,
                        subtitle,
                        media_path,
                        output_format,
                        seek_seconds,
                    )
                    .await?,
                ));
            }
            Ok(_) => {
                let subtitle =
                    fallback_media_item_selector_subtitle(pool, media_item_id, subtitle_id).await;
                return Ok((
                    None,
                    subtitle_input_from_optional_subtitle(
                        pool,
                        subtitle,
                        media_path,
                        output_format,
                        seek_seconds,
                    )
                    .await?,
                ));
            }
            Err(error) => {
                tracing::warn!(%error, media_item_id, "failed to execute troubleshooting media stream selector");
                let subtitle =
                    fallback_media_item_selector_subtitle(pool, media_item_id, subtitle_id).await;
                return Ok((
                    None,
                    subtitle_input_from_optional_subtitle(
                        pool,
                        subtitle,
                        media_path,
                        output_format,
                        seek_seconds,
                    )
                    .await?,
                ));
            }
        }
    }

    let subtitle = fallback_explicit_media_item_subtitle(pool, media_item_id, subtitle_id).await;
    Ok((
        None,
        subtitle_input_from_optional_subtitle(
            pool,
            subtitle,
            media_path,
            output_format,
            seek_seconds,
        )
        .await?,
    ))
}

fn resolve_custom_media_selection(
    selection: CustomStreamSelection,
    explicit_subtitle: Option<PlaybackSubtitle>,
) -> (Option<usize>, Option<PlaybackSubtitle>) {
    (
        selection.audio_stream_index,
        explicit_subtitle.or(selection.subtitle),
    )
}

async fn fallback_explicit_media_item_subtitle(
    pool: &ersatzrs_infra::db::DbPool,
    media_item_id: i64,
    subtitle_id: Option<i64>,
) -> Option<PlaybackSubtitle> {
    let subtitle_id = subtitle_id?;
    load_preview_subtitle(pool, media_item_id, subtitle_id).await
}

async fn fallback_media_item_selector_subtitle(
    pool: &ersatzrs_infra::db::DbPool,
    media_item_id: i64,
    subtitle_id: Option<i64>,
) -> Option<PlaybackSubtitle> {
    if let Some(id) = subtitle_id {
        return load_preview_subtitle(pool, media_item_id, id).await;
    }
    match select_playback_subtitle(
        pool,
        media_item_id,
        &[],
        ChannelSubtitleMode::Any,
        StreamingMode::HttpLiveStreamingSegmenter,
    )
    .await
    {
        Ok(subtitle) => subtitle,
        Err(error) => {
            tracing::warn!(%error, media_item_id, "failed to select troubleshooting media selector fallback subtitle");
            None
        }
    }
}

async fn load_preview_subtitle(
    pool: &ersatzrs_infra::db::DbPool,
    media_item_id: i64,
    subtitle_id: i64,
) -> Option<PlaybackSubtitle> {
    let subtitles = load_subtitles_for_media_item(pool, media_item_id)
        .await
        .ok()?;
    let mut selected = subtitles
        .into_iter()
        .find(|subtitle| subtitle.id == subtitle_id)?;
    selected.forced = true;
    Some(selected)
}

async fn subtitle_input_from_optional_subtitle(
    pool: &ersatzrs_infra::db::DbPool,
    subtitle: Option<PlaybackSubtitle>,
    media_path: &str,
    output_format: OutputFormat,
    seek_seconds: i64,
) -> ApiResult<Option<InputFile>> {
    let subtitle = match subtitle {
        Some(selected) => Some(
            ersatzrs_infra::provider_subtitles::prepare(
                pool,
                &selected,
                &ersatzrs_infra::file_system_layout::subtitle_seek_cache_folder(),
            )
            .await?,
        ),
        None => None,
    };
    let input = subtitle.as_ref().and_then(|subtitle| {
        subtitle_input_for_stream(
            subtitle,
            media_path,
            output_format,
            StreamingMode::HttpLiveStreamingSegmenter,
        )
    });
    Ok(rebase_burned_text_subtitle_input(input, seek_seconds).await)
}

async fn fallback_channel_subtitle_input(
    pool: &ersatzrs_infra::db::DbPool,
    media_item_id: i64,
    preferred_language: Option<&str>,
    subtitle_mode: ChannelSubtitleMode,
    media_path: &str,
    output_format: OutputFormat,
) -> ApiResult<Option<InputFile>> {
    let preferences = match playback_language_preferences(pool).await {
        Ok(preferences) => preferences,
        Err(error) => {
            tracing::warn!(%error, media_item_id, "failed to load troubleshooting playback language preferences");
            return Ok(None);
        }
    };
    let preferred_languages = preferences.subtitle_languages(None, preferred_language);
    match select_playback_subtitle(
        pool,
        media_item_id,
        &preferred_languages,
        subtitle_mode,
        StreamingMode::HttpLiveStreamingSegmenter,
    )
    .await
    {
        Ok(Some(subtitle)) => {
            subtitle_input_from_optional_subtitle(
                pool,
                Some(subtitle),
                media_path,
                output_format,
                0,
            )
            .await
        }
        Ok(None) => Ok(None),
        Err(error) => {
            tracing::warn!(%error, media_item_id, "failed to select troubleshooting channel subtitle");
            Ok(None)
        }
    }
}

async fn preview_watermark_inputs(
    pool: &ersatzrs_infra::db::DbPool,
    watermark_ids: Vec<i64>,
    program_duration: Duration,
) -> Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)> {
    match resolve_playback_watermarks_by_ids(pool, 0, watermark_ids).await {
        Ok(watermarks) => watermarks
            .into_iter()
            .filter_map(|resolved| {
                watermark_overlay_for_stream(&resolved.watermark, Some(program_duration)).map(
                    |overlay| {
                        (
                            InputFile::watermark(resolved.image.input_path().into_owned()),
                            overlay,
                        )
                    },
                )
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, "failed to resolve troubleshooting watermarks");
            Vec::new()
        }
    }
}

async fn preview_channel_watermark_inputs(
    pool: &ersatzrs_infra::db::DbPool,
    channel: &Channel,
    playout_item_id: i64,
    disable_watermarks: bool,
    start_time: DateTime<Utc>,
    program_duration: Duration,
) -> Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)> {
    match select_playback_watermarks(
        pool,
        channel,
        playout_item_id,
        disable_watermarks,
        start_time,
    )
    .await
    {
        Ok(watermarks) => watermarks
            .into_iter()
            .filter_map(|resolved| {
                watermark_overlay_for_stream(&resolved.watermark, Some(program_duration)).map(
                    |overlay| {
                        (
                            InputFile::watermark(resolved.image.input_path().into_owned()),
                            overlay,
                        )
                    },
                )
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, channel_id = channel.id, "failed to resolve troubleshooting channel watermarks");
            Vec::new()
        }
    }
}

struct PreviewGraphicsRequest<'a> {
    pool: &'a ersatzrs_infra::db::DbPool,
    graphics_element_ids: Vec<i64>,
    media: &'a PreviewMediaItem,
    media_path: &'a str,
    playback_profile: &'a PlaybackProfile,
    source_stream: Option<ersatzrs_ffmpeg::SourceVideoStream>,
    seek_seconds: i64,
    program_duration: Duration,
    working_dir: &'a FsPath,
}

async fn preview_graphics_overlays(
    request: PreviewGraphicsRequest<'_>,
) -> PlaybackGraphicsArtifacts {
    let PreviewGraphicsRequest {
        pool,
        graphics_element_ids,
        media,
        media_path,
        playback_profile,
        source_stream,
        seek_seconds,
        program_duration,
        working_dir,
    } = request;
    if graphics_element_ids.is_empty() {
        return PlaybackGraphicsArtifacts::default();
    }
    let frame_size = playback_frame_size(playback_profile);
    let square_pixel_frame_size = source_stream
        .as_ref()
        .and_then(|stream| stream.square_pixel_frame_size(frame_size))
        .unwrap_or(frame_size);
    let now = Utc::now();
    let graphics = match resolve_playback_graphics_by_ids(pool, graphics_element_ids).await {
        Ok(graphics) => graphics,
        Err(error) => {
            tracing::warn!(%error, "failed to resolve troubleshooting graphics");
            return PlaybackGraphicsArtifacts::default();
        }
    };
    match render_playback_graphics_overlays(
        pool,
        &graphics,
        &PlaybackGraphicsRenderContext {
            channel_id: 0,
            channel_number: TRANSCODE_TROUBLESHOOTING_CHANNEL.to_owned(),
            media_item_id: media.media_item_id,
            media_path: media_path.to_owned(),
            channel_start_time: now,
            content_start_time: now,
            stream_seek_seconds: seconds_i64_as_f64(seek_seconds),
            duration_seconds: program_duration.as_secs_f64(),
            frame_size,
            square_pixel_frame_size,
            frame_rate: source_stream.and_then(|stream| stream.frame_rate),
        },
        working_dir.join("graphics"),
    )
    .await
    {
        Ok(overlays) => overlays,
        Err(error) => {
            tracing::warn!(%error, "failed to render troubleshooting graphics");
            PlaybackGraphicsArtifacts::default()
        }
    }
}

struct PreviewChannelGraphicsRequest<'a> {
    pool: &'a ersatzrs_infra::db::DbPool,
    channel: &'a Channel,
    active: &'a stream::ActivePlayoutItem,
    media_path: &'a str,
    playback_profile: &'a PlaybackProfile,
    target_frame_rate: Option<ersatzrs_ffmpeg::FrameRate>,
    source_stream: Option<ersatzrs_ffmpeg::SourceVideoStream>,
    start_position: Duration,
    program_duration: Duration,
    start_time: DateTime<Utc>,
    working_dir: &'a FsPath,
}

async fn preview_channel_graphics_overlays(
    request: PreviewChannelGraphicsRequest<'_>,
) -> PlaybackGraphicsArtifacts {
    let PreviewChannelGraphicsRequest {
        pool,
        channel,
        active,
        media_path,
        playback_profile,
        target_frame_rate,
        source_stream,
        start_position,
        program_duration,
        start_time,
        working_dir,
    } = request;
    let graphics = match select_playback_graphics(pool, channel, active.playout_item_id, start_time)
        .await
    {
        Ok(graphics) => graphics,
        Err(error) => {
            tracing::warn!(%error, channel_id = channel.id, "failed to select troubleshooting channel graphics");
            return PlaybackGraphicsArtifacts::default();
        }
    };
    if graphics.is_empty() {
        return PlaybackGraphicsArtifacts::default();
    }
    let frame_size = playback_frame_size(playback_profile);
    let square_pixel_frame_size = source_stream
        .as_ref()
        .and_then(|stream| stream.square_pixel_frame_size(frame_size))
        .unwrap_or(frame_size);
    match render_playback_graphics_overlays(
        pool,
        &graphics,
        &PlaybackGraphicsRenderContext {
            channel_id: channel.id,
            channel_number: TRANSCODE_TROUBLESHOOTING_CHANNEL.to_owned(),
            media_item_id: active.media_item_id,
            media_path: media_path.to_owned(),
            channel_start_time: start_time,
            content_start_time: parse_sqlite_utc_timestamp(&active.playout_start)
                .unwrap_or(start_time),
            stream_seek_seconds: start_position.as_secs_f64(),
            duration_seconds: program_duration.as_secs_f64(),
            frame_size,
            square_pixel_frame_size,
            frame_rate: target_frame_rate
                .or_else(|| source_stream.as_ref().and_then(|stream| stream.frame_rate)),
        },
        working_dir.join("graphics"),
    )
    .await
    {
        Ok(overlays) => overlays,
        Err(error) => {
            tracing::warn!(%error, channel_id = channel.id, "failed to render troubleshooting channel graphics");
            PlaybackGraphicsArtifacts::default()
        }
    }
}

fn playback_frame_size(playback_profile: &PlaybackProfile) -> FrameSize {
    FrameSize::new(
        u32::try_from(playback_profile.resolution.width)
            .ok()
            .filter(|width| *width > 0)
            .unwrap_or(1920),
        u32::try_from(playback_profile.resolution.height)
            .ok()
            .filter(|height| *height > 0)
            .unwrap_or(1080),
    )
}

async fn write_ffmpeg_arguments(working_dir: &FsPath, command: &FfmpegCommand) -> ApiResult<()> {
    let mut args = Vec::with_capacity(command.arguments.len() + 2);
    args.push(command.executable.clone());
    args.extend(command.arguments.clone());
    tokio::fs::write(working_dir.join("ffmpeg-arguments.txt"), args.join(" ")).await?;
    Ok(())
}

fn spawn_preview_ffmpeg(
    working_dir: &FsPath,
    command: FfmpegCommand,
    dynamic_text_controllers: Vec<DynamicTextController>,
    ownership: OwnedMutexGuard<()>,
) -> ApiResult<()> {
    let report_path = working_dir.join("ffmpeg-troubleshooting.log");
    let mut process = Command::new(&command.executable);
    process
        .arg("-report")
        .args(&command.arguments)
        .current_dir(working_dir)
        .env(
            "FFREPORT",
            format!("file={}:level=32", report_path.to_string_lossy()),
        )
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for variable in command.environment {
        process.env(variable.name, variable.value);
    }

    let mut child = process.spawn()?;
    let stderr = child.stderr.take();
    tokio::spawn(write_preview_logs(
        working_dir.join("logs.txt"),
        stderr,
        child,
        dynamic_text_controllers,
        ownership,
    ));
    Ok(())
}

async fn write_preview_logs(
    log_path: PathBuf,
    stderr: Option<tokio::process::ChildStderr>,
    mut child: tokio::process::Child,
    dynamic_text_controllers: Vec<DynamicTextController>,
    _ownership: OwnedMutexGuard<()>,
) {
    let mut logs = Vec::new();
    if let Some(stderr) = stderr {
        use tokio::io::AsyncReadExt;
        let mut reader = tokio::io::BufReader::new(stderr);
        let _ = reader.read_to_end(&mut logs).await;
    }
    match child.wait().await {
        Ok(status) => logs.extend_from_slice(format!("\nexit status: {status}\n").as_bytes()),
        Err(error) => logs.extend_from_slice(format!("\nwait error: {error}\n").as_bytes()),
    }
    shutdown_dynamic_text_controllers(dynamic_text_controllers).await;
    let _ = tokio::fs::write(log_path, logs).await;
}

async fn wait_for_preview_output(
    working_dir: &PathBuf,
    output_format: OutputFormat,
) -> ApiResult<()> {
    let playlist_path = working_dir.join("live.m3u8");
    let segment_extension = match output_format {
        OutputFormat::HlsMp4 => "m4s",
        _ => "ts",
    };
    let deadline = tokio::time::Instant::now() + Duration::from_secs(20);
    loop {
        if tokio::fs::try_exists(&playlist_path).await?
            && count_segments(working_dir, segment_extension).await? >= 1
        {
            return Ok(());
        }
        if tokio::time::Instant::now() >= deadline {
            return Err("timed out waiting for troubleshooting HLS output".into());
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn count_segments(working_dir: &PathBuf, extension: &str) -> ApiResult<usize> {
    let mut count = 0usize;
    let mut entries = tokio::fs::read_dir(working_dir).await?;
    while let Some(entry) = entries.next_entry().await? {
        if entry
            .path()
            .extension()
            .and_then(|candidate| candidate.to_str())
            .is_some_and(|candidate| candidate.eq_ignore_ascii_case(extension))
        {
            count += 1;
        }
    }
    Ok(count)
}

async fn build_troubleshooting_archive() -> ApiResult<Option<Vec<u8>>> {
    let folder = troubleshooting_folder();
    build_troubleshooting_archive_from(&folder).await
}

async fn build_troubleshooting_archive_from(folder: &FsPath) -> ApiResult<Option<Vec<u8>>> {
    let mut files = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(folder).await else {
        return Ok(None);
    };
    let mut directories = vec![folder.to_owned()];
    while let Some(entry) = entries.next_entry().await? {
        if entry.file_type().await?.is_dir() {
            directories.push(entry.path());
        }
    }
    let mut has_ffmpeg_report = false;
    for directory in directories {
        let mut entries = tokio::fs::read_dir(directory).await?;
        while let Some(entry) = entries.next_entry().await? {
            if !entry.file_type().await?.is_file() {
                continue;
            }
            let name = entry.file_name();
            let Some(name) = name.to_str() else {
                continue;
            };
            if is_archive_member(name) {
                let lower = name.to_ascii_lowercase();
                has_ffmpeg_report |= lower.starts_with("ffmpeg-") || lower.contains("ffreport");
                files.push((name.to_owned(), tokio::fs::read(entry.path()).await?));
            }
        }
    }
    if !has_ffmpeg_report {
        return Ok(None);
    }
    Ok(Some(build_store_zip(files)?))
}

fn is_archive_member(file_name: &str) -> bool {
    let lower = file_name.to_ascii_lowercase();
    lower.starts_with("ffmpeg-")
        || lower == "logs.txt"
        || FsPath::new(&lower)
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("json"))
        || lower.contains("capabilities")
        || lower.contains("stream-selector")
        || lower.contains("ffreport")
        || lower.contains("outcome")
}

async fn build_media_sample_archive(
    state: &NativeAdminTroubleshootingRuntime,
    media_item_id: i64,
) -> ApiResult<Option<Vec<u8>>> {
    let Some(stored_media_path) = state
        .repositories
        .troubleshooting
        .load_media_item_path_admin(media_item_id)
        .await?
    else {
        return Ok(None);
    };
    if stored_media_path.trim().is_empty() {
        return Ok(None);
    }
    let path_replacements = stream::load_path_replacements(&state.repositories.media_source)
        .await
        .unwrap_or_default();
    let media_path = apply_unix_path_replacements(&path_replacements, &stored_media_path);

    let configured_ffmpeg_path = state
        .repositories
        .admin_settings
        .get_optional_string(ersatzrs_domain::CONFIG_KEY_FFMPEG_PATH)
        .await?;
    let ffmpeg_path = ersatzrs_ffmpeg::resolve_ffmpeg_path(configured_ffmpeg_path.as_deref()).path;
    let extension = FsPath::new(&media_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .filter(|extension| !extension.trim().is_empty())
        .unwrap_or("mkv");
    let sample_path = std::env::temp_dir().join(format!(
        "ersatzrs-media-sample-{}.{}",
        uuid::Uuid::new_v4(),
        extension.trim_start_matches('.')
    ));

    let status = Command::new(ffmpeg_path)
        .arg("-nostdin")
        .arg("-i")
        .arg(&media_path)
        .arg("-t")
        .arg("30")
        .arg("-map")
        .arg("0")
        .arg("-c")
        .arg("copy")
        .arg(&sample_path)
        .kill_on_drop(true)
        .status()
        .await?;

    if !status.success() {
        let _ = tokio::fs::remove_file(&sample_path).await;
        return Ok(None);
    }

    let file_name = sample_path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("sample.mkv")
        .to_owned();
    let bytes = tokio::fs::read(&sample_path).await?;
    let _ = tokio::fs::remove_file(&sample_path).await;
    Ok(Some(build_store_zip(vec![(file_name, bytes)])?))
}

fn troubleshooting_folder() -> PathBuf {
    transcode_troubleshooting_folder()
}

fn build_store_zip(files: Vec<(String, Vec<u8>)>) -> ApiResult<Vec<u8>> {
    let mut out = Vec::new();
    let mut central_directory = Vec::new();

    for (name, contents) in files {
        let local_header_offset = u32::try_from(out.len())?;
        let name_bytes = name.as_bytes();
        let crc = crc32(&contents);
        let size = u32::try_from(contents.len())?;
        let name_len = u16::try_from(name_bytes.len())?;

        write_u32(&mut out, 0x0403_4b50);
        write_u16(&mut out, 20);
        write_u16(&mut out, 0);
        write_u16(&mut out, 0);
        write_u16(&mut out, 0);
        write_u16(&mut out, 0);
        write_u32(&mut out, crc);
        write_u32(&mut out, size);
        write_u32(&mut out, size);
        write_u16(&mut out, name_len);
        write_u16(&mut out, 0);
        out.extend_from_slice(name_bytes);
        out.extend_from_slice(&contents);

        write_u32(&mut central_directory, 0x0201_4b50);
        write_u16(&mut central_directory, 20);
        write_u16(&mut central_directory, 20);
        write_u16(&mut central_directory, 0);
        write_u16(&mut central_directory, 0);
        write_u16(&mut central_directory, 0);
        write_u16(&mut central_directory, 0);
        write_u32(&mut central_directory, crc);
        write_u32(&mut central_directory, size);
        write_u32(&mut central_directory, size);
        write_u16(&mut central_directory, name_len);
        write_u16(&mut central_directory, 0);
        write_u16(&mut central_directory, 0);
        write_u16(&mut central_directory, 0);
        write_u16(&mut central_directory, 0);
        write_u32(&mut central_directory, 0);
        write_u32(&mut central_directory, local_header_offset);
        central_directory.extend_from_slice(name_bytes);
    }

    let central_directory_offset = u32::try_from(out.len())?;
    let central_directory_size = u32::try_from(central_directory.len())?;
    let file_count = count_central_directory_entries(&central_directory)?;
    out.extend_from_slice(&central_directory);
    write_u32(&mut out, 0x0605_4b50);
    write_u16(&mut out, 0);
    write_u16(&mut out, 0);
    write_u16(&mut out, file_count);
    write_u16(&mut out, file_count);
    write_u32(&mut out, central_directory_size);
    write_u32(&mut out, central_directory_offset);
    write_u16(&mut out, 0);
    Ok(out)
}

fn count_central_directory_entries(central_directory: &[u8]) -> ApiResult<u16> {
    Ok(u16::try_from(
        central_directory
            .windows(4)
            .filter(|window| *window == [0x50, 0x4b, 0x01, 0x02])
            .count(),
    )?)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffffu32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn write_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn write_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}

#[cfg(test)]
mod tests {
    #[tokio::test]
    async fn preview_ownership_is_shared_by_runtime_clones_and_released_on_preparation_error() {
        use super::*;

        let pool = ersatzrs_infra::db::lazy_sqlite_pool("sqlite::memory:").unwrap();
        let runtime = NativeAdminTroubleshootingRuntime::new(
            pool.clone(),
            Arc::new(ersatzrs_infra::repositories::make_repositories(pool)),
            Arc::new(RwLock::new(FfmpegRuntimeReport::for_tests())),
            Arc::default(),
        );
        let clone = runtime.clone();
        let ownership = Arc::clone(&runtime.preview_lock).try_lock_owned().unwrap();
        let error = clone
            .start_playback_preview(PlaybackPreviewRequest::default())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("already running"));
        drop(ownership);
        let error = clone
            .start_playback_preview(PlaybackPreviewRequest::default())
            .await
            .unwrap_err();
        assert!(error.to_string().contains("requires mediaItem"));
        assert!(runtime.preview_lock.try_lock().is_ok());
    }

    // The shell is only a deterministic exited-child fixture; ownership itself
    // uses Tokio on every supported host platform.
    #[cfg(unix)]
    #[tokio::test]
    async fn preview_ownership_outlives_process_exit_until_diagnostics_are_written() {
        use super::*;

        let root = tempfile::tempdir().unwrap();
        let log = root.path().join("logs.txt");
        let lock = Arc::new(Mutex::new(()));
        let ownership = Arc::clone(&lock).try_lock_owned().unwrap();
        let mut child = Command::new("sh").args(["-c", "exit 0"]).spawn().unwrap();
        child.wait().await.unwrap();
        let completion = write_preview_logs(log.clone(), None, child, Vec::new(), ownership);
        assert!(lock.try_lock().is_err());
        completion.await;
        assert!(lock.try_lock().is_ok());
        assert!(tokio::fs::read_to_string(log)
            .await
            .unwrap()
            .contains("exit status:"));
    }

    #[test]
    fn diagnostic_versions_preserve_contract_and_optional_engine_absence() {
        let ffmpeg = ersatzrs_ffmpeg::version::FfmpegVersion {
            major: 9,
            minor: 2,
            patch: 3,
        };
        for (next, expected) in [(None, "unknown"), (Some("0.9.4-test"), "0.9.4-test")] {
            let json =
                serde_json::to_value(super::TroubleshootingVersions::detected(next, &ffmpeg))
                    .unwrap();
            assert_eq!(json["Version"], env!("CARGO_PKG_VERSION"));
            assert_eq!(json["NextVersion"], expected);
            assert_eq!(json["FFmpegVersion"], "9.2.3");
            assert_eq!(json.as_object().unwrap().len(), 3);
        }
    }
    use super::*;
    use ersatzrs_ffmpeg::format_time_limit;

    #[test]
    fn archive_member_filter_matches_ersatztv() {
        assert!(is_archive_member("ffmpeg-report.txt"));
        assert!(is_archive_member("logs.txt"));
        assert!(is_archive_member("media_info.json"));
        assert!(is_archive_member("capabilities_qsv.txt"));
        assert!(is_archive_member("stream-selector.yml"));
        assert!(is_archive_member("FFREPORT.log"));
        assert!(is_archive_member("playback-outcome.txt"));
        assert!(!is_archive_member("live.m3u8"));
        assert!(!is_archive_member("seg00001.ts"));
    }

    #[tokio::test]
    async fn troubleshooting_archive_collects_next_reports_one_level_deep() {
        let root = tempfile::tempdir().unwrap();
        let report_dir = root.path().join("next-report");
        let deeper = report_dir.join("deeper");
        tokio::fs::create_dir_all(&deeper).await.unwrap();
        for (path, bytes) in [
            (root.path().join("logs.txt"), "native-log"),
            (report_dir.join("playback-outcome.txt"), "next-outcome"),
            (report_dir.join("channel.json"), "next-config"),
            (report_dir.join("seg00001.ts"), "excluded-media"),
            (deeper.join("ffreport.log"), "excluded-deep-report"),
        ] {
            tokio::fs::write(path, bytes).await.unwrap();
        }
        assert!(build_troubleshooting_archive_from(root.path())
            .await
            .unwrap()
            .is_none());
        tokio::fs::write(report_dir.join("FFREPORT.log"), "next-report")
            .await
            .unwrap();
        let zip = build_troubleshooting_archive_from(root.path())
            .await
            .unwrap()
            .expect("Next report makes the archive downloadable");
        for expected in ["native-log", "next-outcome", "next-config", "FFREPORT.log"] {
            assert!(zip
                .windows(expected.len())
                .any(|bytes| bytes == expected.as_bytes()));
        }
        for excluded in ["excluded-media", "excluded-deep-report"] {
            assert!(!zip
                .windows(excluded.len())
                .any(|bytes| bytes == excluded.as_bytes()));
        }
    }

    #[test]
    fn store_zip_contains_local_and_central_headers() {
        let bytes =
            build_store_zip(vec![("logs.txt".to_owned(), b"hello".to_vec())]).expect("zip builds");
        assert!(bytes.starts_with(&[0x50, 0x4b, 0x03, 0x04]));
        assert!(bytes
            .windows(4)
            .any(|window| window == [0x50, 0x4b, 0x01, 0x02]));
        assert!(bytes.ends_with(&[0, 0]));
    }

    #[test]
    fn preview_seek_and_duration_match_ersatztv_thirty_second_limit() {
        assert_eq!(preview_seek_seconds(Some(80), 90.0), 80);
        assert!((preview_duration_seconds(90.0, 80) - 10.0).abs() < f64::EPSILON);
        assert!((preview_duration_seconds(120.0, 0) - 30.0).abs() < f64::EPSILON);
        assert!((preview_duration_seconds(0.0, 0) - 30.0).abs() < f64::EPSILON);
    }

    #[test]
    fn troubleshooting_paths_apply_configured_unix_replacements() {
        let replacements = [PathReplacement {
            source_path: "Z:\\Media".to_owned(),
            replacement_path: "/mnt/media".to_owned(),
            source_platform: PathPlatform::Windows,
        }];

        assert_eq!(
            apply_unix_path_replacements(&replacements, "Z:\\Media\\Movie\\movie.mkv"),
            "/mnt/media/Movie/movie.mkv"
        );
        assert_eq!(
            apply_unix_path_replacements(&[], "/media/movie.mkv"),
            "/media/movie.mkv"
        );
    }

    #[test]
    fn preview_duration_limit_uses_ersatztv_time_limit_format() {
        assert_eq!(
            format_time_limit(Duration::from_millis(12_500)),
            "00:00:12.5000000"
        );
    }

    #[test]
    fn custom_media_selection_honors_subtitle_only_result() {
        let selector_subtitle = test_subtitle(7, "selector");
        let selection = CustomStreamSelection {
            audio_stream_index: None,
            subtitle: Some(selector_subtitle.clone()),
        };

        let (audio, subtitle) = resolve_custom_media_selection(selection, None);

        assert_eq!(audio, None);
        assert_eq!(subtitle, Some(selector_subtitle));
    }

    #[test]
    fn custom_media_selection_keeps_explicit_subtitle_override() {
        let selector_subtitle = test_subtitle(7, "selector");
        let explicit_subtitle = test_subtitle(9, "explicit");
        let selection = CustomStreamSelection {
            audio_stream_index: Some(3),
            subtitle: Some(selector_subtitle),
        };

        let (audio, subtitle) =
            resolve_custom_media_selection(selection, Some(explicit_subtitle.clone()));

        assert_eq!(audio, Some(3));
        assert_eq!(subtitle, Some(explicit_subtitle));
    }

    fn test_subtitle(id: i64, title: &str) -> PlaybackSubtitle {
        PlaybackSubtitle {
            id,
            subtitle_kind: 1,
            title: Some(title.to_owned()),
            stream_index: 4,
            codec: Some("srt".to_owned()),
            default: false,
            forced: false,
            sdh: false,
            language: Some("eng".to_owned()),
            is_extracted: false,
            path: Some(format!("{title}.srt")),
        }
    }

    #[test]
    fn preview_output_format_defaults_to_mpegts_hls() {
        assert_eq!(preview_output_format(None), OutputFormat::Hls);
        assert_eq!(preview_output_format(Some("segmenter")), OutputFormat::Hls);
        assert_eq!(
            preview_output_format(Some("segmenter-fmp4")),
            OutputFormat::HlsMp4
        );
    }

    #[test]
    fn preview_start_accepts_browser_datetime_local_and_rfc3339() {
        assert_eq!(
            parse_preview_start(Some("2026-06-04T21:15"))
                .expect("datetime-local parses")
                .to_rfc3339(),
            "2026-06-04T21:15:00+00:00"
        );
        assert_eq!(
            parse_preview_start(Some("2026-06-04T21:15:30+02:00"))
                .expect("rfc3339 parses")
                .to_rfc3339(),
            "2026-06-04T19:15:30+00:00"
        );
        assert!(parse_preview_start(Some("")).is_none());
    }
}
