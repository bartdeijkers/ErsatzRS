use std::{
    path::PathBuf,
    process::Stdio,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, NaiveDateTime, Utc};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::{ChannelRepository, MediaSourceRepository};
use ersatzrs_domain::{ChannelSubtitleMode, EmbyMediaSource, JellyfinMediaSource, PlexMediaSource};
use ersatzrs_ffmpeg::{
    build_pipeline_for_acceleration, DesiredVideoState, FfmpegCapabilityReport, FfmpegCommand,
    FrameRate, FrameSize, GraphicsPipelineOverlay, InputFile, OutputFormat, PipelineRequest,
    SourceVideoStream,
};
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::playback::{
    load_channel_playback_profile, load_channel_target_frame_rate, load_source_video_stream,
    render_playback_graphics_overlays, select_playback_graphics, select_playback_subtitle,
    select_playback_watermarks, subtitle_fonts_dir, subtitle_input_for_stream,
    watermark_overlay_for_stream, PlaybackGraphicsRenderContext, PlaybackProfile,
};
use ersatzrs_infra::subtitles::rebase_burned_text_subtitle_input;
use sqlx::Row;
use tokio::process::Command;
use tokio_util::io::ReaderStream;

use crate::host::state::AppState;

static NEXT_GRAPHICS_REQUEST_ID: AtomicU64 = AtomicU64::new(1);

pub async fn serve_transport_stream(state: &AppState, channel_number: &str) -> Response {
    serve_channel_stream(state, channel_number, OutputFormat::MpegTs).await
}

