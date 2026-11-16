//! Row292: captions can disappear after an admitted publication, before input open.
use super::*;

struct Runtime<'a> {
    binary: &'a Path,
    ffmpeg: &'a Path,
    ffprobe: &'a Path,
    clip: &'a Path,
    root: &'a Path,
    prepared: bool,
}

pub(super) struct Barrier {
    pub(super) executable: PathBuf,
    pub(super) waiting: PathBuf,
    pub(super) release: PathBuf,
}

pub(super) fn barrier(root: &Path, binary: &Path) -> Barrier {
    use std::os::unix::fs::PermissionsExt as _;
    let result = Barrier {
        executable: root.join("next-start-barrier"),
        waiting: root.join("worker-waiting"),
        release: root.join("worker-release"),
    };
    let literal = |path: &Path| serde_json::to_string(path.to_str().unwrap()).unwrap();
    let script = format!(
        "#!/usr/bin/env python3\nimport os,sys,time\nworker={}\nwaiting={}\nrelease={}\nif sys.argv[1:2] != ['describe']:\n    open(waiting,'x').close()\n    until=time.monotonic()+20\n    while not os.path.exists(release):\n        if time.monotonic()>until: raise RuntimeError('fixture barrier expired')\n        time.sleep(0.005)\nos.execv(worker,[worker]+sys.argv[1:])\n",
        literal(binary), literal(&result.waiting), literal(&result.release)
    );
    std::fs::write(&result.executable, script).unwrap();
    std::fs::set_permissions(&result.executable, std::fs::Permissions::from_mode(0o700)).unwrap();
    result
}

#[tokio::test]
#[ignore = "exact Next/FFmpeg inputs; Unix exec barrier; isolated synthetic AV and caption race"]
async fn real_next_row292_caption_changes_after_publication_preserve_av() {
    caption_cases(false).await;
}

#[tokio::test]
#[ignore = "exact Next/FFmpeg inputs; isolated immutable prepared artifact publication"]
async fn real_next_row292_prepared_caption_changes_after_publication_preserve_av() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::caption_publication_runtime_tests::fixture_child_row292_prepared_captions").await;
}

#[tokio::test]
#[ignore = "child of real_next_row292_prepared_caption_changes_after_publication_preserve_av"]
async fn fixture_child_row292_prepared_captions() {
    caption_cases(true).await;
}

async fn caption_cases(prepared: bool) {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::WARN)
        .with_writer(std::io::stderr)
        .try_init();
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = if prepared {
        prepared_artifact_runtime::temporary_root().await
    } else {
        tempfile::tempdir().unwrap()
    };
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let runtime = Runtime {
        binary: &binary,
        ffmpeg: &ffmpeg,
        ffprobe: &ffprobe,
        clip: &clip,
        root: root.path(),
        prepared,
    };
    for convert in [false, true] {
        for case in ["present", "file-removed", "path-cleared", "row-replaced"] {
            caption_case(&runtime, case, convert).await;
        }
    }
}

async fn caption_case(runtime: &Runtime<'_>, case: &str, convert: bool) {
    let root = runtime.root.join(format!("{case}-{convert}"));
    std::fs::create_dir(&root).unwrap();
    let caption = runtime.root.join("fixture.nl.srt");
    std::fs::write(&caption, CUES).unwrap();
    let (pool, locker) = prepare_schedule(runtime.clip, false, &text_profile()).await;
    configure_competing_text_tracks(&pool, false, convert, false).await;
    if runtime.prepared {
        prepared_artifact_runtime::publish_fixture(&pool, runtime.clip).await;
    }
    let host = Arc::new(host::NextHost::new(
        pool.clone(),
        locker,
        root.join("playout"),
    ));
    let mut callbacks = callback_listener(host.clone()).await;
    let barrier = barrier(&root, runtime.binary);
    let output = root.join("output");
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: barrier.executable,
                output: output.clone(),
                ffmpeg: runtime.ffmpeg.to_owned(),
                ffprobe: runtime.ffprobe.to_owned(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_mins(1),
            },
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(10), async {
        while !barrier.waiting.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("real worker did not reach the post-publication input-open barrier");
    let published = root
        .join("playout/1")
        .join(crate::next_engine::publication::PLAYOUT_FILENAME);
    let playout: serde_json::Value =
        serde_json::from_slice(&std::fs::read(published).unwrap()).unwrap();
    assert!(!playout["items"].as_array().unwrap().is_empty());
    if runtime.prepared && case == "present" && !convert {
        super::prepared_caption_controls::verify(
            &host,
            &playout["items"][0],
            runtime.clip,
            &caption,
        )
        .await;
    }
    mutate_caption(&pool, &caption, case).await;
    if case != "present" {
        assert!(registry.playout_updated("1").await);
    }
    std::fs::write(&barrier.release, b"open").unwrap();
    let handle = registry.try_get("1").await.unwrap();
    timeout(Duration::from_secs(20), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => {
                    panic!("row292 worker failed before readiness: {case}")
                }
                _ => tokio::time::sleep(Duration::from_millis(5)).await,
            }
        }
    })
    .await
    .unwrap();
    let playlist = handle.trim_playlist().await.unwrap();
    let segment =
        assert_text_playlist(&output, &playlist, convert && case == "present", false).await;
    let sample = root.join("decoded-sample.ts");
    std::fs::copy(segment, &sample).unwrap();
    assert!(registry.stop_channel("1").await);
    timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    callbacks.stop().await;
    assert!(!output.exists());
    assert_programme_video(runtime.ffmpeg, &sample, case).await;
    assert_text_video_and_audio(runtime.ffmpeg, &sample, !convert && case == "present").await;
    println!(
        "row292 caption case={case} programme=blue audio=nld880 captions={} convert={convert} prepared={} cleanup=joined",
        case == "present", runtime.prepared
    );
}

async fn mutate_caption(pool: &crate::db::DbPool, caption: &Path, case: &str) {
    match case {
        "file-removed" => std::fs::remove_file(caption).unwrap(),
        "path-cleared" => {
            sqlx::query(r#"UPDATE "Subtitle" SET "Path"=NULL WHERE "SubtitleKind"=1"#)
                .execute(pool)
                .await
                .unwrap();
        }
        "row-replaced" => {
            std::fs::remove_file(caption).unwrap();
            sqlx::query(r#"DELETE FROM "Subtitle" WHERE "SubtitleKind"=1"#)
                .execute(pool)
                .await
                .unwrap();
        }
        "present" => {}
        _ => panic!("unknown caption case"),
    }
}

pub(super) async fn assert_programme_video(ffmpeg: &Path, segment: &Path, case: &str) {
    let mut command = Command::new(ffmpeg);
    command
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "scale=1:1:flags=area",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixel = tool(command).await;
    assert_eq!(pixel.len(), 3);
    assert!(
        pixel[2] > 150 && pixel[0] < 100 && pixel[1] < 100,
        "row292 published-caption change replaced programme video: case={case} rgb={pixel:?}"
    );
}
