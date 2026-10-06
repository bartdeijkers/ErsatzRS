use super::*;
use ersatzrs_core::playout_builder::PlayoutBuilder;
use ersatzrs_core::repositories::PlayoutRepository as _;
use ersatzrs_core::scheduling::{
    PlayoutBuildMode, PlayoutBuildRequest, PlayoutPreservation, SchedulingReferenceData,
    SchedulingResult,
};

async fn reference(pool: &SqlitePool, now: chrono::DateTime<Utc>) -> SchedulingReferenceData {
    make_repositories(pool.clone())
        .scheduling
        .block_reference_data(900, now, 1, Duration::zero())
        .await
        .unwrap()
}

async fn preserving_build(pool: &SqlitePool, now: chrono::DateTime<Utc>, mode: PlayoutBuildMode) {
    ersatzrs_infra::maintenance::run_requested_playout_build(
        pool,
        &ersatzrs_core::locking::EntityLocker::new(),
        PlayoutBuildRequest::new(900, mode).with_preservation(PlayoutPreservation::KeepCurrent),
        now,
        None,
    )
    .await
    .unwrap();
}

async fn block_health(pool: &SqlitePool) -> ersatzrs_core::health::HealthCheckStatus {
    ersatzrs_infra::health::make_health_check_service(
        pool.clone(),
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        ersatzrs_infra::db::DatabaseSchemaStatus::default(),
    )
    .perform_health_checks()
    .await
    .into_iter()
    .find(|result| result.title == "Block Playout State")
    .unwrap()
    .status
}

#[tokio::test]
async fn block_preserving_refresh_restarts_with_identical_rotation_and_current_settings() {
    for order in [
        "Chronological",
        "SeasonEpisode",
        "Shuffle",
        "Random",
        "RandomRotation",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}",
            directory.path().join("block-refresh.db").display()
        );
        let pool = database(&url, order).await;
        let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
        let now = start + Duration::minutes(1);
        let (playout, initial) = build(&pool, start, start + Duration::hours(4)).await;
        make_repositories(pool.clone())
            .playout
            .apply_build_result(&initial, &playout, Duration::zero())
            .await
            .unwrap();
        let before = reference(&pool, now).await;
        let current = before.existing_items[0].clone();
        let seam = current.guide_finish.unwrap();
        pool.execute(r#"UPDATE "BlockItem" SET "DisableWatermarks" = 1 WHERE "Id" = 701"#)
            .await
            .unwrap();
        pool.close().await;

        let pool = ersatzrs_infra::db::init(&url).await.unwrap();
        for _ in 0..2 {
            preserving_build(&pool, now, PlayoutBuildMode::Refresh).await;
            let after = reference(&pool, now).await;
            assert_eq!(after.existing_items[0], current, "{order}");
            assert!(after.existing_items.len() >= before.existing_items.len());
            let expected = before
                .existing_items
                .iter()
                .filter(|item| item.start >= seam);
            let actual = after
                .existing_items
                .iter()
                .filter(|item| item.start >= seam);
            for (actual, expected) in actual.zip(expected) {
                let mut expected = expected.clone();
                expected.id = actual.id;
                expected.disable_watermarks = true;
                assert_eq!(actual, &expected, "{order}");
            }
            assert!(after
                .existing_items
                .windows(2)
                .all(|pair| pair[0].finish <= pair[1].start));
            assert_eq!(
                after
                    .existing_history
                    .iter()
                    .filter(|entry| entry.when < seam)
                    .collect::<Vec<_>>(),
                before
                    .existing_history
                    .iter()
                    .filter(|entry| entry.when < seam)
                    .collect::<Vec<_>>()
            );
            assert_eq!(
                block_health(&pool).await,
                ersatzrs_core::health::HealthCheckStatus::Pass
            );
        }
    }
}

