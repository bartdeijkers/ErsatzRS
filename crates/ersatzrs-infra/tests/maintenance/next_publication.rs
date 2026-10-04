use super::*;
use ersatzrs_core::{hls::HlsPublicationError, locking::PlayoutLockGuard};
use ersatzrs_infra::{
    hls::InMemoryHlsSessionRegistry,
    next_engine::{
        coordinator::{ChannelPublisher, RefreshOutcome},
        protocol, publication,
    },
};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::Notify;

struct PublishingHandle {
    publisher: ChannelPublisher,
    start: chrono::DateTime<Utc>,
    entered: Notify,
    release: Notify,
    fail: AtomicBool,
}

#[async_trait]
impl HlsSessionHandle for PublishingHandle {
    fn channel_number(&self) -> &'static str {
        "1"
    }
    fn touch(&self) {}
    async fn trim_playlist(&self) -> Option<String> {
        None
    }
    async fn cancel(&self) {}
    fn playout_updated(&self) {
        panic!("a notification is not a publication acknowledgement")
    }

    async fn publish_playout_update(
        &self,
        _deadline: std::time::Instant,
    ) -> Result<(), HlsPublicationError> {
        self.entered.notify_one();
        self.release.notified().await;
        if self.fail.load(Ordering::Acquire) {
            return Err(HlsPublicationError::Failed);
        }
        let outcome = self
            .publisher
            .refresh(
                self.start,
                self.start + Duration::days(3),
                |_, items| async {
                    Ok(items
                        .into_iter()
                        .map(|media| protocol::PlayoutItem {
                            id: media.item.id.to_string(),
                            start: media.item.start,
                            finish: media.item.finish,
                            source: Some(protocol::Source::Local {
                                path: media.path,
                                in_point_ms: None,
                                out_point_ms: None,
                                probe_hint: None,
                            }),
                            watermark: None,
                            graphics: Vec::new(),
                            tracks: None,
                        })
                        .collect())
                },
            )
            .await
            .map_err(|_| HlsPublicationError::Failed)?;
        assert!(
            matches!(outcome, RefreshOutcome::Published { .. }),
            "the committed build must admit its publisher without admitting another build"
        );
        Ok(())
    }
}

