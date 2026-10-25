//! Next startup before a late PGS presentation, using the Native proof fixture.
use super::*;

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg9; synthetic 4K/TrueHD late-first-PGS startup"]
async fn real_next_4k_truehd_starts_before_first_pgs_and_preserves_late_picture_audio() {
    let _owned_data = runtime_path(crate::file_system_layout::APP_DATA_DIR_ENV_VAR);
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap().keep();
    println!("Next late-PGS fixture retained at {}", root.display());
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_ansi(false)
        .with_writer(std::sync::Mutex::new(
            std::fs::File::create(root.join("session.log")).unwrap(),
        ))
        .try_init()
        .expect("dedicated nextest process diagnostic subscriber");
    let clip = late_pgs::fixture(&ffmpeg, &root).await;
    late_pgs::assert_unknown_pgs(&ffprobe, &clip, &root).await;
    for off in [false, true] {
        run(&clip, &ffmpeg, &ffprobe, &root, off).await;
    }
}

async fn prepare(
    clip: &Path,
    off: bool,
) -> (crate::db::DbPool, ersatzrs_core::locking::EntityLocker) {
    use ersatzrs_core::repositories::PlayoutRepository as _;
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::H264,
        video_preset: Some("ultrafast".into()),
        audio_format: FFmpegProfileAudioFormat::Aac,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..Default::default()
    };
    let (pool, locker) = prepare_schedule(clip, false, &profile).await;
    sqlx::raw_sql(r#"
        UPDATE "Resolution" SET "Width"=1280,"Height"=720 WHERE "Id"=1;
        UPDATE "MediaVersion" SET "Width"=3840,"Height"=2080,"Duration"='00:00:28' WHERE "Id"=100;
        UPDATE "MediaStream" SET "Codec"='truehd',"Channels"=1 WHERE "MediaStreamKind"=2;
        DELETE FROM "Subtitle";
        DELETE FROM "MediaStream" WHERE "MediaStreamKind"=3;
        INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId")
          VALUES (4,3,'hdmv_pgs_subtitle',3,'nld',0,'Dutch',1,0,100);
        INSERT INTO "Subtitle" ("Id","MovieMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated")
          VALUES (1,20,0,'Dutch',3,'hdmv_pgs_subtitle',1,0,0,'nld',0,NULL,'2026-10-24','2026-10-24');
    "#).execute(&pool).await.unwrap();
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.in_point = chrono::Duration::zero();
    item.out_point = chrono::Duration::seconds(28);
    item.finish = item.start + chrono::Duration::seconds(28);
    if off {
        item.pinned_stream_selection = Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: None,
        });
    }
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
    (pool, locker)
}

async fn run(clip: &Path, ffmpeg: &Path, ffprobe: &Path, root: &Path, off: bool) {
    let (pool, locker) = prepare(clip, off).await;
    let name = if off { "off" } else { "burn" };
    let host = std::sync::Arc::new(host::NextHost::new(
        pool,
        locker,
        root.join(format!("playout-{name}")),
    ));
    let output = root.join(format!("output-{name}"));
    let registry = std::sync::Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let start = Instant::now();
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: ffmpeg.to_owned(),
                ffprobe: ffprobe.to_owned(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let handle = registry.try_get("1").await.unwrap();
    let observed_output = output.clone();
    let observed_root = root.to_owned();
    let ffmpeg = ffmpeg.to_owned();
    let proof = tokio::spawn(async move {
        let ready = timeout(Duration::from_secs(30), async {
            loop {
                match handle.startup_readiness() {
                    HlsStartupReadiness::Ready => break start.elapsed(),
                    HlsStartupReadiness::Failed => panic!("Next late-PGS startup failed"),
                    _ => tokio::time::sleep(Duration::from_millis(10)).await,
                }
            }
        })
        .await
        .expect("Next late-PGS readiness exceeded existing30s budget");
        let early = copy_segment(
            &handle,
            &observed_output,
            &observed_root,
            &format!("{name}-early"),
            0,
        )
        .await;
        late_pgs::assert_picture_and_audio(
            &ffmpeg,
            &early,
            &observed_root,
            &format!("{name}-early"),
            false,
        )
        .await;
        let late = copy_segment(
            &handle,
            &observed_output,
            &observed_root,
            &format!("{name}-late"),
            5,
        )
        .await;
        late_pgs::assert_picture_and_audio(
            &ffmpeg,
            &late,
            &observed_root,
            &format!("{name}-late"),
            !off,
        )
        .await;
        println!("Next {name} readiness_seconds={:.3}", ready.as_secs_f64());
    })
    .await;
    let stopped = registry.stop_channel("1").await;
    task.await.unwrap();
    assert!(!output.exists(), "owned Next output must clean up");
    proof.unwrap();
    assert!(stopped);
}

async fn copy_segment(
    handle: &std::sync::Arc<dyn ersatzrs_core::hls::HlsSessionHandle>,
    output: &Path,
    root: &Path,
    name: &str,
    ordinal: usize,
) -> PathBuf {
    timeout(Duration::from_secs(30), async {
        loop {
            let playlist = handle.trim_playlist().await.unwrap();
            let segments: Vec<_> = playlist
                .lines()
                .filter(|line| {
                    !line.starts_with('#')
                        && Path::new(line)
                            .extension()
                            .is_some_and(|extension| extension.eq_ignore_ascii_case("ts"))
                })
                .collect();
            if let Some(segment) = segments.get(ordinal) {
                std::fs::write(root.join(format!("{name}.m3u8")), &playlist).unwrap();
                let target = root.join(format!("{name}.ts"));
                tokio::fs::copy(output.join(segment), &target)
                    .await
                    .unwrap();
                return target;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("Next did not produce the required pre/post-cue segments")
}
