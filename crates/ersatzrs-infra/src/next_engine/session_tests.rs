use super::*;
use std::sync::Arc;
use tokio::sync::oneshot;

#[tokio::test]
async fn cancellation_before_start_does_not_poll_startup_or_expose_stale_playlist() {
    let root = tempfile::tempdir().unwrap();
    std::fs::write(root.path().join("live.m3u8"), "#EXTM3U\nstale.ts\n").unwrap();
    let session = NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(10),
    );
    assert!(session.trim_playlist().await.is_none());
    session.cancel().await;
    assert_eq!(
        session
            .run(
                async { panic!("cancelled startup must not run") },
                None,
                || async { Ok(()) }
            )
            .await
            .unwrap(),
        SessionExit::Cancelled
    );
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Failed);
    assert!(matches!(
        session
            .run(std::future::pending(), None, || async { Ok(()) })
            .await,
        Err(SessionError::AlreadyStarted)
    ));
}

#[tokio::test]
async fn refresh_does_not_block_heartbeat_or_cancellation() {
    let root = tempfile::tempdir().unwrap();
    let process = super::super::process::fixture_process(root.path(), "ready")
        .await
        .unwrap();
    let session = Arc::new(NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(10),
    ));
    let (entered_tx, entered_rx) = oneshot::channel();
    let mut entered_tx = Some(entered_tx);
    let running = session.clone();
    let task = tokio::spawn(async move {
        running
            .run(async { Ok(process) }, None, move || {
                entered_tx.take().unwrap().send(()).unwrap();
                std::future::pending()
            })
            .await
    });
    session.playout_updated();
    entered_rx.await.unwrap();
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Ready);
    tokio::time::timeout(Duration::from_secs(3), async {
        while !root.path().join(".heartbeat").exists() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    session.cancel().await;
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(3), task)
            .await
            .unwrap()
            .unwrap()
            .unwrap(),
        SessionExit::Cancelled
    );
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Failed);
}

#[tokio::test]
async fn idle_deadline_tracks_recent_demand() {
    let root = tempfile::tempdir().unwrap();
    let process = super::super::process::fixture_process(root.path(), "ready")
        .await
        .unwrap();
    tokio::time::pause();
    let session = Arc::new(NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(30),
    ));
    let (entered_tx, entered_rx) = oneshot::channel();
    let mut entered_tx = Some(entered_tx);
    let running = session.clone();
    let task = tokio::spawn(async move {
        running
            .run(
                async { Ok(process) },
                Some(Duration::from_secs(10)),
                move || {
                    if let Some(tx) = entered_tx.take() {
                        tx.send(()).unwrap();
                    }
                    async { Ok(()) }
                },
            )
            .await
    });
    session.playout_updated();
    entered_rx.await.unwrap();
    tokio::time::advance(Duration::from_secs(8)).await;
    session.touch();
    tokio::time::advance(Duration::from_secs(3)).await;
    assert!(!task.is_finished());
    tokio::time::advance(Duration::from_secs(8)).await;
    tokio::time::resume();
    assert_eq!(task.await.unwrap().unwrap(), SessionExit::Idle);
}

#[tokio::test]
async fn playlist_is_bounded_and_hidden_after_aborting_a_running_session() {
    let root = tempfile::tempdir().unwrap();
    let process = super::super::process::fixture_process(root.path(), "ready")
        .await
        .unwrap();
    let session = Arc::new(NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(30),
    ));
    let (tx, rx) = oneshot::channel();
    let mut tx = Some(tx);
    let running = session.clone();
    let task = tokio::spawn(async move {
        running
            .run(async { Ok(process) }, None, move || {
                tx.take().unwrap().send(()).unwrap();
                async { Ok(()) }
            })
            .await
    });
    session.playout_updated();
    rx.await.unwrap();
    std::fs::write(root.path().join("live.m3u8"), "#EXTM3U\nsegment.ts\n").unwrap();
    assert_eq!(
        session.trim_playlist().await.unwrap(),
        "#EXTM3U\nsegment.ts\n"
    );
    std::fs::write(root.path().join("live.m3u8"), vec![b'x'; 256 * 1024 + 1]).unwrap();
    assert!(session.trim_playlist().await.is_none());
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Failed);
    assert!(session.trim_playlist().await.is_none());
}

#[tokio::test(start_paused = true)]
async fn startup_deadline_is_terminal() {
    let session = NextSession::new(
        "1".into(),
        PathBuf::from("unused-output"),
        Instant::now() + Duration::from_secs(10),
    );
    let result = session
        .run(std::future::pending(), None, || async { Ok(()) })
        .await;
    assert!(matches!(
        result,
        Err(SessionError::Process(ProcessError::StartupTimeout))
    ));
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Failed);
}

#[tokio::test]
async fn unexpected_exit_is_terminal_and_keeps_the_exit_code() {
    let root = tempfile::tempdir().unwrap();
    let process = super::super::process::fixture_process(root.path(), "ready-exit")
        .await
        .unwrap();
    let session = Arc::new(NextSession::new(
        "1".into(),
        root.path().into(),
        Instant::now() + Duration::from_secs(10),
    ));
    let running = session.clone();
    let task = tokio::spawn(async move {
        running
            .run(async { Ok(process) }, None, || async { Ok(()) })
            .await
    });
    std::fs::write(root.path().join("exit-request"), b"").unwrap();
    let result = tokio::time::timeout(Duration::from_secs(3), task)
        .await
        .unwrap()
        .unwrap();
    assert!(
        matches!(result, Err(SessionError::Exit(Some(9)))),
        "{result:?}"
    );
    assert_eq!(session.startup_readiness(), HlsStartupReadiness::Failed);
}
