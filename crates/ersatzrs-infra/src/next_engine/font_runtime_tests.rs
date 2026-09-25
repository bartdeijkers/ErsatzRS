//! Prove that the host font cache reaches the official Next/libass renderer.
use super::*;

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolates the font cache in a child"]
async fn real_next_loads_cached_subtitle_font() {
    prepared_artifact_runtime::run_isolated_test(
        "next_engine::process::runtime_tests::text_subtitle_runtime_tests::font_runtime_tests::fixture_child_font",
    )
    .await;
}

#[tokio::test]
#[ignore = "child of real_next_loads_cached_subtitle_font; never run directly"]
async fn fixture_child_font() {
    let root = prepared_artifact_runtime::temporary_root().await;
    let fonts = crate::file_system_layout::fonts_cache_folder();
    tokio::fs::create_dir_all(&fonts).await.unwrap();
    tokio::fs::write(
        fonts.join("Sen.ttf"),
        include_bytes!("../../../../assets/fonts/Sen.ttf"),
    )
    .await
    .unwrap();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let clip = root.path().join("fixture.mkv");
    generate_clip(&ffmpeg, &clip).await;
    let ass = "[Script Info]\nScriptType: v4.00+\nPlayResX: 320\nPlayResY: 180\n\n[V4+ Styles]\nFormat: Name, Fontname, Fontsize, PrimaryColour, Alignment, MarginV\nStyle: Default,Sen,20,&H00FFFFFF,2,10\n\n[Events]\nFormat: Layer, Start, End, Style, Text\nDialogue: 0,0:00:00.00,0:01:00.00,Default,Cached font proof\n";
    tokio::fs::write(root.path().join("fixture.nl.ass"), ass)
        .await
        .unwrap();
    let (pool, locker) = prepare_schedule(&clip, false, &text_profile()).await;
    sqlx::query(r#"UPDATE "Subtitle" SET "Path"='fixture.nl.ass', "Codec"='ass'"#)
        .execute(&pool)
        .await
        .unwrap();
    let host = std::sync::Arc::new(host::NextHost::new(
        pool,
        locker,
        root.path().join("playout"),
    ));
    let output = root.path().join("output");
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: ffmpeg.clone(),
                ffprobe: runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let observed = timeout(Duration::from_secs(30), async {
        loop {
            if cached_font_selected().await {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await;
    if observed.is_ok() {
        let segment = first_segment(&output).await;
        assert_text_video_and_audio(&ffmpeg, &segment, true).await;
    }
    registry.stop_channel("1").await;
    timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        observed.is_ok(),
        "libass did not select the cached Sen font"
    );
    assert!(!output.exists());
}

async fn first_segment(output: &Path) -> PathBuf {
    timeout(Duration::from_secs(10), async {
        loop {
            if let Ok(playlist) = tokio::fs::read_to_string(output.join("live.m3u8")).await {
                if let Some(segment) = playlist.lines().find(|line| {
                    !line.starts_with('#')
                        && Path::new(line).extension().is_some_and(|ext| ext == "ts")
                }) {
                    break output.join(segment);
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Next must publish a video segment")
}

async fn cached_font_selected() -> bool {
    let Ok(mut files) =
        tokio::fs::read_dir(crate::file_system_layout::ffmpeg_reports_folder()).await
    else {
        return false;
    };
    while let Some(file) = files.next_entry().await.unwrap() {
        if !file
            .file_name()
            .to_string_lossy()
            .starts_with(".in-flight-")
        {
            continue;
        }
        if let Ok(report) = tokio::fs::read_to_string(file.path()).await {
            if report
                .lines()
                .any(|line| line.contains("fontselect:") && line.contains("-> Sen"))
            {
                return true;
            }
        }
    }
    false
}
