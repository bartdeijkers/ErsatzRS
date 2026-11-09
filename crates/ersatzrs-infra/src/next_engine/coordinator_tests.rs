use super::*;
use chrono::Duration;
use ersatzrs_core::{
    locking::PlayoutLockGuard,
    repositories::{PlayoutRepository, RepositoryError},
};
use tokio::sync::oneshot;

#[path = "indexed_coordinator_tests.rs"]
mod indexed;

async fn fixture() -> (DbPool, EntityLocker, Channel, DateTime<Utc>) {
    let (pool, locker, mut channel, start) = schedule::tests::fixture().await;
    channel.streaming_engine = StreamingEngine::Next;
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    (pool, locker, channel, start)
}

async fn project(
    channel: Channel,
    items: Vec<schedule::ScheduledMedia>,
) -> ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>> {
    Ok(items
        .into_iter()
        .map(|media| protocol::PlayoutItem {
            watermark: None,
            graphics: Vec::new(),
            id: format!("{}-{}", channel.name, media.item.id),
            start: media.item.start,
            finish: media.item.finish,
            source: Some(protocol::Source::Local {
                path: media.path,
                in_point_ms: None,
                out_point_ms: None,

                probe_hint: None,
            }),
            tracks: None,
        })
        .collect())
}

fn read(publisher: &ChannelPublisher) -> protocol::Playout {
    serde_json::from_slice(
        &std::fs::read(publisher.folder.join(publication::PLAYOUT_FILENAME)).unwrap(),
    )
    .unwrap()
}

#[tokio::test]
async fn deadline_expiring_during_projection_preserves_previous_manifest() {
    let (pool, locker, channel, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let publisher = ChannelPublisher::new(pool, locker, root.path(), channel.id);
    let finish = start + Duration::minutes(2);
    publisher.refresh(start, finish, project).await.unwrap();
    let filename = publisher.folder.join(publication::PLAYOUT_FILENAME);
    let old = std::fs::read(&filename).unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(100);
    let entered = std::sync::atomic::AtomicBool::new(false);
    let result = publisher
        .refresh_before(start, finish, deadline, |channel, items| async {
            entered.store(true, Ordering::Relaxed);
            tokio::time::sleep_until(deadline.into()).await;
            project(channel, items).await
        })
        .await;
    assert!(entered.load(Ordering::Relaxed));
    assert!(matches!(
        result,
        Err(RefreshError::Publication(
            publication::PublicationError::Expired
        ))
    ));
    assert_eq!(std::fs::read(filename).unwrap(), old);
    assert_eq!(std::fs::read_dir(&publisher.folder).unwrap().count(), 1);
    publisher.refresh(start, finish, project).await.unwrap();
}

#[tokio::test]
async fn switched_or_deleted_channel_is_terminal_without_overwriting_snapshot() {
    let (pool, locker, mut channel, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let publisher = ChannelPublisher::new(pool.clone(), locker, root.path(), channel.id);
    let finish = start + Duration::minutes(2);
    publisher.refresh(start, finish, project).await.unwrap();
    let filename = publisher.folder.join(publication::PLAYOUT_FILENAME);
    let old = std::fs::read(&filename).unwrap();
    channel.streaming_engine = StreamingEngine::default();
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    assert!(matches!(
        publisher.refresh(start, finish, project).await,
        Err(RefreshError::Retired)
    ));
    assert_eq!(std::fs::read(&filename).unwrap(), old);
    sqlx::query(r#"DELETE FROM "Channel" WHERE "Id" = ?"#)
        .bind(channel.id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        publisher.refresh(start, finish, project).await,
        Err(RefreshError::Retired)
    ));
    assert_eq!(std::fs::read(&filename).unwrap(), old);
}

#[tokio::test]
async fn busy_and_failed_refresh_retain_old_items_then_empty_refresh_withdraws() {
    let (pool, locker, channel, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let publisher = ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), channel.id);
    let finish = start + Duration::minutes(2);
    assert_eq!(
        publisher.refresh(start, finish, project).await.unwrap(),
        RefreshOutcome::Published { items: 2 }
    );
    let old = std::fs::read(publisher.folder.join(publication::PLAYOUT_FILENAME)).unwrap();
    let guard = PlayoutLockGuard::try_acquire(&locker, 1).unwrap();
    assert_eq!(
        publisher
            .refresh(start, finish, |_, _| async {
                panic!("busy read must not project")
            })
            .await
            .unwrap(),
        RefreshOutcome::Busy
    );
    drop(guard);
    let error = publisher
        .refresh(start, finish, |_, _| async {
            Err(RepositoryError::Decode {
                kind: "fixture",
                value: "private source must not leak".into(),
            })
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "Next schedule projection failed");
    assert_eq!(
        std::fs::read(publisher.folder.join(publication::PLAYOUT_FILENAME)).unwrap(),
        old
    );
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_some());
    assert_eq!(
        publisher
            .refresh(
                start + Duration::days(1),
                finish + Duration::days(1),
                project
            )
            .await
            .unwrap(),
        RefreshOutcome::Published { items: 0 }
    );
    assert!(read(&publisher).items.is_empty());
}

#[tokio::test]
async fn queued_refresh_reads_current_settings_after_admission() {
    let (pool, locker, channel, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let publisher = ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), channel.id);
    let finish = start + Duration::minutes(2);
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = oneshot::channel();
    let first = publisher.clone();
    let task = tokio::spawn(async move {
        first
            .refresh(start, finish, |channel, items| async move {
                entered_tx.send(()).unwrap();
                release_rx.await.unwrap();
                project(channel, items).await
            })
            .await
            .unwrap()
    });
    entered_rx.await.unwrap();
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_none());
    let second = publisher.refresh(start, finish, project);
    tokio::pin!(second);
    std::future::poll_fn(|cx| {
        assert!(second.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let mut changed = channel;
    changed.name = "Updated".into();
    make_repositories(pool)
        .channel
        .upsert(&changed)
        .await
        .unwrap();
    release_tx.send(()).unwrap();
    task.await.unwrap();
    second.await.unwrap();
    assert_eq!(read(&publisher).items[0].id, "Updated-1");
}

#[tokio::test]
async fn cancelled_projection_releases_locks_without_publishing() {
    let (pool, locker, channel, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let publisher = ChannelPublisher::new(pool, locker.clone(), root.path(), channel.id);
    let (entered_tx, entered_rx) = oneshot::channel();
    let pending = publisher.clone();
    let task = tokio::spawn(async move {
        pending
            .refresh(start, start + Duration::minutes(2), |_, _| async move {
                entered_tx.send(()).unwrap();
                std::future::pending().await
            })
            .await
    });
    entered_rx.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(publisher.gate.try_lock().is_ok());
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_some());
    assert!(!publisher.folder.exists());
}

#[tokio::test]
async fn cancelled_awaiter_cannot_release_admission_before_blocking_write_finishes() {
    let (pool, locker, channel, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let publisher = ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), channel.id);
    let finish = start + Duration::minutes(2);
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let first = publisher.clone();
    let task = tokio::spawn(async move {
        first
            .refresh_with(start, finish, project, move |folder, playout, active| {
                entered_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                publication::publish_playout_checked(
                    folder,
                    playout,
                    || active.load(Ordering::Acquire),
                    None,
                )
            })
            .await
    });
    entered_rx.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(publisher.gate.try_lock().is_err());
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_none());
    let second = publisher.refresh(start, finish, project);
    tokio::pin!(second);
    std::future::poll_fn(|cx| {
        assert!(second.as_mut().poll(cx).is_pending());
        std::task::Poll::Ready(())
    })
    .await;
    let mut changed = channel;
    changed.name = "Latest".into();
    make_repositories(pool)
        .channel
        .upsert(&changed)
        .await
        .unwrap();
    release_tx.send(()).unwrap();
    second.await.unwrap();
    assert_eq!(read(&publisher).items[0].id, "Latest-1");
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_some());
}

