//! Row238 / Next `ChannelSession::reseek_mid_item`: a published replacement
//! starts before the buffered clock and must not replay its red opening.
//! Set `ERSATZRS_APP_DATA_DIR` to an isolated report directory in addition to
//! the parent module's runtime variables and `ERSATZRS_NEXT_TEST_RTSP_SERVER`.
use super::*;
use ersatzrs_core::{hls::HlsSessionHandle, repositories::PlayoutRepository as _};
use std::{collections::BTreeMap, sync::Arc};

// Next needs four 4-second segments before readiness; leave another startup
// window to publish while the paced current programme is still active.
const INITIAL_SECONDS: i64 = 32;

#[tokio::test]
#[ignore = "requires exact Next, FFmpeg9 and pinned MediaMTX; active-playout replacement"]
async fn real_next_playout_refresh_keeps_schedule_clock_picture_and_selected_audio() {
    refresh_case(false).await;
}

#[tokio::test]
#[ignore = "requires qualified indexed Next, FFmpeg9 and pinned MediaMTX"]
async fn real_next_indexed_playout_refresh_preserves_clock_tracks_and_unchanged_files() {
    refresh_case(true).await;
}

async fn refresh_case(indexed: bool) {
    let _owned_data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    let root = tempfile::tempdir().unwrap();
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(
            std::fs::File::create(root.path().join("session.log")).unwrap(),
        ))
        .try_init()
        .expect("dedicated nextest process diagnostic subscriber");
    let folder = root.path().to_owned();
    let result = tokio::spawn(async move { run(&folder, indexed).await }).await;
    let retained = root.keep();
    eprintln!(
        "active playout refresh indexed={indexed}: {}",
        retained.display()
    );
    assert!(
        matches!(&result, Ok(Ok(()))),
        "active playout refresh: {result:?}; retained at {}",
        retained.display()
    );
}

async fn run(root: &Path, indexed: bool) -> Result<(), String> {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let original = root.join("original.mkv");
    generate_clip(&ffmpeg, &original).await;
    let clip = root.join("clock.mkv");
    let mut encode = Command::new(&ffmpeg);
    encode
        .args(["-v", "error", "-nostdin", "-i"])
        .arg(original)
        .args([
            "-map",
            "0",
            "-vf",
            "drawbox=color=red:t=fill:enable='lt(t,5)'",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-threads",
            "2",
            "-g",
            "25",
            "-c:a",
            "copy",
        ])
        .arg(&clip);
    tool(encode).await;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    let (pool, locker) = prepare_schedule(&clip, false, &profile).await;
    let stream = direct_remote_servers::Rtsp::start(&ffmpeg, &clip, root).await;
    prepare_remote(&pool, &stream.uri).await?;
    let mut initial = selection::tests::item();
    initial.id = 1;
    initial.playout_id = 1;
    initial.media_item_id = 11;
    initial.in_point = chrono::Duration::zero();
    initial.finish = initial.start + chrono::Duration::seconds(INITIAL_SECONDS);
    initial.out_point = chrono::Duration::seconds(INITIAL_SECONDS);
    initial.pinned_stream_selection = Some(ersatzrs_domain::PinnedStreamSelection {
        media_version_id: 100,
        video_stream_index: 0,
        audio_stream_index: Some(2),
        subtitle_stream_index: None,
    });
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&initial)
        .await
        .map_err(|error| error.to_string())?;
    let host = refresh_host(&pool, locker, root, indexed).await?;
    let mut callback = text_subtitle_runtime_tests::callback_listener(host.clone()).await;
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = root.join("output");
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
        .ok_or("refresh session admission")?;
    let handle = registry
        .try_get("1")
        .await
        .ok_or("refresh session handle")?;
    let captured = observe(&pool, &initial, handle.as_ref(), &output, root, indexed).await;
    let stopped = registry.stop_channel("1").await;
    task.await.map_err(|error| error.to_string())?;
    callback.stop().await;
    stream.stop().await;
    let captured = captured?;
    assert!(stopped);
    assert!(!output.exists(), "owned output must be cleaned up");
    assert!(registry.try_get("1").await.is_none());
    for saved in captured {
        retained_nut_runtime_tests::assert_blue_video(&ffmpeg, &saved).await;
        retained_nut_runtime_tests::assert_selected_audio(&ffmpeg, &saved).await;
    }
    Ok(())
}

