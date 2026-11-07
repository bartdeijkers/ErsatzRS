use super::*;
use ersatzrs_ffmpeg::{FrameRate, FrameSize};
use std::{
    path::{Path, PathBuf},
    process::{Output, Stdio},
    time::Duration,
};
use tokio::process::Command;

async fn tool(mut command: Command) -> Output {
    command.stdin(Stdio::null()).kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(30), command.output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

async fn probe(binary: &Path, path: &Path) -> serde_json::Value {
    let mut command = Command::new(binary);
    command
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(path);
    serde_json::from_slice(&tool(command).await.stdout).unwrap()
}

// Explicit opt-in test inputs, not the application's configuration surface.
#[allow(
    clippy::disallowed_methods,
    reason = "Ignored local acceptance reads only explicit test tool and artifact inputs."
)]
fn inputs() -> (PathBuf, PathBuf, Option<PathBuf>) {
    (
        PathBuf::from(
            std::env::var_os("ERSATZRS_NEXT_TEST_FFMPEG").expect("verified FFmpeg required"),
        ),
        PathBuf::from(
            std::env::var_os("ERSATZRS_NEXT_TEST_FFPROBE").expect("verified ffprobe required"),
        ),
        std::env::var_os("ERSATZRS_NATIVE_FORMAT_ARTIFACT_DIR").map(PathBuf::from),
    )
}

#[tokio::test]
#[ignore = "requires checksum-verified FFmpeg9/ffprobe; Native non420/12-bit decoded playback"]
async fn native_non420_and_twelve_bit_actual_output_preserves_picture_audio_and_profile() {
    let (ffmpeg, ffprobe, artifacts) = inputs();
    let root = tempfile::tempdir().unwrap();
    for format in ["yuv444p", "yuv420p12le"] {
        let input = generate_input(&ffmpeg, root.path(), format).await;
        let info = probe(&ffprobe, &input).await;
        assert_eq!(info["streams"][3]["pix_fmt"], format);
        assert_eq!(info["streams"][1]["tags"]["language"], "nld");
        let request = playback_request(&input, format).await;
        if let Some(folder) = &artifacts {
            export_nvenc(folder, &input, &request, format);
        }
        let command = SoftwarePipelineBuilder
            .build(request.clone())
            .command(&request.inputs);
        let mut args = command.arguments;
        assert_eq!(args.pop().as_deref(), Some("pipe:1"));
        let output = root.path().join(format!("{format}-output.ts"));
        args.push(output.to_string_lossy().into_owned());
        let mut execute = Command::new(&ffmpeg);
        execute
            .args(&args)
            .envs(command.environment.iter().map(|e| (&e.name, &e.value)));
        tool(execute).await;
        verify_output(&ffmpeg, &ffprobe, &output).await;
    }
}

#[tokio::test]
#[ignore = "requires retained native Windows NVENC captures and verified decoder tools"]
async fn native_non420_and_twelve_bit_windows_nvenc_captures_preserve_picture_audio_and_profile() {
    let (ffmpeg, ffprobe, artifacts) = inputs();
    let folder = artifacts.expect("explicit generated-command/capture directory required");
    for format in ["yuv444p", "yuv420p12le"] {
        verify_output(
            &ffmpeg,
            &ffprobe,
            &folder.join(format!("{format}-nvenc.ts")),
        )
        .await;
    }
}

async fn generate_input(ffmpeg: &Path, root: &Path, format: &str) -> PathBuf {
    let input = root.join(format!("{format}.mkv"));
    let mut generate = Command::new(ffmpeg);
    generate.args([
        "-v",
        "error",
        "-nostdin",
        "-f",
        "lavfi",
        "-i",
        "color=blue:s=320x180:r=25:d=2",
    ]);
    for tone in [
        "sine=frequency=440:sample_rate=48000:duration=2",
        "sine=frequency=880:sample_rate=48000:duration=2",
        "sine=frequency=660:sample_rate=48000:duration=2",
    ] {
        generate.args(["-f", "lavfi", "-i", tone]);
    }
    generate
        .args([
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-map",
            "3:a",
            "-map",
            "0:v",
            "-c:a",
            "pcm_s16le",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=nld",
            "-metadata:s:a:2",
            "language=spa",
            "-c:v",
        ])
        .arg(if format.contains("12") {
            "libx265"
        } else {
            "libx264"
        })
        .args(["-preset", "ultrafast", "-threads", "2", "-pix_fmt", format]);
    if format.contains("12") {
        generate.args(["-x265-params", "pools=1:frame-threads=1:log-level=error"]);
    }
    generate.arg(&input);
    tool(generate).await;
    input
}