#[allow(clippy::too_many_lines)]
pub async fn serve_channel_stream(
    state: &AppState,
    channel_number: &str,
    output_format: OutputFormat,
) -> Response {
    let channel = match state
        .repositories
        .channel
        .get_by_number(channel_number)
        .await
    {
        Ok(channel) if channel.is_enabled => channel,
        Ok(_) => return StatusCode::NOT_FOUND.into_response(),
        Err(error) => {
            tracing::debug!(%error, channel_number, "channel not found for stream");
            return StatusCode::NOT_FOUND.into_response();
        }
    };

    let now = Utc::now();
    let active = match load_active_media_path(&state.pool, channel.id, now).await {
        Ok(Some(path)) => path,
        Ok(None) => {
            tracing::info!(
                channel_number = %channel.number,
                "no active playout item; returning 503"
            );
            return (StatusCode::SERVICE_UNAVAILABLE, "no active playout item").into_response();
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, "failed to load active playout item");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };

    let replacements = match load_path_replacements(&state.repositories.media_source).await {
        Ok(replacements) => replacements,
        Err(error) => {
            tracing::warn!(%error, "failed to load path replacements; continuing with raw path");
            Vec::new()
        }
    };

    let resolved = if replacements.is_empty() {
        active.media_path.clone()
    } else {
        replace_path(&replacements, &active.media_path, PathPlatform::Unix)
    };

    let source_stream = load_source_video_stream(&state.pool, active.media_version_id).await;
    let playback_profile = match load_channel_playback_profile(&state.pool, channel.id).await {
        Ok(Some(profile)) => profile,
        Ok(None) => {
            tracing::error!(channel_number = %channel.number, "channel has no playback ffmpeg profile");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
        Err(error) => {
            tracing::error!(%error, channel_number = %channel.number, "failed to load playback ffmpeg profile");
            return StatusCode::INTERNAL_SERVER_ERROR.into_response();
        }
    };
    let target_frame_rate = if playback_profile.profile.normalize_framerate {
        match load_channel_target_frame_rate(&state.pool, channel.id).await {
            Ok(frame_rate) => frame_rate,
            Err(error) => {
                tracing::warn!(%error, channel_number = %channel.number, "failed to load channel target frame rate");
                None
            }
        }
    } else {
        None
    };
    let runtime_capabilities = state.ffmpeg_runtime.read().await.capabilities.clone();
    let subtitle_mode = active.subtitle_mode.unwrap_or(channel.subtitle_mode);
    let preferred_subtitle_language = active
        .preferred_subtitle_language_code
        .as_deref()
        .or(channel.preferred_subtitle_language_code.as_deref());
    let subtitle = match select_playback_subtitle(
        &state.pool,
        active.media_item_id,
        preferred_subtitle_language,
        subtitle_mode,
        channel.streaming_mode,
    )
    .await
    {
        Ok(subtitle) => subtitle,
        Err(error) => {
            tracing::warn!(%error, media_item_id = active.media_item_id, "failed to select playback subtitle");
            None
        }
    };
    let start_seconds = playout_start_seconds(now, &active.playout_start);
    let subtitle_input = subtitle.as_ref().and_then(|subtitle| {
        subtitle_input_for_stream(subtitle, &resolved, output_format, channel.streaming_mode)
    });
    let subtitle_input = rebase_burned_text_subtitle_input(subtitle_input, start_seconds).await;
    let watermark_inputs = match select_playback_watermarks(
        &state.pool,
        &channel,
        active.playout_item_id,
        active.disable_watermarks,
        now,
    )
    .await
    {
        Ok(watermarks) => watermarks
            .into_iter()
            .filter_map(|resolved| {
                watermark_overlay_for_stream(&resolved.watermark, active.program_duration()).map(
                    |overlay| {
                        (
                            InputFile::watermark(
                                resolved.image_path.to_string_lossy().into_owned(),
                            ),
                            overlay,
                        )
                    },
                )
            })
            .collect(),
        Err(error) => {
            tracing::warn!(%error, channel_number = %channel.number, "failed to select playback watermarks");
            Vec::new()
        }
    };
    let mut graphics_cleanup_dirs = Vec::new();
    let graphics_overlays = match select_playback_graphics(
        &state.pool,
        &channel,
        active.playout_item_id,
        now,
    )
    .await
    {
        Ok(graphics) => {
            let frame_size = playback_frame_size(&playback_profile);
            let square_pixel_frame_size = source_stream
                .as_ref()
                .and_then(|stream| stream.square_pixel_frame_size(frame_size))
                .unwrap_or(frame_size);
            let graphics_dir = playback_graphics_dir(channel.id, active.playout_item_id);
            let graphics_cleanup_dir = graphics_dir.clone();
            let start_time = parse_sqlite_utc_timestamp(&active.playout_start).unwrap_or(now);
            match render_playback_graphics_overlays(
                &state.pool,
                &graphics,
                &PlaybackGraphicsRenderContext {
                    channel_id: channel.id,
                    channel_number: channel.number.clone(),
                    media_item_id: active.media_item_id,
                    media_path: resolved.clone(),
                    channel_start_time: start_time,
                    content_start_time: start_time,
                    stream_seek_seconds: seconds_as_f64(start_seconds),
                    duration_seconds: active
                        .program_duration()
                        .map_or(0.0, |duration| duration.as_secs_f64()),
                    frame_size,
                    square_pixel_frame_size,
                    frame_rate: target_frame_rate
                        .or_else(|| source_stream.as_ref().and_then(|stream| stream.frame_rate)),
                },
                graphics_dir,
            )
            .await
            {
                Ok(overlays) => {
                    graphics_cleanup_dirs.push(graphics_cleanup_dir);
                    overlays
                }
                Err(error) => {
                    let _ = tokio::fs::remove_dir_all(&graphics_cleanup_dir).await;
                    tracing::warn!(%error, channel_number = %channel.number, "failed to render playback graphics");
                    Vec::new()
                }
            }
        }
        Err(error) => {
            tracing::warn!(%error, channel_number = %channel.number, "failed to select playback graphics");
            Vec::new()
        }
    };
    let command = build_stream_command(
        &resolved,
        output_format,
        start_seconds,
        &playback_profile,
        target_frame_rate,
        channel.streaming_mode,
        runtime_capabilities,
        source_stream,
        subtitle_input,
        watermark_inputs,
        graphics_overlays,
    );
    match spawn_stream_body(command, graphics_cleanup_dirs) {
        Ok(body) => {
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(content_type_for_output_format(output_format)),
            );
            response
                .headers_mut()
                .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
            response
        }
        Err(error) => {
            tracing::error!(%error, "failed to spawn ffmpeg for channel stream");
            StatusCode::INTERNAL_SERVER_ERROR.into_response()
        }
    }
}

