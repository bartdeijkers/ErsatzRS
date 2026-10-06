use super::*;
use ersatzrs_core::scheduling::{PlayoutPreservation, SchedulingReferenceData};

async fn reference(pool: &SqlitePool, now: chrono::DateTime<Utc>) -> SchedulingReferenceData {
    make_repositories(pool.clone())
        .scheduling
        .classic_reference_data(88, now, Duration::zero())
        .await
        .unwrap()
}

async fn initial_window(url: &str, start: chrono::DateTime<Utc>) -> SqlitePool {
    let pool = ersatzrs_infra::db::init(url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;
    run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(88, PlayoutBuildMode::Reset),
        start,
        None,
    )
    .await
    .unwrap();
    common::seed_profile_and_channel(&repos, 2).await;
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ScheduleKind", "Seed") VALUES (89, 2, 0, 99)"#).await.unwrap();
    let mut item = reference(&pool, start).await.existing_items.remove(0);
    item.id = 0;
    item.playout_id = 89;
    repos.playout.upsert_item(&item).await.unwrap();
    pool
}

#[tokio::test]
async fn explicit_continue_refuses_refresh_promotion_and_keeps_published_programmes() {
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let now = start + Duration::minutes(1);
    let pool = initial_window("sqlite::memory:", start).await;
    pool.execute(r#"DELETE FROM "CollectionEnumeratorState"
        WHERE "PlayoutProgramScheduleAnchorId" IN (
            SELECT "Id" FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId"=88 AND "AnchorDate" IS NULL
        );
        DELETE FROM "PlayoutProgramScheduleAnchor" WHERE "PlayoutId"=88 AND "AnchorDate" IS NULL;"#)
        .await.unwrap();
    let before = reference(&pool, now).await;
    let rows = persisted_playout_item_rows(&pool, 88).await;
    let registry = FakeHlsRegistry::default();
    let error = run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(88, PlayoutBuildMode::Continue)
            .with_preservation(PlayoutPreservation::KeepCurrent),
        now,
        Some(&registry),
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("Classic Continue refused"),
        "{error}"
    );
    assert_eq!(persisted_playout_item_rows(&pool, 88).await, rows);
    assert_eq!(
        reference(&pool, now).await.existing_history,
        before.existing_history
    );
    assert!(registry.playout_updates.lock().unwrap().is_empty());
    let message: String = sqlx::query_scalar(
        "SELECT Message FROM PlayoutBuildStatus WHERE PlayoutId=88 AND Success=0",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(message.contains("Classic Continue refused"));
}

#[tokio::test]
async fn live_rebuild_keeps_prefetched_prefix_including_leading_mirror_programmes() {
    for (offset, retained_minutes) in [(None, 20), (Some("-01:00:00"), 80), (Some("01:00:00"), 20)]
    {
        for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
            let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
            let now = start + Duration::minutes(9) + Duration::seconds(59);
            let pool = initial_window("sqlite::memory:", start).await;
            if let Some(offset) = offset {
                sqlx::query(r#"UPDATE "Channel" SET "PlayoutSource"=1, "MirrorSourceChannelId"=1, "PlayoutOffset"=? WHERE "Id"=2"#)
                    .bind(offset).execute(&pool).await.unwrap();
            }
            let before = reference(&pool, now).await;
            let other = persisted_playout_item_rows(&pool, 89).await;
            pool.execute(r#"UPDATE "ProgramScheduleItem" SET "SubtitleMode"=3 WHERE "Id"=401"#)
                .await
                .unwrap();
            let registry = FakeHlsRegistry::default();
            run_requested_playout_build(
                &pool,
                &EntityLocker::new(),
                PlayoutBuildRequest::new(88, mode)
                    .with_preservation(PlayoutPreservation::KeepCurrent),
                now,
                Some(&registry),
            )
            .await
            .unwrap();
            let after = reference(&pool, now).await;
            let seam = start + Duration::minutes(retained_minutes);
            let retained = before
                .existing_items
                .iter()
                .filter(|item| item.finish <= seam)
                .cloned()
                .collect::<Vec<_>>();
            assert_eq!(
                &after.existing_items[..retained.len()],
                retained,
                "{mode:?}, {offset:?}"
            );
            assert!(
                retained.len() >= 2,
                "the already-prefetched next programme must retain its row and settings"
            );
            assert_eq!(after.existing_items[retained.len()].start, seam);
            assert!(after.existing_items[retained.len()..]
                .iter()
                .all(|item| item.subtitle_mode == Some(ChannelSubtitleMode::Any)));
            assert!(after
                .existing_items
                .windows(2)
                .all(|pair| pair[0].finish <= pair[1].start));
            assert_eq!(persisted_playout_item_rows(&pool, 89).await, other);
            assert!(
                registry.playout_updates.lock().unwrap().is_empty(),
                "native playback must not be restarted"
            );
        }
    }
}

#[tokio::test]
async fn classic_preserving_rebuild_survives_restart_and_keeps_current_settings() {
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let directory = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", directory.path().join("classic.db").display());
        let start = Utc.with_ymd_and_hms(2026, 4, 15, 23, 48, 0).unwrap();
        let now = start + Duration::minutes(1);
        let pool = initial_window(&url, start).await;
        let before = reference(&pool, now).await;
        let current = before
            .existing_items
            .iter()
            .find(|item| item.start <= now && item.finish > now)
            .unwrap()
            .clone();
        let other = persisted_playout_item_rows(&pool, 89).await;
        pool.execute(r#"UPDATE "ProgramScheduleItem" SET "SubtitleMode" = 3 WHERE "Id" = 401"#)
            .await
            .unwrap();
        pool.close().await;

        let pool = ersatzrs_infra::db::init(&url).await.unwrap();
        let registry = FakeHlsRegistry::default();
        run_requested_playout_build(
            &pool,
            &EntityLocker::new(),
            PlayoutBuildRequest::new(88, mode).with_preservation(PlayoutPreservation::KeepCurrent),
            now,
            Some(&registry),
        )
        .await
        .unwrap();
        let after = reference(&pool, now).await;
        assert_eq!(
            after
                .existing_items
                .iter()
                .find(|item| item.start <= now && item.finish > now),
            Some(&current)
        );
        let old_history = |data: &SchedulingReferenceData| {
            data.existing_history
                .iter()
                .filter(|entry| entry.when < current.finish)
                .cloned()
                .collect::<Vec<_>>()
        };
        assert_eq!(old_history(&after), old_history(&before));
        assert!(after
            .existing_items
            .iter()
            .filter(|item| item.start >= current.finish)
            .all(|item| item.subtitle_mode == Some(ChannelSubtitleMode::Any)));
        assert!(after
            .existing_items
            .windows(2)
            .all(|pair| pair[0].finish <= pair[1].start));
        assert!(
            registry.playout_updates.lock().unwrap().is_empty(),
            "preserved native playback receives no immediate restart"
        );
        assert_eq!(persisted_playout_item_rows(&pool, 89).await, other);

        // A legacy or damaged restore seam must fail before replacing rows.
        sqlx::query(
            r#"DELETE FROM "PlayoutClassicCheckpoint" WHERE "PlayoutId" = 88
            AND "CheckpointDate" = (SELECT "Finish" FROM "PlayoutItem" WHERE "Id" = ?)"#,
        )
        .bind(current.id)
        .execute(&pool)
        .await
        .unwrap();
        let retained = persisted_playout_item_rows(&pool, 88).await;
        let error = run_requested_playout_build(
            &pool,
            &EntityLocker::new(),
            PlayoutBuildRequest::new(88, PlayoutBuildMode::Refresh)
                .with_preservation(PlayoutPreservation::KeepCurrent),
            now,
            None,
        )
        .await
        .unwrap_err();
        assert!(error.to_string().contains("checkpoint"));
        assert_eq!(persisted_playout_item_rows(&pool, 88).await, retained);
        assert_eq!(persisted_playout_item_rows(&pool, 89).await, other);
    }
}

#[tokio::test]
async fn preserving_build_rejects_elapsed_boundary_without_losing_durable_retry() {
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
        let pool = initial_window("sqlite::memory:", start).await;
        let before = reference(&pool, start).await;
        let seam = before.existing_items[0].finish;
        // One persisted timestamp tick is less than the real production input-read
        // and build work. No sleep or timing-sensitive scheduling race is required.
        let now = seam - Duration::nanoseconds(100);
        let repos = make_repositories(pool.clone());
        let playout = repos.playout.get_with_anchor(88).await.unwrap();
        let other = persisted_playout_item_rows(&pool, 89).await;
        let mut tx = pool.begin().await.unwrap();
        ersatzrs_infra::scan_refresh::persist_unowned_refresh_intents(
            &mut tx,
            ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
            &[88],
        )
        .await
        .unwrap();
        tx.commit().await.unwrap();
        let request =
            PlayoutBuildRequest::new(88, mode).with_preservation(PlayoutPreservation::KeepCurrent);
        // With no live registry, the request uses its immediate programme seam.
        // Connected producers select a later safe seam, covered separately below.
        let error = run_requested_playout_build(&pool, &EntityLocker::new(), request, now, None)
            .await
            .unwrap_err();
        assert!(error.to_string().contains("programme boundary"), "{error}");
        let after = reference(&pool, now).await;
        assert_eq!(after.existing_items, before.existing_items);
        assert_eq!(after.existing_history, before.existing_history);
        assert_eq!(
            after.existing_classic_checkpoints,
            before.existing_classic_checkpoints
        );
        let mut rejected_playout = repos.playout.get_with_anchor(88).await.unwrap();
        assert!(!rejected_playout.build_status.as_ref().unwrap().success);
        rejected_playout.build_status = playout.build_status.clone();
        assert_eq!(rejected_playout, playout);
        assert_eq!(persisted_playout_item_rows(&pool, 89).await, other);
        let intent: (i64, i64) = sqlx::query_as(
            r#"SELECT "State", "Attempts" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 88"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(
            intent,
            (0, 1),
            "the unchanged generation stays pending with backoff"
        );

        run_requested_playout_build(
            &pool,
            &EntityLocker::new(),
            request,
            seam + Duration::seconds(1),
            None,
        )
        .await
        .unwrap();
        let after_retry = reference(&pool, seam).await;
        assert_eq!(after_retry.existing_items[1], before.existing_items[1]);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"SELECT COUNT(*) FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId" = 88"#,
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            0
        );
    }
}
