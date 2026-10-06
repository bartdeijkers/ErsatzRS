//! Finite Next inputs: fractional seek, image handoff and natural termination.
use super::*;
use crate::next_engine::protocol::{Playout, Source, PLAYOUT_VERSION};
use ersatzrs_core::repositories::ChannelRepository as _;

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; finite GIF seek and static WebP handoff"]
async fn real_next_image_seek_and_handoff_finish_at_the_schedule_boundary() {
    run(false, None).await;
}

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; seekable HTTP GIF repeat and termination"]
async fn real_next_http_image_seek_and_handoff_finish_at_the_schedule_boundary() {
    run(true, None).await;
}

#[tokio::test]
#[ignore = "requires real Next and patched FFmpeg 9; finite animated WebP seek/repeat/handoff"]
async fn real_next_finite_webp_seek_and_handoff_finish_at_the_schedule_boundary() {
    run(false, Some(1)).await;
}

#[tokio::test]
#[ignore = "requires real Next and patched FFmpeg 9; infinite animated WebP finite schedule"]
async fn real_next_infinite_webp_seek_and_handoff_finish_at_the_schedule_boundary() {
    run(false, Some(0)).await;
}

#[tokio::test]
#[ignore = "requires real Next and patched FFmpeg 9; seekable HTTP finite animated WebP"]
async fn real_next_http_finite_webp_seek_and_handoff_finish_at_the_schedule_boundary() {
    run(true, Some(1)).await;
}

#[tokio::test]
#[ignore = "requires real Next and patched FFmpeg 9; seekable HTTP infinite animated WebP"]
async fn real_next_http_infinite_webp_seek_and_handoff_finish_at_the_schedule_boundary() {
    run(true, Some(0)).await;
}

async fn run(http: bool, webp_loop_count: Option<u16>) {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let root = tempfile::tempdir().unwrap();
    let (pool, locker, animation) = prepare_animation(&ffmpeg, root.path(), webp_loop_count).await;
    let start = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0).unwrap();
    let second_start = start + chrono::Duration::seconds(6);
    let finish = second_start + chrono::Duration::seconds(2);
    let (pool2, _) = prepare_image(&ffmpeg, root.path(), "webp").await;
    let mut first = project(&pool, &animation, start, second_start, 500).await;
    let mut second = project(
        &pool2,
        &root.path().join("still.webp"),
        second_start,
        finish,
        0,
    )
    .await;
    second.id = "second-image".to_owned();
    let origin = if http {
        Some(serve_image(&mut first, animation).await)
    } else {
        None
    };
    let playout = root.path().join("playout");
    tokio::fs::create_dir(&playout).await.unwrap();
    tokio::fs::write(
        playout.join(format!(
            "{}_{}.json",
            start.timestamp_millis(),
            finish.timestamp_millis()
        )),
        serde_json::to_vec(&Playout {
            version: PLAYOUT_VERSION.to_owned(),
            items: vec![first, second],
        })
        .unwrap(),
    )
    .await
    .unwrap();
    let mut config = super::super::super::tests::config();
    config.playout.folder = playout.to_str().unwrap().to_owned();
    config.playout.virtual_start = Some(start);
    config.ffmpeg.ffmpeg_path = ffmpeg.to_str().unwrap().to_owned();
    config.ffmpeg.ffprobe_path = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE")
        .to_str()
        .unwrap()
        .to_owned();
    config.normalization.audio.sample_rate_hz = Some(48_000);
    let output = root.path().join("output");
    let mut process = NextProcess::start_troubleshooting(
        &runtime_path("ERSATZRS_NEXT_TEST_BIN"),
        "1",
        &output,
        &config,
        None,
        Instant::now() + Duration::from_secs(30),
    )
    .await
    .unwrap();
    let status = timeout(Duration::from_secs(30), process.wait()).await;
    if status.is_err() {
        process.stop().await.unwrap();
    }
    if let Some(origin) = origin {
        origin.stop().await;
    }
    let retained = root.keep();
    eprintln!(
        "decoded finite image seek/handoff, HTTP={http}, WebP loop={webp_loop_count:?}: {}",
        retained.display()
    );
    let status = status
        .expect("looped image must terminate naturally at the schedule boundary")
        .unwrap();
    assert!(
        status.success(),
        "finite image process must succeed: {status}; {}",
        process.diagnostics()
    );
    let playlist = output.join("ffmpeg.m3u8");
    let body = tokio::fs::read_to_string(&playlist).await.unwrap();
    assert!(body.contains("#EXT-X-ENDLIST"));
    assert_animation_cycle(&ffmpeg, &playlist, Some(1)).await;
    assert_audio_and_subtitle(
        &ffmpeg,
        &runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"),
        &playlist,
        true,
        "aac",
    )
    .await;
    assert_blue_handoff(&ffmpeg, &playlist).await;
    assert_schedule_duration(&playlist).await;
    // Only disposable fixture pools are touched; no running host/database is used.
    drop(locker);
}

async fn prepare_animation(
    ffmpeg: &Path,
    root: &Path,
    webp_loop_count: Option<u16>,
) -> (
    crate::db::DbPool,
    ersatzrs_core::locking::EntityLocker,
    PathBuf,
) {
    let (pool, locker) = prepare_image(ffmpeg, root, "animated.gif").await;
    let animation = if let Some(loop_count) = webp_loop_count {
        prepare_webp(ffmpeg, root, &pool, loop_count).await
    } else {
        root.join("still.animated.gif")
    };
    (pool, locker, animation)
}

