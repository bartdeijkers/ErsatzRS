use super::*;
use ersatzrs_core::{
    hls::{HlsPublicationError, HlsSessionHandle, HlsSessionRegistry},
    locking::EntityLocker,
    scheduling::PlayoutBuildRequest,
};
use ersatzrs_infra::{
    hls::InMemoryHlsSessionRegistry,
    maintenance::{run_requested_playout_build, run_requested_playout_build_cancellable},
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn action(mode: PlayoutBuildMode, policy: PlayoutPreservation) -> PlayoutBuildRequest {
    PlayoutBuildRequest::new(1, mode).with_preservation(policy)
}

async fn run(
    pool: &DbPool,
    request: PlayoutBuildRequest,
) -> ersatzrs_infra::maintenance::Result<bool> {
    run_requested_playout_build(
        pool,
        &EntityLocker::new(),
        request,
        common::fixed_time() + Duration::seconds(1),
        None,
    )
    .await
}

async fn adopt(pool: &DbPool) {
    assert!(run(
        pool,
        action(
            PlayoutBuildMode::Refresh,
            PlayoutPreservation::ReplaceCurrent
        )
    )
    .await
    .unwrap());
}

async fn revision(pool: &DbPool) -> i64 {
    snapshot::accepted(pool, 1).await.unwrap().unwrap().revision
}

#[tokio::test]
async fn external_worker_requires_explicit_baseline_and_continues_after_restart() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.json");
    let db = root.path().join("worker.db");
    let pool = open(&db).await;
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    seed(&pool, &source).await;
    std::fs::write(&source, document(&["A", "B", "C"])).unwrap();
    assert!(run(
        &pool,
        PlayoutBuildRequest::new(1, PlayoutBuildMode::Continue)
    )
    .await
    .unwrap());
    assert!(snapshot::accepted(&pool, 1).await.unwrap().is_none());
    for mode in [
        PlayoutBuildMode::Continue,
        PlayoutBuildMode::Refresh,
        PlayoutBuildMode::Reset,
    ] {
        let error = run(&pool, action(mode, PlayoutPreservation::KeepCurrent))
            .await
            .unwrap_err();
        assert!(error.to_string().contains("accepted planning baseline"));
        assert!(snapshot::accepted(&pool, 1).await.unwrap().is_none());
        assert_consumers(&pool, &["A", "B", "C"]).await;
    }
    adopt(&pool).await;
    assert_eq!(revision(&pool).await, 1);
    pool.close().await;
    let pool = open(&db).await;
    std::fs::write(&source, document(&["A", "B", "C", "D"])).unwrap();
    // Exercise ordinary maintenance policy after adoption, not only explicit API calls.
    assert!(run(
        &pool,
        PlayoutBuildRequest::new(1, PlayoutBuildMode::Continue)
    )
    .await
    .unwrap());
    assert_eq!(revision(&pool).await, 2);
    assert_consumers(&pool, &["A", "B", "C", "D"]).await;
    assert!(run(
        &pool,
        action(PlayoutBuildMode::Continue, PlayoutPreservation::KeepCurrent)
    )
    .await
    .unwrap());
    assert_eq!(
        revision(&pool).await,
        2,
        "unchanged polling must not republish"
    );
    std::fs::write(&source, document(&["Changed", "B", "C", "D"])).unwrap();
    assert!(run(
        &pool,
        action(PlayoutBuildMode::Continue, PlayoutPreservation::KeepCurrent)
    )
    .await
    .is_err());
    assert_eq!(revision(&pool).await, 2);
    assert_consumers(&pool, &["A", "B", "C", "D"]).await;
    let status: (bool, Option<String>) = sqlx::query_as(
        r#"SELECT "Success", "LastSuccessfulBuild" FROM "PlayoutBuildStatus" WHERE "PlayoutId"=1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!status.0);
    assert!(status.1.is_some());
}

#[tokio::test]
async fn external_worker_preserves_native_prefetch_and_leading_mirrors_for_refresh_and_reset() {
    for mode in [PlayoutBuildMode::Refresh, PlayoutBuildMode::Reset] {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source.json");
        let pool = common::test_db().await;
        seed(&pool, &source).await;
        pool.execute(r#"UPDATE "Channel" SET "PlayoutOffset"='-00:01:00' WHERE "Id"=2"#)
            .await
            .unwrap();
        std::fs::write(&source, document(&["A", "B", "C", "D", "E", "F"])).unwrap();
        adopt(&pool).await;
        let before = snapshot::accepted(&pool, 1)
            .await
            .unwrap()
            .unwrap()
            .schedule;
        std::fs::write(&source, document(&["X", "Y", "Z", "Q", "New", "Next"])).unwrap();
        let sessions = InMemoryHlsSessionRegistry::default();
        let owner = Arc::new(Owner::default());
        sessions.insert(owner.clone()).await;
        assert!(run_requested_playout_build(
            &pool,
            &EntityLocker::new(),
            action(mode, PlayoutPreservation::KeepCurrent),
            common::fixed_time() + Duration::seconds(1),
            Some(&sessions)
        )
        .await
        .unwrap());
        let after = snapshot::accepted(&pool, 1)
            .await
            .unwrap()
            .unwrap()
            .schedule;
        assert!(after.items[..4] == before.items[..4]);
        assert_eq!(
            after.items[4].start,
            common::fixed_time() + Duration::minutes(4)
        );
        assert_eq!(after.items[4].program.title.as_deref(), Some("New"));
        assert_eq!(owner.notifications.load(Ordering::SeqCst), 0);
        assert!(sessions.try_get("1").await.is_some());
        assert_eq!(revision(&pool).await, 2);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(r#"SELECT "Seed" FROM "Playout" WHERE "Id"=1"#)
                .fetch_one(&pool)
                .await
                .unwrap(),
            1
        );
    }
}

#[tokio::test]
async fn external_worker_refuses_missing_sources_next_and_cancelled_or_locked_work() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.json");
    let pool = common::test_db().await;
    seed(&pool, &source).await;
    std::fs::write(&source, document(&["A", "B", "C"])).unwrap();
    adopt(&pool).await;
    let request = action(PlayoutBuildMode::Refresh, PlayoutPreservation::KeepCurrent);
    let locker = EntityLocker::new();
    let lock = ersatzrs_core::locking::PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
    assert!(
        !run_requested_playout_build(&pool, &locker, request, common::fixed_time(), None)
            .await
            .unwrap()
    );
    drop(lock);
    let cancel: ersatzrs_infra::maintenance::PlayoutBuildCancellation = Arc::new(|| true);
    assert!(run_requested_playout_build_cancellable(
        &pool,
        &locker,
        request,
        common::fixed_time(),
        None,
        Some(&cancel)
    )
    .await
    .is_err());
    sqlx::query(r#"UPDATE "Channel" SET "StreamingEngine"=? WHERE "Id"=2"#)
        .bind(ersatzrs_domain::StreamingEngine::Next as i32)
        .execute(&pool)
        .await
        .unwrap();
    assert!(run(&pool, request)
        .await
        .unwrap_err()
        .to_string()
        .contains("Next does not consume"));
    pool.execute(r#"UPDATE "Channel" SET "StreamingEngine"=0 WHERE "Id"=2"#)
        .await
        .unwrap();
    std::fs::remove_file(&source).unwrap();
    assert!(run(&pool, request)
        .await
        .unwrap_err()
        .to_string()
        .contains("source is missing"));
    assert_eq!(revision(&pool).await, 1);
    assert_consumers(&pool, &["A", "B", "C"]).await;
}

#[tokio::test]
async fn external_worker_retries_replacement_notification_without_republishing() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("source.json");
    let pool = common::test_db().await;
    seed(&pool, &source).await;
    std::fs::write(&source, document(&["A", "B", "C"])).unwrap();
    let owner = Arc::new(Owner {
        fail_first: true,
        ..Owner::default()
    });
    let sessions = InMemoryHlsSessionRegistry::default();
    sessions.insert(owner.clone()).await;
    let request = action(PlayoutBuildMode::Reset, PlayoutPreservation::ReplaceCurrent);
    let locker = EntityLocker::new();
    assert!(run_requested_playout_build(
        &pool,
        &locker,
        request,
        common::fixed_time(),
        Some(&sessions)
    )
    .await
    .is_err());
    assert_eq!(revision(&pool).await, 1);
    assert!(run_requested_playout_build(
        &pool,
        &locker,
        request,
        common::fixed_time(),
        Some(&sessions)
    )
    .await
    .unwrap());
    assert_eq!(revision(&pool).await, 1);
    assert_eq!(owner.notifications.load(Ordering::SeqCst), 2);
}

#[derive(Default)]
struct Owner {
    notifications: AtomicUsize,
    fail_first: bool,
}

#[async_trait::async_trait]
impl HlsSessionHandle for Owner {
    fn channel_number(&self) -> &'static str {
        "1"
    }
    fn touch(&self) {}
    async fn trim_playlist(&self) -> Option<String> {
        None
    }
    async fn cancel(&self) {
        panic!("adaptation must not cancel an existing owner")
    }
    fn playout_updated(&self) {
        self.notifications.fetch_add(1, Ordering::SeqCst);
    }
    async fn publish_playout_update(&self, _: Instant) -> Result<(), HlsPublicationError> {
        let previous = self.notifications.fetch_add(1, Ordering::SeqCst);
        if self.fail_first && previous == 0 {
            Err(HlsPublicationError::Failed)
        } else {
            Ok(())
        }
    }
}
