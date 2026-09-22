use super::*;
use chrono::Duration;
use ersatzrs_core::{locking::PlayoutLockGuard, repositories::RepositoryError};
use tokio::sync::oneshot;

async fn project(
    channel: Channel,
    items: Vec<schedule::ScheduledMedia>,
) -> ersatzrs_core::repositories::Result<Vec<protocol::PlayoutItem>> {
    Ok(items
        .into_iter()
        .map(|media| protocol::PlayoutItem {
            id: format!("{}-{}", channel.name, media.item.id),
            start: media.item.start,
            finish: media.item.finish,
            source: Some(protocol::Source::Local {
                path: media.path,
                in_point_ms: None,
                out_point_ms: None,
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
async fn busy_and_failed_refresh_retain_old_items_then_empty_refresh_withdraws() {
    let (pool, locker, channel, start) = schedule::tests::fixture().await;
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
    let (pool, locker, channel, start) = schedule::tests::fixture().await;
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
    let (pool, locker, channel, start) = schedule::tests::fixture().await;
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
    let (pool, locker, channel, start) = schedule::tests::fixture().await;
    let root = tempfile::tempdir().unwrap();
    let publisher = ChannelPublisher::new(pool.clone(), locker.clone(), root.path(), channel.id);
    let finish = start + Duration::minutes(2);
    let (entered_tx, entered_rx) = oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let first = publisher.clone();
    let task = tokio::spawn(async move {
        first
            .refresh_with(start, finish, project, move |folder, playout| {
                entered_tx.send(()).unwrap();
                release_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .unwrap();
                publication::publish_playout(folder, playout)
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
