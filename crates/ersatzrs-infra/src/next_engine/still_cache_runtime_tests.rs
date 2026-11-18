//! Row296: actual worker reuse, invalidation, retention and full-image fallback.
//! These opt-in tests require isolated application data/state roots, in addition
//! to the parent module's verified worker and `FFmpeg` runtime inputs.
use super::*;
use std::collections::BTreeSet;
use std::time::SystemTime;

struct OverlayCase {
    number: &'static str,
    green: bool,
    width_percent: u32,
    opacity: i32,
}

async fn large_logo(ffmpeg: &Path, image: &Path, green: bool) {
    let mut generate = Command::new(ffmpeg);
    generate
        .args(["-v", "error", "-nostdin", "-y", "-f", "lavfi", "-i"])
        .arg(if green {
            "color=lime:size=3200x1600"
        } else {
            "color=red:size=3200x1600"
        })
        .args(["-frames:v", "1", "-threads", "1"])
        .arg(image);
    tool(generate).await;
}

fn age(path: &Path, elapsed: Duration) -> SystemTime {
    let modified = SystemTime::now() - elapsed;
    std::fs::File::options()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(modified)
        .unwrap();
    modified
}

pub(super) fn rendered_frames(folder: &Path) -> BTreeSet<PathBuf> {
    std::fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension().is_some_and(|ext| ext == "raw")
                && path
                    .file_name()
                    .unwrap()
                    .to_string_lossy()
                    .split('_')
                    .next()
                    .is_some_and(|prefix| {
                        prefix.len() == 16 && prefix.chars().all(|c| c.is_ascii_hexdigit())
                    })
        })
        .collect()
}

