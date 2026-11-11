//! Seeded filler draws use the real builder and stored complete checkpoints.
use super::*;
use ersatzrs_core::{
    playout_builder::PlayoutBuilder,
    repositories::PlayoutRepository,
    scheduling::{PlayoutBuildMode, SchedulingReferenceData, SchedulingResult},
};
use ersatzrs_domain::{FillerKind, FillerMode, FillerPreset};

async fn database(url: &str) -> SqlitePool {
    let pool = ersatzrs_infra::db::init(url).await.unwrap();
    pool.execute(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'test', 1920, 1080)"#).await.unwrap();
    seed_scheduling_fixture(&pool).await;
    pool.execute(
        r#"UPDATE "MediaVersion" SET "Duration"='00:01:00' WHERE "Id" IN (120,130,200);
        UPDATE "MediaVersion" SET "Duration"='00:00:01' WHERE "Id"=210;"#,
    )
    .await
    .unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 901).await;
    repos
        .filler
        .upsert(&FillerPreset {
            id: 91,
            name: "Seeded filler".into(),
            filler_kind: FillerKind::PreRoll,
            filler_mode: FillerMode::RandomCount,
            count: Some(3),
            collection_type: CollectionType::Movie,
            media_item_id: Some(21),
            ..FillerPreset::default()
        })
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert(&ProgramSchedule {
            id: 301,
            name: "Random Count".into(),
            ..ProgramSchedule::default()
        })
        .await
        .unwrap();
    repos
        .program_schedule
        .upsert_item(&ProgramScheduleItem {
            id: 401,
            program_schedule_id: 301,
            pre_roll_filler_id: Some(91),
            collection_type: CollectionType::Collection,
            collection_id: Some(1),
            playback_order: PlaybackOrder::Chronological,
            playout_mode: PlayoutMode::One,
            preferred_audio_language_code: Some("nld".into()),
            preferred_subtitle_language_code: Some("eng".into()),
            ..ProgramScheduleItem::default()
        })
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Playout" ("Id", "ChannelId", "ProgramScheduleId", "ScheduleKind", "Seed") VALUES (901,901,301,'Classic',12345)"#).await.unwrap();
    pool
}

async fn reference(pool: &SqlitePool, start: chrono::DateTime<Utc>) -> SchedulingReferenceData {
    make_repositories(pool.clone())
        .scheduling
        .classic_reference_data(901, start, Duration::zero())
        .await
        .unwrap()
}
async fn build(
    pool: &SqlitePool,
    start: chrono::DateTime<Utc>,
    finish: chrono::DateTime<Utc>,
    mode: PlayoutBuildMode,
) -> (Playout, SchedulingResult) {
    let repos = make_repositories(pool.clone());
    let playout = repos.playout.get_with_anchor(901).await.unwrap();
    let result = PlayoutBuilder::default()
        .build(SchedulingRequest {
            playout: playout.clone(),
            start,
            finish,
            mode,
            reference_data: reference(pool, start).await,
        })
        .unwrap();
    (playout, result)
}

