use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{
    atomic::{AtomicI64, Ordering},
    Arc,
};
use std::time::Duration;

use async_trait::async_trait;
use chrono::Utc;
use ersatzrs_core::hls::{HlsSessionHandle, HlsSessionRegistry};
use ersatzrs_core::path_replacement::{replace_path, PathPlatform, PathReplacement};
use ersatzrs_ffmpeg::{
    DesiredVideoState, FfmpegCommand, HlsOutput, InputFile, OutputFormat, PipelineBuilder,
    PipelineRequest, SoftwarePipelineBuilder,
};
use sqlx::{Row, SqlitePool};
use tokio::process::Command;
use tokio::sync::Notify;

/// Concrete HLS segmenter worker backed by `FFmpeg` CLI output in a temp folder.
pub struct HlsSessionWorker {
    channel_number: String,
    channel_id: i64,
    working_dir: PathBuf,
    path_replacements: Vec<PathReplacement>,
    last_access_ms: Arc<AtomicI64>,
    cancel: Arc<Notify>,
}

impl HlsSessionWorker {
    #[must_use]
    pub fn new(
        channel_number: String,
        channel_id: i64,
        working_dir: PathBuf,
        path_replacements: Vec<PathReplacement>,
    ) -> Arc<Self> {
        Arc::new(Self {
            channel_number,
            channel_id,
            working_dir,
            path_replacements,
            last_access_ms: Arc::new(AtomicI64::new(0)),
            cancel: Arc::new(Notify::new()),
        })
    }

    pub fn start(
        self: Arc<Self>,
        pool: SqlitePool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
    ) {
        tokio::spawn(async move {
            self.run(pool, registry, idle_timeout).await;
        });
    }

    async fn run(
        self: Arc<Self>,
        pool: SqlitePool,
        registry: Arc<dyn HlsSessionRegistry>,
        idle_timeout: Option<Duration>,
    ) {
        let _ = tokio::fs::create_dir_all(&self.working_dir).await;
        self.touch();

        let mut is_first = true;
        loop {
            if let Some(timeout) = idle_timeout {
                let timeout_ms = i64::try_from(timeout.as_millis()).unwrap_or(i64::MAX);
                let idle_for_ms =
                    Utc::now().timestamp_millis() - self.last_access_ms.load(Ordering::Relaxed);
                if idle_for_ms > timeout_ms {
                    break;
                }
            }

            let Some(media_path) =
                load_active_hls_item(&pool, self.channel_id, &self.path_replacements).await
            else {
                tokio::time::sleep(Duration::from_secs(1)).await;
                continue;
            };

            let command = build_hls_command(&media_path, &self.working_dir, is_first);
            let mut process = Command::new(&command.executable);
            process
                .args(&command.arguments)
                .current_dir(&self.working_dir)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .kill_on_drop(true);

            for variable in command.environment {
                process.env(variable.name, variable.value);
            }

            let Ok(mut child) = process.spawn() else {
                break;
            };

            tokio::select! {
                result = child.wait() => {
                    match result {
                        Ok(status) if status.success() => {}
                        Ok(_) | Err(_) => break,
                    }
                }
                () = self.cancel.notified() => {
                    break;
                }
            }

            is_first = false;
        }

        let _ = tokio::fs::remove_dir_all(&self.working_dir).await;
        registry.remove(&self.channel_number).await;
    }
}

#[async_trait]
impl HlsSessionHandle for HlsSessionWorker {
    fn channel_number(&self) -> &str {
        &self.channel_number
    }

    fn touch(&self) {
        self.last_access_ms
            .store(Utc::now().timestamp_millis(), Ordering::Relaxed);
    }

    async fn trim_playlist(&self) -> Option<String> {
        let playlist_path = self.working_dir.join("live.m3u8");
        let playlist = tokio::fs::read_to_string(playlist_path).await.ok()?;
        if playlist.lines().any(|line| line.starts_with("#EXTINF:")) {
            Some(playlist)
        } else {
            None
        }
    }
}

fn build_hls_command(
    media_path: &str,
    _working_dir: &Path,
    is_first_transcode: bool,
) -> FfmpegCommand {
    let input = InputFile::video(media_path.to_string());
    let mut request = PipelineRequest::new(input, DesiredVideoState::h264_1080p());
    request.state.output_format = OutputFormat::Hls;
    request.state.report_progress = false;
    request.state.hls = Some(HlsOutput {
        segment_template: "seg%05d.ts".to_string(),
        init_template: None,
        playlist_path: "live.m3u8".to_string(),
        segment_options: None,
        is_first_transcode,
        troubleshooting: false,
    });

    SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
}

async fn load_active_hls_item(
    pool: &SqlitePool,
    channel_id: i64,
    path_replacements: &[PathReplacement],
) -> Option<String> {
    let now_text = Utc::now().format("%Y-%m-%d %H:%M:%S").to_string();
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
    .await
    .ok()??;

    let media_path = row.get::<String, _>("MediaPath");
    if path_replacements.is_empty() {
        Some(media_path)
    } else {
        Some(replace_path(
            path_replacements,
            &media_path,
            PathPlatform::Unix,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_hls_command_produces_hls_flags() {
        let command = build_hls_command("/media/test.mkv", Path::new("/tmp/hls/1"), true);
        insta::assert_snapshot!(command.arguments.join(" "));
    }
}
