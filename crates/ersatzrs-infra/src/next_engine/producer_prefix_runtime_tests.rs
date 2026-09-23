//! Ordinary producer metadata prerequisite; this does not prove Next playback.
use super::*;

#[tokio::test]
#[ignore = "requires FFmpeg 9 and ffprobe runtime paths"]
async fn retained_prefix_initializes_rawvideo_pixel_format_without_complete_frame() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.nut");
    let mut generate = Command::new(ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=1920x1080:r=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000",
            "-t",
            "0.1",
            "-c:v",
            "rawvideo",
            "-pix_fmt",
            "yuv420p",
            "-threads",
            "1",
            "-c:a",
            "pcm_f32le",
            "-f",
            "nut",
        ])
        .arg(&source);
    tool(generate).await;
    let mut prefix = vec![0; 65_536];
    tokio::fs::File::open(&source)
        .await
        .unwrap()
        .read_exact(&mut prefix)
        .await
        .unwrap();
    tokio::fs::remove_file(source).await.unwrap();
    let (_cancel, cancellation) = tokio::sync::oneshot::channel();
    let metadata = crate::ffprobe::FfprobeExtractor::new(ffprobe.to_str().unwrap())
        .extract_retained_prefix_cancellable(
            &prefix,
            Instant::now() + Duration::from_secs(10),
            cancellation,
        )
        .await
        .unwrap();
    let hint = crate::next_engine::probe_hint::retained(&metadata, 100).unwrap();
    assert_eq!(hint.video[0].pix_fmt, "yuv420p");
    assert_eq!((hint.video[0].width, hint.video[0].height), (1920, 1080));
    assert_eq!(hint.video[0].frame_rate.as_deref(), Some("25/1"));
    assert_eq!(hint.audio[0].codec, "pcm_f32le");
}

#[tokio::test]
#[ignore = "requires FFmpeg 9 and ffprobe runtime paths"]
async fn compressed_producer_prefix_preserves_fresh_video_and_audio_metadata() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let extractor = crate::ffprobe::FfprobeExtractor::new(ffprobe.to_str().unwrap());
    for format in ["mpegts", "matroska"] {
        let source = root.path().join(format);
        let mut generate = Command::new(&ffmpeg);
        generate
            .args([
                "-v",
                "error",
                "-nostdin",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=s=320x180:r=25",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=880:sample_rate=48000",
                "-t",
                "8",
                "-c:v",
                "libx264",
                "-preset",
                "ultrafast",
                "-threads",
                "2",
                "-c:a",
                "aac",
                "-metadata:s:a:0",
                "language=nld",
                "-f",
                format,
            ])
            .arg(&source);
        tool(generate).await;
        assert!(tokio::fs::metadata(&source).await.unwrap().len() > 65_536);
        let mut prefix = vec![0; 65_536];
        tokio::fs::File::open(&source)
            .await
            .unwrap()
            .read_exact(&mut prefix)
            .await
            .unwrap();
        // Only a truncated prefix remains: probing cannot reopen the full file
        // or consume bytes owned by the running producer.
        tokio::fs::remove_file(&source).await.unwrap();
        let (_cancel, cancellation) = tokio::sync::oneshot::channel();
        let metadata = extractor
            .extract_producer_prefix_cancellable(
                &prefix,
                Instant::now() + Duration::from_secs(10),
                cancellation,
            )
            .await
            .unwrap();
        let video = metadata
            .streams
            .iter()
            .find(|stream| stream.kind == ersatzrs_domain::MediaStreamKind::Video)
            .unwrap();
        assert_eq!(video.codec.as_deref(), Some("h264"));
        assert_eq!((video.width, video.height), (Some(320), Some(180)));
        assert_eq!(metadata.version.r_frame_rate.as_deref(), Some("25/1"));
        let audio = metadata
            .streams
            .iter()
            .find(|stream| stream.kind == ersatzrs_domain::MediaStreamKind::Audio)
            .unwrap();
        assert_eq!(audio.codec.as_deref(), Some("aac"));
        assert_eq!(audio.channels, Some(1));
        assert_eq!(audio.language.as_deref(), Some("nld"));
        assert_ne!(audio.index, video.index);
        let hint = crate::next_engine::probe_hint::producer(&metadata).unwrap();
        let detected = if format == "matroska" {
            "matroska,webm"
        } else {
            "mpegts"
        };
        assert_eq!(metadata.format_name.as_deref(), Some(detected));
        assert_eq!(hint.format_name.as_deref(), Some(detected));
        assert_eq!(
            hint.duration_ms, None,
            "prefix duration is not whole-stream duration"
        );
        assert_eq!(hint.video[0].codec, "h264");
        assert_eq!(hint.audio[0].codec, "aac");
    }
}