#[tokio::test]
async fn abandoned_replacement_cannot_publish_after_old_programme_was_reserved() {
    let (pool, locker, channel, start) = fixture().await;
    let root = tempfile::tempdir().unwrap();
    let publisher = ChannelPublisher::new(pool.clone(), locker, root.path(), channel.id);
    let finish = start + Duration::minutes(3);
    publisher.refresh(start, finish, project).await.unwrap();
    let filename = publisher.folder.join(publication::PLAYOUT_FILENAME);
    let old_bytes = std::fs::read(&filename).unwrap();
    let old_reserved_finish = read(&publisher).items[1].finish;
    // The replacement is longer and has a distinct occurrence identity. A late
    // replacement at the old programme's finish would join it in the middle.
    let repos = make_repositories(pool.clone());
    let mut candidate = repos
        .playout
        .items_in_range(1, start, finish)
        .await
        .unwrap()
        .remove(1);
    sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "Id"=2"#)
        .execute(&pool)
        .await
        .unwrap();
    candidate.id = 3;
    candidate.finish = start + Duration::seconds(150);
    candidate.guide_finish = Some(candidate.finish);
    repos.playout.upsert_item(&candidate).await.unwrap();
    let (entered, waiting) = oneshot::channel();
    let (release, held) = std::sync::mpsc::channel();
    let (finished, completed) = oneshot::channel();
    let pending = publisher.clone();
    let task = tokio::spawn(async move {
        pending
            .refresh_with(start, finish, project, move |folder, playout, active| {
                entered.send(()).unwrap();
                held.recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                let result = publication::publish_playout_checked(
                    folder,
                    playout,
                    || active.load(Ordering::Acquire),
                    None,
                );
                finished.send(()).unwrap();
                result
            })
            .await
    });
    waiting.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(std::fs::read(&filename).unwrap(), old_bytes);
    release.send(()).unwrap();
    completed.await.unwrap();
    let after = read(&publisher);
    assert!(
        after.items.iter().all(|item| {
            !(item.start < old_reserved_finish && old_reserved_finish < item.finish)
        }),
        "a cancelled publication must not expose a replacement already in progress"
    );
    assert_eq!(std::fs::read(&filename).unwrap(), old_bytes);
}
