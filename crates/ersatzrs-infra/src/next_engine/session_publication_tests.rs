use super::*;
use std::{sync::Arc, task::Poll};
use tokio::sync::{mpsc, oneshot};

#[tokio::test]
async fn acknowledgement_requires_its_generation_and_reports_failure_or_session_end() {
    let root = tempfile::tempdir().unwrap();
    let process = super::super::process::fixture_process(root.path(), "ready")
        .await
        .unwrap();
    let session = Arc::new(NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(30),
    ));
    let (entered, mut attempts) = mpsc::unbounded_channel();
    let running = session.clone();
    let task = tokio::spawn(async move {
        running
            .run(async { Ok(process) }, None, move |_| {
                let (release, wait) = oneshot::channel();
                entered.send(release).unwrap();
                async { wait.await.unwrap() }
            })
            .await
    });
    let deadline = (Instant::now() + Duration::from_secs(20)).into_std();
    let first = session.publish_playout_update(deadline);
    tokio::pin!(first);
    std::future::poll_fn(|cx| {
        assert!(first.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let release_first = attempts.recv().await.unwrap();
    let second = session.publish_playout_update(deadline);
    tokio::pin!(second);
    std::future::poll_fn(|cx| {
        assert!(second.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    release_first.send(Ok(())).unwrap();
    first.await.unwrap();
    std::future::poll_fn(|cx| {
        assert!(
            second.as_mut().poll(cx).is_pending(),
            "older publication cannot acknowledge a newer request"
        );
        Poll::Ready(())
    })
    .await;
    attempts
        .recv()
        .await
        .unwrap()
        .send(Err(RefreshError::Projection))
        .unwrap();
    assert_eq!(second.await, Err(HlsPublicationError::Failed));
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Ready);
    let third = session.publish_playout_update(deadline);
    tokio::pin!(third);
    std::future::poll_fn(|cx| {
        assert!(third.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    session.cancel().await;
    assert_eq!(third.await, Err(HlsPublicationError::SessionEnded));
    assert_eq!(task.await.unwrap().unwrap(), SessionExit::Cancelled);
}

#[tokio::test]
async fn publication_deadline_cancels_refresh_but_retains_running_producer() {
    let root = tempfile::tempdir().unwrap();
    let process = super::super::process::fixture_process(root.path(), "ready")
        .await
        .unwrap();
    let session = Arc::new(NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(30),
    ));
    let (entered, received) = oneshot::channel();
    let mut entered = Some(entered);
    let running = session.clone();
    let task = tokio::spawn(async move {
        running
            .run(async { Ok(process) }, None, move |deadline| {
                let (held, cancelled) = oneshot::channel::<()>();
                entered.take().unwrap().send((deadline, cancelled)).unwrap();
                async move {
                    let _held = held;
                    std::future::pending().await
                }
            })
            .await
    });
    tokio::time::pause();
    let deadline = (Instant::now() + Duration::from_secs(2)).into_std();
    let wait = session.publish_playout_update(deadline);
    tokio::pin!(wait);
    std::future::poll_fn(|cx| {
        assert!(wait.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let (forwarded, cancelled) = received.await.unwrap();
    assert_eq!(forwarded, deadline);
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(wait.await, Err(HlsPublicationError::TimedOut));
    assert!(
        cancelled.await.is_err(),
        "expired refresh must relinquish publication ownership"
    );
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Ready);
    assert!(!task.is_finished());
    session.cancel().await;
    tokio::time::resume();
    assert_eq!(task.await.unwrap().unwrap(), SessionExit::Cancelled);
}

#[tokio::test]
async fn coalesced_requests_keep_earliest_deadline_and_fresh_request_can_recover() {
    let root = tempfile::tempdir().unwrap();
    let process = super::super::process::fixture_process(root.path(), "ready")
        .await
        .unwrap();
    let session = Arc::new(NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(30),
    ));
    tokio::time::pause();
    let earliest = (Instant::now() + Duration::from_secs(2)).into_std();
    let later = (Instant::now() + Duration::from_secs(20)).into_std();
    let first = session.publish_playout_update(earliest);
    let second = session.publish_playout_update(later);
    tokio::pin!(first, second);
    std::future::poll_fn(|cx| {
        assert!(first.as_mut().poll(cx).is_pending());
        assert!(second.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    session.playout_updated();
    let (entered, mut attempts) = mpsc::unbounded_channel();
    let running = session.clone();
    let task = tokio::spawn(async move {
        running
            .run(async { Ok(process) }, None, move |deadline| {
                let (release, wait) = oneshot::channel();
                entered.send((deadline, release)).unwrap();
                async { wait.await.unwrap() }
            })
            .await
    });
    let (forwarded, abandoned) = attempts.recv().await.unwrap();
    assert_eq!(forwarded, earliest);
    tokio::time::advance(Duration::from_secs(2)).await;
    assert_eq!(first.await, Err(HlsPublicationError::TimedOut));
    assert_eq!(second.await, Err(HlsPublicationError::Failed));
    assert!(abandoned.send(Ok(())).is_err());
    tokio::time::advance(Duration::from_secs(3)).await;
    tokio::task::yield_now().await;
    assert!(
        attempts.try_recv().is_err(),
        "expired generation cannot retry"
    );
    let fresh = session.publish_playout_update(later);
    tokio::pin!(fresh);
    std::future::poll_fn(|cx| {
        assert!(fresh.as_mut().poll(cx).is_pending());
        Poll::Ready(())
    })
    .await;
    let (forwarded, release) = attempts.recv().await.unwrap();
    assert_eq!(forwarded, later);
    release.send(Ok(())).unwrap();
    fresh.await.unwrap();
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Ready);
    session.cancel().await;
    tokio::time::resume();
    assert_eq!(task.await.unwrap().unwrap(), SessionExit::Cancelled);
}