#[derive(Debug, Clone, thiserror::Error)]
enum StreamSpawnError {
    #[error("failed to spawn ffmpeg: {0}")]
    Spawn(String),
    #[error("ffmpeg did not expose stdout")]
    MissingStdout,
}

#[derive(Debug, Clone)]
pub(crate) struct ActivePlayoutItem {
    pub(crate) playout_item_id: i64,
    pub(crate) media_path: String,
    pub(crate) media_item_id: i64,
    pub(crate) media_version_id: i64,
    pub(crate) playout_start: String,
    pub(crate) playout_finish: String,
    pub(crate) disable_watermarks: bool,
    pub(crate) preferred_subtitle_language_code: Option<String>,
    pub(crate) subtitle_mode: Option<ChannelSubtitleMode>,
}

impl ActivePlayoutItem {
    fn program_duration(&self) -> Option<std::time::Duration> {
        let start = parse_sqlite_utc_timestamp(&self.playout_start)?;
        let finish = parse_sqlite_utc_timestamp(&self.playout_finish)?;
        (finish - start).to_std().ok()
    }
}

pub(crate) async fn load_active_media_path(
    pool: &DbPool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<ActivePlayoutItem>, sqlx::Error> {
    let now_text = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "MediaPath",
                  pi."Id" AS "PlayoutItemId",
                  pi."MediaItemId" AS "MediaItemId",
                  mv."Id" AS "MediaVersionId",
                  pi."Start" AS "PlayoutStart",
                  pi."Finish" AS "PlayoutFinish",
                  pi."DisableWatermarks" AS "DisableWatermarks",
                  pi."PreferredSubtitleLanguageCode" AS "PreferredSubtitleLanguageCode",
                  pi."SubtitleMode" AS "SubtitleMode"
           FROM "PlayoutItem" pi
           JOIN "Playout" p ON p."Id" = pi."PlayoutId"
           JOIN "MediaVersion" mv
               ON mv."MovieId" = pi."MediaItemId"
               OR mv."EpisodeId" = pi."MediaItemId"
               OR mv."MusicVideoId" = pi."MediaItemId"
               OR mv."OtherVideoId" = pi."MediaItemId"
               OR mv."RemoteStreamId" = pi."MediaItemId"
               OR mv."SongId" = pi."MediaItemId"
               OR mv."ImageId" = pi."MediaItemId"
           JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
           WHERE p."ChannelId" = ?
             AND pi."Start" <= ?
             AND pi."Finish" > ?
             AND mf."Path" IS NOT NULL
           ORDER BY pi."Start" DESC, pi."Id" DESC
           LIMIT 1"#,
    )
    .bind(channel_id)
    .bind(&now_text)
    .bind(&now_text)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|row| ActivePlayoutItem {
        playout_item_id: row.get::<i64, _>("PlayoutItemId"),
        media_path: row.get::<String, _>("MediaPath"),
        media_item_id: row.get::<i64, _>("MediaItemId"),
        media_version_id: row.get::<i64, _>("MediaVersionId"),
        playout_start: row.get::<String, _>("PlayoutStart"),
        playout_finish: row.get::<String, _>("PlayoutFinish"),
        disable_watermarks: row
            .try_get::<i64, _>("DisableWatermarks")
            .is_ok_and(|value| value != 0),
        preferred_subtitle_language_code: row
            .try_get::<Option<String>, _>("PreferredSubtitleLanguageCode")
            .ok()
            .flatten(),
        subtitle_mode: row
            .try_get::<Option<i64>, _>("SubtitleMode")
            .ok()
            .flatten()
            .and_then(decode_subtitle_mode),
    }))
}

