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

#[tokio::test]
async fn block_rebuild_removes_future_orphan_and_duplicate_history_after_sqlite_restart() {
    for mode in [PlayoutBuildMode::Continue, PlayoutBuildMode::Refresh] {
        let directory = tempfile::tempdir().unwrap();
        let clean = row187_rebuild(
            &directory.path().join("clean.db"),
            mode,
            PlayoutPreservation::EngineDefault,
            None,
        )
        .await;
        let repaired = row187_rebuild(
            &directory.path().join("orphan.db"),
            mode,
            PlayoutPreservation::EngineDefault,
            Some(false),
        )
        .await;
        assert_eq!(repaired.existing_items, clean.existing_items, "{mode:?}");
        assert_eq!(
            repaired.existing_history, clean.existing_history,
            "{mode:?}"
        );
    }
}

#[tokio::test]
async fn block_preserving_refresh_removes_future_orphans_before_the_retained_boundary() {
    for malformed in [false, true] {
        let directory = tempfile::tempdir().unwrap();
        let clean = row187_rebuild(
            &directory.path().join("clean.db"),
            PlayoutBuildMode::Refresh,
            PlayoutPreservation::KeepCurrent,
            None,
        )
        .await;
        let repaired = row187_rebuild(
            &directory.path().join("orphan.db"),
            PlayoutBuildMode::Refresh,
            PlayoutPreservation::KeepCurrent,
            Some(malformed),
        )
        .await;
        assert_eq!(
            repaired.existing_items, clean.existing_items,
            "malformed={malformed}"
        );
        assert_eq!(
            repaired.existing_history, clean.existing_history,
            "malformed={malformed}"
        );
    }
}

async fn row187_rebuild(
    path: &std::path::Path,
    mode: PlayoutBuildMode,
    preservation: PlayoutPreservation,
    contamination: Option<bool>,
) -> SchedulingReferenceData {
    let url = format!("sqlite://{}", path.display());
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 8, 0, 0).unwrap();
    let boundary = start + Duration::hours(1);
    let preserving = preservation == PlayoutPreservation::KeepCurrent;
    let clock = if preserving {
        start + Duration::minutes(1)
    } else {
        boundary
    };
    let offset = Duration::hours(2);
    let pool = row187_database(&url, preserving).await;
    let (playout, initial) = build(
        &pool,
        start - Duration::hours(1),
        start + Duration::hours(4),
    )
    .await;
    make_repositories(pool.clone())
        .playout
        .apply_build_result(&initial, &playout, offset)
        .await
        .unwrap();
    let before = row187_reference(&pool, clock).await;
    assert!(before
        .existing_items
        .iter()
        .any(|item| !item.watermark_ids.is_empty()));
    assert!(before
        .existing_items
        .iter()
        .any(|item| !item.graphics_element_ids.is_empty()));
    let consumed = before
        .existing_history
        .iter()
        .find(|entry| entry.when < start)
        .unwrap()
        .clone();
    // Simulate the normal removal of a played row, retaining its generated
    // checkpoint; this past orphan must still support continuation.
    sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId"=900 AND "Start"=?"#)
        .bind(consumed.when.format("%Y-%m-%d %H:%M:%S").to_string())
        .execute(&pool)
        .await
        .unwrap();
    if let Some(malformed) = contamination {
        row187_inject_history(&pool, &before, clock, boundary, malformed).await;
    }
    pool.execute(r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId", "PreferredAudioLanguageCode", "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "SubtitleMode") VALUES (1, 20, 'nld', 'Dutch', 'eng', 1)"#)
        .await
        .unwrap();
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let repos = make_repositories(pool.clone());
    let (current_playout, result) = row187_candidate(&pool, start, clock, mode, preservation).await;
    assert_eq!(current_playout.seed, playout.seed);
    if preserving {
        assert_eq!(result.retain_through, Some(boundary));
    }
    if contamination.is_some() {
        assert!(result.history_to_remove.contains(&100_001));
        assert!(result.history_to_remove.contains(&100_002) || result.clear_items);
    }
    assert!(!result.history_to_remove.contains(&consumed.id));
    // The production transaction must publish the repaired timeline and exact
    // history removals together, including removals before a retained seam.
    repos
        .playout
        .apply_build_result(&result, &current_playout, offset)
        .await
        .unwrap();
    drop(repos);
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let mut after = row187_reference(&pool, clock).await;
    row187_assert_retained(&pool, &before, &after, &consumed, boundary, playout.seed).await;
    // Separate databases allocate history identities independently after the
    // deliberate corruption. Compare every persisted history field otherwise.
    for entry in &mut after.existing_history {
        entry.id = 0;
    }
    pool.close().await;
    after
}

