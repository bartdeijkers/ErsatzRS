use super::*;
use ersatzrs_core::{
    health::HealthCheckStatus, locking::EntityLocker, repositories::PlayoutRepository,
    scheduling::PlayoutBuildRequest,
};

async fn state(pool: &DbPool) -> ersatzrs_core::health::HealthCheckResult {
    ersatzrs_infra::health::make_health_check_service(
        pool.clone(),
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        ersatzrs_infra::db::DatabaseSchemaStatus::default(),
    )
    .perform_health_checks()
    .await
    .into_iter()
    .find(|result| result.title == "External JSON Playout State")
    .unwrap()
}

async fn replace(pool: &DbPool) {
    ersatzrs_infra::maintenance::run_requested_playout_build(
        pool,
        &EntityLocker::new(),
        PlayoutBuildRequest::new(1, PlayoutBuildMode::Reset)
            .with_preservation(PlayoutPreservation::ReplaceCurrent),
        chrono::Utc::now(),
        None,
    )
    .await
    .unwrap();
}

#[tokio::test]
async fn external_json_health_and_list_follow_adoption_and_explicit_corruption_recovery() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("health.json");
    seed(&pool, &source).await;
    let repos = make_repositories(pool.clone());
    let mut content: serde_json::Value =
        serde_json::from_slice(&document(&["A", "B", "C"])).unwrap();
    content["startTime"] = (chrono::Utc::now() + Duration::hours(1))
        .to_rfc3339()
        .into();
    let bytes = serde_json::to_vec(&content).unwrap();
    std::fs::write(&source, &bytes).unwrap();
    assert_eq!(state(&pool).await.status, HealthCheckStatus::Info);
    assert_eq!(
        repos.playout.list_admin("", 0, 10).await.unwrap()[0].source_revision,
        None
    );
    replace(&pool).await;
    assert_eq!(state(&pool).await.status, HealthCheckStatus::Pass);
    assert_eq!(
        repos.playout.list_admin("", 0, 10).await.unwrap()[0].source_revision,
        Some(1)
    );
    pool.execute(r#"UPDATE "PlayoutExternalJsonSnapshot" SET "SchemaVersion"=999, "Document"='{}' WHERE "PlayoutId"=1"#).await.unwrap();
    let invalid = state(&pool).await;
    assert_eq!(invalid.status, HealthCheckStatus::Fail);
    assert!(invalid.message.contains("invalid or unsupported"));
    assert!(!invalid.message.contains(source.to_str().unwrap()));
    let request = PlayoutBuildRequest::new(1, PlayoutBuildMode::Refresh)
        .with_preservation(PlayoutPreservation::KeepCurrent);
    assert!(ersatzrs_infra::maintenance::run_requested_playout_build(
        &pool,
        &EntityLocker::new(),
        request,
        chrono::Utc::now(),
        None
    )
    .await
    .is_err());
    assert!(snapshot::accepted(&pool, 1).await.is_err());
    replace(&pool).await;
    assert_eq!(state(&pool).await.status, HealthCheckStatus::Pass);
    let rows = repos.playout.list_admin("", 0, 10).await.unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].source_revision, Some(2));
    assert_eq!(std::fs::read(&source).unwrap(), bytes);
}

#[tokio::test]
async fn external_json_health_distinguishes_source_change_expiry_and_unsupported_mirrors() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("health.json");
    seed(&pool, &source).await;
    std::fs::write(&source, document(&["A"])).unwrap();
    replace(&pool).await;
    // This fixture's fixed programme time is deliberately in the past.
    let expired = state(&pool).await;
    assert_eq!(expired.status, HealthCheckStatus::Warning);
    assert!(expired.message.contains("no future programmes"));
    pool.execute(r#"UPDATE "Playout" SET "ScheduleFile"='replacement.json' WHERE "Id"=1"#)
        .await
        .unwrap();
    let changed = state(&pool).await;
    assert_eq!(changed.status, HealthCheckStatus::Warning);
    assert!(changed.message.contains("source path differs"));
    assert!(!changed.message.contains("replacement.json"));
    pool.execute(r#"UPDATE "Channel" SET "StreamingEngine"=1 WHERE "Id"=2"#)
        .await
        .unwrap();
    let incompatible = state(&pool).await;
    assert_eq!(incompatible.status, HealthCheckStatus::Fail);
    assert!(incompatible.message.contains("Next does not consume"));
}
