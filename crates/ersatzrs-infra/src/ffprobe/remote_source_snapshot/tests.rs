use super::*;
use crate::{
    ffprobe::LocalProbeCleanupScope,
    provider_source_storage::{Policy, ProviderSourceStore},
};
use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use futures_util::FutureExt as _;
use std::time::Duration;
use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};

// One existing-style raw TCP response fixture distinguishes protocol EOF from
// a prefix/partial body; its listener and socket are always explicitly joined.
async fn response(
    raw: &str,
    hold_open: bool,
) -> (Response, oneshot::Sender<()>, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let raw = raw.as_bytes().to_vec();
    let (release, released) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut headers = Vec::new();
        while !headers.ends_with(b"\r\n\r\n") {
            assert!(headers.len() < 8192);
            headers.push(socket.read_u8().await.unwrap());
        }
        socket.write_all(&raw).await.unwrap();
        if hold_open {
            let _ = released.await;
        }
    });
    let response = reqwest::Client::builder()
        .no_proxy()
        .build()
        .unwrap()
        .get(format!("http://{address}/synthetic-source"))
        .send()
        .await
        .unwrap();
    (response, release, server)
}

async fn close(release: oneshot::Sender<()>, server: tokio::task::JoinHandle<()>) {
    let _ = release.send(());
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}

fn hold(locker: &EntityLocker) -> TeletextWorkHold {
    let admitted = TeletextWorkGuard::try_acquire(locker, 1).unwrap();
    admitted.hold_work()
}

#[tokio::test]
async fn complete_http_snapshot_requires_real_eof_and_preserves_exact_bytes_and_file_lifetime() {
    for raw in [
        "HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nABCD",
        "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nAB\r\n2\r\nCD\r\n0\r\n\r\n",
    ] {
        let (response, release, server) = response(raw, true).await;
        let locker = EntityLocker::new();
        let scope = LocalProbeCleanupScope::new();
        let file = scope
            .scope(retain_complete_http_source(
                response,
                Instant::now() + Duration::from_secs(3),
                4,
                hold(&locker),
            ))
            .await
            .unwrap();
        assert_eq!(tokio::fs::read(file.path()).await.unwrap(), b"ABCD");
        let path = file.path().to_owned();
        assert_eq!(scope.cleanup_status(), Ok(true));
        assert_eq!(locker.active_teletext_work(), 0);
        drop(file);
        assert!(!path.exists());
        scope.close();
        scope.wait_cleanup().await.unwrap();
        close(release, server).await;
    }
}

#[tokio::test]
async fn partial_truncated_empty_encoded_and_excessive_http_sources_are_not_complete_snapshots() {
    for (raw, maximum, limit) in [
        (
            "HTTP/1.1 206 Partial Content\r\nContent-Length: 4\r\n\r\nABCD",
            4,
            false,
        ),
        (
            "HTTP/1.1 416 Range Not Satisfiable\r\nContent-Length: 0\r\n\r\n",
            4,
            false,
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Range: bytes 0-3/9\r\nContent-Length: 4\r\n\r\nABCD",
            4,
            false,
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: 4\r\n\r\nABCD",
            4,
            false,
        ),
        (
            "HTTP/1.1 200 OK\r\nContent-Encoding: identity\r\nContent-Encoding: gzip\r\nContent-Length: 4\r\n\r\nABCD",
            4,
            false,
        ),
        ("HTTP/1.1 200 OK\r\nContent-Length: 9\r\n\r\nABC", 9, false),
        ("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n", 4, false),
        ("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nABCD", 3, true),
        (
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nABCD\r\n0\r\n\r\n",
            3,
            true,
        ),
        ("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nABCD", 0, true),
    ] {
        let (response, release, server) = response(raw, false).await;
        let locker = EntityLocker::new();
        let scope = LocalProbeCleanupScope::new();
        let error = scope
            .scope(retain_complete_http_source(
                response,
                Instant::now() + Duration::from_secs(3),
                maximum,
                hold(&locker),
            ))
            .await
            .err()
            .unwrap();
        assert_eq!(matches!(error, FfprobeError::OutputTooLarge { .. }), limit);
        assert!(!error.to_string().contains("http://"));
        assert!(!error.to_string().contains("synthetic-source"));
        scope.close();
        scope.wait_cleanup().await.unwrap();
        assert_eq!(locker.active_teletext_work(), 0);
        close(release, server).await;
    }
}

#[tokio::test]
async fn unmanaged_and_closed_scopes_refuse_http_acquisition_without_retained_work() {
    for managed in [false, true] {
        let (response, release, server) =
            response("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nABCD", false).await;
        let locker = EntityLocker::new();
        let scope = LocalProbeCleanupScope::new();
        scope.close();
        let operation = retain_complete_http_source(
            response,
            Instant::now() + Duration::from_secs(3),
            4,
            hold(&locker),
        );
        let result = if managed {
            scope.scope(operation).await
        } else {
            operation.await
        };
        assert!(matches!(result, Err(FfprobeError::Cancelled { .. })));
        assert_eq!(locker.active_teletext_work(), 0);
        scope.wait_cleanup().await.unwrap();
        close(release, server).await;
    }
}

