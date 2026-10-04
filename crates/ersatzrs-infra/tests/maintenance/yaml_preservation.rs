use super::*;
use ersatzrs_core::scheduling::{PlayoutPreservation, SchedulingReferenceData};

const SERIES: &str = "content:\n  - key: main\n    collection: Refresh Collection\n    order: chronological\nplayout:\n  - count: 3\n    content: main\n  - count: 1\n    content: main\n  - repeat: true\n";

#[tokio::test]
async fn yaml_preserving_refresh_restores_persisted_mid_instruction_checkpoint() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!(
        "sqlite://{}",
        directory.path().join("yaml-refresh.db").display()
    );
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 23, 48, 0).unwrap();
    let now = start + Duration::minutes(1);
    let pool = initial_window(&url, start).await;
    let before = reference(&pool).await;
    let current = before.existing_items[0].clone();
    assert!(!before.existing_yaml_checkpoints.is_empty());
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let request = PlayoutBuildRequest::new(88, PlayoutBuildMode::Refresh)
        .with_preservation(PlayoutPreservation::KeepCurrent);
    assert_eq!(
        yaml_health(&pool).await,
        ersatzrs_core::health::HealthCheckStatus::Pass
    );
    for _ in 0..2 {
        run_requested_playout_build(&pool, &EntityLocker::new(), request, now, None)
            .await
            .unwrap();
        let after = reference(&pool).await;
        assert_eq!(after.existing_items[0], current);
        for (actual, expected) in after.existing_items.iter().zip(&before.existing_items) {
            let mut actual = actual.clone();
            actual.id = expected.id;
            assert_eq!(
                &actual, expected,
                "restart/Refresh must resume the remaining Count iterations"
            );
        }
    }
    let stable = persisted_playout_item_rows(&pool, 88).await;
    sqlx::query(r#"DELETE FROM "PlayoutYamlCheckpoint" WHERE "PlayoutId" = 88 AND "CheckpointDate" = (SELECT "Finish" FROM "PlayoutItem" WHERE "Id" = ?)"#)
        .bind(current.id).execute(&pool).await.unwrap();
    let error = run_requested_playout_build(&pool, &EntityLocker::new(), request, now, None)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("no checkpoint"));
    assert_eq!(persisted_playout_item_rows(&pool, 88).await, stable);
    assert_eq!(
        yaml_health(&pool).await,
        ersatzrs_core::health::HealthCheckStatus::Fail
    );
}

async fn yaml_health(pool: &SqlitePool) -> ersatzrs_core::health::HealthCheckStatus {
    ersatzrs_infra::health::make_health_check_service(
        pool.clone(),
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        DatabaseSchemaStatus::default(),
    )
    .perform_health_checks()
    .await
    .into_iter()
    .find(|result| result.title == "YAML Playout State")
    .unwrap()
    .status
}

