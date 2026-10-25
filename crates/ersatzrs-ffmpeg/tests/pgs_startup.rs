//! Actual Native output before a late PGS cue on 4K video with `TrueHD` audio.

#[path = "../../../tests/support/late_pgs.rs"]
mod late_pgs;

use ersatzrs_domain::{
    FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, Resolution, StreamingMode,
};
use ersatzrs_ffmpeg::{
    DesiredVideoState, FrameRate, FrameSize, InputFile, PipelineBuilder, PipelineRequest,
    PixelFormat, SoftwarePipelineBuilder, SourceAudioCodec, SourceAudioStream, SourceVideoCodec,
    SourceVideoStream, SubtitleMethod,
};
use std::{
    path::{Path, PathBuf},
    time::Duration,
};

fn request(clip: &Path, burn: bool, seek: u64) -> PipelineRequest {
    let path = clip.to_str().unwrap();
    let mut video = SourceVideoStream::new(SourceVideoCodec::H264);
    video.frame_size = Some(FrameSize::new(3840, 2080));
    video.pixel_format = Some(PixelFormat::Yuv420P);
    video.frame_rate = Some(FrameRate::new(24, 1));
    video.sample_aspect_ratio = Some("1:1".into());
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        transcode_video: true,
        video_preset: Some("ultrafast".into()),
        video_bitrate: 8000,
        video_buffer_size: 16000,
        audio_format: FFmpegProfileAudioFormat::Aac,
        transcode_audio: true,
        audio_sample_rate: 48,
        audio_channels: 1,
        audio_bitrate: 192,
        audio_buffer_size: 384,
        normalize_colors: false,
        normalize_framerate: true,
        ..Default::default()
    };
    let desired = DesiredVideoState::from_profile(
        &profile,
        &Resolution {
            width: 1280,
            height: 720,
            ..Default::default()
        },
        Some(&video),
        Some(FrameRate::new(24, 1)),
        StreamingMode::TransportStream,
    );
    let mut audio = SourceAudioStream::new(SourceAudioCodec::Unknown).with_index(2);
    audio.channels = Some(1);
    audio.sample_rate = Some(48000);
    audio.language = Some("nld".into());
    let mut request = PipelineRequest::new(InputFile::video(path), desired)
        .with_source_video_stream(video)
        .with_source_audio_streams(vec![audio]);
    request.state.report_progress = false;
    request.state.thread_count = Some(2);
    request.state.duration = Some(Duration::from_secs(4));
    request.state.start = (seek != 0).then(|| Duration::from_secs(seek));
    request.state.metadata.audio_language = Some("nld".into());
    if burn {
        request.inputs.push(
            InputFile::image_subtitle(path, SubtitleMethod::Burn)
                .with_subtitle_codec("hdmv_pgs_subtitle")
                .with_stream_index(3),
        );
    }
    request
}

#[tokio::test]
#[ignore = "requires packaged FFmpeg9 and ffprobe; synthetic 4K/TrueHD late PGS proof"]
#[allow(
    clippy::disallowed_methods,
    reason = "manual acceptance receives exact runtime paths from its runner"
)]
async fn native_4k_truehd_starts_before_first_pgs_and_preserves_late_picture_audio() {
    let ffmpeg =
        PathBuf::from(std::env::var_os("ERSATZRS_FFMPEG").expect("packaged FFmpeg required"));
    let ffprobe =
        PathBuf::from(std::env::var_os("ERSATZRS_FFPROBE").expect("packaged ffprobe required"));
    let root = tempfile::tempdir().unwrap().keep();
    println!("Native late-PGS fixture retained at {}", root.display());
    let clip = late_pgs::fixture(&ffmpeg, &root).await;
    late_pgs::assert_unknown_pgs(&ffprobe, &clip, &root).await;
    let mut metrics = Vec::new();
    for (name, burn, seek) in [
        ("before-off", false, 0),
        ("before-burn", true, 0),
        ("after-burn", true, 20),
        ("after-off", false, 20),
    ] {
        let request = request(&clip, burn, seek);
        let pipeline = SoftwarePipelineBuilder.build(request.clone());
        let command = pipeline.command(&request.inputs);
        let mut child = tokio::process::Command::new(&ffmpeg);
        child.args(&command.arguments);
        let (ts, elapsed) = late_pgs::run(child, &root, name, Duration::from_secs(30)).await;
        assert!(ts.len() > 188 * 10, "missing actual MPEG-TS output");
        let output = root.join(format!("{name}.ts"));
        std::fs::write(&output, ts).unwrap();
        late_pgs::assert_picture_and_audio(&ffmpeg, &output, &root, name, burn && seek != 0).await;
        metrics.push(serde_json::json!({"case":name,"four_second_generation_seconds":elapsed.as_secs_f64(),"argv":command.arguments}));
    }
    std::fs::write(
        root.join("results.json"),
        serde_json::to_vec_pretty(&metrics).unwrap(),
    )
    .unwrap();
    println!("Native late-PGS proof retained at {}", root.display());
    println!("{}", serde_json::to_string(&metrics.iter().map(|row| serde_json::json!({"case":row["case"],"four_second_generation_seconds":row["four_second_generation_seconds"]})).collect::<Vec<_>>()).unwrap());
}
