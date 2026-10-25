//! Actual requested-QSV fallback on a host with VPL but no Intel device.
use super::*;
use ersatzrs_core::repositories::FfmpegProfileRepository as _;
use ersatzrs_domain::HardwareAccelerationKind;

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg9 and VPL with no Intel hardware; actual software fallback"]
async fn real_next_qsv_without_intel_device_preserves_software_playback_and_profile() {
    let _owned_data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "Requested-QSV fallback proof retained at {}",
        root.display()
    );
    let log = root.join("session.log");
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(std::fs::File::create(&log).unwrap()))
        .try_init()
        .expect("dedicated nextest process diagnostic subscriber");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = root.join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    std::fs::write(
        root.join("fixture.nl.srt"),
        "1\n00:00:00,000 --> 00:01:00,000\nNext subtitle proof\n",
    )
    .unwrap();
    let profile = FFmpegProfile {
        hardware_acceleration: HardwareAccelerationKind::Qsv,
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    let host = std::sync::Arc::new(host::NextHost::new(
        pool.clone(),
        locker,
        root.join("playout"),
    ));
    assert_next_session(
        &host,
        host::NextStart {
            channel_id: 1,
            preview: false,
            number: "1".into(),
            executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
            output: root.join("output"),
            ffmpeg,
            ffprobe,
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: Instant::now() + Duration::from_secs(30),
        },
        false,
    )
    .await;
    let stored = crate::repositories::make_repositories(pool)
        .ffmpeg_profile
        .get(1)
        .await
        .unwrap();
    assert_eq!(
        stored.hardware_acceleration,
        HardwareAccelerationKind::Qsv,
        "fallback must not rewrite the stored profile"
    );
    let diagnostics = std::fs::read_to_string(log).unwrap();
    assert!(
        diagnostics.contains("failed to probe QSV capabilities"),
        "requested QSV probe must actually run"
    );
    assert!(
        diagnostics.contains("no hardware implementation found"),
        "requires loaded VPL and absent hardware, not a missing library or skipped probe"
    );
    let pipelines: Vec<_> = diagnostics
        .lines()
        .filter(|line| line.contains("optimized pipeline:"))
        .collect();
    assert!(
        !pipelines.is_empty(),
        "actual optimized pipeline must be retained"
    );
    for pipeline in pipelines {
        assert!(
            pipeline.contains("-vcodec libx264"),
            "QSV absence must use software encoding"
        );
        assert!(
            !pipeline.contains("_qsv") && !pipeline.contains("-hwaccel"),
            "no hardware decode/filter/encode may remain after fallback"
        );
    }
}
