//! Row292: successful real local scans invalidate only dependent projections.
use super::*;
use ersatzrs_core::repositories::PlayoutRepository as _;

async fn scan_fixture() -> (crate::host::state::AppState, tempfile::TempDir) {
    let (state, _) = application_fixture().await;
    let root = tempfile::tempdir().unwrap();
    sqlx::raw_sql(
        r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId")
            VALUES (1,'Scanned',4,1),(2,'Unrelated',4,1);
        INSERT INTO "LocalLibrary" ("Id") VALUES (1),(2);
    "#,
    )
    .execute(&state.pool)
    .await
    .unwrap();
    for id in [1, 2] {
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (?,?,?)"#)
            .bind(id)
            .bind(id)
            .bind(root.path().to_str().unwrap())
            .execute(&state.pool)
            .await
            .unwrap();
    }
    sqlx::raw_sql(
        r#"INSERT INTO "MediaItem" ("Id","LibraryPathId","State") VALUES (10,1,0),(11,2,0);
        INSERT INTO "OtherVideo" ("Id") VALUES (10),(11);
        INSERT INTO "OtherVideoMetadata" ("OtherVideoId","MetadataKind","DateAdded","DateUpdated")
            VALUES (10,0,'2026-11-16','2026-11-16'),(11,0,'2026-11-16','2026-11-16');
        INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","Seed")
            VALUES (1,1,0,17),(2,2,0,18),(3,3,0,19);"#,
    )
    .execute(&state.pool)
    .await
    .unwrap();
    let now = chrono::Utc::now();
    for id in 1..=3 {
        state
            .repositories
            .playout
            .upsert_item(&ersatzrs_domain::PlayoutItem {
                id,
                playout_id: id,
                media_item_id: if id == 3 { 11 } else { 10 },
                start: now,
                finish: now + chrono::Duration::minutes(1),
                out_point: chrono::Duration::minutes(1),
                ..Default::default()
            })
            .await
            .unwrap();
    }
    (state, root)
}

#[tokio::test]
async fn row292_successful_scan_notifies_only_next_dependents_and_mirrors() {
    let (state, _root) = scan_fixture().await;
    let mut workers = Vec::new();
    for registry in [&state.hls_sessions, &state.preview_hls_sessions] {
        for number in ["1", "2", "3", "4", "5"] {
            let worker = Arc::new(Worker {
                number: number.into(),
                updates: AtomicUsize::new(0),
            });
            registry.insert(worker.clone()).await;
            workers.push(worker);
        }
    }
    let start = chrono::Utc::now() - chrono::Duration::hours(1);
    let finish = start + chrono::Duration::hours(2);
    let before = state
        .repositories
        .playout
        .items_in_range(1, start, finish)
        .await
        .unwrap();
    for deep in [false, true] {
        assert_eq!(
            execute_scan(&state, 1, deep).await,
            ersatzrs_app::commands::ExecuteManagedLibraryOperationOutcome::Completed
        );
    }
    assert_eq!(
        state
            .repositories
            .playout
            .items_in_range(1, start, finish)
            .await
            .unwrap(),
        before
    );
    assert_eq!(
        workers
            .iter()
            .map(|w| w.updates.load(Ordering::SeqCst))
            .collect::<Vec<_>>(),
        [2, 0, 0, 2, 2, 2, 0, 0, 2, 2]
    );
    assert_eq!(
        execute_scan(&state, 2, false).await,
        ersatzrs_app::commands::ExecuteManagedLibraryOperationOutcome::Completed
    );
    assert_eq!(
        workers
            .iter()
            .map(|worker| worker.updates.load(Ordering::SeqCst))
            .collect::<Vec<_>>(),
        [2, 0, 1, 2, 2, 2, 0, 1, 2, 2]
    );
}

async fn execute_scan(
    state: &crate::host::state::AppState,
    library: i64,
    deep: bool,
) -> ersatzrs_app::commands::ExecuteManagedLibraryOperationOutcome {
    let admitted = state
        .application
        .request_library_scan(ersatzrs_app::commands::RequestLibraryScan::new(
            library, deep,
        ))
        .await;
    assert!(
        matches!(
            admitted,
            ersatzrs_app::commands::RequestScanOutcome::Accepted { .. }
        ),
        "{admitted:?}"
    );
    let claim = state
        .application
        .claim_managed_library_operation()
        .await
        .unwrap()
        .unwrap();
    ersatzrs_infra::ffprobe::LocalProbeCleanupScope::new()
        .scope(state.application.execute_managed_library_operation(claim))
        .await
}