async fn assert_cached_picture(ffmpeg: &Path, segment: &Path, case: &OverlayCase) {
    let mut decode = Command::new(ffmpeg);
    decode.args(["-v", "error", "-nostdin", "-i"]).arg(segment);
    decode.args([
        "-ss",
        "0.5",
        "-map",
        "0:v:0",
        "-frames:v",
        "1",
        "-vf",
        "crop=160:70:0:0",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    let pixels = tool(decode).await;
    assert_eq!(pixels.len(), 160 * 70 * 3);
    let active = usize::from(case.green);
    let target = if case.opacity == 50 { 128 } else { 255 };
    let pixel = &pixels[(20 * 160 + 30) * 3..][..3];
    assert!(pixel[active].abs_diff(target) < 25, "overlay: {pixel:?}");
    assert!(pixel[1 - active] < 35, "wrong source color: {pixel:?}");
    let blue = if case.opacity == 50 { 128 } else { 0 };
    assert!(pixel[2].abs_diff(blue) < 25, "opacity: {pixel:?}");
    let width = usize::try_from(case.width_percent * 320 / 100).unwrap();
    let before = &pixels[(20 * 160 + 16 + width - 4) * 3..][..3];
    let after = &pixels[(20 * 160 + 16 + width + 4) * 3..][..3];
    assert!(before[active] > 90, "scaled right edge: {before:?}");
    assert!(after[0] < 35 && after[1] < 35 && after[2] > 210);
}

async fn cached_overlay(root: &Path, clip: &Path, image: &Path, case: &OverlayCase) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let (pool, locker) = prepare_schedule(clip, false, &overlay_profile()).await;
    configure_text_tracks(&pool, true, false, false).await;
    seed_watermark(&pool, image, true, false).await;
    sqlx::query(r#"UPDATE "ChannelWatermark" SET "WidthPercent"=?,"Opacity"=?"#)
        .bind(case.width_percent)
        .bind(case.opacity)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "Number"=? WHERE "Id"=1"#)
        .bind(case.number)
        .execute(&pool)
        .await
        .unwrap();
    let host = Arc::new(host::NextHost::new(pool, locker, root.join("playout")));
    let mut callback = callback_listener(host.clone()).await;
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = root.join("output");
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: case.number.into(),
                executable: binary,
                output: output.clone(),
                ffmpeg: ffmpeg.clone(),
                ffprobe,
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let handle = registry.try_get(case.number).await.unwrap();
    timeout(Duration::from_secs(30), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => panic!("cached watermark startup failed"),
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .unwrap();
    let playlist = handle.trim_playlist().await.unwrap();
    let segment = output.join(first_transport_segment(&playlist));
    assert_cached_picture(&ffmpeg, &segment, case).await;
    if image.extension().is_some_and(|ext| ext == "gif") {
        assert_animation_advances(&ffmpeg, &segment).await;
    }
    assert_text_video_and_audio(&ffmpeg, &segment, true).await;
    assert!(registry.stop_channel(case.number).await);
    task.await.unwrap();
    callback.stop().await;
    assert!(!output.exists(), "session output must be removed on stop");
}

fn retention_controls(cache: &Path) -> Vec<PathBuf> {
    std::fs::create_dir_all(cache).unwrap();
    let names = [
        "control-stale.raw",
        ".control-old.tmp",
        "control-recent.raw",
        ".control-recent.tmp",
        "control-unrelated.png",
    ];
    let controls: Vec<_> = names.iter().map(|name| cache.join(name)).collect();
    for path in &controls {
        std::fs::write(path, b"retention control").unwrap();
    }
    age(&controls[0], Duration::from_hours(31 * 24));
    age(&controls[1], Duration::from_hours(2));
    age(&controls[2], Duration::from_hours(24));
    age(&controls[3], Duration::from_mins(30));
    age(&controls[4], Duration::from_hours(31 * 24));
    controls
}

#[tokio::test]
#[ignore = "requires fixed cache-capable worker, FFmpeg 9 and fresh isolated app-data/state roots"]
async fn row296_real_still_cache_reuses_invalidates_expires_and_falls_back() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::watermark_runtime_tests::still_cache_runtime_tests::fixture_child_row296_still_cache").await;
}

#[tokio::test]
#[ignore = "child of the isolated row296 still-cache runtime proof"]
async fn fixture_child_row296_still_cache() {
    let data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    let _state = runtime_path(crate::file_system_layout::APP_STATE_DIR_ENV_VAR);
    let cache = data.join("cache").join("next").join("still-images");
    assert!(!cache.exists(), "use a fresh isolated app-data root");
    let controls = retention_controls(&cache);
    let outside = cache.parent().unwrap().join("control-outside.raw");
    std::fs::write(&outside, b"unowned by still-image sweeper").unwrap();
    age(&outside, Duration::from_hours(31 * 24));
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let image = root.path().join("large.png");
    large_logo(&ffmpeg, &image, false).await;
    let mut case = OverlayCase {
        number: "1",
        green: false,
        width_percent: 20,
        opacity: 50,
    };
    cached_overlay(root.path(), &clip, &image, &case).await;
    let cold = rendered_frames(&cache);
    assert_eq!(
        cold.len(),
        1,
        "one still render, independent of programme duration"
    );
    let frame = cold.first().unwrap();
    assert!(std::fs::metadata(frame).unwrap().len() < 3200 * 1600);
    let bytes = std::fs::read(frame).unwrap();
    let created = std::fs::metadata(frame).unwrap().created().unwrap();
    assert!(!controls[0].exists() && !controls[1].exists());
    assert!(controls[2..].iter().all(|p| p.exists()) && outside.exists());
    let old = age(frame, Duration::from_hours(24));
    // A different channel and source pathname with identical content must hit.
    let same_image = root.path().join("same-content.png");
    std::fs::copy(&image, &same_image).unwrap();
    case.number = "2";
    cached_overlay(root.path(), &clip, &same_image, &case).await;
    assert_eq!(rendered_frames(&cache), cold);
    assert_eq!(std::fs::read(frame).unwrap(), bytes);
    assert_eq!(
        std::fs::metadata(frame).unwrap().created().unwrap(),
        created,
        "a warm hit must not replace the rendered file"
    );
    assert!(std::fs::metadata(frame).unwrap().modified().unwrap() > old + Duration::from_hours(1));
    // Mutating the same source path must invalidate the baked pixels.
    large_logo(&ffmpeg, &image, true).await;
    case.green = true;
    cached_overlay(root.path(), &clip, &image, &case).await;
    let changed = rendered_frames(&cache);
    assert_eq!(changed.len(), 2);
    assert!(changed.is_superset(&cold));
    case.width_percent = 30;
    case.opacity = 100;
    cached_overlay(root.path(), &clip, &image, &case).await;
    let settings = rendered_frames(&cache);
    assert_eq!(settings.len(), 3);
    assert!(settings.is_superset(&changed));
    // Deny cache creation without denying the original watermark or media.
    let saved = root.path().join("retained-cache");
    std::fs::rename(&cache, &saved).unwrap();
    std::fs::write(&cache, b"intentional cache-only failure").unwrap();
    cached_overlay(root.path(), &clip, &image, &case).await;
    assert!(
        cache.is_file(),
        "fallback must not replace another filesystem object"
    );
    std::fs::remove_file(&cache).unwrap();
    std::fs::rename(saved, &cache).unwrap();
    assert_eq!(
        rendered_frames(&cache),
        settings,
        "cache survives session cleanup"
    );
    println!("row296 cache: cold=1 cross-channel/content hit=1 changed-source=2 changed-settings=3; retention namespace and full-image fallback pass with selected audio/Burn");
}

// Linux exec barriers and /proc provide exact prescaler membership evidence.
// Required Windows job/runtime proof remains with the shared platform gate.
#[cfg(target_os = "linux")]
#[path = "still_cache_lifecycle_runtime_tests.rs"]
mod lifecycle;

async fn assert_animation_advances(ffmpeg: &Path, segment: &Path) {
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-ss",
            "1.5",
            "-frames:v",
            "1",
            "-vf",
            "crop=2:2:30:20",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(decode).await;
    assert_eq!(pixels.len(), 12);
    assert!(
        pixels[0] < 35 && pixels[1] > 210 && pixels[2] < 35,
        "animation must advance to green, not freeze at a cached red frame: {pixels:?}"
    );
}

#[tokio::test]
#[ignore = "requires fixed worker and FFmpeg 9; isolated animated-logo no-bake proof"]
async fn row296_real_animated_watermark_is_not_frozen_by_still_cache() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::watermark_runtime_tests::still_cache_runtime_tests::fixture_child_row296_animation").await;
}

#[tokio::test]
#[ignore = "child of isolated row296 animated-logo proof"]
async fn fixture_child_row296_animation() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let image = root.path().join("animated.gif");
    let mut generate = Command::new(&ffmpeg);
    generate
        .args([
            "-v",
            "error",
            "-nostdin",
            "-y",
            "-f",
            "lavfi",
            "-i",
            "color=red:size=640x320:rate=2:duration=1",
            "-f",
            "lavfi",
            "-i",
            "color=lime:size=640x320:rate=2:duration=1",
            "-filter_complex",
            "[0:v][1:v]concat=n=2:v=1:a=0",
            "-loop",
            "0",
            "-threads",
            "1",
        ])
        .arg(&image);
    tool(generate).await;
    cached_overlay(
        root.path(),
        &clip,
        &image,
        &OverlayCase {
            number: "1",
            green: false,
            width_percent: 20,
            opacity: 100,
        },
    )
    .await;
    let cache = crate::file_system_layout::next_cache_folder().join("still-images");
    assert!(
        !cache.exists() || rendered_frames(&cache).is_empty(),
        "animated logo must not become a single cached frame"
    );
}
