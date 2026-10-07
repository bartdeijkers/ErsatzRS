use super::*;
use crate::{
    next_engine::{host::publication_fixture, publication::PLAYOUT_FILENAME},
    repositories::SqlitePlayoutRepository,
};
use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::{
    locking::{EntityLocker, PlayoutLockGuard, PlayoutReadGuard},
    repositories::PlayoutRepository,
    scheduling::SchedulingResult,
};
use ersatzrs_domain::{Playout, PlayoutAnchor, PlayoutItem};
use std::{
    path::PathBuf,
    sync::atomic::AtomicUsize,
    time::{Duration as StdDuration, Instant},
};

struct Fixture {
    root: tempfile::TempDir,
    pool: crate::db::DbPool,
    locker: EntityLocker,
    start: DateTime<Utc>,
    old_playout: Playout,
    old_items: Vec<PlayoutItem>,
    pending: PendingPlayoutBuild,
    prepared: Vec<PreparedHostPlayout>,
    guard: PlayoutLockGuard,
}

async fn fixture() -> Fixture {
    let root = tempfile::tempdir().unwrap();
    let (host, pool, locker, start) = publication_fixture(root.path()).await;
    let repository = SqlitePlayoutRepository(pool.clone());
    let old_playout = repository.get_with_anchor(1).await.unwrap();
    let old_items = repository
        .items_in_range(1, start, start + Duration::hours(1))
        .await
        .unwrap();
    let guard = PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
    let mut item = old_items[0].clone();
    item.id = 0;
    item.start = start + Duration::minutes(3);
    item.finish = item.start + Duration::minutes(1);
    item.guide_start = Some(item.start);
    item.guide_finish = Some(item.finish);
    let result = SchedulingResult {
        clear_items: true,
        added_items: vec![item],
        anchor: Some(PlayoutAnchor {
            next_start: start + Duration::minutes(4),
            ..Default::default()
        }),
        publication_deadline: Some(Instant::now() + StdDuration::from_secs(15)),
        ..Default::default()
    };
    let mut pending = repository
        .prepare_build_result(&result, &old_playout, Duration::zero())
        .await
        .unwrap();
    let prepared = host
        .prepare_active_build(
            &mut pending,
            start..start + Duration::hours(1),
            result.publication_deadline.unwrap(),
        )
        .await
        .unwrap()
        .unwrap();
    for id in [1, 2] {
        std::fs::write(path(root.path(), id), format!("accepted-{id}")).unwrap();
    }
    Fixture {
        root,
        pool,
        locker,
        start,
        old_playout,
        old_items,
        pending,
        prepared,
        guard,
    }
}

fn path(root: &std::path::Path, id: i64) -> PathBuf {
    root.join(id.to_string()).join(PLAYOUT_FILENAME)
}