pub(crate) async fn load_path_replacements(
    media_sources: &impl MediaSourceRepository,
) -> Result<Vec<PathReplacement>, ersatzrs_core::repositories::RepositoryError> {
    let all = media_sources.list_all().await?;
    let mut replacements = Vec::new();
    for source in all {
        match source.kind {
            ersatzrs_domain::MediaSourceKind::Plex => {
                let PlexMediaSource {
                    path_replacements, ..
                } = media_sources.get_plex(source.id).await?;
                push_replacements(
                    path_replacements
                        .into_iter()
                        .map(|r| (r.path, r.replacement_path)),
                    &mut replacements,
                );
            }
            ersatzrs_domain::MediaSourceKind::Jellyfin => {
                let JellyfinMediaSource {
                    path_replacements, ..
                } = media_sources.get_jellyfin(source.id).await?;
                push_replacements(
                    path_replacements
                        .into_iter()
                        .map(|r| (r.path, r.replacement_path)),
                    &mut replacements,
                );
            }
            ersatzrs_domain::MediaSourceKind::Emby => {
                let EmbyMediaSource {
                    path_replacements, ..
                } = media_sources.get_emby(source.id).await?;
                push_replacements(
                    path_replacements
                        .into_iter()
                        .map(|r| (r.path, r.replacement_path)),
                    &mut replacements,
                );
            }
            ersatzrs_domain::MediaSourceKind::Local => {}
        }
    }
    Ok(replacements)
}

fn push_replacements(
    source: impl IntoIterator<Item = (String, String)>,
    out: &mut Vec<PathReplacement>,
) {
    for (path, replacement_path) in source {
        let source_platform = detect_platform(&path);
        out.push(PathReplacement {
            source_platform,
            source_path: path,
            replacement_path,
        });
    }
}

fn detect_platform(path: &str) -> PathPlatform {
    if path.contains('\\') || path.contains(':') {
        PathPlatform::Windows
    } else {
        PathPlatform::Unix
    }
}

pub(crate) fn content_type_for_output_format(output_format: OutputFormat) -> &'static str {
    match output_format {
        OutputFormat::MpegTs => "video/mp2t",
        OutputFormat::Mp4 => "video/mp4",
        OutputFormat::Mkv => "video/x-matroska",
        OutputFormat::Nut | OutputFormat::Hls | OutputFormat::HlsMp4 => "application/octet-stream",
    }
}

pub(crate) fn extension_for_output_format(output_format: OutputFormat) -> &'static str {
    match output_format {
        OutputFormat::MpegTs => ".ts",
        OutputFormat::Mp4 => ".mp4",
        OutputFormat::Mkv => ".mkv",
        OutputFormat::Nut | OutputFormat::Hls | OutputFormat::HlsMp4 => "",
    }
}

#[allow(clippy::too_many_arguments)]
fn build_stream_command(
    path: &str,
    output_format: OutputFormat,
    start_seconds: i64,
    playback_profile: &PlaybackProfile,
    target_frame_rate: Option<FrameRate>,
    streaming_mode: ersatzrs_domain::StreamingMode,
    runtime_capabilities: FfmpegCapabilityReport,
    source_stream: Option<SourceVideoStream>,
    subtitle_input: Option<InputFile>,
    watermark_inputs: Vec<(InputFile, ersatzrs_ffmpeg::WatermarkOverlay)>,
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
) -> FfmpegCommand {
    let input = InputFile::video(path.to_string());
    let desired = DesiredVideoState::from_profile(
        &playback_profile.profile,
        &playback_profile.resolution,
        source_stream.as_ref(),
        target_frame_rate,
        streaming_mode,
    );
    let mut request =
        PipelineRequest::new(input, desired).with_runtime_capabilities(runtime_capabilities);
    if let Some(source_stream) = source_stream {
        request = request.with_source_video_stream(source_stream);
    }
    if let Some(subtitle_input) = subtitle_input {
        request.inputs.push(subtitle_input);
        request = request.with_subtitle_fonts_dir(subtitle_fonts_dir());
    }
    for (input, overlay) in watermark_inputs {
        request.inputs.push(input);
        request.watermarks.push(overlay);
    }
    request.graphics_overlays.extend(graphics_overlays);
    request.state.output_format = output_format;
    if start_seconds > 0 {
        request.state.start_seconds = Some(start_seconds);
    }
    request.state.report_progress = false;
    if let Ok(thread_count) = u32::try_from(playback_profile.profile.thread_count) {
        if thread_count > 0 {
            request.state.thread_count = Some(thread_count);
        }
    }

    let acceleration = if streaming_mode == ersatzrs_domain::StreamingMode::HttpLiveStreamingDirect
    {
        ersatzrs_domain::HardwareAccelerationKind::None
    } else {
        playback_profile.profile.hardware_acceleration
    };
    let pipeline = build_pipeline_for_acceleration(acceleration, request.clone());
    pipeline.command(&request.inputs)
}