async fn fixture() -> (SqlitePool, EntityLocker, chrono::DateTime<Utc>) {
    let pool = common::test_db().await;
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    seed_refresh_rebuild_fixture(&pool, &repos).await;
    pool.execute(r#"INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash") VALUES (1, 1000, 'media/fixture.mkv', 'fixture');
    UPDATE "Channel" SET "StreamingEngine"=1 WHERE "Id"=1;"#).await.unwrap();
    let start = Utc.with_ymd_and_hms(2026, 4, 15, 10, 0, 0).unwrap();
    let locker = EntityLocker::new();
    run_requested_playout_build(
        &pool,
        &locker,
        PlayoutBuildRequest::new(88, PlayoutBuildMode::Reset),
        start,
        None,
    )
    .await
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    ersatzrs_infra::scan_refresh::persist_unowned_refresh_intents(
        &mut tx,
        ScanPlayoutRefreshReason::RemoteStreamMembershipAdded,
        &[88],
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    (pool, locker, start)
}

async fn success_time(pool: &SqlitePool) -> String {
    sqlx::query_scalar(
        r#"SELECT "LastSuccessfulBuild" FROM "PlayoutBuildStatus" WHERE "PlayoutId"=88"#,
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn cancelled_refresh_retains_bounded_retry_and_recovery_eligibility() {
    let (pool, locker, start) = fixture().await;
    let old_success = success_time(&pool).await;
    let repository = make_repositories(pool.clone()).playout;
    let before = repository
        .items_in_range(88, start, start + Duration::days(3))
        .await
        .unwrap();
    let cancelled: ersatzrs_infra::maintenance::PlayoutBuildCancellation = Arc::new(|| true);
    let request = PlayoutBuildRequest::new(88, PlayoutBuildMode::Refresh);
    let error = ersatzrs_infra::maintenance::run_requested_playout_build_cancellable(
        &pool,
        &locker,
        request,
        start,
        None,
        Some(&cancelled),
    )
    .await
    .unwrap_err();
    assert!(error.to_string().contains("cancel"));
    assert_eq!(
        repository
            .items_in_range(88, start, start + Duration::days(3))
            .await
            .unwrap(),
        before
    );
    assert_eq!(success_time(&pool).await, old_success);
    let (state, attempts, next_attempt): (i64, i64, Option<String>) = sqlx::query_as(
        r#"SELECT "State", "Attempts", "NextAttempt" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId"=88"#,
    ).fetch_one(&pool).await.unwrap();
    assert_eq!(
        state, 0,
        "cancellation must not strand the generation as Failed"
    );
    assert_eq!(attempts, 1);
    assert!(next_attempt.is_some());
    assert!(ersatzrs_infra::scan_refresh::recoverable_refreshes(&pool)
        .await
        .unwrap()
        .is_empty());
    pool.execute(
        r#"UPDATE "ScanPlayoutRefreshIntent" SET "NextAttempt"=NULL WHERE "PlayoutId"=88"#,
    )
    .await
    .unwrap();
    let recovered = ersatzrs_infra::scan_refresh::recoverable_refreshes(&pool)
        .await
        .unwrap();
    assert_eq!(recovered.len(), 1);
    assert_eq!(recovered[0].playout_id, 88);
    assert!(
        run_requested_playout_build(&pool, &locker, request, start, None)
            .await
            .unwrap()
    );
    let remaining: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId"=88"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(remaining, 0);
}

#[tokio::test]
async fn stalled_session_cannot_hold_build_completion_past_publication_deadline() {
    let (pool, locker, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let handle = Arc::new(PublishingHandle {
        publisher: ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), 1),
        start,
        entered: Notify::new(),
        release: Notify::new(),
        fail: AtomicBool::new(false),
    });
    let registry = Arc::new(InMemoryHlsSessionRegistry::new());
    registry.insert(handle.clone()).await;
    let old_success = success_time(&pool).await;
    let (db, locks) = (pool.clone(), locker.clone());
    let build = tokio::spawn(async move {
        run_requested_playout_build(
            &db,
            &locks,
            PlayoutBuildRequest::new(88, PlayoutBuildMode::Refresh)
                .with_preservation(ersatzrs_core::scheduling::PlayoutPreservation::KeepCurrent),
            start + Duration::minutes(1),
            Some(registry.as_ref()),
        )
        .await
    });
    tokio::time::timeout(std::time::Duration::from_secs(5), handle.entered.notified())
        .await
        .unwrap();
    tokio::time::pause();
    tokio::time::advance(std::time::Duration::from_secs(30)).await;
    // Return to real time before SQLite status/retry writes: virtual-time
    // auto-advance must not expire pool acquisition while its worker does I/O.
    tokio::time::resume();
    let error = build.await.unwrap().unwrap_err();
    assert!(
        error.to_string().contains("publication timed out"),
        "{error}"
    );
    assert_eq!(success_time(&pool).await, old_success);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT "State" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId"=88"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert!(PlayoutLockGuard::try_acquire(&locker, 88).is_some());
    assert!(!root
        .path()
        .join("1")
        .join(publication::PLAYOUT_FILENAME)
        .exists());
}

#[tokio::test]
async fn refresh_completion_waits_for_snapshot_and_failure_preserves_durable_retry() {
    let (pool, locker, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let handle = Arc::new(PublishingHandle {
        publisher: ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), 1),
        start,
        entered: Notify::new(),
        release: Notify::new(),
        fail: AtomicBool::new(true),
    });
    let registry = Arc::new(InMemoryHlsSessionRegistry::new());
    registry.insert(handle.clone()).await;
    let old_success = success_time(&pool).await;
    let now = start + Duration::minutes(1);
    let request = PlayoutBuildRequest::new(88, PlayoutBuildMode::Refresh)
        .with_preservation(ersatzrs_core::scheduling::PlayoutPreservation::KeepCurrent);
    for fail in [true, false] {
        handle.fail.store(fail, Ordering::Release);
        let (db, locks, sessions) = (pool.clone(), locker.clone(), registry.clone());
        let build = tokio::spawn(async move {
            run_requested_playout_build(&db, &locks, request, now, Some(sessions.as_ref())).await
        });
        tokio::time::timeout(std::time::Duration::from_secs(5), handle.entered.notified())
            .await
            .unwrap();
        assert!(!build.is_finished());
        assert_eq!(success_time(&pool).await, old_success);
        assert!(PlayoutLockGuard::try_acquire(&locker, 88).is_none());
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                r#"SELECT "State" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId"=88"#
            )
            .fetch_one(&pool)
            .await
            .unwrap(),
            1
        );
        handle.release.notify_one();
        let result = build.await.unwrap();
        if fail {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("session publication"));
            assert_eq!(success_time(&pool).await, old_success);
            let pending: (i64, i64, String) = sqlx::query_as(r#"SELECT "State", "Attempts", "Diagnostic" FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId"=88"#).fetch_one(&pool).await.unwrap();
            assert_eq!(
                pending,
                (0, 1, "playout-session-publication-pending".into())
            );
            assert!(!root
                .path()
                .join("1")
                .join(publication::PLAYOUT_FILENAME)
                .exists());
        } else {
            assert!(result.unwrap());
            assert_ne!(success_time(&pool).await, old_success);
        }
    }
    let published: protocol::Playout = serde_json::from_slice(
        &std::fs::read(root.path().join("1").join(publication::PLAYOUT_FILENAME)).unwrap(),
    )
    .unwrap();
    assert!(!published.items.is_empty());
    let stored = make_repositories(pool.clone())
        .playout
        .items_in_range(88, start, start + Duration::days(3))
        .await
        .unwrap();
    assert_eq!(
        published
            .items
            .iter()
            .map(|item| (item.id.clone(), item.start, item.finish))
            .collect::<Vec<_>>(),
        stored
            .iter()
            .map(|item| (item.id.to_string(), item.start, item.finish))
            .collect::<Vec<_>>()
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "ScanPlayoutRefreshIntent" WHERE "PlayoutId"=88"#
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert!(PlayoutLockGuard::try_acquire(&locker, 88).is_some());
}
