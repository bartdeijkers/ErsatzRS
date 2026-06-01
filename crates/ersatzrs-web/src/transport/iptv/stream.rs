use std::process::Stdio;

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
    FrameRate, InputFile, OutputFormat, PipelineRequest, SourceVideoStream,
};
use ersatzrs_infra::db::DbPool;
use ersatzrs_infra::playback::{
    load_channel_playback_profile, load_channel_target_frame_rate, load_source_video_stream,
    select_playback_subtitle, subtitle_fonts_dir, subtitle_input_for_stream, PlaybackProfile,
};
use sqlx::Row;
use tokio::process::Command;
use tokio_util::io::ReaderStream;

use crate::host::state::AppState;

pub async fn serve_transport_stream(state: &AppState, channel_number: &str) -> Response {
    serve_channel_stream(state, channel_number, OutputFormat::MpegTs).await
}

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
    let command = build_stream_command(
        &resolved,
        output_format,
        playout_start_seconds(now, &active.playout_start),
        &playback_profile,
        target_frame_rate,
        channel.streaming_mode,
        runtime_capabilities,
        source_stream,
        subtitle.as_ref().and_then(|subtitle| {
            subtitle_input_for_stream(subtitle, &resolved, output_format, channel.streaming_mode)
        }),
    );
    match spawn_stream_body(command) {
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
    pub(crate) media_path: String,
    pub(crate) media_item_id: i64,
    pub(crate) media_version_id: i64,
    pub(crate) playout_start: String,
    pub(crate) playout_finish: String,
    pub(crate) preferred_subtitle_language_code: Option<String>,
    pub(crate) subtitle_mode: Option<ChannelSubtitleMode>,
}

pub(crate) async fn load_active_media_path(
    pool: &DbPool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<ActivePlayoutItem>, sqlx::Error> {
    let now_text = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "MediaPath",
                  pi."MediaItemId" AS "MediaItemId",
                  mv."Id" AS "MediaVersionId",
                  pi."Start" AS "PlayoutStart",
                  pi."Finish" AS "PlayoutFinish",
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
        media_path: row.get::<String, _>("MediaPath"),
        media_item_id: row.get::<i64, _>("MediaItemId"),
        media_version_id: row.get::<i64, _>("MediaVersionId"),
        playout_start: row.get::<String, _>("PlayoutStart"),
        playout_finish: row.get::<String, _>("PlayoutFinish"),
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

pub(crate) fn playout_start_seconds(now: DateTime<Utc>, playout_start: &str) -> i64 {
    parse_sqlite_utc_timestamp(playout_start).map_or(0, |start| (now - start).num_seconds().max(0))
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

fn spawn_stream_body(command: FfmpegCommand) -> Result<Body, StreamSpawnError> {
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
    });

    Ok(Body::from_stream(ReaderStream::new(stdout)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ersatzrs_domain::{
        FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, Resolution,
        StreamingMode,
    };
    use ersatzrs_ffmpeg::FfmpegCapabilityReport;
    use std::path::PathBuf;
    use std::time::Duration;

    const TESTDATA_CANDIDATES: &[&str] = &[
        "testdata/bbb_sunflower_1080p_30fps_normal.mp4",
        "testdata/bbb_sunflower_1080p_60fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_30fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_60fps_normal.mp4",
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
        );
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-ss 900 -i /media/movie.mkv"));
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
        if tokio::process::Command::new(ersatzrs_ffmpeg::ffmpeg_path_from_env())
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
