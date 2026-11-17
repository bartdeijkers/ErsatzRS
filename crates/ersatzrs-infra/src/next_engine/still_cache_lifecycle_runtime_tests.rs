//! Actual fixed-worker cold render interrupted at a known `FFmpeg` boundary.
use super::*;
use std::os::unix::fs::PermissionsExt as _;

fn prescale_barrier(root: &Path, ffmpeg: &Path) -> (PathBuf, PathBuf) {
    let executable = root.join("ffmpeg-prescale-barrier");
    let waiting = root.join("prescaler-pid");
    let literal = |p: &Path| serde_json::to_string(p.to_str().unwrap()).unwrap();
    let script = format!(
        "#!/usr/bin/env python3\nimport os,sys,time\nreal={}\nwaiting={}\nif '-frames:v' in sys.argv and sys.argv[-1].endswith('.tmp'):\n    open(waiting,'x').write(str(os.getpid()))\n    time.sleep(120)\nos.execv(real,[real]+sys.argv[1:])\n",
        literal(ffmpeg), literal(&waiting)
    );
    std::fs::write(&executable, script).unwrap();
    std::fs::set_permissions(&executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    (executable, waiting)
}

async fn wait_prescaler(waiting: &Path) -> u32 {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(text) = std::fs::read_to_string(waiting) {
                if let Ok(pid) = text.parse() {
                    break pid;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("worker must reach the actual cold-render subprocess")
}

async fn assert_reaped(pid: u32) {
    timeout(Duration::from_secs(5), async {
        loop {
            let stat = std::fs::read_to_string(format!("/proc/{pid}/stat"));
            // A terminated orphan can briefly remain as a zombie until init reaps
            // it. It cannot execute or keep media/cache files open.
            if stat
                .as_ref()
                .is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound)
                || stat
                    .as_ref()
                    .is_ok_and(|s| s.rsplit_once(") ").is_some_and(|(_, s)| s.starts_with('Z')))
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("managed worker cleanup must terminate its prescaler descendant");
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Termination {
    Cancel,
    StartupDeadline,
    RenderDeadline,
}

async fn interrupted_render(root: &Path, clip: &Path, image: &Path, end: Termination) {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let (renderer, waiting) = prescale_barrier(root, &ffmpeg);
    let (pool, locker) = prepare_schedule(clip, false, &overlay_profile()).await;
    configure_text_tracks(&pool, true, false, false).await;
    seed_watermark(&pool, image, true, false).await;
    let host = Arc::new(host::NextHost::new(pool, locker, root.join("playout")));
    let mut callback = callback_listener(host.clone()).await;
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = root.join("output");
    let started = Instant::now();
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: renderer,
                ffprobe: runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: started
                    + Duration::from_secs(match end {
                        Termination::Cancel => 20,
                        Termination::StartupDeadline => 5,
                        Termination::RenderDeadline => 60,
                    }),
            },
        )
        .await
        .unwrap();
    let handle = registry.try_get("1").await.unwrap();
    let pid = wait_prescaler(&waiting).await;
    assert_ne!(handle.startup_readiness(), HlsStartupReadiness::Ready);
    if end == Termination::RenderDeadline {
        timeout(Duration::from_secs(40), async {
            loop {
                match handle.startup_readiness() {
                    HlsStartupReadiness::Ready => break,
                    HlsStartupReadiness::Failed => {
                        panic!("render timeout must retain original-image fallback")
                    }
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .unwrap();
        assert!(started.elapsed() >= Duration::from_secs(28));
        assert_reaped(pid).await;
        let playlist = handle.trim_playlist().await.unwrap();
        let segment = output.join(first_transport_segment(&playlist));
        assert_cached_picture(
            &ffmpeg,
            &segment,
            &OverlayCase {
                number: "1",
                green: true,
                width_percent: 20,
                opacity: 50,
            },
        )
        .await;
        assert_text_video_and_audio(&ffmpeg, &segment, true).await;
    }
    if end != Termination::StartupDeadline {
        assert!(registry.stop_channel("1").await);
    }
    timeout(Duration::from_secs(12), task)
        .await
        .unwrap()
        .unwrap();
    assert!(registry.try_get("1").await.is_none());
    assert!(!output.exists());
    assert_reaped(pid).await;
    callback.stop().await;
    assert!(
        started.elapsed()
            < Duration::from_secs(if end == Termination::RenderDeadline {
                45
            } else {
                15
            })
    );
    println!("row296 prescaler end={end:?}: admitted cold render reached, readiness/fallback checked, task joined, registry/output released, descendant terminated");
}

#[tokio::test]
#[ignore = "requires fixed worker, FFmpeg 9, Python exec barrier and isolated app-data/state"]
async fn row296_real_prescaler_cancel_and_startup_deadline_reap_descendants() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::watermark_runtime_tests::still_cache_runtime_tests::lifecycle::fixture_child_row296_prescaler_lifecycle").await;
}

#[tokio::test]
#[ignore = "child of isolated row296 real prescaler ownership proof"]
async fn fixture_child_row296_prescaler_lifecycle() {
    let _data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    let _state = runtime_path(crate::file_system_layout::APP_STATE_DIR_ENV_VAR);
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    for end in [
        Termination::Cancel,
        Termination::StartupDeadline,
        Termination::RenderDeadline,
    ] {
        let case = root.path().join(format!("{end:?}"));
        std::fs::create_dir(&case).unwrap();
        let image = case.join("large.png");
        large_logo(&ffmpeg, &image, end != Termination::Cancel).await;
        interrupted_render(&case, &clip, &image, end).await;
    }
}