async fn row187_candidate(
    pool: &SqlitePool,
    start: chrono::DateTime<Utc>,
    clock: chrono::DateTime<Utc>,
    mode: PlayoutBuildMode,
    preservation: PlayoutPreservation,
) -> (Playout, SchedulingResult) {
    let mut reference_data = row187_reference(pool, clock).await;
    reference_data.preservation = preservation;
    let playout = make_repositories(pool.clone())
        .playout
        .get_with_anchor(900)
        .await
        .unwrap();
    let result = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start: clock,
            finish: start + Duration::hours(4),
            mode,
            reference_data,
        })
        .unwrap();
    (playout, result)
}

async fn row187_reference(
    pool: &SqlitePool,
    now: chrono::DateTime<Utc>,
) -> SchedulingReferenceData {
    make_repositories(pool.clone())
        .scheduling
        .block_reference_data(900, now, 1, Duration::hours(2))
        .await
        .unwrap()
}

async fn row187_database(url: &str, preserving: bool) -> SqlitePool {
    let pool = database(url, "Shuffle").await;
    pool.execute(r#"UPDATE "CollectionItem" SET "PreferredAudioLanguageCode"='nld', "PreferredAudioTitle"='Dutch', "PreferredSubtitleLanguageCode"='eng', "SubtitleMode"=1 WHERE "CollectionId"=1"#)
        .await.unwrap();
    pool.execute(r#"INSERT INTO "TemplateItem" ("Id", "TemplateId", "BlockId", "StartTime") VALUES (790, 800, 700, '07:00:00')"#)
        .await.unwrap();
    if preserving {
        pool.execute(r#"UPDATE "MediaVersion" SET "Duration"='01:00:00' WHERE "EpisodeId" IN (12, 13) OR "MovieId"=20"#)
            .await.unwrap();
    } else {
        pool.execute(r#"UPDATE "MediaVersion" SET "Duration"=CASE WHEN "EpisodeId"=12 THEN '00:00:15' WHEN "EpisodeId"=13 THEN '00:00:30' ELSE '00:00:45' END WHERE "EpisodeId" IN (12, 13) OR "MovieId"=20"#)
            .await.unwrap();
        pool.execute(r#"UPDATE "BlockItem" SET "IncludeInProgramGuide"=1 WHERE "Id"=701"#)
            .await
            .unwrap();
        for id in 20_001..=20_019 {
            sqlx::query(r#"INSERT INTO "BlockItem" ("Id", "Index", "BlockId", "CollectionType", "CollectionId", "PlaybackOrder", "IncludeInProgramGuide") VALUES (?, ?, 700, 'Collection', 1, 'Shuffle', 1)"#)
                .bind(id).bind(id - 20_000).execute(&pool).await.unwrap();
            sqlx::query(r#"INSERT INTO "BlockItemWatermark" ("BlockItemId", "WatermarkId") SELECT ?, "WatermarkId" FROM "BlockItemWatermark" WHERE "BlockItemId"=701"#)
                .bind(id).execute(&pool).await.unwrap();
            sqlx::query(r#"INSERT INTO "BlockItemGraphicsElement" ("BlockItemId", "GraphicsElementId") SELECT ?, "GraphicsElementId" FROM "BlockItemGraphicsElement" WHERE "BlockItemId"=701"#)
                .bind(id).execute(&pool).await.unwrap();
        }
    }
    pool
}

async fn row187_inject_history(
    pool: &SqlitePool,
    before: &SchedulingReferenceData,
    clock: chrono::DateTime<Utc>,
    boundary: chrono::DateTime<Utc>,
    malformed: bool,
) {
    let stale = before
        .existing_history
        .iter()
        .find(|entry| entry.when < clock)
        .unwrap();
    let orphan_when = clock + Duration::seconds(1);
    assert!(!before
        .existing_items
        .iter()
        .any(|item| item.start == orphan_when));
    row187_copy_history(
        pool,
        stale.id,
        100_001,
        orphan_when,
        orphan_when + Duration::seconds(30),
    )
    .await;
    let removed = before
        .existing_history
        .iter()
        .find(|entry| entry.when == boundary)
        .unwrap();
    row187_copy_history(
        pool,
        removed.id,
        100_002,
        removed.when,
        removed.finish.unwrap(),
    )
    .await;
    if malformed {
        pool.execute(r#"UPDATE "PlayoutHistory" SET "Details"=json_set("Details", '$.BlockCheckpoint.Version', 99) WHERE "Id"=100001"#)
            .await.unwrap();
    }
}

async fn row187_copy_history(
    pool: &SqlitePool,
    source_id: i64,
    id: i64,
    when: chrono::DateTime<Utc>,
    finish: chrono::DateTime<Utc>,
) {
    sqlx::query(r#"INSERT INTO "PlayoutHistory" ("Id", "PlayoutId", "BlockId", "PlaybackOrder", "Key", "ChildKey", "When", "Finish", "Index", "Seed", "Details", "IsCurrentChild") SELECT ?, "PlayoutId", "BlockId", "PlaybackOrder", "Key", "ChildKey", ?, ?, "Index", "Seed", "Details", "IsCurrentChild" FROM "PlayoutHistory" WHERE "Id"=?"#)
        .bind(id)
        .bind(when.format("%Y-%m-%d %H:%M:%S").to_string())
        .bind(finish.format("%Y-%m-%d %H:%M:%S").to_string())
        .bind(source_id)
        .execute(pool).await.unwrap();
}

async fn row187_assert_retained(
    pool: &SqlitePool,
    before: &SchedulingReferenceData,
    after: &SchedulingReferenceData,
    consumed: &ersatzrs_domain::PlayoutHistory,
    boundary: chrono::DateTime<Utc>,
    seed: i32,
) {
    assert!(!after
        .existing_history
        .iter()
        .any(|entry| [100_001, 100_002].contains(&entry.id)));
    assert!(after.existing_history.contains(consumed));
    assert!(after.existing_items.iter().all(|item| {
        item.preferred_audio_language_code.as_deref() == Some("nld")
            && item.preferred_audio_title.as_deref() == Some("Dutch")
            && item.preferred_subtitle_language_code.as_deref() == Some("eng")
            && item.subtitle_mode == Some(ChannelSubtitleMode::Forced)
    }));
    let retained = before
        .existing_items
        .iter()
        .filter(|item| item.start < boundary && item.start != consumed.when)
        .collect::<Vec<_>>();
    assert_eq!(
        after
            .existing_items
            .iter()
            .filter(|item| item.start < boundary)
            .collect::<Vec<_>>(),
        retained
    );
    assert_eq!(
        after
            .existing_history
            .iter()
            .filter(|entry| entry.when < boundary)
            .collect::<Vec<_>>(),
        before
            .existing_history
            .iter()
            .filter(|entry| entry.when < boundary)
            .collect::<Vec<_>>()
    );
    assert_eq!(
        make_repositories(pool.clone())
            .playout
            .get_with_anchor(900)
            .await
            .unwrap()
            .seed,
        seed
    );
    assert_eq!(
        block_health(pool).await,
        ersatzrs_core::health::HealthCheckStatus::Pass
    );
}
