//! Finite Next inputs: fractional seek, image handoff and natural termination.
use super::*;
use crate::next_engine::protocol::{Playout, Source, PLAYOUT_VERSION};
use ersatzrs_core::repositories::ChannelRepository as _;

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; finite GIF seek and static WebP handoff"]
async fn real_next_image_seek_and_handoff_finish_at_the_schedule_boundary() {
    run(false).await;
}

#[tokio::test]
#[ignore = "requires real Next and FFmpeg 9; seekable HTTP GIF repeat and termination"]
async fn real_next_http_image_seek_and_handoff_finish_at_the_schedule_boundary() {
    run(true).await;
}

async fn run(http: bool) {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let root = tempfile::tempdir().unwrap();
    let (pool, locker) = prepare_image(&ffmpeg, root.path(), "animated.gif").await;
    let gif = root.path().join("still.animated.gif");
    let start = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0).unwrap();
    let second_start = start + chrono::Duration::seconds(6);
    let finish = second_start + chrono::Duration::seconds(2);
    let (pool2, _) = prepare_image(&ffmpeg, root.path(), "webp").await;
    let mut first = project(&pool, &gif, start, second_start, 500).await;
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
        Some(serve_image(&mut first, gif).await)
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
        "decoded finite image seek/handoff, HTTP={http}: {}",
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
    // Only disposable fixture pools are touched; no running host/database is used.
    drop(locker);
}

async fn serve_image(
    first: &mut crate::next_engine::protocol::PlayoutItem,
    gif: PathBuf,
) -> direct_remote_servers::Http {
    let origin = direct_remote_servers::Http::start(gif).await;
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
