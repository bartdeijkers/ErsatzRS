//! Decode the gap filler and the following persisted programme through official Next.
use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;

async fn sample(ffmpeg: &Path, segment: &Path) -> (usize, f64) {
    let mut video = Command::new(ffmpeg);
    video
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-ss",
            "0.5",
            "-map",
            "0:v:0",
            "-frames:v",
            "1",
            "-vf",
            "crop=320:48:0:132",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ]);
    let pixels = tool(video).await;
    assert_eq!(pixels.len(), 320 * 48 * 3);
    let white = pixels
        .chunks_exact(3)
        .filter(|pixel| pixel.iter().all(|value| *value > 180))
        .count();
    let mut audio = Command::new(ffmpeg);
    audio
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(segment)
        .args([
            "-map", "0:a:0", "-t", "1", "-ac", "1", "-ar", "8000", "-f", "s16le", "pipe:1",
        ]);
    let pcm = tool(audio).await;
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|bytes| i16::from_le_bytes([bytes[0], bytes[1]]))
        .collect();
    assert!(samples.len() >= 7000);
    let crossings = samples
        .windows(2)
        .filter(|pair| pair[0] <= 0 && pair[1] > 0)
        .count();
    let hz = f64::from(u32::try_from(crossings).unwrap()) * 8000.0
        / f64::from(u32::try_from(samples.len()).unwrap());
    (white, hz)
}

#[tokio::test]
#[allow(clippy::too_many_lines)] // One disposable gap-to-programme playback lifecycle.
#[ignore = "requires official Next and FFmpeg; disposable synthetic gap playback"]
async fn real_next_dynamic_fallback_preserves_tracks_and_returns_to_programme() {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    let (pool, locker) = prepare_schedule(&clip, false, &text_profile()).await;
    configure_text_tracks(&pool, true, false, false).await;
    let start = chrono::Utc::now() - chrono::Duration::seconds(2);
    let repos = crate::repositories::make_repositories(pool.clone());
    for (id, begin, end) in [(1, 0, 6), (2, 26, 86)] {
        repos
            .playout
            .upsert_item(&ersatzrs_domain::PlayoutItem {
                id,
                playout_id: 1,
                media_item_id: 10,
                start: start + chrono::Duration::seconds(begin),
                finish: start + chrono::Duration::seconds(end),
                out_point: chrono::Duration::seconds(end - begin),
                preferred_audio_language_code: Some("eng".into()),
                subtitle_mode: Some(ersatzrs_domain::ChannelSubtitleMode::None),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    sqlx::query(r#"INSERT INTO "FillerPreset" ("Id","AllowWatermarks","CollectionType","MediaItemId","FillerKind","FillerMode","UseChaptersAsMediaItems") VALUES (1,0,10,10,4,0,0)"#)
        .execute(&pool).await.unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "FallbackFillerId"=1 WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    let host = std::sync::Arc::new(host::NextHost::new(
        pool.clone(),
        locker,
        root.path().join("playout"),
    ));
    let mut callback = callback_listener(host.clone()).await;
    let output = root.path().join("output");
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: binary,
                output: output.clone(),
                ffmpeg: ffmpeg.clone(),
                ffprobe,
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(45),
            },
        )
        .await
        .unwrap();
    let handle = registry.try_get("1").await.unwrap();
    let mut checked = std::collections::HashSet::new();
    let proof = timeout(Duration::from_mins(1), async {
        let mut saw_fallback = false;
        loop {
            if let Some(playlist) = handle.trim_playlist().await {
                for name in playlist.lines().filter(|line| {
                    !line.starts_with('#')
                        && Path::new(line)
                            .extension()
                            .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
                }) {
                    if checked.contains(name) {
                        continue;
                    }
                    let segment = output.join(name);
                    if !segment.exists() {
                        continue;
                    }
                    let (white, hz) = sample(&ffmpeg, &segment).await;
                    checked.insert(name.to_owned());
                    if white > 20 && (820.0..940.0).contains(&hz) {
                        saw_fallback = true;
                    }
                    if saw_fallback && white == 0 && (410.0..470.0).contains(&hz) {
                        return;
                    }
                }
            }
            assert_ne!(handle.startup_readiness(), HlsStartupReadiness::Failed);
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
    })
    .await;
    assert!(registry.stop_channel("1").await);
    task.await.unwrap();
    callback.stop().await;
    assert!(!output.exists());
    proof.expect(
        "Dutch fallback captions must be followed by the English programme without captions",
    );
    let rows: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "PlayoutItem""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 2);
}
