//! Cancel after the real Next file changes, verify rollback, then let the parent
//! test retry and decode the uninterrupted old-to-new programme transition.

use super::*;
use std::sync::atomic::{AtomicBool, Ordering};

pub(super) async fn cancel_once(
    pool: &SqlitePool,
    locker: &EntityLocker,
    registry: &InMemoryHlsSessionRegistry,
    host: &Arc<ersatzrs_infra::next_engine::host::NextHost>,
    folder: &Path,
) {
    let manifest = folder
        .parent()
        .unwrap()
        .join("playout")
        .join("1")
        .join(ersatzrs_infra::next_engine::publication::PLAYOUT_FILENAME);
    let previous = Arc::new(std::fs::read(&manifest).unwrap());
    let previous_modified = std::fs::metadata(&manifest).unwrap().modified().unwrap();
    let before = snapshot(pool).await;
    let observed = Arc::new(AtomicBool::new(false));
    let cancelled: ersatzrs_infra::maintenance::PlayoutBuildCancellation = {
        let manifest = manifest.clone();
        let previous = previous.clone();
        let observed = observed.clone();
        Arc::new(move || {
            if observed.load(Ordering::Acquire) {
                return true;
            }
            // Avoid reading the full file on every Lua cancellation poll.
            let changed = std::fs::metadata(&manifest)
                .and_then(|metadata| metadata.modified())
                .is_ok_and(|modified| modified != previous_modified)
                && std::fs::read(&manifest).is_ok_and(|bytes| bytes != *previous);
            if changed {
                observed.store(true, Ordering::Release);
            }
            changed
        })
    };
    let failure = ersatzrs_infra::maintenance::run_requested_playout_build_with_execution(
        pool,
        locker,
        PlayoutBuildRequest::new(88, PlayoutBuildMode::Refresh)
            .with_preservation(PlayoutPreservation::KeepCurrent),
        Utc::now(),
        ersatzrs_infra::maintenance::PlayoutExecution {
            hls_sessions: Some(registry),
            next: Some(host),
            cancelled: Some(&cancelled),
        },
    )
    .await
    .unwrap_err();
    assert!(
        observed.load(Ordering::Acquire),
        "failure must follow candidate file replacement"
    );
    assert!(failure.to_string().contains("cancel"));
    assert_eq!(std::fs::read(&manifest).unwrap(), *previous);
    assert_eq!(
        snapshot(pool).await,
        before,
        "complete accepted scheduling state survives cancellation"
    );
    assert!(
        !make_repositories(pool.clone())
            .playout
            .get_with_anchor(88)
            .await
            .unwrap()
            .build_status
            .unwrap()
            .success
    );
    assert_eq!(
        std::fs::read_dir(manifest.parent().unwrap())
            .unwrap()
            .count(),
        1
    );
    eprintln!("producer recovery: cancelled after Next replacement; accepted SQL and manifest restored before fresh retry");
}

async fn snapshot(pool: &SqlitePool) -> serde_json::Value {
    let repository = make_repositories(pool.clone()).playout;
    let mut playout = repository.get_with_anchor(88).await.unwrap();
    playout.build_status = None;
    let items = repository
        .items_in_range(
            88,
            Utc::now() - Duration::days(1),
            Utc::now() + Duration::days(3),
        )
        .await
        .unwrap();
    let history: String = sqlx::query_scalar(
        r#"SELECT json_group_array(json_array(
        "Id", "BlockId", "Details", "Index", "Key", "PlaybackOrder", "PlayoutId",
        "When", "Finish", "ChildKey", "IsCurrentChild", "Seed")) FROM
        (SELECT * FROM "PlayoutHistory" WHERE "PlayoutId"=88 ORDER BY "Id")"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let scripted: String = sqlx::query_scalar(
        r#"SELECT json_group_array(json_array(
        "CheckpointDate", "Context")) FROM (SELECT * FROM "PlayoutScriptedCheckpoint"
        WHERE "PlayoutId"=88 ORDER BY "CheckpointDate")"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let classic: String = sqlx::query_scalar(
        r#"SELECT json_group_array(json_array(
        "CheckpointDate", "Context")) FROM (SELECT * FROM "PlayoutClassicCheckpoint"
        WHERE "PlayoutId"=88 ORDER BY "CheckpointDate")"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let yaml: String = sqlx::query_scalar(
        r#"SELECT json_group_array(json_array(
        "CheckpointDate", "Context")) FROM (SELECT * FROM "PlayoutYamlCheckpoint"
        WHERE "PlayoutId"=88 ORDER BY "CheckpointDate")"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    serde_json::json!({"playout": playout, "items": items, "history": history,
        "scripted_checkpoints": scripted, "classic_checkpoints": classic,
        "yaml_checkpoints": yaml})
}
