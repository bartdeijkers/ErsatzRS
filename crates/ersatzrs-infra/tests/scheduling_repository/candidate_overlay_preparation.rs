//! Pending item variables and template precedence must match the committed reader.

use super::{
    next_preparation::{bind_fixture_files, project},
    scripted_refresh::published_fixture,
    scripted_sources::build,
    *,
};
use ersatzrs_core::{
    locking::{EntityLocker, PlayoutLockGuard},
    repositories::{ChannelRepository, PlayoutRepository},
    scheduling::PlayoutBuildMode,
};
use ersatzrs_domain::{PlayoutTemplate, StreamingEngine, StreamingMode};
use ersatzrs_infra::{
    next_engine::coordinator::ChannelPublisher, playback, repositories::SqlitePlayoutRepository,
};
use std::time::{Duration as StdDuration, Instant};

#[tokio::test]
async fn candidate_overlays_use_uncommitted_variables_and_template_precedence() {
    let root = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", root.path().join("overlays.db").display());
    let (pool, _, first) = published_fixture(&url, &root.path().join("schedule.lua")).await;
    bind_fixture_files(&pool).await;
    seed_overlay_choices(&pool).await;
    let repos = make_repositories(pool.clone());
    let mut channel = repos.channel.get_with_profile(903).await.unwrap();
    channel.streaming_engine = StreamingEngine::Next;
    channel.streaming_mode = StreamingMode::TransportStream;
    repos.channel.upsert(&channel).await.unwrap();
    let repository = SqlitePlayoutRepository(pool.clone());
    let mut playout = repository.get_with_anchor(903).await.unwrap();
    let start = first.added_items[0].start;
    let finish = start + Duration::hours(6);
    let mut result = build(
        &pool,
        playout.clone(),
        start + Duration::seconds(1),
        finish,
        PlayoutBuildMode::Refresh,
    )
    .await;
    for item in &mut result.added_items {
        for graphics in &mut item.graphics_elements {
            graphics.variables = Some(r#"{"Caption":"Candidate"}"#.into());
        }
    }
    let locker = EntityLocker::new();
    let publisher = ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), 903);
    let _guard = PlayoutLockGuard::try_acquire(&locker, 903).unwrap();
    for template in [false, true] {
        let committed_ids: std::collections::HashSet<i64> = repository
            .items_in_range(903, start, finish)
            .await
            .unwrap()
            .into_iter()
            .map(|item| item.id)
            .collect();
        if template {
            playout.templates.push(PlayoutTemplate {
                playout_id: 903,
                template_id: 800,
                deco_template_id: Some(770),
                ..PlayoutTemplate::default()
            });
        }
        let mut pending = repository
            .prepare_build_result(&result, &playout, Duration::zero())
            .await
            .unwrap();
        let seen = std::sync::Mutex::new(None);
        let prepared = publisher
            .prepare_build(
                &mut pending,
                start,
                finish,
                Instant::now() + StdDuration::from_secs(10),
                |target, media| async {
                    let item = media
                        .iter()
                        .find(|media| !committed_ids.contains(&media.item.id))
                        .expect("candidate contains an unpublished item");
                    assert!(playback::select_playback_graphics(
                        &pool,
                        &target,
                        item.item.id,
                        item.item.start
                    )
                    .await
                    .is_err());
                    let overlays = item.overlays(&pool, &target).await.unwrap();
                    assert_overlay_choices(&overlays, template);
                    *seen.lock().unwrap() = Some((item.item.id, item.item.start, overlays));
                    project(target, media).await
                },
            )
            .await
            .unwrap()
            .unwrap();
        let (item_id, item_start, overlays) = seen.into_inner().unwrap().unwrap();
        pending.commit().await.unwrap();
        prepared.publish().await.unwrap();
        assert_committed_overlays(&pool, &channel, item_id, item_start, &overlays).await;
        // Restore the original fixture for the separate template case.
        if !template {
            repository
                .apply_build_result(&first, &playout, Duration::zero())
                .await
                .unwrap();
        }
    }
}

async fn assert_committed_overlays(
    pool: &SqlitePool,
    channel: &ersatzrs_domain::Channel,
    item_id: i64,
    item_start: chrono::DateTime<Utc>,
    overlays: &(
        Vec<playback::ResolvedPlaybackWatermark>,
        Vec<playback::ResolvedPlaybackGraphicsElement>,
    ),
) {
    assert_eq!(
        overlays.0,
        playback::select_playback_watermarks(pool, channel, item_id, false, item_start)
            .await
            .unwrap()
    );
    assert_eq!(
        overlays.1,
        playback::select_playback_graphics(pool, channel, item_id, item_start)
            .await
            .unwrap()
    );
}

fn assert_overlay_choices(
    overlays: &(
        Vec<playback::ResolvedPlaybackWatermark>,
        Vec<playback::ResolvedPlaybackGraphicsElement>,
    ),
    template: bool,
) {
    assert_eq!(
        overlays
            .0
            .iter()
            .map(|w| w.watermark.id)
            .collect::<Vec<_>>(),
        [if template { 502 } else { 501 }]
    );
    assert_eq!(
        overlays.1.iter().map(|g| g.element.id).collect::<Vec<_>>(),
        [if template { 602 } else { 601 }]
    );
    assert_eq!(
        overlays.1[0].variables.as_deref(),
        if template {
            None
        } else {
            Some(r#"{"Caption":"Candidate"}"#)
        }
    );
}

async fn seed_overlay_choices(pool: &SqlitePool) {
    seed_block_deco_fixture(pool).await;
    pool.execute(r#"
        UPDATE "ChannelWatermark" SET "ImageSource"=1 WHERE "Id"=501;
        INSERT INTO "ChannelWatermark" (
            "Id", "Name", "Mode", "ImageSource", "Location", "Size", "WidthPercent",
            "HorizontalMarginPercent", "VerticalMarginPercent", "FrequencyMinutes",
            "DurationSeconds", "Opacity", "PlaceWithinSourceContent", "ZIndex"
        ) SELECT 502, 'template bug', "Mode", 1, "Location", "Size", "WidthPercent",
            "HorizontalMarginPercent", "VerticalMarginPercent", "FrequencyMinutes",
            "DurationSeconds", "Opacity", "PlaceWithinSourceContent", "ZIndex"
          FROM "ChannelWatermark" WHERE "Id"=501;
        INSERT INTO "Artwork" ("Id", "ArtworkKind", "ChannelId", "Path", "DateAdded", "DateUpdated")
            VALUES (950, 2, 903, 'https://example.test/logo.png', '2026-04-15T00:00:00Z', '2026-04-15T00:00:00Z');
        INSERT INTO "GraphicsElement" ("Id", "Name", "Kind", "Path") VALUES (602, 'template graphic', 'Image', 'template.svg');
        UPDATE "Deco" SET "WatermarkMode"='Override', "GraphicsElementsMode"='Override' WHERE "Id"=760;
        INSERT INTO "DecoWatermark" ("DecoId", "WatermarkId") VALUES (760, 502);
        INSERT INTO "DecoGraphicsElement" ("DecoId", "GraphicsElementId") VALUES (760, 602);
        UPDATE "DecoTemplateItem" SET "StartTime"='00:00:00', "EndTime"='00:00:00' WHERE "Id"=771;
        INSERT INTO "TemplateGroup" ("Id", "Name") VALUES (72, 'candidate template');
        INSERT INTO "Template" ("Id", "TemplateGroupId", "Name", "DateUpdated") VALUES (800, 72, 'candidate', '0');
    "#).await.unwrap();
}
