//! Finite preview of the existing installed retained-interval producer fixture.
//! Inherits its Unix-only shell/Python fixture boundary.
use super::*;
use ersatzrs_core::repositories::ChannelRepository as _;

#[tokio::test]
#[ignore = "requires official Next, FFmpeg and Python 3; isolated retained preview"]
async fn real_next_retained_troubleshooting_preserves_cut_timeline() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::retained_callback_runtime::troubleshooting::fixture_child_retained_preview").await;
}

#[tokio::test]
#[ignore = "child of real_next_retained_troubleshooting_preserves_cut_timeline"]
async fn fixture_child_retained_preview() {
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    ersatzrs_ffmpeg::set_resolved_ffmpeg_tools(ffmpeg.to_str().unwrap(), ffprobe.to_str().unwrap());
    let root = prepared_artifact_runtime::temporary_root().await;
    let clip = timeline_fixture::generate_source(&ffmpeg, root.path()).await;
    let (pool, locker) = prepare_schedule(&clip, false, &text_profile()).await;
    publish_retained(&pool, root.path(), &clip, false).await;
    let (channel, profile, subtitle, media) = preview_selection(&pool).await;
    let start = media.item.start;
    let host = std::sync::Arc::new(host::NextHost::new(pool, locker, root.path().join("host")));
    let mut callback = callback_listener(host.clone()).await;
    let output = root.path().join("preview");
    let request = host::NextStart {
        channel_id: 0,
        preview: true,
        number: channel.number.clone(),
        executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
        output: output.clone(),
        ffmpeg: ffmpeg.clone(),
        ffprobe: ffprobe.clone(),
        replacements: Vec::new(),
        idle_timeout: None,
        deadline: Instant::now() + Duration::from_secs(30),
    };
    let (instance, item) = host
        .prepare_troubleshooting_session(
            crate::next_engine::troubleshooting::TroubleshootingPreview {
                media: &media,
                channel: &channel,
                profile: &profile,
                subtitle_id: Some(subtitle),
                selection_time: None,
                watermark_ids: &[],
                replacements: &[],
                ffmpeg: &ffmpeg,
                deadline: request.deadline,
                diagnostics: None,
            },
            request.clone(),
        )
        .await
        .unwrap();
    let mut config = manifest(root.path(), &profile, &request, item).await;
    config.playout.virtual_start = Some(start);
    let mut process = NextProcess::start_troubleshooting_with_session(
        &request.executable,
        &request.number,
        &output,
        &config,
        None,
        Some(instance),
        request.deadline,
    )
    .await
    .unwrap();
    let result = timeout(Duration::from_mins(1), process.wait()).await;
    if !matches!(&result, Ok(Ok(status)) if status.success()) {
        let _ = process.stop().await;
    }
    let cleanup = host.finish_troubleshooting_session(instance).await;
    callback.stop().await;
    assert!(cleanup.is_ok());
    assert!(
        matches!(result, Ok(Ok(status)) if status.success()),
        "retained preview failed; synthetic fixture retained at {}",
        root.keep().display()
    );
    let playlist = output.join("ffmpeg.m3u8");
    assert!(tokio::fs::read_to_string(&playlist)
        .await
        .unwrap()
        .contains("#EXT-X-ENDLIST"));
    assert_eq!(
        tokio::fs::read_to_string(root.path().join("producer-starts"))
            .await
            .unwrap(),
        "play-intervals\n"
    );
    assert_request(root.path()).await;
    timeline_fixture::assert_timeline(&ffmpeg, &playlist, &[], false).await;
    assert_troubleshooting_dossier(&output);
}

async fn preview_selection(
    pool: &crate::db::DbPool,
) -> (
    ersatzrs_domain::Channel,
    PlaybackProfile,
    i64,
    schedule::ScheduledMedia,
) {
    let mut channel = crate::repositories::make_repositories(pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    channel.id = 0;
    channel.number = ".troubleshooting".into();
    let profile = crate::playback::load_playback_profile(pool, 1)
        .await
        .unwrap()
        .unwrap();
    let subtitle = sqlx::query_scalar::<_, i64>(
        r#"SELECT "Id" FROM "Subtitle" WHERE "RemoteStreamMetadataId"=30"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    sqlx::query(r#"DELETE FROM "PlayoutItem""#)
        .execute(pool)
        .await
        .unwrap();
    let start = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0).unwrap();
    let media = schedule::bind_media(
        pool,
        ersatzrs_domain::PlayoutItem {
            start,
            finish: start + chrono::Duration::seconds(14),
            in_point: chrono::Duration::seconds(10),
            out_point: chrono::Duration::seconds(24),
            ..selection::tests::item()
        },
    )
    .await
    .unwrap();
    (channel, profile, subtitle, media)
}

async fn manifest(
    root: &Path,
    profile: &PlaybackProfile,
    request: &host::NextStart,
    item: crate::next_engine::protocol::PlayoutItem,
) -> crate::next_engine::protocol::ChannelConfig {
    let folder = root.join("manifest");
    tokio::fs::create_dir_all(&folder).await.unwrap();
    let name = format!(
        "{}_{}.json",
        item.start.timestamp_millis(),
        item.finish.timestamp_millis()
    );
    let playout = crate::next_engine::protocol::Playout {
        version: crate::next_engine::protocol::PLAYOUT_VERSION.into(),
        items: vec![item],
    };
    tokio::fs::write(folder.join(name), serde_json::to_vec(&playout).unwrap())
        .await
        .unwrap();
    let mut config = crate::next_engine::config::channel_config(
        profile,
        ersatzrs_domain::NextEngineTextSubtitleMode::Burn,
        &folder,
        &request.ffmpeg,
        &request.ffprobe,
    )
    .unwrap();
    config.ffmpeg.reports_folder = Some(request.output.to_str().unwrap().into());
    config
}