fn playback_frame_size(playback_profile: &PlaybackProfile) -> FrameSize {
    FrameSize::new(
        u32::try_from(playback_profile.resolution.width)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(1920),
        u32::try_from(playback_profile.resolution.height)
            .ok()
            .filter(|value| *value > 0)
            .unwrap_or(1080),
    )
}

pub(crate) fn playout_start_seconds(now: DateTime<Utc>, playout_start: &str) -> i64 {
    parse_sqlite_utc_timestamp(playout_start).map_or(0, |start| (now - start).num_seconds().max(0))
}

fn seconds_as_f64(seconds: i64) -> f64 {
    u64::try_from(seconds)
        .map(Duration::from_secs)
        .map_or(0.0, |duration| duration.as_secs_f64())
}

fn parse_sqlite_utc_timestamp(value: &str) -> Option<DateTime<Utc>> {
    for format in ["%Y-%m-%d %H:%M:%S%.f", "%Y-%m-%d %H:%M:%S"] {
        if let Ok(parsed) = NaiveDateTime::parse_from_str(value, format) {
            return Some(DateTime::<Utc>::from_naive_utc_and_offset(parsed, Utc));
        }
    }
    None
}

const fn decode_subtitle_mode(value: i64) -> Option<ChannelSubtitleMode> {
    match value {
        0 => Some(ChannelSubtitleMode::None),
        1 => Some(ChannelSubtitleMode::Forced),
        2 => Some(ChannelSubtitleMode::Default),
        3 => Some(ChannelSubtitleMode::Any),
        _ => None,
    }
}

fn playback_graphics_dir(channel_id: i64, playout_item_id: i64) -> PathBuf {
    let request_id = NEXT_GRAPHICS_REQUEST_ID.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir()
        .join("ersatzrs-graphics")
        .join(format!("channel-{channel_id}"))
        .join(format!("item-{playout_item_id}"))
        .join(format!("request-{request_id}"))
}