#[tokio::test]
async fn one_absolute_deadline_refuses_incomplete_http_body_and_drains_actual_work() {
    for settled_network in [true, false] {
        let (response, release, server) =
            response("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nA", true).await;
        let locker = EntityLocker::new();
        let scope = LocalProbeCleanupScope::new();
        let deadline = Instant::now() + Duration::from_millis(50);
        if settled_network {
            let owner = scope
                .scope(async { local_cleanup_scope::register_current().unwrap().unwrap() })
                .await;
            let (mut reply, _receive) = oneshot::channel();
            let hold = hold(&locker);
            let result = acquire(
                response,
                4,
                &hold,
                Stop {
                    owner: &owner,
                    reply: &mut reply,
                    deadline,
                },
                None,
            )
            .await;
            assert!(matches!(result, Err(FfprobeError::Timeout { .. })));
            drop(hold);
            owner.complete(true);
        } else {
            let result = scope
                .scope(retain_complete_http_source(
                    response,
                    deadline,
                    4,
                    hold(&locker),
                ))
                .await;
            match result {
                Err(FfprobeError::Timeout { .. }) => {
                    // A completed worker may win the shared deadline race; this
                    // result is valid only after its actual cleanup has joined.
                    assert_eq!(scope.cleanup_status(), Ok(true));
                    assert_eq!(locker.active_teletext_work(), 0);
                }
                Err(FfprobeError::CleanupUnconfirmed { .. }) => {
                    // Caller deadline alone supplies no settlement authority.
                    // The existing scope drain below remains mandatory.
                }
                _ => panic!("an unfinished finite response cannot become a snapshot"),
            }
        }
        scope.close();
        scope.wait_cleanup().await.unwrap();
        assert_eq!(locker.active_teletext_work(), 0);
        close(release, server).await;
    }
}

#[tokio::test]
async fn caller_loss_and_scope_close_cancel_incomplete_http_without_publishing_a_prefix() {
    for close_scope in [false, true] {
        let (response, release, server) =
            response("HTTP/1.1 200 OK\r\nContent-Length: 4\r\n\r\nA", true).await;
        let locker = EntityLocker::new();
        let scope = LocalProbeCleanupScope::new();
        let caller_scope = scope.clone();
        let hold = hold(&locker);
        let caller = tokio::spawn(async move {
            caller_scope
                .scope(retain_complete_http_source(
                    response,
                    Instant::now() + Duration::from_secs(3),
                    4,
                    hold,
                ))
                .await
        });
        tokio::time::timeout(Duration::from_secs(3), async {
            while scope.cleanup_status() != Ok(false) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert_eq!(locker.active_teletext_work(), 1);
        assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
        if close_scope {
            scope.close();
            assert!(matches!(
                caller.await.unwrap(),
                Err(FfprobeError::Cancelled { .. })
            ));
        } else {
            caller.abort();
            assert!(caller.await.unwrap_err().is_cancelled());
            scope.close();
        }
        scope.wait_cleanup().await.unwrap();
        assert_eq!(locker.active_teletext_work(), 0);
        close(release, server).await;
    }
}

#[tokio::test]
async fn actual_blocking_file_operation_keeps_snapshot_and_hold_after_its_awaiter_is_aborted() {
    let locker = EntityLocker::new();
    let hold = hold(&locker);
    let root = tempfile::tempdir().unwrap();
    let storage = ProviderSourceStore::new(root.path().to_owned())
        .reserve(
            Policy {
                total_bytes: 8,
                maximum_source_bytes: 8,
                minimum_free_bytes: 0,
            },
            8,
            8,
            hold.clone(),
        )
        .await
        .unwrap();
    let cleanup = storage.cleanup();
    let file = Arc::new(NamedTempFile::new_in(storage.directory()).unwrap());
    let path = file.path().to_owned();
    let weak = Arc::downgrade(&file);
    let (started, ready) = oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let caller = tokio::spawn(file_operation(file, hold, Some(storage), move |file| {
        started.send(()).unwrap();
        wait.recv().unwrap();
        let mut writer = file.as_file();
        writer.write_all(b"settled")
    }));
    ready.await.unwrap();
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    assert!(path.exists() && weak.upgrade().is_some());
    assert!(cleanup.wait().now_or_never().is_none());
    assert_eq!(locker.active_teletext_work(), 1);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
    release.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(3), async {
        while locker.active_teletext_work() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(weak.upgrade().is_none());
    assert!(!path.exists());
    cleanup.wait().await.unwrap();
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 1);
}
