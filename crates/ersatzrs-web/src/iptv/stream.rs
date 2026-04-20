use std::process::Stdio;

use axum::{
    body::Body,
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
};
use chrono::{DateTime, Utc};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_core::repositories::{ChannelRepository, MediaSourceRepository};
use ersatzrs_domain::{EmbyMediaSource, JellyfinMediaSource, PlexMediaSource};
use ersatzrs_ffmpeg::{
    DesiredVideoState, FfmpegCommand, InputFile, OutputFormat, PipelineBuilder, PipelineRequest,
    SoftwarePipelineBuilder,
};
use sqlx::{Row, SqlitePool};
use tokio::process::Command;
use tokio_util::io::ReaderStream;

use crate::state::AppState;

pub async fn serve_transport_stream(state: &AppState, channel_number: &str) -> Response {
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

    let command = build_stream_command(&resolved);
    match spawn_stream_body(command) {
        Ok(body) => {
            let mut response = body.into_response();
            response
                .headers_mut()
                .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
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
struct ActivePlayoutItem {
    media_path: String,
}

async fn load_active_media_path(
    pool: &SqlitePool,
    channel_id: i64,
    now: DateTime<Utc>,
) -> Result<Option<ActivePlayoutItem>, sqlx::Error> {
    let now_text = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let row = sqlx::query(
        r#"SELECT mf."Path" AS "MediaPath"
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
    }))
}

async fn load_path_replacements(
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

fn build_stream_command(path: &str) -> FfmpegCommand {
    let input = InputFile::video(path.to_string());
    let mut request = PipelineRequest::new(input, DesiredVideoState::h264_1080p());
    request.state.output_format = OutputFormat::MpegTs;
    request.state.report_progress = false;

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    pipeline.command(&request.inputs)
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
        let command = build_stream_command("/media/movie.mkv");
        let arguments = command.arguments.join(" ");
        assert!(arguments.contains("-f mpegts"));
        assert!(arguments.ends_with("pipe:1"));
        assert!(!arguments.contains("-progress pipe:1"));
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
        if tokio::process::Command::new("ffmpeg")
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

        let command = build_stream_command(fixture.to_string_lossy().as_ref());
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
}