async fn prepare_webp(
    ffmpeg: &Path,
    root: &Path,
    pool: &crate::db::DbPool,
    loop_count: u16,
) -> PathBuf {
    let animation = root.join("animated.webp");
    let mut encode = Command::new(ffmpeg);
    encode
        .args(["-v", "error", "-nostdin", "-ignore_loop", "1", "-i"])
        .arg(root.join("still.animated.gif"))
        .args(["-c:v", "libwebp_anim", "-lossless", "1", "-loop"])
        .arg(loop_count.to_string())
        .arg(&animation);
    tool(encode).await;
    // Assert the embedded loop count itself; both fixtures must remain animated.
    let bytes = tokio::fs::read(&animation).await.unwrap();
    let mut offset = 12;
    let mut observed_loop_count = None;
    let mut frame_durations = Vec::new();
    while offset + 8 <= bytes.len() {
        let size = usize::try_from(u32::from_le_bytes(
            bytes[offset + 4..offset + 8].try_into().unwrap(),
        ))
        .unwrap();
        assert!(
            offset + 8 + size <= bytes.len(),
            "fixture chunk is complete"
        );
        if &bytes[offset..offset + 4] == b"ANIM" {
            assert_eq!(size, 6, "WebP ANIM chunk must contain its loop count");
            observed_loop_count = Some(u16::from_le_bytes(
                bytes[offset + 12..offset + 14].try_into().unwrap(),
            ));
        } else if &bytes[offset..offset + 4] == b"ANMF" {
            assert!(size >= 16, "WebP frame must contain its duration");
            frame_durations.push(u32::from_le_bytes([
                bytes[offset + 20],
                bytes[offset + 21],
                bytes[offset + 22],
                0,
            ]));
        }
        offset += 8 + size + size % 2;
    }
    assert_eq!(observed_loop_count, Some(loop_count));
    assert_eq!(frame_durations, vec![500; 4]);
    let mut probe = Command::new(runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"));
    probe
        .args(["-v", "error", "-show_streams", "-of", "json"])
        .arg(&animation);
    let metadata: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let stream = &metadata["streams"][0];
    assert_eq!(stream["codec_name"], "webp_anim");
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(animation.to_str().unwrap())
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaStream" SET "Codec"=?,"PixelFormat"=? WHERE "MediaVersionId"=100 AND "Index"=0"#)
        .bind(stream["codec_name"].as_str().unwrap())
        .bind(stream["pix_fmt"].as_str().unwrap())
        .execute(pool).await.unwrap();
    animation
}

async fn assert_schedule_duration(playlist: &Path) {
    let mut probe = Command::new(runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"));
    probe
        .args([
            "-v",
            "error",
            "-show_entries",
            "format=duration",
            "-of",
            "json",
        ])
        .arg(playlist);
    let metadata: serde_json::Value = serde_json::from_slice(&tool(probe).await).unwrap();
    let duration: f64 = metadata["format"]["duration"]
        .as_str()
        .unwrap()
        .parse()
        .unwrap();
    assert!(
        (duration - 8.0).abs() < 0.05,
        "animation and handoff must fill only their eight-second schedule: {duration}"
    );
}

async fn serve_image(
    first: &mut crate::next_engine::protocol::PlayoutItem,
    image: PathBuf,
) -> direct_remote_servers::Http {
    let origin = direct_remote_servers::Http::start(image).await;
    let source = first
        .tracks
        .as_mut()
        .unwrap()
        .video
        .as_mut()
        .unwrap()
        .source
        .as_mut()
        .unwrap();
    let Source::Local {
        probe_hint,
        in_point_ms,
        out_point_ms,
        ..
    } = source
    else {
        panic!("image fixture must project a local source");
    };
    *source = Source::Http {
        uri: origin.uri.clone(),
        probe_hint: probe_hint.take(),
        in_point_ms: *in_point_ms,
        out_point_ms: *out_point_ms,
        is_live: None,
        headers: Vec::new(),
        user_agent: None,
        keep_alive: None,
        reconnect: None,
        reconnect_delay_max: None,
        timeout_us: None,
    };
    origin
}

async fn project(
    pool: &crate::db::DbPool,
    path: &Path,
    start: chrono::DateTime<chrono::Utc>,
    finish: chrono::DateTime<chrono::Utc>,
    seek_ms: i64,
) -> crate::next_engine::protocol::PlayoutItem {
    let item = ersatzrs_domain::PlayoutItem {
        start,
        finish,
        in_point: chrono::Duration::milliseconds(seek_ms),
        out_point: chrono::Duration::milliseconds(seek_ms) + (finish - start),
        ..selection::tests::item()
    };
    let channel = crate::repositories::make_repositories(pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    selection::project_item(
        pool,
        &channel,
        &item,
        100,
        Source::Local {
            path: path.to_str().unwrap().to_owned(),
            in_point_ms: None,
            out_point_ms: None,
            probe_hint: Some(
                crate::next_engine::probe_hint::persisted(pool, 100)
                    .await
                    .unwrap()
                    .unwrap(),
            ),
        },
        &Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

async fn assert_blue_handoff(ffmpeg: &Path, playlist: &Path) {
    for seek in ["6.25", "7.25"] {
        let mut decode = Command::new(ffmpeg);
        decode
            .args(["-v", "error", "-nostdin", "-i"])
            .arg(playlist)
            .args([
                "-ss",
                seek,
                "-map",
                "0:v:0",
                "-frames:v",
                "1",
                "-vf",
                "scale=1:1",
                "-pix_fmt",
                "rgb24",
                "-f",
                "rawvideo",
                "pipe:1",
            ]);
        let pixel = tool(decode).await;
        assert!(
            matches!(pixel.as_slice(), [r,g,b] if *b > 180 && *r < 40 && *g < 40),
            "next scheduled image must replace the repeating animation at {seek}s: {pixel:?}"
        );
    }
}