async fn refresh_host(
    pool: &crate::db::DbPool,
    locker: ersatzrs_core::locking::EntityLocker,
    root: &Path,
    indexed: bool,
) -> Result<Arc<host::NextHost>, String> {
    let folder = root.join("playout");
    if !indexed {
        return Ok(Arc::new(host::NextHost::new(pool.clone(), locker, folder)));
    }
    let permit = crate::next_engine::indexed_runtime::IndexedReaderPermit::qualify(
        &runtime_path("ERSATZRS_NEXT_TEST_BIN"),
        None,
    )
    .await
    .map_err(|error| error.to_string())?;
    let clock = ersatzrs_core::programme::ProgrammeClock::named("UTC")
        .map_err(|error| error.to_string())?;
    let overlays = root.join("overlays");
    std::fs::create_dir(&overlays).map_err(|error| error.to_string())?;
    std::fs::write(
        overlays.join("default.json"),
        r#"{"version":"https://ersatztv.org/channel/version/0.1.0","normalization":{"audio":{"bitrate_kbps":176,"buffer_kbps":352}}}"#,
    )
    .map_err(|error| error.to_string())?;
    Ok(Arc::new(
        host::NextHost::new_indexed(pool.clone(), locker, folder, clock, permit)
            .with_config_overlays(overlays),
    ))
}

async fn prepare_remote(pool: &crate::db::DbPool, uri: &str) -> Result<(), String> {
    sqlx::raw_sql(r#"
        INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (11,1,0);
        INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (11,'https://example.test/replaced',2,1);
        INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (30,11,0,'Synthetic refresh','2026-10-24','2026-10-24');
        UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=11 WHERE "Id"=100;
    "#).execute(pool).await.map_err(|error| error.to_string())?;
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"=? WHERE "Id"=11"#)
        .bind(uri)
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
    Ok(())
}

#[derive(Clone)]
struct Segment {
    sequence: u64,
    at: chrono::DateTime<chrono::Utc>,
    duration: f64,
    file: String,
}

fn segments(body: &str) -> Result<Vec<Segment>, String> {
    let mut sequence = None;
    let mut at = None;
    let mut duration = None;
    let mut result = Vec::new();
    for line in body.lines() {
        if let Some(value) = line.strip_prefix("#EXT-X-MEDIA-SEQUENCE:") {
            sequence = value.parse().ok();
        } else if let Some(value) = line.strip_prefix("#EXT-X-PROGRAM-DATE-TIME:") {
            at = Some(
                chrono::DateTime::parse_from_str(value, "%Y-%m-%dT%H:%M:%S%.f%z")
                    .map_err(|error| format!("invalid Next programme timestamp {value}: {error}"))?
                    .with_timezone(&chrono::Utc),
            );
        } else if let Some(value) = line.strip_prefix("#EXTINF:") {
            duration = value.split(',').next().and_then(|value| value.parse().ok());
        } else if !line.starts_with('#')
            && Path::new(line)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("ts"))
        {
            let number = sequence.ok_or("missing media sequence")?;
            result.push(Segment {
                sequence: number,
                at: at.take().ok_or("missing programme timestamp")?,
                duration: duration.take().ok_or("missing duration")?,
                file: line.into(),
            });
            sequence = Some(number + 1);
        }
    }
    Ok(result)
}