async fn playback_request(input: &Path, format: &str) -> PipelineRequest {
    let source = source(format).await;
    let mut desired = DesiredVideoState::h264_1080p();
    desired.frame_size = FrameSize::new(160, 90);
    desired.scaled_size = desired.frame_size;
    desired.frame_rate = Some(FrameRate::new(25, 1));
    desired.video_preset = Some("ultrafast".into());
    desired.audio_channels = Some(1);
    let mut audio = SourceAudioStream::new(SourceAudioCodec::Unknown).with_index(1);
    audio.channels = Some(1);
    audio.sample_rate = Some(48000);
    audio.language = Some("nld".into());
    let mut request = PipelineRequest::new(InputFile::video(input.to_str().unwrap()), desired)
        .with_source_video_stream(source)
        .with_source_audio_streams(vec![audio]);
    request.state.duration = Some(Duration::from_secs(2));
    request.state.thread_count = Some(2);
    request.state.report_progress = false;
    request.state.metadata.audio_language = Some("nld".into());
    request
}

fn export_nvenc(folder: &Path, input: &Path, request: &PipelineRequest, format: &str) {
    std::fs::create_dir_all(folder).unwrap();
    let copy = folder.join(format!("{format}.mkv"));
    std::fs::copy(input, &copy).unwrap();
    let mut request = request.clone();
    request.desired.video_preset = Some("p1".into());
    request.runtime_capabilities = Some(capability_report_for_tests());
    for input in &mut request.inputs {
        if matches!(
            input.kind,
            ersatzrs_ffmpeg::InputKind::Video | ersatzrs_ffmpeg::InputKind::Audio
        ) {
            input.path = copy.to_string_lossy().into_owned();
        }
    }
    let command = NvencPipelineBuilder
        .build(request.clone())
        .command(&request.inputs);
    let mut args = command.arguments;
    assert_eq!(args.pop().as_deref(), Some("pipe:1"));
    let output = folder.join(format!("{format}-nvenc.ts"));
    args.push(output.to_string_lossy().into_owned());
    let data = serde_json::json!({"format":format,"input":copy,"output":output,"arguments":args,
        "environment":command.environment.iter().map(|e|(&e.name,&e.value)).collect::<Vec<_>>(),
        "scope":"Platform-neutral Native builder command; actual Windows FFmpeg execution is separate. The synthetic capability report alone is not hardware evidence."});
    std::fs::write(
        folder.join(format!("{format}-command.json")),
        serde_json::to_vec_pretty(&data).unwrap(),
    )
    .unwrap();
}

async fn verify_output(ffmpeg: &Path, ffprobe: &Path, output: &Path) {
    let info = probe(ffprobe, output).await;
    let streams = info["streams"].as_array().unwrap();
    let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
    let audio = streams.iter().find(|s| s["codec_type"] == "audio").unwrap();
    assert_eq!(video["pix_fmt"], "yuv420p");
    assert_eq!(video["width"], 160);
    assert_eq!(video["height"], 90);
    assert_eq!(audio["tags"]["language"], "nld");
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(output)
        .args(["-map", "0:v:0", "-map", "0:a:0", "-f", "null", "-"]);
    tool(decode).await;
    let mut picture = Command::new(ffmpeg);
    picture
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(output)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let rgb = tool(picture).await.stdout;
    assert_eq!(rgb.len(), 160 * 90 * 3);
    assert!(
        rgb[2] > 180 && rgb[0] < 60,
        "decoded picture is the blue programme"
    );
    let mut sound = Command::new(ffmpeg);
    sound
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(output)
        .args([
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-c:a",
            "pcm_s16le",
            "-f",
            "s16le",
            "pipe:1",
        ]);
    let pcm = tool(sound).await.stdout;
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    assert!(samples.len() > 9600);
    let stable = &samples[4800..samples.len() - 4800];
    let crossings =
        u32::try_from(stable.windows(2).filter(|s| s[0] <= 0 && s[1] > 0).count()).unwrap();
    let frequency =
        f64::from(crossings) * 48000.0 / f64::from(u32::try_from(stable.len()).unwrap());
    assert!(
        (frequency - 880.0).abs() < 15.0,
        "selected Dutch880Hz, not440/660Hz: {frequency}"
    );
}
