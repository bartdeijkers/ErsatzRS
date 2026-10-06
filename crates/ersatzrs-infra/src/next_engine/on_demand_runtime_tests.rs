//! Decoded stop/resume proof with a red opening and a blue remainder.
use super::*;
use ersatzrs_core::{hls::HlsSessionHandle, repositories::PlayoutRepository as _};
use std::sync::Arc;

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; disposable on-demand playback"]
async fn real_next_on_demand_resume_keeps_viewer_position_audio_and_subtitles() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let root = tempfile::tempdir().unwrap();
    let clip = phased_clip(&ffmpeg, root.path()).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..FFmpegProfile::default()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    configure_text_tracks(&pool, true, false, false).await;
    prepare_automatic_text_tracks(&pool, &ffmpeg, &clip, root.path()).await;
    seed_on_demand(&pool).await;
    let host = Arc::new(host::NextHost::new(
        pool.clone(),
        locker,
        root.path().join("playout"),
    ));
    let mut callback = callback_listener(host.clone()).await;
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = root.path().join("output");
    let (task, session) = start(&host, &registry, &output).await;
    assert_color(&ffmpeg, &output, session.as_ref(), false).await;
    tokio::time::sleep(Duration::from_secs(12)).await;
    let touched_before = chrono::Utc::now();
    session.touch();
    let touched_after = chrono::Utc::now();
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    let saved = checkpoint(&pool).await;
    assert!(
        saved >= touched_before - chrono::Duration::seconds(1) - chrono::Duration::microseconds(1)
    );
    assert!(saved <= touched_after - chrono::Duration::seconds(1));
    // Time without viewers must not count as watched time.
    tokio::time::sleep(Duration::from_secs(2)).await;
    assert_eq!(checkpoint(&pool).await, saved);
    let (task, session) = start(&host, &registry, &output).await;
    assert_color(&ffmpeg, &output, session.as_ref(), true).await;
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    callback.stop().await;
    assert!(!output.exists());
}

async fn phased_clip(ffmpeg: &Path, root: &Path) -> PathBuf {
    let source = generate_text_clip_with_cues(
        ffmpeg,
        root,
        "1\n00:00:00,000 --> 00:01:00,000\nOn-demand subtitle proof\n",
    )
    .await;
    let clip = root.join("phases.mkv");
    let mut encode = Command::new(ffmpeg);
    encode
        .args(["-v", "error", "-nostdin", "-y", "-i"])
        .arg(source)
        .args([
            "-map",
            "0",
            "-vf",
            "drawbox=color=red:t=fill:enable='lt(t,10)'",
            "-c",
            "copy",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "2",
        ])
        .arg(&clip);
    tool(encode).await;
    clip
}

async fn seed_on_demand(pool: &crate::db::DbPool) {
    sqlx::query(r#"UPDATE "Channel" SET "PlayoutMode"=1 WHERE "Id"=1"#)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?, '1')"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT)
        .execute(pool)
        .await
        .unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.start -= chrono::Duration::hours(1);
    item.finish = item.start + chrono::Duration::seconds(60);
    item.in_point = chrono::Duration::zero();
    item.out_point = chrono::Duration::seconds(60);
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
}

async fn checkpoint(pool: &crate::db::DbPool) -> chrono::DateTime<chrono::Utc> {
    let value: String =
        sqlx::query_scalar(r#"SELECT "OnDemandCheckpoint" FROM "Playout" WHERE "Id"=1"#)
            .fetch_one(pool)
            .await
            .unwrap();
    crate::repositories::common::decode_datetime("on-demand checkpoint", &value).unwrap()
}

async fn start(
    host: &Arc<host::NextHost>,
    registry: &Arc<crate::hls::InMemoryHlsSessionRegistry>,
    output: &Path,
) -> (tokio::task::JoinHandle<()>, Arc<dyn HlsSessionHandle>) {
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.into(),
                ffmpeg: runtime_path("ERSATZRS_NEXT_TEST_FFMPEG"),
                ffprobe: runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let session = registry.try_get("1").await.unwrap();
    timeout(Duration::from_secs(30), async {
        loop {
            match session.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => panic!("on-demand startup failed"),
                _ => tokio::time::sleep(Duration::from_millis(10)).await,
            }
        }
    })
    .await
    .unwrap();
    (task, session)
}

async fn assert_color(ffmpeg: &Path, output: &Path, session: &dyn HlsSessionHandle, resumed: bool) {
    let playlist = session.trim_playlist().await.unwrap();
    let segment = playlist
        .lines()
        .find(|line| {
            !line.starts_with('#')
                && Path::new(line)
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
        })
        .unwrap();
    let segment = output.join(segment);
    let mut decode = Command::new(ffmpeg);
    decode
        .args(["-v", "error", "-nostdin", "-ss", "0.5", "-i"])
        .arg(&segment)
        .args([
            "-frames:v",
            "1",
            "-vf",
            "format=rgb24,crop=1:1:160:60",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixel = tool(decode).await;
    assert_eq!(pixel.len(), 3);
    if resumed {
        assert!(
            pixel[2] > 180 && pixel[0] < 50,
            "resume must skip the opening: {pixel:?}"
        );
    } else {
        assert!(
            pixel[0] > 180 && pixel[2] < 50,
            "initial playback must start at the opening: {pixel:?}"
        );
    }
    assert_text_video_and_audio(ffmpeg, &segment, true).await;
}
