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
    fallback_case(FallbackSource::Local, false).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; disposable Remote Stream gap playback"]
async fn real_next_remote_fallback_preserves_tracks_and_returns_to_programme() {
    fallback_case(FallbackSource::Direct, false).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated prepared-artifact gap playback"]
async fn real_next_prepared_fallback_preserves_tracks_and_returns_to_programme() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::fallback_runtime_tests::fixture_child_prepared_fallback").await;
}

#[tokio::test]
#[ignore = "child of real_next_prepared_fallback_preserves_tracks_and_returns_to_programme"]
async fn fixture_child_prepared_fallback() {
    fallback_case(FallbackSource::Prepared, false).await;
}

// Installed fixture add-on uses a POSIX shell; production preparation remains portable.
#[cfg(unix)]
#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated ordinary-producer gap playback"]
async fn real_next_ordinary_fallback_preserves_tracks_and_returns_to_programme() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::fallback_runtime_tests::fixture_child_ordinary_fallback").await;
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "child of real_next_ordinary_fallback_preserves_tracks_and_returns_to_programme"]
async fn fixture_child_ordinary_fallback() {
    fallback_case(FallbackSource::Ordinary, false).await;
}

// Retained fixture uses a POSIX shell and Python; the production owner is portable.
#[cfg(unix)]
#[tokio::test]
#[ignore = "requires official Next, FFmpeg and Python; isolated retained-interval gap playback"]
async fn real_next_retained_fallback_preserves_tracks_and_returns_to_programme() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::fallback_runtime_tests::fixture_child_retained_fallback").await;
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "child of real_next_retained_fallback_preserves_tracks_and_returns_to_programme"]
async fn fixture_child_retained_fallback() {
    fallback_case(FallbackSource::Retained, false).await;
}

#[tokio::test]
#[ignore = "requires official Next and FFmpeg; isolated fallback Convert matrix"]
async fn real_next_fallback_convert_matrix_preserves_cues_and_programme_handoff() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::fallback_runtime_tests::fixture_child_fallback_convert_matrix").await;
}

#[tokio::test]
#[ignore = "child of real_next_fallback_convert_matrix_preserves_cues_and_programme_handoff"]
async fn fixture_child_fallback_convert_matrix() {
    for source in [
        FallbackSource::Local,
        FallbackSource::Direct,
        FallbackSource::Prepared,
        #[cfg(unix)]
        FallbackSource::Ordinary,
        #[cfg(unix)]
        FallbackSource::Retained,
    ] {
        fallback_case(source, true).await;
    }
}

