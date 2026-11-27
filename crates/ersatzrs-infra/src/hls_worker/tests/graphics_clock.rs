use super::*;

fn graphics_request(
    input: InputFile,
    timeline: HlsInputTimeline,
    elapsed: Duration,
    source_seek: Duration,
) -> (HardwareAccelerationKind, PipelineRequest) {
    let mut profile = playback_profile();
    profile.resolution.width = 320;
    profile.resolution.height = 180;
    build_hls_request_with_input_explicit_discontinuity(
        input,
        OutputFormat::Hls,
        true,
        false,
        false,
        source_seek,
        Some(Duration::from_secs(3)),
        None,
        &profile,
        Some(FrameRate::new(25, 1)),
        OutputMetadata::default(),
        FfmpegCapabilityReport::default(),
        Some(source_320p()),
        Vec::new(),
        PlaybackAudioSelection::Disabled,
        None,
        Vec::new(),
        vec![GraphicsPipelineOverlay::drawvg(0, "drawvg=file=rating.vgs")],
        Some(elapsed),
        HlsDiscontinuity::Absent,
        None,
        timeline,
        None,
    )
}

#[test]
fn native_hls_graphics_clock_accounts_for_input_and_output_seeks() {
    let elapsed = Duration::from_millis(44_250);
    let seek = elapsed + Duration::from_secs(10);
    let mut filters = Vec::new();
    for (input, timeline) in [
        (InputFile::video("movie.mkv"), HlsInputTimeline::Original),
        (
            InputFile::video("pipe:0").with_seek_after_input(),
            HlsInputTimeline::Original,
        ),
        (
            InputFile::video("pipe:0"),
            HlsInputTimeline::RetainedIntervals,
        ),
    ] {
        let (_, request) = graphics_request(input, timeline, elapsed, seek);
        let GraphicsPipelineOverlay::Drawvg { filter, .. } = &request.graphics_overlays[0] else {
            panic!("expected the rating overlay");
        };
        filters.push(filter.clone());
    }
    insta::assert_snapshot!(filters.join("\n"), @"
    settb=AVTB,setpts=PTS+44.25/TB,drawvg=file=rating.vgs,setpts=PTS-44.25/TB
    settb=AVTB,setpts=PTS-10/TB,drawvg=file=rating.vgs,setpts=PTS+10/TB
    settb=AVTB,setpts=PTS+44.25/TB,drawvg=file=rating.vgs,setpts=PTS-44.25/TB
    ");
}

#[tokio::test]
#[ignore = "requires FFmpeg with drawvg and libx264"]
async fn native_hls_graphics_clock_follows_one_programme_across_encoder_starts() {
    let temp = tempfile::tempdir().unwrap();
    let ffmpeg = ffmpeg_path();
    let source = temp.path().join("source.mkv");
    let mut generate = Command::new(&ffmpeg);
    generate
        .args([
            "-loglevel",
            "error",
            "-f",
            "lavfi",
            "-i",
            "color=black:s=320x180:r=25:d=1215",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "1",
            "-g",
            "100",
        ])
        .arg(&source);
    apply_ffmpeg_library_path(&mut generate);
    let output = generate.output().await.unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    // The scheduled programme starts ten seconds into the source. The last
    // generation crosses the 20-minute repeat while its encoder is running.
    for (elapsed, visible) in [
        (0, [true, true, true]),
        (44, [false, false, false]),
        (185, [false, false, false]),
        (1199, [false, false, true]),
    ] {
        let dir = temp.path().join(elapsed.to_string());
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(
            dir.join("rating.vgs"),
            "if (gte(mod(t,1200),6.5)) { break }\nrect 0 0 80 80 setcolor #ff0000 fill\n",
        )
        .unwrap();
        let (acceleration, request) = graphics_request(
            InputFile::video(source.to_str().unwrap()),
            HlsInputTimeline::Original,
            Duration::from_secs(elapsed),
            Duration::from_secs(elapsed + 10),
        );
        let (command, _) = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
            acceleration,
            request,
        )
        .unwrap();
        run_hls_command(command, &dir).await;
        let segment = find_generated_file(&dir, ".ts").await.unwrap();
        let samples = decode_opacity_samples(&ffmpeg, &segment).await;
        for (rgb, on) in samples.into_iter().zip(visible) {
            let message = format!("generation at programme second {elapsed}");
            if on {
                assert_red(rgb, &message);
            } else {
                assert_black(rgb, &message);
            }
        }
    }
}
