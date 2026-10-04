//! Deferred schedule saves through the repository, durable state and natural maintenance.

use super::*;

async fn setup(pool: &SqlitePool) -> DateTime<Utc> {
    seed_acceptance_fixture(pool).await;
    seed_continuity_filler_chapters(pool).await;
    pool.execute(
        r#"UPDATE "ProgramScheduleItem" SET "PreRollFillerId"=NULL,
        "PostRollFillerId"=NULL, "FallbackFillerId"=NULL WHERE "Id"=401;
        UPDATE "FillerPreset" SET "FillerKind"='PreRoll' WHERE "Id"=77;
        INSERT INTO "PlaylistItem" ("Id", "Index", "PlaylistId", "CollectionType", "MediaItemId",
            "IncludeInProgramGuide", "PlaybackOrder", "PlayAll")
        VALUES (102, 1, 11, 'Movie', 102, 1, 'Chronological', 1);"#,
    )
    .await
    .unwrap();
    let start = Utc::now().with_nanosecond(0).unwrap() - Duration::minutes(10);
    build_filler_window(
        pool,
        PlayoutBuildMode::Reset,
        start,
        start + Duration::minutes(45),
        true,
    )
    .await;
    start
}

async fn edited_items(pool: &SqlitePool) -> Vec<ersatzrs_core::repositories::ScheduleItemAdmin> {
    let mut items = make_repositories(pool.clone())
        .program_schedule
        .list_items_admin(301)
        .await
        .unwrap();
    assert_eq!(items[0].id, 401);
    items[0].pre_roll_filler_id = Some(77);
    items[0].custom_title = "Deferred programme".into();
    items[0].preferred_audio_language_code = Some("nld".into());
    items[0].preferred_subtitle_language_code = Some("eng".into());
    items
}

#[tokio::test]
async fn deferred_save_preserves_state_and_natural_continue_consumes_it_after_restart() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("deferred.sqlite");
    let pool = open_database(&path, true).await;
    let start = setup(&pool).await;
    let repos = make_repositories(pool.clone());
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let boundary = playout.anchor.unwrap().next_start;
    let prefix = repos
        .playout
        .items_in_range(RELATED_PLAYOUT_ID, start, boundary)
        .await
        .unwrap();
    let before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    let unrelated = persisted_classic_rows(&pool, UNRELATED_PLAYOUT_ID).await;
    let oracle = build_filler_window(
        &pool,
        PlayoutBuildMode::Continue,
        start,
        start + Duration::hours(6),
        false,
    )
    .await;
    let expected_next_media = oracle.added_items[0].media_item_id;
    let edit = edited_items(&pool).await;
    let saved = repos
        .program_schedule
        .defer_items_admin(301, &edit, &[RELATED_PLAYOUT_ID])
        .await
        .unwrap();
    assert_eq!(saved.len(), 1);
    assert_eq!(saved[0].effective_after, Some(boundary));
    assert_eq!(saved[0].applied_from, None);
    assert!(before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await);
    assert!(unrelated == persisted_classic_rows(&pool, UNRELATED_PLAYOUT_ID).await);
    assert_eq!(
        repos.program_schedule.list_items_admin(301).await.unwrap()[0].id,
        401
    );
    assert_eq!(
        repos
            .program_schedule
            .defer_items_admin(301, &edit, &[RELATED_PLAYOUT_ID])
            .await
            .unwrap(),
        saved,
        "identical save must not replace the receipt or create more work"
    );
    pool.close().await;
    run_natural_maintenance_in_child(&path).await;
    let reopened = open_database(&path, false).await;
    let repos = make_repositories(reopened.clone());
    assert_eq!(
        repos
            .playout
            .items_in_range(RELATED_PLAYOUT_ID, start, boundary)
            .await
            .unwrap(),
        prefix
    );
    let appended = repos
        .playout
        .items_in_range(RELATED_PLAYOUT_ID, boundary, boundary + Duration::hours(1))
        .await
        .unwrap();
    assert_eq!(appended[0].start, boundary);
    assert_eq!(appended[0].chapter_title.as_deref(), Some("Opening"));
    assert_eq!(
        appended[0].filler_kind,
        ersatzrs_domain::FillerKind::PreRoll
    );
    let primary = appended
        .iter()
        .find(|item| item.filler_kind == ersatzrs_domain::FillerKind::None)
        .unwrap();
    assert_eq!(
        primary.media_item_id, expected_next_media,
        "continuation must retain series rotation"
    );
    assert_eq!(primary.custom_title.as_deref(), Some("Deferred programme"));
    assert_eq!(
        primary.preferred_audio_language_code.as_deref(),
        Some("nld")
    );
    assert_eq!(
        primary.preferred_subtitle_language_code.as_deref(),
        Some("eng")
    );
    let applied = repos
        .program_schedule
        .deferred_changes_admin(301)
        .await
        .unwrap();
    assert_eq!(applied[0].saved_at, saved[0].saved_at);
    assert_eq!(applied[0].applied_from, Some(boundary));
    assert_eq!(applied[0].build_succeeded, Some(true));
    reopened.close().await;
}

