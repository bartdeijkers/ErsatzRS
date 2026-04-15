use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use axum::{
    body::Body,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};
use ersatzrs_ffmpeg::{
    DesiredVideoState, FfmpegCommand, InputFile, OutputFormat, PipelineBuilder, PipelineRequest,
    SoftwarePipelineBuilder,
};
use tokio::process::Command;
use tokio_util::io::ReaderStream;

const TESTDATA_CANDIDATES: &[&str] = &[
    "testdata/bbb_sunflower_1080p_30fps_normal.mp4",
    "testdata/bbb_sunflower_1080p_60fps_normal.mp4",
    "testdata/bbb_sunflower_2160p_30fps_normal.mp4",
    "testdata/bbb_sunflower_2160p_60fps_normal.mp4",
];

#[derive(Debug, thiserror::Error)]
pub enum StreamError {
    #[error("no Big Buck Bunny test MP4 was found in testdata")]
    MissingTestFixture,
    #[error("failed to start ffmpeg: {0}")]
    Spawn(std::io::Error),
    #[error("ffmpeg did not expose stdout")]
    MissingStdout,
}

impl IntoResponse for StreamError {
    fn into_response(self) -> axum::response::Response {
        let status = match self {
            Self::MissingTestFixture => StatusCode::NOT_FOUND,
            Self::Spawn(_) | Self::MissingStdout => StatusCode::INTERNAL_SERVER_ERROR,
        };

        (status, self.to_string()).into_response()
    }
}

pub fn stream_big_buck_bunny() -> Result<Response<Body>, StreamError> {
    let input_path = find_big_buck_bunny_fixture().ok_or(StreamError::MissingTestFixture)?;
    stream_mpegts_file(&input_path)
}

fn stream_mpegts_file(path: &Path) -> Result<Response<Body>, StreamError> {
    let command = build_mpegts_stream_command(path);
    let body = spawn_ffmpeg_body(command)?;

    let mut response = Response::new(body);
    response
        .headers_mut()
        .insert(header::CONTENT_TYPE, HeaderValue::from_static("video/mp2t"));
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    Ok(response)
}

fn build_mpegts_stream_command(path: &Path) -> FfmpegCommand {
    let input = InputFile::video(path.to_string_lossy().into_owned());
    let mut request = PipelineRequest::new(input, DesiredVideoState::h264_1080p());
    request.state.output_format = OutputFormat::MpegTs;
    request.state.report_progress = false;

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    pipeline.command(&request.inputs)
}

fn spawn_ffmpeg_body(command: FfmpegCommand) -> Result<Body, StreamError> {
    let mut process = Command::new(&command.executable);
    process
        .args(&command.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .kill_on_drop(true);

    for variable in command.environment {
        process.env(variable.name, variable.value);
    }

    let mut child = process.spawn().map_err(StreamError::Spawn)?;
    let stdout = child.stdout.take().ok_or(StreamError::MissingStdout)?;

    tokio::spawn(async move {
        match child.wait().await {
            Ok(status) if status.success() => {}
            Ok(status) => tracing::debug!(%status, "ffmpeg stream process exited"),
            Err(error) => tracing::debug!(%error, "failed to wait for ffmpeg stream process"),
        }
    });

    Ok(Body::from_stream(ReaderStream::new(stdout)))
}

fn find_big_buck_bunny_fixture() -> Option<PathBuf> {
    TESTDATA_CANDIDATES
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_command_uses_mpegts_stdout_without_progress_stdout() {
        let command = build_mpegts_stream_command(Path::new("/media/bbb.mp4"));
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-f mpegts"));
        assert!(arguments.ends_with("pipe:1"));
        assert!(!arguments.contains("-progress pipe:1"));
    }

    #[test]
    fn fixture_search_prefers_1080p_30fps_file_when_available() {
        if let Some(fixture) = find_big_buck_bunny_fixture() {
            assert_eq!(
                fixture,
                PathBuf::from("testdata/bbb_sunflower_1080p_30fps_normal.mp4")
            );
        }
    }
}
