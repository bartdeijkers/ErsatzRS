use super::*;
use ersatzrs_core::repositories::ConfigRepository;

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; isolates report files in a child process"]
async fn real_next_reports_follow_persisted_opt_in() {
    let root = tempfile::tempdir().unwrap();
    let mut command = Command::new(std::env::current_exe().unwrap());
    command
        .args([
            "--exact",
            "next_engine::process::runtime_tests::reports_runtime_tests::fixture_child_reports",
            "--ignored",
        ])
        .env(crate::file_system_layout::APP_DATA_DIR_ENV_VAR, root.path())
        .env(
            crate::file_system_layout::APP_STATE_DIR_ENV_VAR,
            root.path().join("state"),
        )
        .env("ERSATZRS_NEXT_REPORTS_CHILD", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());
    let mut child = spawn_managed_addon_process(command).unwrap();
    let mut stdout = child.stdout().take().unwrap().take(128 * 1024);
    let observed = timeout(Duration::from_mins(3), async {
        let mut output = String::new();
        stdout.read_to_string(&mut output).await.unwrap();
        (child.wait().await.unwrap(), output)
    })
    .await;
    let Ok((status, output)) = observed else {
        let _ = child.start_kill();
        if !matches!(
            timeout(Duration::from_secs(15), child.wait()).await,
            Ok(Ok(_))
        ) {
            // Do not remove private files while an unconfirmed child may use them.
            let _ = root.keep();
        }
        panic!("isolated reports fixture timed out");
    };
    assert!(
        status.success(),
        "isolated reports fixture failed: {output}"
    );
    assert!(
        output.contains("1 passed"),
        "reports fixture was not selected"
    );
}

#[tokio::test]
#[ignore = "child of real_next_reports_follow_persisted_opt_in; never run directly"]
#[allow(clippy::disallowed_methods)] // Guard the isolated fixture before filesystem writes.
async fn fixture_child_reports() {
    assert_eq!(
        std::env::var("ERSATZRS_NEXT_REPORTS_CHILD").as_deref(),
        Ok("1")
    );
    let data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    assert_eq!(crate::file_system_layout::app_data_folder(), data);
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = data.join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    tokio::fs::write(
        data.join("fixture.nl.srt"),
        "1\n00:00:00,000 --> 00:01:00,000\nNext subtitle proof\n",
    )
    .await
    .unwrap();
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_bitrate: 1000,
        video_buffer_size: 2000,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    let reports = crate::file_system_layout::ffmpeg_reports_folder();
    // Fresh sessions prove both enabling and disabling after an enabled run.
    for enabled in [false, true, false] {
        if reports.exists() {
            tokio::fs::remove_dir_all(&reports).await.unwrap();
        }
        let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
        crate::repositories::make_repositories(pool.clone())
            .config
            .set(
                ersatzrs_domain::CONFIG_KEY_FFMPEG_SAVE_REPORTS,
                if enabled { "true" } else { "false" },
            )
            .await
            .unwrap();
        let host = std::sync::Arc::new(host::NextHost::new(pool, locker, data.join("playout")));
        let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
        let request = host::NextStart {
            channel_id: 1,
            preview: false,
            number: "1".into(),
            executable: binary.clone(),
            output: data.join("output"),
            ffmpeg: ffmpeg.clone(),
            ffprobe: ffprobe.clone(),
            replacements: Vec::new(),
            idle_timeout: None,
            deadline: Instant::now() + Duration::from_secs(30),
        };
        let task = host.start(registry.clone(), request.clone()).await.unwrap();
        let handle = registry.try_get("1").await.unwrap();
        let observed = timeout(Duration::from_secs(30), async {
            loop {
                if handle.startup_readiness() == HlsStartupReadiness::Failed {
                    return false;
                }
                if handle.startup_readiness() == HlsStartupReadiness::Ready {
                    let report_exists = has_nonempty_report(&reports).await;
                    if !enabled || report_exists {
                        return report_exists == enabled && handle.trim_playlist().await.is_some();
                    }
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await;
        // Reap before asserting, also when observing readiness fails.
        registry.stop_channel("1").await;
        timeout(Duration::from_secs(15), task)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(
            observed,
            Ok(true),
            "report opt-in did not match active playback"
        );
        assert!(!request.output.exists());
        if !enabled {
            assert!(!reports.exists(), "disabled reports created a directory");
        }
    }
}

async fn has_nonempty_report(folder: &Path) -> bool {
    let Ok(mut files) = tokio::fs::read_dir(folder).await else {
        return false;
    };
    while let Some(file) = files.next_entry().await.unwrap() {
        if file
            .file_name()
            .to_string_lossy()
            .starts_with(".in-flight-")
            && file
                .metadata()
                .await
                .is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
        {
            return true;
        }
    }
    false
}
