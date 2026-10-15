//! Opt-in native software-pipeline bitmap caption proof. This component test
//! does not establish scanner selection, routed playback or engine acceptance.

use std::time::Duration;

use ersatzrs_domain::{FFmpegProfileAudioFormat, TeletextPageNumber};

use crate::{
    teletext_probe::{run_probe, TELETEXT_DECODE_FIXTURE},
    DesiredVideoState, FrameRate, FrameSize, InputFile, OutputFormat, PipelineBuilder as _,
    PipelineRequest, PixelFormat, SoftwarePipelineBuilder, SourceAudioCodec, SourceAudioStream,
    SourceVideoCodec, SourceVideoStream,
};

const WIDTH: usize = 492;
const HEIGHT: usize = 250;
const FRAME_BYTES: usize = WIDTH * HEIGHT * 3;
const MAX_TRANSPORT_BYTES: usize = 8 * 1024 * 1024;

async fn source(ffmpeg: &str) -> Vec<u8> {
    run_probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:s=492x250:r=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-t",
            "2",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "2:s:0",
            "-c:v",
            "mpeg2video",
            "-b:v",
            "1M",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-c:s",
            "copy",
            "-metadata:s:a:0",
            "language=eng",
            "-f",
            "mpegts",
            "pipe:1",
        ],
        TELETEXT_DECODE_FIXTURE,
        MAX_TRANSPORT_BYTES,
    )
    .await
    .expect("generate finite AV and independently authored teletext source")
}

async fn caption_text(ffmpeg: &str, source: &[u8]) -> String {
    let bytes = run_probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-txt_page:2",
            "888",
            "-txt_format:2",
            "text",
            "-txt_default_region:2",
            "-1",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-map",
            "0:2",
            "-c:s",
            "srt",
            "-f",
            "srt",
            "pipe:1",
        ],
        source,
        256 * 1024,
    )
    .await
    .expect("independent exact-page caption text decoding");
    String::from_utf8(bytes).unwrap()
}

fn command() -> crate::FfmpegCommand {
    let frame_size = FrameSize::new(492, 250);
    let mut desired = DesiredVideoState::h264_1080p();
    desired.scaled_size = frame_size;
    desired.frame_size = frame_size;
    desired.frame_rate = Some(FrameRate::new(25, 1));
    desired.normalize_colors = false;
    desired.deinterlace = false;
    desired.audio_format = FFmpegProfileAudioFormat::Copy;
    let mut video = SourceVideoStream::new(SourceVideoCodec::Mpeg2Video);
    video.frame_size = Some(frame_size);
    video.frame_rate = Some(FrameRate::new(25, 1));
    video.pixel_format = Some(PixelFormat::Yuv420P);
    video.sample_aspect_ratio = Some("1:1".to_owned());
    let mut request = PipelineRequest::new(
        InputFile::video("pipe:0").with_input_options(["-f", "mpegts"]),
        desired,
    )
    .with_source_video_stream(video)
    .with_source_audio_streams(vec![
        SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1)
    ]);
    request.inputs.push(InputFile::teletext_subtitle(
        "pipe:0",
        2,
        TeletextPageNumber::new(888).unwrap(),
    ));
    request.state.output_format = OutputFormat::MpegTs;
    request.state.duration = Some(Duration::from_secs(2));
    request.state.report_progress = false;
    request.state.thread_count = Some(1);
    SoftwarePipelineBuilder
        .build(request.clone())
        .command(&request.inputs)
}

fn verify_single_source(command: &crate::FfmpegCommand) {
    let args = &command.arguments;
    let inputs: Vec<_> = args.windows(2).filter(|pair| pair[0] == "-i").collect();
    assert_eq!(inputs.len(), 1, "shared pipe must be consumed exactly once");
    assert_eq!(inputs[0][1], "pipe:0");
    let input = args.iter().position(|argument| argument == "-i").unwrap();
    for (option, value) in [
        ("-txt_page:2", "888"),
        ("-txt_format:2", "bitmap"),
        ("-txt_default_region:2", "-1"),
    ] {
        assert!(args[..input].windows(2).any(|pair| pair == [option, value]));
        assert_eq!(
            args.iter().filter(|argument| *argument == option).count(),
            1
        );
    }
}

