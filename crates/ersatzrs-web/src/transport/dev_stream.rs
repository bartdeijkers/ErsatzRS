use std::{
    path::{Path, PathBuf},
    process::Stdio,
};

use axum::{
    body::Body,
    extract::Query,
    http::{header, HeaderValue, Response, StatusCode},
    response::IntoResponse,
};
use ersatzrs_domain::{WatermarkLocation, WatermarkSize};
use ersatzrs_ffmpeg::{
    DesiredVideoState, FfmpegCommand, InputFile, OutputFormat, PipelineBuilder, PipelineRequest,
    SoftwarePipelineBuilder, WatermarkFadeKind, WatermarkFadePoint, WatermarkOverlay,
};
use serde::Deserialize;
use tokio::process::Command;
use tokio_util::io::ReaderStream;

const TESTDATA_CANDIDATES: &[&str] = &[
    "testdata/bbb_sunflower_1080p_30fps_normal.mp4",
    "testdata/bbb_sunflower_1080p_60fps_normal.mp4",
    "testdata/bbb_sunflower_2160p_30fps_normal.mp4",
    "testdata/bbb_sunflower_2160p_60fps_normal.mp4",
];

#[derive(Debug, Clone, Deserialize, Default)]
pub struct DevStreamQuery {
    watermark: Option<String>,
    watermark_mode: Option<String>,
    opacity_expression: Option<String>,
    frequency_minutes: Option<u64>,
    duration_seconds: Option<u64>,
    width_percent: Option<f64>,
    opacity: Option<u8>,
}

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

pub async fn stream_big_buck_bunny(
    Query(query): Query<DevStreamQuery>,
) -> Result<Response<Body>, StreamError> {
    let input_path = find_big_buck_bunny_fixture().ok_or(StreamError::MissingTestFixture)?;
    stream_mpegts_file(&input_path, dev_watermark_inputs(&query))
}

fn stream_mpegts_file(
    path: &Path,
    watermark_inputs: Vec<(InputFile, WatermarkOverlay)>,
) -> Result<Response<Body>, StreamError> {
    let command = build_mpegts_stream_command(path, watermark_inputs);
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

fn build_mpegts_stream_command(
    path: &Path,
    watermark_inputs: Vec<(InputFile, WatermarkOverlay)>,
) -> FfmpegCommand {
    let input = InputFile::video(path.to_string_lossy().into_owned());
    let mut request = PipelineRequest::new(input, DesiredVideoState::h264_1080p());
    for (input, overlay) in watermark_inputs {
        request.inputs.push(input);
        request.watermarks.push(overlay);
    }
    request.state.output_format = OutputFormat::MpegTs;
    request.state.report_progress = false;

    let pipeline = SoftwarePipelineBuilder.build(request.clone());
    pipeline.command(&request.inputs)
}

fn dev_watermark_inputs(query: &DevStreamQuery) -> Vec<(InputFile, WatermarkOverlay)> {
    let Some(path) = query
        .watermark
        .as_deref()
        .map(str::trim)
        .filter(|path| !path.is_empty())
    else {
        return Vec::new();
    };

    let mut overlay = WatermarkOverlay {
        location: WatermarkLocation::BottomRight,
        size: WatermarkSize::Scaled,
        width_percent: query.width_percent.unwrap_or(10.0),
        opacity: query.opacity.unwrap_or(100).min(100),
        ..WatermarkOverlay::default()
    };
    match query
        .watermark_mode
        .as_deref()
        .unwrap_or("permanent")
        .to_ascii_lowercase()
        .as_str()
    {
        "intermittent" => {
            overlay.fade_points = dev_fade_points(
                query.frequency_minutes.unwrap_or(15),
                query.duration_seconds.unwrap_or(15),
            );
        }
        "opacity-expression" | "opacity_expression" | "expression" => {
            overlay
                .opacity_expression
                .clone_from(&query.opacity_expression);
        }
        _ => {}
    }
    vec![(InputFile::watermark(path.to_owned()), overlay)]
}

fn dev_fade_points(frequency_minutes: u64, duration_seconds: u64) -> Vec<WatermarkFadePoint> {
    if frequency_minutes == 0 || duration_seconds == 0 {
        return Vec::new();
    }
    let visible = std::time::Duration::from_secs(duration_seconds);
    let frequency = std::time::Duration::from_secs(frequency_minutes.saturating_mul(60));
    let mut points = Vec::new();
    let mut start = std::time::Duration::ZERO;
    while start < std::time::Duration::from_hours(24) {
        let hold_finish = start
            .saturating_add(std::time::Duration::from_secs(1))
            .saturating_add(visible);
        points.push(WatermarkFadePoint {
            kind: WatermarkFadeKind::In,
            time: start,
            enable_start: start,
            enable_finish: start.saturating_add(std::time::Duration::from_secs(1)),
        });
        points.push(WatermarkFadePoint {
            kind: WatermarkFadeKind::Out,
            time: hold_finish,
            enable_start: hold_finish,
            enable_finish: hold_finish.saturating_add(std::time::Duration::from_secs(1)),
        });
        start = start.saturating_add(frequency);
    }
    points
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
        let command = build_mpegts_stream_command(Path::new("/media/bbb.mp4"), Vec::new());
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-f mpegts"));
        assert!(arguments.ends_with("pipe:1"));
        assert!(!arguments.contains("-progress pipe:1"));
    }

    #[test]
    fn stream_command_can_attach_dev_watermark() {
        let command = build_mpegts_stream_command(
            Path::new("/media/bbb.mp4"),
            dev_watermark_inputs(&DevStreamQuery {
                watermark: Some("/media/logo.png".to_owned()),
                watermark_mode: Some("opacity-expression".to_owned()),
                opacity_expression: Some("if(time_of_day_seconds < 10, 1, 0)".to_owned()),
                width_percent: Some(12.5),
                ..DevStreamQuery::default()
            }),
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-loop 1 -i /media/logo.png"));
        assert!(arguments.contains("scale=240:-1"));
        assert!(arguments.contains("geq=a='clip(255*(if(t < 10, 1, 0)),0,255)'"));
        assert!(arguments.contains("overlay=x=W-w-38:y=H-h-22:format=0"));
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