async fn assert_old(
    pool: &crate::db::DbPool,
    start: DateTime<Utc>,
    playout: &Playout,
    items: &[PlayoutItem],
    root: &std::path::Path,
) {
    let repository = SqlitePlayoutRepository(pool.clone());
    // Queue behind a dropped transaction's rollback before checking both views.
    crate::db::begin_write(pool)
        .await
        .unwrap()
        .rollback()
        .await
        .unwrap();
    assert_eq!(repository.get_with_anchor(1).await.unwrap(), *playout);
    assert_eq!(
        repository
            .items_in_range(1, start, start + Duration::hours(1))
            .await
            .unwrap(),
        items
    );
    for id in [1, 2] {
        assert_eq!(
            std::fs::read(path(root, id)).unwrap(),
            format!("accepted-{id}").as_bytes()
        );
        assert_eq!(
            std::fs::read_dir(root.join(id.to_string()))
                .unwrap()
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn later_file_failure_restores_earlier_owner_and_rolls_back_sql() {
    let Fixture {
        root,
        pool,
        locker: _,
        start,
        old_playout,
        old_items,
        pending,
        prepared,
        guard,
    } = Box::pin(fixture()).await;
    // A destination directory cannot be atomically replaced by a regular file.
    // The first publisher succeeds; the mirror fails after its preparation.
    let mirror = path(root.path(), 2);
    std::fs::remove_file(&mirror).unwrap();
    std::fs::create_dir(&mirror).unwrap();
    let failure = settle(pending, prepared, None, guard.hold_finalization())
        .await
        .unwrap_err();
    assert!(failure.to_string().contains("publication"));
    std::fs::remove_dir(&mirror).unwrap();
    std::fs::write(&mirror, b"accepted-2").unwrap();
    assert_old(&pool, start, &old_playout, &old_items, root.path()).await;
}

#[tokio::test]
async fn sql_commit_deadline_failure_restores_every_published_file() {
    let Fixture {
        root,
        pool,
        locker: _,
        start,
        old_playout,
        old_items,
        pending,
        prepared,
        guard,
    } = Box::pin(fixture()).await;
    let deadline = decision_deadline(pending.publication_deadline(), prepared.len())
        .unwrap()
        .unwrap();
    let root_path = root.path().to_owned();
    let runtime = tokio::runtime::Handle::current();
    let failure = tokio::task::spawn_blocking(move || {
        let _hold = guard.hold_finalization();
        let calls = AtomicUsize::new(0);
        settle_blocking(pending, prepared, &runtime, || {
            if calls.fetch_add(1, Ordering::SeqCst) == 2 {
                for id in [1, 2] {
                    assert_ne!(
                        std::fs::read(path(&root_path, id)).unwrap(),
                        format!("accepted-{id}").as_bytes()
                    );
                }
                std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            }
            true
        })
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(failure.to_string().contains("deadline"));
    assert_old(&pool, start, &old_playout, &old_items, root.path()).await;
}

#[tokio::test]
async fn aborted_awaiter_cannot_release_build_before_file_rollback_finishes() {
    let Fixture {
        root,
        pool,
        locker,
        start,
        old_playout,
        old_items,
        pending,
        prepared,
        guard,
    } = Box::pin(fixture()).await;
    let (entered, waiting) = tokio::sync::oneshot::channel();
    let entered = std::sync::Mutex::new(Some(entered));
    let (release, blocked) = std::sync::mpsc::channel();
    let blocked = std::sync::Mutex::new(blocked);
    let calls = AtomicUsize::new(0);
    let cancelled: PlayoutBuildCancellation = Arc::new(move || {
        if calls.fetch_add(1, Ordering::SeqCst) == 2 {
            entered.lock().unwrap().take().unwrap().send(()).unwrap();
            blocked
                .lock()
                .unwrap()
                .recv_timeout(StdDuration::from_secs(5))
                .unwrap();
        }
        false
    });
    let task = tokio::spawn(settle(
        pending,
        prepared,
        Some(cancelled),
        guard.hold_finalization(),
    ));
    drop(guard);
    tokio::time::timeout(StdDuration::from_secs(5), waiting)
        .await
        .unwrap()
        .unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(locker.is_playout_locked(1));
    assert!(PlayoutReadGuard::try_acquire(&locker, 1).is_none());
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_none());
    release.send(()).unwrap();
    tokio::time::timeout(StdDuration::from_secs(5), async {
        while locker.is_playout_locked(1) {
            tokio::time::sleep(StdDuration::from_millis(10)).await;
        }
    })
    .await
    .unwrap();
    assert_old(&pool, start, &old_playout, &old_items, root.path()).await;
}

#[tokio::test]
async fn successful_group_commits_sql_and_keeps_both_candidate_files() {
    let Fixture {
        root,
        pool,
        locker: _,
        start,
        old_playout: _,
        old_items,
        pending,
        prepared,
        guard,
    } = Box::pin(fixture()).await;
    assert_eq!(
        settle(pending, prepared, None, guard.hold_finalization())
            .await
            .unwrap(),
        [1, 2]
    );
    let items = SqlitePlayoutRepository(pool)
        .items_in_range(1, start, start + Duration::hours(1))
        .await
        .unwrap();
    assert_ne!(items, old_items);
    for id in [1, 2] {
        let file: crate::next_engine::protocol::Playout =
            serde_json::from_slice(&std::fs::read(path(root.path(), id)).unwrap()).unwrap();
        assert!(file
            .items
            .iter()
            .any(|item| item.id == items[0].id.to_string()));
        assert_eq!(
            std::fs::read_dir(root.path().join(id.to_string()))
                .unwrap()
                .count(),
            1
        );
    }
}

#[tokio::test]
async fn insufficient_recovery_window_refuses_before_any_file_changes() {
    let Fixture {
        root,
        pool,
        locker: _,
        start,
        old_playout,
        old_items,
        mut pending,
        prepared,
        guard,
    } = Box::pin(fixture()).await;
    pending.restrict_deadline(Instant::now() + StdDuration::from_secs(1));
    let error = settle(pending, prepared, None, guard.hold_finalization())
        .await
        .unwrap_err();
    assert!(error.to_string().contains("recovery window"));
    assert_old(&pool, start, &old_playout, &old_items, root.path()).await;
}

#[tokio::test]
async fn failed_compensation_reports_storage_failure_and_retains_previous_bytes() {
    let Fixture {
        root,
        pool,
        locker: _,
        start,
        old_playout,
        old_items,
        pending,
        prepared,
        guard,
    } = Box::pin(fixture()).await;
    let root_path = root.path().to_owned();
    let runtime = tokio::runtime::Handle::current();
    let error = tokio::task::spawn_blocking(move || {
        let _hold = guard.hold_finalization();
        let calls = AtomicUsize::new(0);
        settle_blocking(pending, prepared, &runtime, || {
            if calls.fetch_add(1, Ordering::SeqCst) == 2 {
                let destination = path(&root_path, 1);
                std::fs::remove_file(&destination).unwrap();
                std::fs::create_dir(&destination).unwrap();
                return false;
            }
            true
        })
    })
    .await
    .unwrap()
    .unwrap_err();
    assert!(error.to_string().contains("restoration failed"));
    crate::db::begin_write(&pool)
        .await
        .unwrap()
        .rollback()
        .await
        .unwrap();
    let repository = SqlitePlayoutRepository(pool);
    assert_eq!(repository.get_with_anchor(1).await.unwrap(), old_playout);
    assert_eq!(
        repository
            .items_in_range(1, start, start + Duration::hours(1))
            .await
            .unwrap(),
        old_items
    );
    assert_eq!(std::fs::read(path(root.path(), 2)).unwrap(), b"accepted-2");
    let backups: Vec<_> = std::fs::read_dir(root.path().join("1"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with(".next-rollback-")
        })
        .collect();
    assert_eq!(backups.len(), 1);
    assert_eq!(std::fs::read(&backups[0]).unwrap(), b"accepted-1");
}