async fn observe(
    pool: &crate::db::DbPool,
    initial: &ersatzrs_domain::PlayoutItem,
    handle: &dyn HlsSessionHandle,
    output: &Path,
    root: &Path,
    indexed: bool,
) -> Result<Vec<PathBuf>, String> {
    timeout(Duration::from_secs(45), async {
        let before = loop {
            handle.touch();
            if let Some(body) = handle.trim_playlist().await {
                if segments(&body)?.len() >= 2 {
                    break body;
                }
            }
            if handle.startup_readiness() == HlsStartupReadiness::Failed {
                return Err("refresh startup failed".into());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        };
        std::fs::write(root.join("before.m3u8"), &before).map_err(|error| error.to_string())?;
        let mut seen: BTreeMap<_, _> = segments(&before)?
            .into_iter()
            .map(|s| (s.sequence, s))
            .collect();
        if seen.values().any(|s| s.at >= initial.finish)
            || chrono::Utc::now() + chrono::Duration::seconds(3) >= initial.finish
        {
            return Err("fixture missed the active-item update window".into());
        }
        if indexed {
            assert_unchanged_index(handle, root).await?;
        }
        let mut replacement = initial.clone();
        replacement.id = 2;
        replacement.media_item_id = 10;
        replacement.finish = initial.start + chrono::Duration::seconds(60);
        replacement.out_point = chrono::Duration::seconds(60);
        sqlx::query(
            r#"UPDATE "MediaVersion" SET "MovieId"=10,"RemoteStreamId"=NULL WHERE "Id"=100"#,
        )
        .execute(pool)
        .await
        .map_err(|error| error.to_string())?;
        sqlx::query(r#"DELETE FROM "PlayoutItem""#)
            .execute(pool)
            .await
            .map_err(|error| error.to_string())?;
        crate::repositories::make_repositories(pool.clone())
            .playout
            .upsert_item(&replacement)
            .await
            .map_err(|error| error.to_string())?;
        handle
            .publish_playout_update(std::time::Instant::now() + Duration::from_secs(5))
            .await
            .map_err(|error| error.to_string())?;
        if indexed {
            let index = root.join("playout/1/indexed/index.json");
            let before = std::fs::read(root.join("unchanged-index.json")).unwrap();
            assert_ne!(before, std::fs::read(index).unwrap());
        }
        let mut captured = Vec::new();
        loop {
            handle.touch();
            let body = handle
                .trim_playlist()
                .await
                .ok_or("active playlist disappeared")?;
            for segment in segments(&body)? {
                if seen.contains_key(&segment.sequence) {
                    continue;
                }
                if segment.at >= initial.finish {
                    let saved = root.join(format!("after-{}.ts", segment.sequence));
                    tokio::fs::copy(output.join(&segment.file), &saved)
                        .await
                        .map_err(|error| error.to_string())?;
                    captured.push(saved);
                }
                seen.insert(segment.sequence, segment);
            }
            if captured.len() >= 3 {
                std::fs::write(root.join("after.m3u8"), &body)
                    .map_err(|error| error.to_string())?;
                check_continuity(&seen)?;
                return Ok(captured);
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "replacement did not publish three post-boundary segments".to_owned())?
}

fn check_continuity(seen: &BTreeMap<u64, Segment>) -> Result<(), String> {
    let samples: Vec<_> = seen.values().collect();
    for pair in samples.windows(2) {
        if pair[1].sequence != pair[0].sequence + 1 {
            return Err("media sequence skipped or reset during refresh".into());
        }
        let delta_ms = i32::try_from((pair[1].at - pair[0].at).num_milliseconds())
            .map_err(|_| "programme clock exceeded the fixture interval")?;
        let drift = (f64::from(delta_ms) - pair[0].duration * 1000.0).abs();
        if drift > 100.0 {
            return Err(format!(
                "programme clock jumped at sequence{}: {drift}ms",
                pair[1].sequence
            ));
        }
    }
    Ok(())
}

async fn assert_unchanged_index(handle: &dyn HlsSessionHandle, root: &Path) -> Result<(), String> {
    let folder = root.join("playout/1/indexed");
    let index = folder.join("index.json");
    let bytes = std::fs::read(&index).map_err(|error| error.to_string())?;
    std::fs::write(root.join("unchanged-index.json"), &bytes).map_err(|error| error.to_string())?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let mut paths = vec![index];
    paths.extend(
        value["days"]
            .as_array()
            .unwrap()
            .iter()
            .map(|day| folder.join(day["file"].as_str().unwrap())),
    );
    let marked = std::time::SystemTime::UNIX_EPOCH + Duration::from_secs(1_600_000_000);
    let before: Vec<_> = paths
        .iter()
        .map(|path| {
            std::fs::OpenOptions::new()
                .write(true)
                .open(path)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(marked))
                .unwrap();
            (
                std::fs::read(path).unwrap(),
                path.metadata().unwrap().modified().unwrap(),
            )
        })
        .collect();
    handle
        .publish_playout_update(std::time::Instant::now() + Duration::from_secs(5))
        .await
        .map_err(|error| error.to_string())?;
    for (path, (content, modified)) in paths.iter().zip(before) {
        assert_eq!(std::fs::read(path).unwrap(), content);
        assert_eq!(path.metadata().unwrap().modified().unwrap(), modified);
    }
    assert!(!root
        .join("playout/1")
        .join(crate::next_engine::publication::PLAYOUT_FILENAME)
        .exists());
    Ok(())
}
