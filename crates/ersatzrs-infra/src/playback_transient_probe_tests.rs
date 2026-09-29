use super::*;

#[tokio::test]
async fn transient_video_enrichment_preserves_flags_after_expired_deadline() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let mut original = SourceVideoStream::new(SourceVideoCodec::H264);
    original.scan_kind = VideoScanKind::Progressive;
    let result = enrich_transient_source_video_stream_for_playback(
        &pool,
        Some(original),
        "absent-fixture",
        Duration::from_secs(30),
        false,
        HardwareAccelerationKind::Qsv,
        tokio::time::Instant::now(),
    )
    .await
    .unwrap();
    assert!(!result.has_multiple_profiles);
    assert_eq!(result.scan_kind, VideoScanKind::Progressive);
}

#[tokio::test]
#[ignore = "requires the verified FFmpeg runtime; software-only H.264 profile probe"]
async fn real_transient_qsv_profile_safeguard_needs_no_library_or_gpu() {
    let root = tempfile::tempdir().unwrap();
    let mut combined = Vec::new();
    for profile in ["baseline", "high"] {
        let path = root.path().join(format!("{profile}.h264"));
        let mut command = Command::new(ffmpeg_path());
        command
            .args([
                "-nostdin",
                "-v",
                "error",
                "-f",
                "lavfi",
                "-i",
                "testsrc2=size=64x64:rate=25",
                "-frames:v",
                "3",
                "-c:v",
                "libx264",
                "-profile:v",
                profile,
                "-pix_fmt",
                "yuv420p",
                "-f",
                "h264",
            ])
            .arg(&path)
            .kill_on_drop(true);
        let output = tokio::time::timeout(Duration::from_secs(10), command.output())
            .await
            .unwrap()
            .unwrap();
        assert!(output.status.success(), "synthetic H.264 generation failed");
        combined.extend(std::fs::read(path).unwrap());
    }
    let path = root.path().join("mixed.h264");
    std::fs::write(&path, combined).unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let stream = enrich_transient_source_video_stream_for_playback(
        &pool,
        Some(SourceVideoStream::new(SourceVideoCodec::H264)),
        path.to_str().unwrap(),
        Duration::from_secs(1),
        false,
        HardwareAccelerationKind::Qsv,
        tokio::time::Instant::now() + Duration::from_secs(10),
    )
    .await
    .unwrap();
    assert!(
        stream.has_multiple_profiles,
        "native QSV planning must see both H.264 profiles"
    );
}
