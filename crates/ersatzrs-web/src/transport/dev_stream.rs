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
    DesiredVideoState, FfmpegCommand, GraphicsPipelineOverlay, InputFile, OutputFormat,
    PipelineBuilder, PipelineRequest, SoftwarePipelineBuilder, WatermarkFadeKind,
    WatermarkFadePoint, WatermarkOverlay,
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
    let command = build_mpegts_stream_command(path, watermark_inputs, Vec::new());
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
    graphics_overlays: Vec<GraphicsPipelineOverlay>,
) -> FfmpegCommand {
    let input = InputFile::video(path.to_string_lossy().into_owned());
    let mut request = PipelineRequest::new(input, DesiredVideoState::h264_1080p());
    for (input, overlay) in watermark_inputs {
        request.inputs.push(input);
        request.watermarks.push(overlay);
    }
    request.graphics_overlays.extend(graphics_overlays);
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
        let command =
            build_mpegts_stream_command(Path::new("/media/bbb.mp4"), Vec::new(), Vec::new());
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
            Vec::new(),
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-loop 1 -i /media/logo.png"));
        assert!(arguments.contains("scale=240:-1"));
        assert!(arguments.contains("geq=a='clip(255*(if(t < 10, 1, 0)),0,255)'"));
        assert!(arguments.contains("overlay=x=W-w-38:y=H-h-22:format=0"));
    }

    #[test]
    fn stream_command_can_attach_graphics_overlays() {
        let command = build_mpegts_stream_command(
            Path::new("/media/bbb.mp4"),
            Vec::new(),
            vec![GraphicsPipelineOverlay::drawvg(
                5,
                "drawvg=file=/tmp/dev-graphics/title.vgs",
            )],
        );
        let arguments = command.arguments.join(" ");

        assert!(arguments.contains("-filter_complex"));
        assert!(
            arguments.contains("drawvg=file=/tmp/dev-graphics/title.vgs"),
            "{arguments}"
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    #[ignore = "requires ffmpeg with drawvg on PATH"]
    async fn dev_stream_composites_drawvg_graphics_from_generated_input() {
        let ffmpeg = ersatzrs_ffmpeg::ffmpeg_path_from_env();
        if !ffmpeg_has_drawvg(&ffmpeg).await {
            eprintln!("skipping: ffmpeg drawvg filter not available");
            return;
        }

        let temp = tempfile::tempdir().expect("tempdir");
        let input = temp.path().join("black.mp4");
        let script = temp.path().join("green.vgs");
        std::fs::write(&script, green_drawvg_script()).expect("write graphics script");
        generate_dev_graphics_input(&ffmpeg, &input).await;

        let output = run_dev_graphics_stream(&input, &script).await;
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

    async fn generate_dev_graphics_input(ffmpeg: &str, input: &Path) {
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

    async fn run_dev_graphics_stream(input: &Path, script: &Path) -> Vec<u8> {
        let command = build_mpegts_stream_command(
            input,
            Vec::new(),
            vec![GraphicsPipelineOverlay::drawvg(
                0,
                format!("drawvg=file={}", script.to_string_lossy()),
            )],
        );
        let mut process = tokio::process::Command::new(&command.executable);
        process
            .args(&command.arguments)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        for variable in command.environment {
            process.env(variable.name, variable.value);
        }
        let output = process.output().await.expect("run dev graphics stream");
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
            .expect("decode dev graphics frame");
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

    #[test]
    fn fixture_search_prefers_1080p_30fps_file_when_available() {
        if let Some(fixture) = find_big_buck_bunny_fixture() {
            assert_eq!(
                fixture,
                PathBuf::from("testdata/bbb_sunflower_1080p_30fps_normal.mp4")
            );
        }
    }

    fn green_drawvg_script() -> &'static str {
        "rect 0 0 480 270 setcolor #00ff00 fill\n"
    }
}
