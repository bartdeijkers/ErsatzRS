use super::*;
use ersatzrs_core::scheduling::{
    BlockKey, EffectiveBlock, EffectiveBlockItem, SchedulingCollectionKey, SchedulingMediaItem,
};
use ersatzrs_domain::{BlockStopScheduling, ChannelSubtitleMode, PlaybackOrder};

#[tokio::test]
async fn block_future_boundary_preserves_generated_items_after_reload() {
    for mode in [
        PlayoutBuildMode::Continue,
        PlayoutBuildMode::Refresh,
        PlayoutBuildMode::Reset,
    ] {
        for delay in [Duration::zero(), Duration::minutes(30)] {
            let pool = common::test_db().await;
            let repos = make_repositories(pool.clone());
            common::seed_profile_and_channel(&repos, 1).await;
            common::seed_media_graph(&pool).await;
            pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (1, 1, 'Block', 42)"#)
                .await.unwrap();
            let now = common::fixed_time();
            let playout = repos.playout.get_with_anchor(1).await.unwrap();
            let mut reference = reference_data(now, delay);
            let builder = PlayoutBuilder::default();
            let initial = builder
                .build(SchedulingRequest {
                    playout: playout.clone(),
                    start: now - Duration::minutes(100),
                    finish: now + Duration::hours(2),
                    mode: PlayoutBuildMode::Reset,
                    reference_data: reference.clone(),
                })
                .unwrap();
            assert_eq!(initial.added_items.len(), 4);
            repos
                .playout
                .apply_build_result(&initial, &playout, delay)
                .await
                .unwrap();
            drop(repos);

            let repos = make_repositories(pool.clone());
            let persisted = repos
                .playout
                .items_in_range(1, now - Duration::hours(2), now + Duration::hours(2))
                .await
                .unwrap();
            assert_eq!(persisted.len(), 4);
            reference.existing_items = persisted.clone();
            reference
                .effective_blocks
                .retain(|block| block.start > now + Duration::minutes(30));
            let playout = repos.playout.get_with_anchor(1).await.unwrap();
            let continued = builder
                .build(SchedulingRequest {
                    playout: playout.clone(),
                    start: now,
                    finish: now + Duration::hours(2),
                    mode,
                    reference_data: reference,
                })
                .unwrap();
            assert!(continued.items_to_remove.is_empty());
            assert!(continued.added_items.is_empty());
            repos
                .playout
                .apply_build_result(&continued, &playout, delay)
                .await
                .unwrap();
            let retained = repos
                .playout
                .items_in_range(1, now - Duration::hours(2), now + Duration::hours(2))
                .await
                .unwrap();
            let expected = persisted
                .into_iter()
                .filter(|item| item.finish >= now - delay)
                .collect::<Vec<_>>();
            assert_eq!(retained.len(), if delay.is_zero() { 2 } else { 3 });
            assert_eq!(retained, expected, "{mode:?}, delay={delay}");
            assert!(retained
                .iter()
                .all(
                    |item| item.preferred_audio_language_code.as_deref() == Some("nld")
                        && item.preferred_subtitle_language_code.as_deref() == Some("eng")
                        && item.subtitle_mode == Some(ChannelSubtitleMode::Forced)
                ));
        }
    }
}

fn reference_data(now: chrono::DateTime<Utc>, delay: Duration) -> SchedulingReferenceData {
    let mut media = SchedulingMediaItem::new(1, Duration::minutes(30));
    media.preferred_audio_language_code = Some("nld".to_owned());
    media.preferred_subtitle_language_code = Some("eng".to_owned());
    media.subtitle_mode = Some(ChannelSubtitleMode::Forced);
    let mut reference = SchedulingReferenceData::default();
    reference
        .media_by_key
        .insert("movies".to_owned(), vec![media]);
    reference.max_playout_offset = delay;
    reference.effective_blocks = [-100, -50, -20, 60]
        .into_iter()
        .map(|minutes| EffectiveBlock {
            block_id: 1,
            block_key: Some(BlockKey {
                block_id: 1,
                block_updated_ticks: 1,
                template_id: 1,
                template_updated_ticks: 1,
                playout_template_id: 1,
                playout_template_updated_ticks: 1,
            }),
            template_item_id: 1,
            name: "Synthetic block".to_owned(),
            start: now + Duration::minutes(minutes),
            duration: Duration::minutes(30),
            stop_scheduling: BlockStopScheduling::BeforeDurationEnd,
            items: vec![EffectiveBlockItem {
                id: 1,
                block_id: 1,
                content: "movies".to_owned(),
                collection_key: SchedulingCollectionKey::default(),
                playback_order: PlaybackOrder::Chronological,
                include_in_program_guide: true,
                disable_watermarks: false,
                watermark_ids: Vec::new(),
                graphics_element_ids: Vec::new(),
            }],
            deco: None,
        })
        .collect();
    reference
}