async fn run_natural_maintenance_in_child(path: &Path) {
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(45),
        tokio::process::Command::new(std::env::current_exe().unwrap())
            .kill_on_drop(true)
            .args([
                "--exact",
                "deferred_save::continue_deferred_in_fresh_process",
                "--ignored",
                "--nocapture",
            ])
            .env("ERSATZRS_DEFERRED_TEST_DB", path)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

#[tokio::test]
#[ignore = "invoked by deferred-save acceptance in a fresh process"]
#[allow(clippy::disallowed_methods)] // Parent passes only the disposable fixture database.
async fn continue_deferred_in_fresh_process() {
    let path = std::env::var_os("ERSATZRS_DEFERRED_TEST_DB").unwrap();
    let pool = open_database(Path::new(&path), false).await;
    ersatzrs_infra::maintenance::run_maintenance_once(
        pool.clone(),
        &ersatzrs_infra::cache_cleanup::CacheCleanupConfig::default(),
        None,
        &EntityLocker::new(),
        ersatzrs_infra::db::DatabaseSchemaStatus::default(),
        None,
    )
    .await
    .unwrap();
    pool.close().await;
}

#[tokio::test]
async fn deferred_save_rejects_incompatible_edits_and_inflight_old_candidates() {
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("fences.sqlite"), true).await;
    let start = setup(&pool).await;
    let repos = make_repositories(pool.clone());
    let before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    let old_items = repos.program_schedule.list_items_admin(301).await.unwrap();
    let mut edit = edited_items(&pool).await;
    edit[0].playlist_id = Some(12);
    assert!(repos
        .program_schedule
        .defer_items_admin(301, &edit, &[RELATED_PLAYOUT_ID])
        .await
        .is_err());
    assert_eq!(
        repos.program_schedule.list_items_admin(301).await.unwrap(),
        old_items
    );
    assert!(before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await);
    let guard = scan_refresh::publication_guard(&pool, RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let mut old_candidate = build_filler_window(
        &pool,
        PlayoutBuildMode::Continue,
        start,
        start + Duration::hours(6),
        false,
    )
    .await;
    old_candidate.publication_guard = Some(guard);
    repos
        .program_schedule
        .defer_items_admin(301, &edited_items(&pool).await, &[RELATED_PLAYOUT_ID])
        .await
        .unwrap();
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    assert!(repos
        .playout
        .apply_build_result(&old_candidate, &playout, Duration::zero())
        .await
        .is_err());
    assert!(before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await);
    assert!(repos
        .program_schedule
        .deferred_changes_admin(301)
        .await
        .unwrap()[0]
        .applied_from
        .is_none());
}

#[tokio::test]
async fn deferred_save_rolls_back_settings_when_receipt_publication_fails() {
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("rollback.sqlite"), true).await;
    setup(&pool).await;
    let repos = make_repositories(pool.clone());
    let old_items = repos.program_schedule.list_items_admin(301).await.unwrap();
    let before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    pool.execute(
        r#"CREATE TRIGGER reject_deferred_receipt BEFORE INSERT ON "ConfigElement"
        WHEN NEW."Key" LIKE 'playout.classic.deferred.%'
        BEGIN SELECT RAISE(ABORT, 'synthetic receipt failure'); END;"#,
    )
    .await
    .unwrap();
    assert!(repos
        .program_schedule
        .defer_items_admin(301, &edited_items(&pool).await, &[RELATED_PLAYOUT_ID])
        .await
        .is_err());
    assert_eq!(
        repos.program_schedule.list_items_admin(301).await.unwrap(),
        old_items
    );
    assert!(before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await);
    assert!(repos
        .program_schedule
        .deferred_changes_admin(301)
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test]
async fn deferred_save_includes_alternates_once_and_rechecks_locked_targets() {
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("targets.sqlite"), true).await;
    setup(&pool).await;
    pool.execute(
        r#"INSERT INTO "ProgramScheduleAlternate" ("PlayoutId", "ProgramScheduleId", "Index")
        VALUES (901, 301, 0), (902, 301, 0), (902, 301, 1);"#,
    )
    .await
    .unwrap();
    let repos = make_repositories(pool.clone());
    let ids = repos
        .program_schedule
        .playout_ids_using_admin(301)
        .await
        .unwrap();
    assert_eq!(ids, [RELATED_PLAYOUT_ID, UNRELATED_PLAYOUT_ID]);
    let edit = edited_items(&pool).await;
    let old_items = repos.program_schedule.list_items_admin(301).await.unwrap();
    assert!(repos
        .program_schedule
        .defer_items_admin(301, &edit, &[RELATED_PLAYOUT_ID])
        .await
        .is_err());
    assert_eq!(
        repos.program_schedule.list_items_admin(301).await.unwrap(),
        old_items
    );
    let saved = repos
        .program_schedule
        .defer_items_admin(301, &edit, &ids)
        .await
        .unwrap();
    assert_eq!(saved.len(), 2);
    assert!(saved[0].effective_after.is_some());
    assert!(saved[1].effective_after.is_none());
    repos
        .playout
        .delete_admin(UNRELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let receipt_count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ConfigElement" WHERE "Key"='playout.classic.deferred.902'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipt_count, 0);
    repos
        .program_schedule
        .replace_items_admin(301, &edit)
        .await
        .unwrap();
    assert!(
        repos
            .program_schedule
            .deferred_changes_admin(301)
            .await
            .unwrap()
            .is_empty(),
        "an immediate save supersedes the earlier deferred edit"
    );
    pool.execute(r#"INSERT INTO "ProgramScheduleAlternate" ("PlayoutId", "ProgramScheduleId", "Index") VALUES (901, 302, 1)"#).await.unwrap();
    let mut alternate = repos.program_schedule.list_items_admin(302).await.unwrap();
    alternate[0].custom_title = "Deferred alternate".into();
    repos
        .program_schedule
        .defer_items_admin(302, &alternate, &[901])
        .await
        .unwrap();
    assert_eq!(
        repos
            .program_schedule
            .deferred_changes_admin(302)
            .await
            .unwrap()
            .len(),
        1
    );
    repos.program_schedule.delete_admin(301).await.unwrap();
    let count: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ConfigElement" WHERE "Key"='playout.classic.deferred.901'"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        count, 0,
        "primary-schedule cascade must remove alternate receipts too"
    );
}

#[tokio::test]
async fn corrupt_deferred_receipt_is_visible_in_health_and_full_reset_recovers() {
    use ersatzrs_core::health::HealthCheckStatus;
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("receipt-health.sqlite"), true).await;
    let start = setup(&pool).await;
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
    for value in [
        Some("broken"),
        None,
        Some(r#"{"version":2,"schedules":{}}"#),
    ] {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('playout.classic.deferred.901', ?)"#)
            .bind(value).execute(&pool).await.unwrap();
        assert_eq!(status().await, HealthCheckStatus::Fail);
        build_filler_window(
            &pool,
            PlayoutBuildMode::Reset,
            start,
            start + Duration::hours(2),
            true,
        )
        .await;
        assert_eq!(status().await, HealthCheckStatus::Pass);
    }
}

#[tokio::test]
async fn deferred_application_receipt_and_timeline_roll_back_together() {
    let directory = tempfile::tempdir().unwrap();
    let pool = open_database(&directory.path().join("apply-rollback.sqlite"), true).await;
    let start = setup(&pool).await;
    let repos = make_repositories(pool.clone());
    repos
        .program_schedule
        .defer_items_admin(301, &edited_items(&pool).await, &[RELATED_PLAYOUT_ID])
        .await
        .unwrap();
    let before = persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await;
    let pending = repos
        .program_schedule
        .deferred_changes_admin(301)
        .await
        .unwrap();
    let guard = scan_refresh::publication_guard(&pool, RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    let mut candidate = build_filler_window(
        &pool,
        PlayoutBuildMode::Continue,
        start,
        start + Duration::hours(6),
        false,
    )
    .await;
    candidate.publication_guard = Some(guard);
    let playout = repos
        .playout
        .get_with_anchor(RELATED_PLAYOUT_ID)
        .await
        .unwrap();
    pool.execute(
        r#"CREATE TRIGGER reject_deferred_application BEFORE UPDATE ON "ConfigElement"
        WHEN NEW."Key"='playout.classic.deferred.901'
        BEGIN SELECT RAISE(ABORT, 'synthetic application receipt failure'); END;"#,
    )
    .await
    .unwrap();
    let error = repos
        .playout
        .apply_build_result(&candidate, &playout, Duration::zero())
        .await
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("synthetic application receipt failure"),
        "{error}"
    );
    assert!(before == persisted_classic_rows(&pool, RELATED_PLAYOUT_ID).await);
    assert_eq!(
        repos
            .program_schedule
            .deferred_changes_admin(301)
            .await
            .unwrap(),
        pending
    );
    pool.execute("DROP TRIGGER reject_deferred_application")
        .await
        .unwrap();
    repos
        .playout
        .apply_build_result(&candidate, &playout, Duration::zero())
        .await
        .unwrap();
    assert!(repos
        .program_schedule
        .deferred_changes_admin(301)
        .await
        .unwrap()[0]
        .applied_from
        .is_some());
}