async fn selected_frame(ffmpeg: &str, bytes: &[u8], page: Option<&str>) -> Vec<u8> {
    let mut args = vec!["-hide_banner", "-loglevel", "error", "-nostdin"];
    if let Some(page) = page {
        args.extend([
            "-txt_page:2",
            page,
            "-txt_format:2",
            "bitmap",
            "-txt_default_region:2",
            "-1",
        ]);
    }
    args.extend(["-f", "mpegts", "-i", "pipe:0"]);
    if page.is_some() {
        // Independent decoder/overlay oracle, without the application pipeline.
        // Explicitly fit and transparently pad the possibly header-chopped
        // bitmap before centering it on the same native output canvas.
        args.extend([
            "-filter_complex",
            "[0:2]scale=492:250:force_original_aspect_ratio=decrease,pad=w=492:h=250:x=-1:y=-1:color=black@0[caption];[0:0][caption]overlay=x=(W-w)/2:y=(H-h)/2:format=0,select=between(t\\,1.59\\,1.61)[frame]",
            "-map",
            "[frame]",
        ]);
    } else {
        args.extend(["-map", "0:v:0", "-vf", "select=between(t\\,1.59\\,1.61)"]);
    }
    // Sample the same media-relative 1.6s window for every role. MPEG-TS source
    // and video start times can differ; CFR may duplicate leading frames, so
    // equal frame ordinals do not establish the same subtitle observation time.
    // Select one frame while consuming the entire bounded source. Early exit
    // could break the probe's stdin writer and mask the actual decoder result.
    args.extend([
        "-an",
        "-fps_mode",
        "passthrough",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    let pixels = run_probe(ffmpeg, &args, bytes, FRAME_BYTES)
        .await
        .expect("decode exact-frame bitmap proof");
    assert_eq!(pixels.len(), FRAME_BYTES);
    pixels
}

fn compare_pixels(actual: &[u8], selected: &[u8], other_page: &[u8], plain: &[u8]) {
    let foreground = |pixel: &[u8]| pixel.iter().any(|value| *value > 160);
    let selected_cells = selected
        .chunks_exact(3)
        .filter(|pixel| foreground(pixel))
        .count();
    assert!(
        selected_cells > 500,
        "independent page888 must contain visible caption pixels"
    );
    let disagreements = actual
        .chunks_exact(3)
        .zip(selected.chunks_exact(3))
        .filter(|(actual, expected)| foreground(actual) != foreground(expected))
        .count();
    assert!(
        disagreements <= selected_cells / 5 + 300,
        "native bitmap caption positions differ from independently decoded page888"
    );
    let difference = |left: &[u8], right: &[u8]| -> u64 {
        left.iter()
            .zip(right)
            .map(|(left, right)| u64::from(left.abs_diff(*right)))
            .sum()
    };
    let selected_error = difference(actual, selected);
    assert!(
        selected_error < difference(actual, other_page) / 2,
        "burn must select888 rather than the main200 page"
    );
    assert!(
        selected_error < difference(actual, plain) / 2,
        "burn must not silently omit captions"
    );
    assert!(
        plain.iter().all(|value| *value <= 20),
        "unburned source remains the black calibration video"
    );
}

async fn verify_audio(ffmpeg: &str, bytes: &[u8]) {
    let pcm = run_probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "pipe:1",
        ],
        bytes,
        1024 * 1024,
    )
    .await
    .expect("decode selected source audio");
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()))
        .collect();
    let matching = samples
        .chunks_exact(4800)
        .filter(|chunk| {
            let crossings = chunk
                .windows(2)
                .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
                .count();
            (crossings * 48000 / (2 * chunk.len())).abs_diff(440) <= 20
                && chunk.iter().any(|sample| sample.unsigned_abs() > 100)
        })
        .count();
    assert!(
        matching >= 10,
        "source440Hz audio must survive caption burn"
    );
}

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit opt-in pinned native test tool; never changes production configuration"
)]
#[tokio::test]
#[ignore = "requires explicitly pinned FFmpeg9 with libzvbi and libx264; native software-pipeline component proof only"]
async fn native_bitmap_burn_selects_exact_caption_page_on_one_shared_pipe() {
    let ffmpeg = std::env::var("ERSATZRS_TELETEXT_TEST_FFMPEG")
        .expect("select pinned native FFmpeg for this ignored component test");
    assert_eq!(crate::ffmpeg_path(), ffmpeg);
    let version = run_probe(&ffmpeg, &["-version"], &[], 65536).await.unwrap();
    let version = String::from_utf8(version).unwrap();
    assert_eq!(
        crate::version::parse_version(version.lines().next().unwrap())
            .unwrap()
            .major,
        9
    );
    let root = tempfile::tempdir().unwrap();
    let source = source(&ffmpeg).await;
    std::fs::write(root.path().join("synthetic-av-caption.ts"), &source).unwrap();
    let text = caption_text(&ffmpeg, &source).await;
    assert!(text.contains("Synthetic subtitle page 888"));
    assert!(
        !text.contains("TV GUIDE INDEX"),
        "exact caption text must not mix main200"
    );
    let command = command();
    assert!(command.environment.is_empty());
    verify_single_source(&command);
    let arguments: Vec<_> = command.arguments.iter().map(String::as_str).collect();
    let burned = run_probe(&ffmpeg, &arguments, &source, MAX_TRANSPORT_BYTES)
        .await
        .expect("actual software pipeline must burn selected bitmap caption");
    let actual = selected_frame(&ffmpeg, &burned, None).await;
    let caption = selected_frame(&ffmpeg, &source, Some("888")).await;
    let main_page = selected_frame(&ffmpeg, &source, Some("200")).await;
    let plain = selected_frame(&ffmpeg, &source, None).await;
    // Bounded failure evidence preserves the exact source, generated command
    // and independently sampled 492x250 RGB24 observations from the common
    // media-relative 1.59..1.61s window. Equal first PTS is not assumed.
    std::fs::write(
        root.path().join("argv.json"),
        serde_json::to_vec_pretty(&command.arguments).unwrap(),
    )
    .unwrap();
    std::fs::write(root.path().join("burned.ts"), &burned).unwrap();
    for (name, pixels) in [
        ("actual-media1_6.rgb", &actual),
        ("page888-media1_6.rgb", &caption),
        ("page200-media1_6.rgb", &main_page),
        ("plain-media1_6.rgb", &plain),
    ] {
        assert_eq!(pixels.len(), FRAME_BYTES);
        std::fs::write(root.path().join(name), pixels).unwrap();
    }
    if let Err(failure) =
        std::panic::catch_unwind(|| compare_pixels(&actual, &caption, &main_page, &plain))
    {
        let retained = root.keep();
        eprintln!(
            "Native caption comparison failed; bounded 25fps media-relative 1.59..1.61s evidence retained at {}",
            retained.display()
        );
        std::panic::resume_unwind(failure);
    }
    verify_audio(&ffmpeg, &source).await;
    verify_audio(&ffmpeg, &burned).await;
}
