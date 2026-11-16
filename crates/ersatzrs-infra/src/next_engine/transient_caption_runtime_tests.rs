//! R292-D: automatic preview captions can disappear after private publication.
use super::*;
use crate::next_engine::{protocol, troubleshooting::TroubleshootingPreview};
use ersatzrs_core::repositories::ChannelRepository as _;

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg; isolated automatic local-preview caption race"]
async fn real_next_row292_local_preview_captions_preserve_av() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::transient_caption_runtime::fixture_child_local_captions").await;
}

#[tokio::test]
#[ignore = "requires exact Next/FFmpeg; isolated automatic prepared-preview caption race"]
async fn real_next_row292_prepared_preview_captions_preserve_av() {
    prepared_artifact_runtime::run_isolated_test("next_engine::process::runtime_tests::text_subtitle_runtime_tests::transient_caption_runtime::fixture_child_prepared_captions").await;
}

#[tokio::test]
#[ignore = "child of real_next_row292_local_preview_captions_preserve_av"]
async fn fixture_child_local_captions() {
    cases(false).await;
}

#[tokio::test]
#[ignore = "child of real_next_row292_prepared_preview_captions_preserve_av"]
async fn fixture_child_prepared_captions() {
    cases(true).await;
}

async fn cases(prepared: bool) {
    let root = prepared_artifact_runtime::temporary_root().await;
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let clip = generate_text_clip(&ffmpeg, root.path()).await;
    for convert in [false, true] {
        for removed in [false, true] {
            Box::pin(preview_case(root.path(), &clip, prepared, convert, removed)).await;
        }
    }
}

async fn preview_case(root: &Path, clip: &Path, prepared: bool, convert: bool, removed: bool) {
    let label = format!("prepared={prepared} convert={convert} removed={removed}");
    let root = root.join(format!("{prepared}-{convert}-{removed}"));
    tokio::fs::create_dir(&root).await.unwrap();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let (pool, locker) = prepare_schedule(clip, false, &text_profile()).await;
    configure_text_tracks(&pool, false, convert, false).await;
    let caption = clip.parent().unwrap().join("fixture.nl.srt");
    tokio::fs::write(&caption, CUES).await.unwrap();
    if prepared {
        prepared_artifact_runtime::publish_fixture(&pool, clip).await;
    }
    let mut channel = crate::repositories::make_repositories(pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    channel.id = 0;
    channel.number = ".row292".into();
    let profile = crate::playback::load_playback_profile(&pool, 1)
        .await
        .unwrap()
        .unwrap();
    let start = chrono::DateTime::from_timestamp(chrono::Utc::now().timestamp(), 0).unwrap();
    let media = schedule::bind_media(
        &pool,
        ersatzrs_domain::PlayoutItem {
            id: 0,
            start,
            finish: start + chrono::Duration::seconds(12),
            in_point: chrono::Duration::seconds(5),
            out_point: chrono::Duration::seconds(17),
            ..selection::tests::item()
        },
    )
    .await
    .unwrap();
    sqlx::query(r#"DELETE FROM "PlayoutItem""#)
        .execute(&pool)
        .await
        .unwrap();
    let host = Arc::new(host::NextHost::new(pool.clone(), locker, root.join("host")));
    let mut callback = callback_listener(host.clone()).await;
    let request = preview_request(&root, &channel, &ffmpeg);
    let (instance, item) = host
        .prepare_troubleshooting_session(
            TroubleshootingPreview {
                media: &media,
                channel: &channel,
                profile: &profile,
                subtitle_id: None,
                selection_time: None,
                watermark_ids: &[],
                graphics_ids: &[],
                replacements: &[],
                ffmpeg: &ffmpeg,
                deadline: request.deadline,
                diagnostics: None,
            },
            request.clone(),
        )
        .await
        .unwrap();
    if prepared && !convert && !removed {
        super::prepared_caption_controls::verify(
            &host,
            &serde_json::to_value(&item).unwrap(),
            clip,
            &caption,
        )
        .await;
    }
    if removed {
        remove_caption(&pool, &caption, prepared).await;
    }
    let mut config = manifest(&root, &profile, &request, item, convert).await;
    config.playout.virtual_start = Some(start);
    let result = run_preview(&request, &config, instance).await;
    host.finish_troubleshooting_session(instance).await.unwrap();
    callback.stop().await;
    assert!(result, "row292 preview producer failed: {label}");
    let playlist = tokio::fs::read_to_string(request.output.join("ffmpeg.m3u8"))
        .await
        .unwrap();
    assert!(playlist.contains("#EXT-X-ENDLIST"));
    let segment =
        assert_text_playlist(&request.output, &playlist, convert && !removed, false).await;
    super::caption_publication_runtime_tests::assert_programme_video(&ffmpeg, &segment, &label)
        .await;
    assert_text_video_and_audio(&ffmpeg, &segment, !convert && !removed).await;
    println!(
        "row292 preview {label} programme=blue audio=nld880 captions={} cleanup=joined",
        !removed
    );
}

async fn remove_caption(pool: &crate::db::DbPool, caption: &Path, prepared: bool) {
    if prepared {
        sqlx::query(r#"DELETE FROM "Subtitle" WHERE "SubtitleKind"=1"#)
            .execute(pool)
            .await
            .unwrap();
    } else {
        tokio::fs::remove_file(caption).await.unwrap();
    }
}

fn preview_request(
    root: &Path,
    channel: &ersatzrs_domain::Channel,
    ffmpeg: &Path,
) -> host::NextStart {
    host::NextStart {
        channel_id: channel.id,
        preview: true,
        number: channel.number.clone(),
        executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
        output: root.join("output"),
        ffmpeg: ffmpeg.to_owned(),
        ffprobe: runtime_path("ERSATZRS_NEXT_TEST_FFPROBE"),
        replacements: Vec::new(),
        idle_timeout: None,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}

async fn run_preview(
    request: &host::NextStart,
    config: &protocol::ChannelConfig,
    instance: uuid::Uuid,
) -> bool {
    let process = NextProcess::start_troubleshooting_with_session(
        &request.executable,
        &request.number,
        &request.output,
        config,
        None,
        Some(instance),
        request.deadline,
    )
    .await;
    match process {
        Ok(mut process) => {
            let result = timeout(Duration::from_secs(45), process.wait()).await;
            if !matches!(result, Ok(Ok(status)) if status.success()) {
                let _ = process.stop().await;
            }
            matches!(result, Ok(Ok(status)) if status.success())
        }
        Err(_) => false,
    }
}

async fn manifest(
    root: &Path,
    profile: &PlaybackProfile,
    request: &host::NextStart,
    item: protocol::PlayoutItem,
    convert: bool,
) -> protocol::ChannelConfig {
    let folder = root.join("manifest");
    tokio::fs::create_dir(&folder).await.unwrap();
    let name = format!(
        "{}_{}.json",
        item.start.timestamp_millis(),
        item.finish.timestamp_millis()
    );
    let playout = protocol::Playout {
        version: protocol::PLAYOUT_VERSION.into(),
        items: vec![item],
    };
    tokio::fs::write(folder.join(name), serde_json::to_vec(&playout).unwrap())
        .await
        .unwrap();
    let mode = if convert {
        ersatzrs_domain::NextEngineTextSubtitleMode::Convert
    } else {
        ersatzrs_domain::NextEngineTextSubtitleMode::Burn
    };
    crate::next_engine::config::channel_config(
        profile,
        mode,
        &folder,
        &request.ffmpeg,
        &request.ffprobe,
    )
    .unwrap()
}