#[tokio::test]
async fn random_count_sqlite_restart_continue_and_refresh_match_uninterrupted() {
    for mode in [PlayoutBuildMode::Continue, PlayoutBuildMode::Refresh] {
        let directory = tempfile::tempdir().unwrap();
        let url = format!(
            "sqlite://{}",
            directory.path().join("random-count.db").display()
        );
        let pool = database(&url).await;
        let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 58, 0).unwrap();
        let seam = start + Duration::minutes(2);
        let finish = start + Duration::minutes(20);
        let (_, full) = build(&pool, start, finish, PlayoutBuildMode::Continue).await;
        let (playout, first) = build(&pool, start, seam, PlayoutBuildMode::Continue).await;
        make_repositories(pool.clone())
            .playout
            .apply_build_result(&first, &playout, Duration::zero())
            .await
            .unwrap();
        pool.close().await;
        let pool = ersatzrs_infra::db::init(&url).await.unwrap();
        let (playout, resumed) = build(&pool, seam, finish, mode).await;
        assert_eq!(
            resumed.added_items,
            full.added_items[first.added_items.len()..],
            "{mode:?}"
        );
        assert_eq!(resumed.anchor, full.anchor);
        make_repositories(pool.clone())
            .playout
            .apply_build_result(&resumed, &playout, Duration::zero())
            .await
            .unwrap();
        let stored = reference(&pool, seam).await;
        assert!(ersatzrs_core::scheduling::audit_classic_runtime_state(&stored).is_complete());
        assert!(
            ersatzrs_core::scheduling::audit_classic_refresh_checkpoint(&stored, 901, seam)
                .is_complete()
        );
        assert!(stored
            .existing_items
            .iter()
            .filter(|i| i.media_item_id != 21)
            .all(
                |i| i.preferred_audio_language_code.as_deref() == Some("nld")
                    && i.preferred_subtitle_language_code.as_deref() == Some("eng")
            ));
    }
}

#[tokio::test]
async fn random_count_legacy_missing_state_refuses_maintenance_continue_without_sql_clear() {
    use ersatzrs_core::{
        locking::EntityLocker,
        scheduling::{PlayoutBuildRequest, PlayoutPreservation},
    };
    let pool = database("sqlite::memory:").await;
    let start = Utc.with_ymd_and_hms(2026, 11, 11, 23, 58, 0).unwrap();
    let (playout, first) = build(
        &pool,
        start,
        start + Duration::minutes(10),
        PlayoutBuildMode::Continue,
    )
    .await;
    make_repositories(pool.clone())
        .playout
        .apply_build_result(&first, &playout, Duration::zero())
        .await
        .unwrap();
    pool.execute(r#"UPDATE "PlayoutAnchor" SET "Context"=json_remove("Context",'$.random_count_state') WHERE "PlayoutId"=901"#).await.unwrap();
    let before = reference(&pool, start).await;
    let error = ersatzrs_infra::maintenance::run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(901, PlayoutBuildMode::Continue)
            .with_preservation(PlayoutPreservation::KeepCurrent),
        start + Duration::seconds(1),
        None,
    )
    .await
    .unwrap_err();
    assert!(
        error.to_string().contains("Classic Continue refused"),
        "{error}"
    );
    let after = reference(&pool, start).await;
    assert_eq!(after.existing_items, before.existing_items);
    assert_eq!(after.existing_history, before.existing_history);
    assert_eq!(
        after.existing_program_schedule_anchors,
        before.existing_program_schedule_anchors
    );
    assert_eq!(
        after.existing_classic_checkpoints,
        before.existing_classic_checkpoints
    );
}

#[tokio::test]
async fn random_count_health_reports_complete_then_missing_draw_state() {
    use ersatzrs_core::health::HealthCheckStatus;
    let pool = database("sqlite::memory:").await;
    let start = Utc::now()
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let (playout, result) = build(
        &pool,
        start,
        start + Duration::minutes(20),
        PlayoutBuildMode::Continue,
    )
    .await;
    make_repositories(pool.clone())
        .playout
        .apply_build_result(&result, &playout, Duration::zero())
        .await
        .unwrap();
    let health = ersatzrs_infra::health::make_health_check_service(
        pool.clone(),
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        ersatzrs_infra::db::DatabaseSchemaStatus::default(),
    );
    let status = || async {
        health
            .perform_health_checks()
            .await
            .into_iter()
            .find(|result| result.title == "Classic Playout State")
            .unwrap()
            .status
    };
    assert_eq!(status().await, HealthCheckStatus::Pass);
    pool.execute(r#"UPDATE "PlayoutAnchor" SET "Context"=json_remove("Context",'$.random_count_state') WHERE "PlayoutId"=901"#).await.unwrap();
    assert_eq!(status().await, HealthCheckStatus::Fail);
}