fn spawn_stream_body(
    command: FfmpegCommand,
    cleanup_dirs: Vec<PathBuf>,
) -> Result<Body, StreamSpawnError> {
    let mut process = Command::new(&command.executable);
    process
        .args(&command.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    for variable in command.environment {
        process.env(variable.name, variable.value);
    }

    let mut child = process
        .spawn()
        .map_err(|error| StreamSpawnError::Spawn(error.to_string()))?;
    let stdout = child.stdout.take().ok_or(StreamSpawnError::MissingStdout)?;

    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::debug!(%status, "ffmpeg channel stream exited"),
            Err(error) => tracing::debug!(%error, "failed to wait for ffmpeg channel stream"),
        }
        for path in cleanup_dirs {
            if let Err(error) = tokio::fs::remove_dir_all(&path).await {
                if error.kind() != std::io::ErrorKind::NotFound {
                    tracing::debug!(%error, path = %path.display(), "failed to clean up IPTV graphics artifacts");
                }
            }
        }
    });

    Ok(Body::from_stream(ReaderStream::new(stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, Resolution,
        StreamingMode, WatermarkLocation, WatermarkSize,
    };
    use ersatzrs_ffmpeg::{FfmpegCapabilityReport, GraphicsPipelineOverlay, WatermarkOverlay};
    use std::path::{Path, PathBuf};
    use std::time::Duration;

    const TESTDATA_CANDIDATES: &[&str] = &[
        "testdata/bbb_sunflower_1080p_30fps_normal.mp4",
        "testdata/bbb_sunflower_1080p_60fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_30fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_60fps_normal.mp4",
        "../../testdata/bbb_sunflower_1080p_30fps_normal.mp4",
        "../../testdata/bbb_sunflower_1080p_60fps_normal.mp4",
        "../../testdata/bbb_sunflower_2160p_30fps_normal.mp4",
        "../../testdata/bbb_sunflower_2160p_60fps_normal.mp4",
    ];

    #[test]
    fn build_stream_command_pipes_mpegts_to_stdout() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            0,
            &profile,
            None,
            StreamingMode::TransportStream,
            FfmpegCapabilityReport::default(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-f mpegts"));
        assert!(arguments.ends_with("pipe:1"));
        assert!(!arguments.contains("-progress pipe:1"));
    }

    #[test]
    fn build_stream_command_can_target_mp4() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::Mp4,
            0,
            &profile,
            None,
            StreamingMode::HttpLiveStreamingDirect,
            FfmpegCapabilityReport::default(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-f mp4"));
        assert!(arguments.ends_with("pipe:1"));
    }

    #[test]
    fn build_stream_command_seeks_to_playout_offset() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            900,
            &profile,
            None,
            StreamingMode::TransportStream,
            FfmpegCapabilityReport::default(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-ss 900 -i /media/movie.mkv"));
    }

    #[test]
    fn build_stream_command_attaches_graphics_overlays() {
        let profile = playback_profile();
        let command = build_stream_command(
            "/media/movie.mkv",
            OutputFormat::MpegTs,
            0,
            &profile,
            None,
            StreamingMode::TransportStream,
            FfmpegCapabilityReport::default(),
            None,
            None,
            Vec::new(),
            vec![GraphicsPipelineOverlay::drawvg(
                8,
                "drawvg=file=/tmp/iptv-graphics/title.vgs",
            )],
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-filter_complex"));
        assert!(
            arguments.contains("drawvg=file=/tmp/iptv-graphics/title.vgs"),
            "{arguments}"
        );
    }

    #[test]
    fn playback_graphics_dir_uses_unique_request_subdirectories() {
        let first = playback_graphics_dir(10, 20);
        let second = playback_graphics_dir(10, 20);

        assert_ne!(first, second);
        assert!(first
            .parent()
            .is_some_and(|path| path.ends_with(PathBuf::from("channel-10/item-20"))));
        assert!(first
            .file_name()
            .is_some_and(|name| name.to_string_lossy().starts_with("request-")));
    }

    #[test]
    fn playout_start_seconds_matches_mid_item_offset() {
        let now = DateTime::parse_from_rfc3339("2026-05-31T21:45:00Z")
            .expect("valid timestamp")
            .with_timezone(&Utc);
        assert_eq!(playout_start_seconds(now, "2026-05-31 21:30:00"), 900);
        assert_eq!(playout_start_seconds(now, "2026-05-31 21:45:00"), 0);
        assert_eq!(playout_start_seconds(now, "not a timestamp"), 0);
    }

    #[test]
    fn output_format_helpers_match_hls_direct_variants() {
        assert_eq!(extension_for_output_format(OutputFormat::MpegTs), ".ts");
        assert_eq!(extension_for_output_format(OutputFormat::Mp4), ".mp4");
        assert_eq!(extension_for_output_format(OutputFormat::Mkv), ".mkv");
        assert_eq!(
            content_type_for_output_format(OutputFormat::MpegTs),
            "video/mp2t"
        );
        assert_eq!(
            content_type_for_output_format(OutputFormat::Mp4),
            "video/mp4"
        );
        assert_eq!(
            content_type_for_output_format(OutputFormat::Mkv),
            "video/x-matroska"
        );
    }

    #[test]
    fn detect_platform_picks_windows_for_drive_letters() {
        assert_eq!(detect_platform(r"D:\Movies"), PathPlatform::Windows);
        assert_eq!(detect_platform("/media/movies"), PathPlatform::Unix);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg on PATH and the Big Buck Bunny fixture under testdata/"]
    async fn stream_body_emits_non_empty_bytes_from_fixture() {
        use tokio::io::AsyncReadExt;
        use tokio::time::timeout;

        let Some(fixture) = TESTDATA_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
        else {
            eprintln!("skipping: no Big Buck Bunny fixture found in testdata/");
            return;
        };
        if tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_err()
        {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }

        let command = build_stream_command(
            fixture.to_string_lossy().as_ref(),
            OutputFormat::MpegTs,
            0,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            FfmpegCapabilityReport::default(),
            None,
            None,
            Vec::new(),
            Vec::new(),
        );
        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        let mut child = process.spawn().expect("spawn ffmpeg");
        let mut stdout = child.stdout.take().expect("stdout pipe");

        let mut buffer = vec![0u8; 64 * 1024];
        let read = timeout(Duration::from_secs(10), stdout.read(&mut buffer))
            .await
            .expect("timed out waiting for ffmpeg output")
            .expect("read ffmpeg stdout");
        assert!(read > 0, "expected some bytes from ffmpeg");

        child.kill().await.ok();
        let _ = child.wait().await;
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg on PATH and the Big Buck Bunny fixture under testdata/"]
    async fn stream_body_composites_watermark_from_fixture() {
        use tokio::io::AsyncReadExt;
        use tokio::time::timeout;

        let Some(fixture) = TESTDATA_CANDIDATES
            .iter()
            .map(PathBuf::from)
            .find(|p| p.is_file())
        else {
            eprintln!("skipping: no Big Buck Bunny fixture found in testdata/");
            return;
        };
        if tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .arg("-version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .await
            .is_err()
        {
            eprintln!("skipping: ffmpeg not on PATH");
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let logo = temp.path().join("logo.ppm");
        std::fs::write(&logo, red_ppm(64, 32)).expect("write watermark fixture");

        let command = build_stream_command(
            fixture.to_string_lossy().as_ref(),
            OutputFormat::MpegTs,
            0,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            FfmpegCapabilityReport::default(),
            None,
            None,
            vec![(
                InputFile::watermark(logo.to_string_lossy().into_owned()),
                WatermarkOverlay {
                    location: WatermarkLocation::TopLeft,
                    size: WatermarkSize::Scaled,
                    width_percent: 10.0,
                    horizontal_margin_percent: 0.0,
                    vertical_margin_percent: 0.0,
                    opacity: 100,
                    ..WatermarkOverlay::default()
                },
            )],
            Vec::new(),
        );
        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        for variable in command.environment {
            process.env(variable.name, variable.value);
        }
        let mut child = process.spawn().expect("spawn ffmpeg");
        let mut stdout = child.stdout.take().expect("stdout pipe");

        let mut buffer = vec![0u8; 2 * 1024 * 1024];
        let read = timeout(Duration::from_secs(20), stdout.read(&mut buffer))
            .await
            .expect("timed out waiting for ffmpeg output")
            .expect("read ffmpeg stdout");
        assert!(read > 0, "expected some bytes from ffmpeg");
        child.kill().await.ok();
        let _ = child.wait().await;

        let stream = temp.path().join("watermarked.ts");
        std::fs::write(&stream, &buffer[..read]).expect("write mpegts sample");

        let output = tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path())
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                stream.to_string_lossy().as_ref(),
                "-vf",
                "crop=160:80:0:0,scale=1:1,format=rgb24",
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .await
            .expect("decode watermarked frame");
        assert!(
            output.status.success(),
            "ffmpeg decode failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.stdout.len(), 3);
        assert!(
            output.stdout[0] > 180 && output.stdout[1] < 80 && output.stdout[2] < 80,
            "expected red watermark crop, got rgb={:?}",
            output.stdout
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg with drawvg on PATH"]
    async fn stream_body_composites_drawvg_graphics_from_generated_input() {
        let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path();
        if !ffmpeg_has_drawvg(&ffmpeg).await {
            eprintln!("skipping: ffmpeg drawvg filter not available");
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let input = temp.path().join("black.mp4");
        let script = temp.path().join("green.vgs");
        std::fs::write(&script, green_drawvg_script()).expect("write graphics script");
        generate_graphics_input(&ffmpeg, &input).await;

        let output = run_iptv_graphics_stream(&input, &script).await;
        assert!(!output.is_empty(), "expected MPEG-TS bytes");

        let stream = temp.path().join("graphics.ts");
        std::fs::write(&stream, &output).expect("write mpegts sample");
        assert_stream_has_green_graphics(&ffmpeg, &stream).await;
    }

    async fn ffmpeg_has_drawvg(ffmpeg: &str) -> bool {
        tokio::process::Command::new(ffmpeg)
            .args(["-hide_banner", "-filters"])
            .output()
            .await
            .is_ok_and(|output| String::from_utf8_lossy(&output.stdout).contains("drawvg"))
    }

    async fn generate_graphics_input(ffmpeg: &str, input: &Path) {
        let generated = tokio::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "color=c=black:s=320x180:r=25:d=1",
                "-f",
                "lavfi",
                "-i",
                "anullsrc=channel_layout=stereo:sample_rate=48000",
                "-t",
                "1",
                "-shortest",
                "-c:v",
                "libx264",
                "-pix_fmt",
                "yuv420p",
                "-c:a",
                "aac",
                input.to_string_lossy().as_ref(),
            ])
            .output()
            .await
            .expect("generate input video");
        assert!(
            generated.status.success(),
            "ffmpeg input generation failed: {}",
            String::from_utf8_lossy(&generated.stderr)
        );
    }

    async fn run_iptv_graphics_stream(input: &Path, script: &Path) -> Vec<u8> {
        let command = build_stream_command(
            input.to_string_lossy().as_ref(),
            OutputFormat::MpegTs,
            0,
            &playback_profile(),
            None,
            StreamingMode::TransportStream,
            FfmpegCapabilityReport::default(),
            None,
            None,
            Vec::new(),
            vec![GraphicsPipelineOverlay::drawvg(
                0,
                format!("drawvg=file={}", script.to_string_lossy()),
            )],
        );
        let mut process = Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for variable in command.environment {
            process.env(variable.name, variable.value);
        }
        let output = process.output().await.expect("run graphics stream");
        assert!(
            output.status.success(),
            "ffmpeg stream failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        output.stdout
    }

    async fn assert_stream_has_green_graphics(ffmpeg: &str, stream: &Path) {
        let decoded = tokio::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-i",
                stream.to_string_lossy().as_ref(),
                "-vf",
                "crop=80:80:0:0,scale=1:1,format=rgb24",
                "-frames:v",
                "1",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .await
            .expect("decode graphics frame");
        assert!(
            decoded.status.success(),
            "ffmpeg decode failed: {}",
            String::from_utf8_lossy(&decoded.stderr)
        );
        assert_eq!(decoded.stdout.len(), 3);
        assert!(
            decoded.stdout[1] > 180 && decoded.stdout[0] < 80 && decoded.stdout[2] < 80,
            "expected green graphics crop, got rgb={:?}",
            decoded.stdout
        );
    }

    fn red_ppm(width: usize, height: usize) -> Vec<u8> {
        let mut ppm = format!("P6\n{width} {height}\n255\n").into_bytes();
        ppm.extend(std::iter::repeat_n([255, 0, 0], width * height).flatten());
        ppm
    }

    fn green_drawvg_script() -> &'static str {
        "rect 0 0 480 270 setcolor #00ff00 fill\n"
    }

    fn playback_profile() -> PlaybackProfile {
        PlaybackProfile {
            profile: FFmpegProfile {
                video_format: FFmpegProfileVideoFormat::H264,
                audio_format: FFmpegProfileAudioFormat::Aac,
                video_profile: Some("high".to_owned()),
                video_preset: Some("veryfast".to_owned()),
                video_bitrate: 8_000,
                video_buffer_size: 16_000,
                audio_bitrate: 192,
                audio_buffer_size: 384,
                audio_channels: 2,
                audio_sample_rate: 48,
                ..FFmpegProfile::default()
            },
            resolution: Resolution {
                width: 1920,
                height: 1080,
                ..Resolution::default()
            },
        }
    }
}