#[tokio::test]
async fn block_preserving_reset_recovers_bad_history_and_retains_new_rotation_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("block-reset.db").display()
    );
    let pool = database(&url, "Shuffle").await;
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let now = start + Duration::minutes(1);
    let (playout, initial) = build(&pool, start, start + Duration::hours(4)).await;
    make_repositories(pool.clone())
        .playout
        .apply_build_result(&initial, &playout, Duration::zero())
        .await
        .unwrap();
    pool.execute(r#"UPDATE "PlayoutHistory" SET "Details" = '{}' WHERE "Id" = (SELECT MIN("Id") FROM "PlayoutHistory" WHERE "PlayoutId" = 900)"#).await.unwrap();
    let before = reference(&pool, now).await;
    assert_eq!(
        block_health(&pool).await,
        ersatzrs_core::health::HealthCheckStatus::Fail
    );
    let error = ersatzrs_infra::maintenance::run_requested_playout_build(
        &pool,
        &ersatzrs_core::locking::EntityLocker::new(),
        PlayoutBuildRequest::new(900, PlayoutBuildMode::Refresh)
            .with_preservation(PlayoutPreservation::KeepCurrent),
        now,
        None,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("checkpoint"), "{error}");
    let rejected = reference(&pool, now).await;
    assert_eq!(rejected.existing_items, before.existing_items);
    assert_eq!(rejected.existing_history, before.existing_history);

    preserving_build(&pool, now, PlayoutBuildMode::Reset).await;
    let reset = reference(&pool, now).await;
    assert_eq!(reset.existing_items[0], before.existing_items[0]);
    assert_eq!(
        block_health(&pool).await,
        ersatzrs_core::health::HealthCheckStatus::Pass
    );
    let reset_playout = make_repositories(pool.clone())
        .playout
        .get_with_anchor(900)
        .await
        .unwrap();
    assert_ne!(reset_playout.seed, playout.seed);
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let next = reset.existing_items[1].start + Duration::minutes(1);
    preserving_build(&pool, next, PlayoutBuildMode::Refresh).await;
    let refreshed = reference(&pool, next).await;
    assert!(refreshed.existing_items.len() >= reset.existing_items.len());
    for (actual, expected) in refreshed.existing_items.iter().zip(&reset.existing_items) {
        let mut expected = expected.clone();
        expected.id = actual.id;
        assert_eq!(actual, &expected);
    }
    assert_eq!(refreshed.existing_items[1].id, reset.existing_items[1].id);
    assert_eq!(
        make_repositories(pool.clone())
            .playout
            .get_with_anchor(900)
            .await
            .unwrap()
            .seed,
        reset_playout.seed
    );
    assert_eq!(
        block_health(&pool).await,
        ersatzrs_core::health::HealthCheckStatus::Pass
    );
}

#[tokio::test]
async fn block_corrupt_source_checkpoint_refuses_continuation_without_publication() {
    let pool = database("sqlite::memory:", "Shuffle").await;
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let now = start + Duration::hours(2);
    let (playout, first) = build(&pool, start, now).await;
    let repos = make_repositories(pool.clone());
    repos
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "PlayoutHistory" SET "Details" = json_set("Details", '$.BlockCheckpoint.Version', 99) WHERE "PlayoutId" = 900"#)
        .execute(&pool).await.unwrap();
    let before = repos
        .scheduling
        .block_reference_data(900, now, 1, Duration::zero())
        .await
        .unwrap();
    let error = ersatzrs_infra::maintenance::run_requested_playout_build(
        &pool,
        &ersatzrs_core::locking::EntityLocker::new(),
        ersatzrs_core::scheduling::PlayoutBuildRequest::new(900, PlayoutBuildMode::Continue),
        now,
        None,
    )
    .await
    .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("incompatible history checkpoint"),
        "{error}"
    );
    let after = repos
        .scheduling
        .block_reference_data(900, now, 1, Duration::zero())
        .await
        .unwrap();
    assert_eq!(after.existing_items, before.existing_items);
    assert_eq!(after.existing_history, before.existing_history);
}