#[cfg(unix)]
#[tokio::test]
#[ignore = "focused child requires explicit temporary app data and Next runtime tools"]
async fn fixture_child_ordinary_fallback_convert() {
    fallback_case(FallbackSource::Ordinary, true).await;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FallbackSource {
    Local,
    Direct,
    Prepared,
    #[cfg(unix)]
    Ordinary,
    #[cfg(unix)]
    Retained,
}

#[allow(clippy::too_many_lines)] // One disposable source, callback and playback lifecycle.
async fn fallback_case(source: FallbackSource, convert: bool) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = if matches!(source, FallbackSource::Local | FallbackSource::Direct) {
        tempfile::tempdir().unwrap()
    } else {
        prepared_artifact_runtime::temporary_root().await
    };
    let clip = match source {
        #[cfg(unix)]
        FallbackSource::Ordinary => {
            ordinary_callback_runtime::generate_source(&ffmpeg, root.path()).await
        }
        #[cfg(unix)]
        FallbackSource::Retained => {
            retained_callback_runtime::generate_fallback_source(&ffmpeg, root.path()).await
        }
        _ => generate_text_clip(&ffmpeg, root.path()).await,
    };
    let (pool, locker) = prepare_schedule(&clip, false, &text_profile()).await;
    configure_text_tracks(&pool, true, convert, false).await;
    if source == FallbackSource::Prepared {
        prepared_artifact_runtime::publish_fixture(&pool, &clip).await;
    }
    #[cfg(unix)]
    if source == FallbackSource::Ordinary {
        ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(
            ffmpeg.to_str().unwrap(),
            ffprobe.to_str().unwrap(),
        );
        ordinary_callback_runtime::install(&pool, root.path(), false).await;
        ordinary_callback_runtime::configure_sidecar(&pool, root.path(), Some(convert), true).await;
    }
    #[cfg(unix)]
    if source == FallbackSource::Retained {
        ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(
            ffmpeg.to_str().unwrap(),
            ffprobe.to_str().unwrap(),
        );
        retained_callback_runtime::publish_retained(&pool, root.path(), &clip, convert).await;
    }
    let server = if source == FallbackSource::Direct {
        Some(httpmock::MockServer::start_async().await)
    } else {
        None
    };
    let remote_mock = if let Some(server) = &server {
        let bytes = std::sync::Arc::new(tokio::fs::read(&clip).await.unwrap());
        let mock = server
            .mock_async(|when, then| {
                when.method(httpmock::Method::GET).path("/fallback.mkv");
                then.respond_with(move |request| fixture_range_response(request, &bytes));
            })
            .await;
        configure_remote(&pool, &format!("{}/fallback.mkv", server.base_url())).await;
        Some(mock)
    } else {
        None
    };
    let start = chrono::Utc::now() - chrono::Duration::seconds(2);
    let repos = crate::repositories::make_repositories(pool.clone());
    let last_finish = match source {
        #[cfg(unix)]
        FallbackSource::Retained => 50, // Its published programme is 24 seconds after cuts.
        // Keep the following programme within one 44-second Next work-ahead chunk,
        // so exactly one producer per occurrence is a deterministic assertion.
        _ => 56,
    };
    for (id, begin, end) in [(1, 0, 6), (2, 26, last_finish)] {
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
                    if (820.0..940.0).contains(&hz)
                        && (if convert { white == 0 } else { white > 20 })
                    {
                        if convert {
                            assert_fallback_vtt(&segment.with_extension("vtt")).await;
                        }
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
    if let Some(mock) = remote_mock {
        assert!(mock.calls_async().await > 0);
    }
    #[cfg(unix)]
    if source == FallbackSource::Ordinary {
        assert_eq!(
            tokio::fs::read_to_string(root.path().join("producer-starts"))
                .await
                .unwrap(),
            "play\nplay\nplay\n",
            "each of two programmes and one ephemeral filler must own exactly one producer; last seek: {}",
            tokio::fs::read_to_string(root.path().join("producer-seek")).await.unwrap()
        );
    }
    #[cfg(unix)]
    if source == FallbackSource::Retained {
        assert_eq!(
            tokio::fs::read_to_string(root.path().join("producer-starts"))
                .await
                .unwrap(),
            "play-intervals\nplay-intervals\nplay-intervals\n"
        );
        let log = tokio::fs::read_to_string(root.path().join("requests.ndjson"))
            .await
            .unwrap();
        let requests = serde_json::Deserializer::from_str(&log)
            .into_iter::<ersatzrs_addon_contract::RemoteStreamRetainedPlayRequest>()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(requests.len(), 3);
        assert_eq!(
            requests[1].retained_intervals_milliseconds,
            vec![0..12_000, 18_000..30_000]
        );
    }
}

async fn configure_remote(pool: &crate::db::DbPool, url: &str) {
    for sql in [
        r#"INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (10,'https://example.test/finite',1,0)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=10 WHERE "Id"=100"#,
        r#"INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (30,10,0,'Synthetic','2026-09-24','2026-09-24')"#,
        r#"UPDATE "Subtitle" SET "MovieMetadataId"=NULL,"RemoteStreamMetadataId"=30"#,
        r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=10"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"=? WHERE "Id"=10"#)
        .bind(url)
        .execute(pool)
        .await
        .unwrap();
}

async fn assert_fallback_vtt(path: &Path) {
    let body = timeout(Duration::from_secs(5), async {
        loop {
            if let Ok(body) = tokio::fs::read_to_string(path).await {
                return body;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("converted fallback must publish matching-segment VTT");
    assert!(body.starts_with("WEBVTT\n"));
    assert!(
        body.contains(" --> "),
        "selected fallback caption must have a timed cue"
    );
    assert!(
        body.contains("BEFORE SEEK")
            || body.contains("Next text conversion proof")
            || body.contains("ACROSS CUT")
            || body.contains("AFTER CUT")
    );
    assert!(
        !body.contains("REMOVED CUT"),
        "excluded source captions must not leak into fallback"
    );
    assert!(body.contains("X-TIMESTAMP-MAP=LOCAL:00:00:00.000,MPEGTS:"));
}