#[tokio::test]
async fn yaml_preserving_refresh_upgrade_retains_legacy_rows_until_explicit_reset() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    let migrations = sqlx::migrate!("../../migrations");
    sqlx::migrate::Migrator::with_migrations(
        migrations
            .iter()
            .filter(|migration| migration.version < 202_610_041_233)
            .cloned()
            .collect(),
    )
    .run(&pool)
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind" = 3, "ProgramScheduleId" = NULL, "ScheduleFile" = ? WHERE "Id" = 88"#)
        .bind(SERIES).execute(&pool).await.unwrap();
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 23, 58, 0).unwrap();
    repos
        .playout
        .upsert_item(&ersatzrs_domain::PlayoutItem {
            playout_id: 88,
            media_item_id: 100,
            start,
            finish: start + Duration::minutes(10),
            out_point: Duration::minutes(10),
            ..ersatzrs_domain::PlayoutItem::default()
        })
        .await
        .unwrap();
    let legacy_rows = persisted_playout_item_rows(&pool, 88).await;
    migrations.run(&pool).await.unwrap();
    assert_eq!(persisted_playout_item_rows(&pool, 88).await, legacy_rows);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "PlayoutYamlCheckpoint""#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let now = start + Duration::minutes(1);
    let preserving = |mode| {
        PlayoutBuildRequest::new(88, mode).with_preservation(PlayoutPreservation::KeepCurrent)
    };
    let error = run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        preserving(PlayoutBuildMode::Refresh),
        now,
        None,
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("no checkpoint"));
    assert_eq!(persisted_playout_item_rows(&pool, 88).await, legacy_rows);
    run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        preserving(PlayoutBuildMode::Reset),
        now,
        None,
    )
    .await
    .unwrap();
    assert_eq!(
        persisted_playout_item_rows(&pool, 88).await[0],
        legacy_rows[0]
    );
    assert!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "PlayoutYamlCheckpoint""#)
            .fetch_one(&pool)
            .await
            .unwrap()
            > 0
    );
    pool.execute(r#"DELETE FROM "Playout" WHERE "Id" = 88"#)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "PlayoutYamlCheckpoint""#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

async fn reference(pool: &SqlitePool) -> SchedulingReferenceData {
    make_repositories(pool.clone())
        .scheduling
        .yaml_reference_data(88, SERIES, Duration::zero())
        .await
        .unwrap()
}

async fn assert_refresh_keeps_reset_rotation(
    pool: &SqlitePool,
    now: chrono::DateTime<Utc>,
    before: &SchedulingReferenceData,
) {
    run_requested_playout_build(
        pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(88, PlayoutBuildMode::Refresh)
            .with_preservation(PlayoutPreservation::KeepCurrent),
        now,
        None,
    )
    .await
    .unwrap();
    let after = reference(pool).await;
    assert_eq!(after.existing_items.len(), before.existing_items.len());
    for (actual, expected) in after.existing_items.iter().zip(&before.existing_items) {
        if expected.start <= now {
            assert_eq!(actual, expected);
        } else {
            let mut actual = actual.clone();
            actual.id = expected.id;
            assert_eq!(&actual, expected);
        }
    }
}

async fn initial_window(url: &str, start: chrono::DateTime<Utc>) -> SqlitePool {
    let pool = ersatzrs_infra::db::init(url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#).await.unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;
    common::seed_media_item(&pool, 101, 1).await;
    pool.execute(r#"
        INSERT INTO "Movie" ("Id") VALUES (101);
        INSERT INTO "MovieMetadata" ("Id", "MovieId", "MetadataKind", "Title", "SortTitle", "DateAdded", "DateUpdated")
        SELECT 101, 101, "MetadataKind", 'Second Fixture', 'Second Fixture', "DateAdded", "DateUpdated"
        FROM "MovieMetadata" WHERE "Id" = 100;
        INSERT INTO "MediaVersion" ("Id", "MovieId", "Name", "Duration", "VideoScanKind", "DateAdded", "DateUpdated", "Width", "Height")
        SELECT 1001, 101, "Name", "Duration", "VideoScanKind", "DateAdded", "DateUpdated", "Width", "Height"
        FROM "MediaVersion" WHERE "Id" = 1000;
        INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (1, 101);
    "#).await.unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleKind" = 3, "ProgramScheduleId" = NULL, "ScheduleFile" = ? WHERE "Id" = 88"#)
        .bind(SERIES).execute(&pool).await.unwrap();
    run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(88, PlayoutBuildMode::Reset),
        start,
        None,
    )
    .await
    .unwrap();
    pool
}

#[tokio::test]
async fn yaml_preserving_reset_survives_sqlite_restart_and_refuses_overlapping_rewind() {
    let directory = tempfile::tempdir().unwrap();
    let url = format!("sqlite://{}", directory.path().join("yaml.db").display());
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 23, 48, 0).unwrap();
    let now = start + Duration::minutes(13);
    let pool = initial_window(&url, start).await;
    let before = reference(&pool).await;
    let current = before
        .existing_items
        .iter()
        .find(|item| item.start <= now && item.finish > now)
        .unwrap()
        .clone();
    assert_eq!(current.media_item_id, 101);
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    let request = PlayoutBuildRequest::new(88, PlayoutBuildMode::Reset)
        .with_preservation(PlayoutPreservation::KeepCurrent);
    run_requested_playout_build(&pool, &EntityLocker::new(), request, now, None)
        .await
        .unwrap();
    let after = reference(&pool).await;
    assert_eq!(
        after
            .existing_items
            .iter()
            .find(|item| item.id == current.id),
        Some(&current)
    );
    let next = after
        .existing_items
        .iter()
        .find(|item| item.start == current.finish)
        .unwrap();
    assert_eq!(
        next.media_item_id, 100,
        "explicit Reset starts fresh rotation at the retained boundary"
    );
    let history = |data: &SchedulingReferenceData| {
        data.existing_history
            .iter()
            .filter(|entry| entry.when < current.finish)
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(history(&before), history(&after));
    let anchor = make_repositories(pool.clone())
        .playout
        .get_with_anchor(88)
        .await
        .unwrap()
        .anchor
        .unwrap();
    assert!(anchor.context.as_ref().unwrap().contains("history_start"));
    pool.close().await;

    let pool = ersatzrs_infra::db::init(&url).await.unwrap();
    assert_refresh_keeps_reset_rotation(&pool, now, &after).await;
    run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(88, PlayoutBuildMode::Continue),
        anchor.next_start - Duration::hours(1),
        None,
    )
    .await
    .unwrap();
    let continued = reference(&pool).await;
    assert!(continued
        .existing_items
        .windows(2)
        .all(|pair| pair[0].finish == pair[1].start));
    let extension = continued
        .existing_items
        .iter()
        .find(|item| item.start == anchor.next_start)
        .unwrap();
    let tail = after.existing_items.last().unwrap();
    assert_ne!(
        extension.media_item_id, tail.media_item_id,
        "Continue resumes the new rotation"
    );
    let stable = persisted_playout_item_rows(&pool, 88).await;
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleFile" = ? WHERE "Id" = 88"#)
        .bind(format!("reset:\n  - rewind: '00:05:00'\n{SERIES}"))
        .execute(&pool)
        .await
        .unwrap();
    let error = run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        request,
        extension.start + Duration::minutes(1),
        None,
    )
    .await
    .unwrap_err();
    assert!(error
        .to_string()
        .contains("rewound into the retained programme"));
    assert_eq!(persisted_playout_item_rows(&pool, 88).await, stable);
}