#[tokio::test]
async fn preserving_publication_rechecks_deadline_after_waiting_for_database_writer() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("publication.db").display()
    );
    let pool = database(&url, "Shuffle").await;
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let finish = start + Duration::hours(4);
    let (playout, initial) = build(&pool, start, finish).await;
    let repos = make_repositories(pool.clone());
    repos
        .playout
        .apply_build_result(&initial, &playout, Duration::zero())
        .await
        .unwrap();
    let now = start + Duration::minutes(1);
    let before = reference(&pool, now).await;
    let mut reference_data = before.clone();
    let stored_playout = repos.playout.get_with_anchor(900).await.unwrap();
    reference_data.preservation = PlayoutPreservation::KeepCurrent;
    let mut candidate = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start: now,
            finish,
            mode: PlayoutBuildMode::Refresh,
            reference_data,
        })
        .unwrap();
    let writer = ersatzrs_infra::db::begin_write(&pool).await.unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    candidate.publication_deadline = Some(deadline);
    let mut publication = Box::pin(repos.playout.apply_build_result(
        &candidate,
        &playout,
        Duration::zero(),
    ));
    // Poll into the real database wait while the deadline is still valid.
    std::future::poll_fn(|cx| {
        assert!(std::future::Future::poll(publication.as_mut(), cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    tokio::time::sleep_until(tokio::time::Instant::from_std(deadline)).await;
    writer.rollback().await.unwrap();
    let error = publication.await.unwrap_err();
    assert!(error.to_string().contains("programme boundary"), "{error}");
    let after = reference(&pool, now).await;
    assert_eq!(after.existing_items, before.existing_items);
    assert_eq!(after.existing_history, before.existing_history);
    assert_eq!(
        repos.playout.get_with_anchor(900).await.unwrap(),
        stored_playout
    );
}

async fn database(url: &str, order: &str) -> SqlitePool {
    let pool = ersatzrs_infra::db::init(url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#).await.unwrap();
    seed_scheduling_fixture(&pool).await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 900).await;
    seed_block_scheduling_fixture(&pool).await;
    pool.execute(
        r#"
        DELETE FROM "PlayoutItem" WHERE "PlayoutId" = 900;
        DELETE FROM "PlayoutHistory" WHERE "PlayoutId" = 900;
        DELETE FROM "PlayoutTemplate" WHERE "PlayoutId" = 900 AND "TemplateId" = 810;
        DELETE FROM "CollectionItem" WHERE "CollectionId" = 1 AND "MediaItemId" = 20;
        UPDATE "Playout" SET "DecoId" = NULL WHERE "Id" = 900;
        INSERT INTO "TemplateItem" ("Id", "TemplateId", "BlockId", "StartTime") VALUES
            (802, 800, 700, '09:00:00'), (803, 800, 700, '10:00:00'),
            (804, 800, 700, '11:00:00');
    "#,
    )
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "BlockItem" SET "PlaybackOrder" = ? WHERE "Id" = 701"#)
        .bind(order)
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn build(
    pool: &SqlitePool,
    start: chrono::DateTime<Utc>,
    finish: chrono::DateTime<Utc>,
) -> (Playout, SchedulingResult) {
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(900).await.unwrap();
    let reference_data = repos
        .scheduling
        .block_reference_data(900, start, 1, Duration::zero())
        .await
        .unwrap();
    let result = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start,
            finish,
            mode: PlayoutBuildMode::Continue,
            reference_data,
        })
        .unwrap();
    (playout, result)
}

#[tokio::test]
async fn block_source_rotation_survives_sqlite_restart() {
    for order in [
        "Chronological",
        "SeasonEpisode",
        "Shuffle",
        "Random",
        "RandomRotation",
    ] {
        let directory = tempfile::tempdir().unwrap();
        let url = format!("sqlite://{}", directory.path().join("block.db").display());
        let pool = database(&url, order).await;
        let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
        let finish = start + Duration::hours(4);
        let (_, uninterrupted) = build(&pool, start, finish).await;
        if matches!(order, "Chronological" | "SeasonEpisode") {
            assert_eq!(
                uninterrupted
                    .added_items
                    .iter()
                    .map(|item| item.media_item_id)
                    .collect::<Vec<_>>(),
                vec![12, 13, 12, 13],
                "{order}"
            );
        }
        let (playout, first) = build(&pool, start, start + Duration::hours(2)).await;
        make_repositories(pool.clone())
            .playout
            .apply_build_result(&first, &playout, Duration::zero())
            .await
            .unwrap();
        pool.close().await;

        let pool = ersatzrs_infra::db::init(&url).await.unwrap();
        let (playout, continued) = build(&pool, start + Duration::hours(2), finish).await;
        assert_eq!(
            continued.added_items,
            uninterrupted.added_items[2..],
            "{order}"
        );
        make_repositories(pool.clone())
            .playout
            .apply_build_result(&continued, &playout, Duration::zero())
            .await
            .unwrap();
        let stored = make_repositories(pool.clone())
            .scheduling
            .block_reference_data(900, start + Duration::hours(2), 1, Duration::zero())
            .await
            .unwrap();
        let actual = stored
            .existing_items
            .iter()
            .filter(|item| item.start >= start + Duration::hours(2))
            .map(|item| item.media_item_id)
            .collect::<Vec<_>>();
        assert_eq!(
            actual,
            uninterrupted.added_items[2..]
                .iter()
                .map(|item| item.media_item_id)
                .collect::<Vec<_>>(),
            "{order}"
        );
    }
}
