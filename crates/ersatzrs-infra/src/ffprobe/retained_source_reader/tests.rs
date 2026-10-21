use super::*;
use crate::{
    ffprobe::LocalProbeCleanupScope,
    provider_source_storage::{Policy, ProviderSourceCleanup, ProviderSourceStore},
};
use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use std::{io::Write as _, time::Duration};
use tokio::io::AsyncReadExt as _;

async fn source(
    bytes: &[u8],
) -> (
    tempfile::TempDir,
    Arc<RetainedProviderSource>,
    ProviderSourceCleanup,
) {
    let root = tempfile::tempdir().unwrap();
    let limit = u64::try_from(bytes.len().max(1)).unwrap();
    let policy = Policy {
        total_bytes: limit * 2,
        maximum_source_bytes: limit,
        minimum_free_bytes: 0,
    };
    let locker = EntityLocker::default();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let storage = ProviderSourceStore::new(root.path().to_owned())
        .reserve(policy, limit, limit, guard.hold_work())
        .await
        .unwrap();
    let cleanup = storage.cleanup();
    let mut file = tempfile::NamedTempFile::new_in(storage.directory()).unwrap();
    file.write_all(bytes).unwrap();
    file.flush().unwrap();
    let source = Arc::new(RetainedProviderSource {
        file: Arc::new(file),
        storage,
        length: u64::try_from(bytes.len()).unwrap(),
        digest: [0; 32],
    });
    (root, source, cleanup)
}

#[tokio::test]
async fn retained_ranges_reopen_independent_cursors_and_report_exact_bytes_and_eof() {
    let (_root, source, cleanup) = source(b"0123456789").await;
    let scope = LocalProbeCleanupScope::new();
    let mut first = scope
        .scope(open_retained_source_reader(source.clone(), 2, 5))
        .await
        .unwrap();
    let mut second = scope
        .scope(open_retained_source_reader(source.clone(), 1, 3))
        .await
        .unwrap();
    let mut a = Vec::new();
    let mut b = Vec::new();
    first.read_to_end(&mut a).await.unwrap();
    second.read_to_end(&mut b).await.unwrap();
    assert_eq!(a, b"23456");
    assert_eq!(b, b"123");
    let mut empty = scope
        .scope(open_retained_source_reader(source.clone(), 10, 0))
        .await
        .unwrap();
    assert_eq!(
        empty.read_u8().await.unwrap_err().kind(),
        io::ErrorKind::UnexpectedEof
    );
    drop(source);
    scope.close();
    scope.wait_cleanup().await.unwrap();
    cleanup.wait().await.unwrap();
}

#[tokio::test]
async fn retained_reader_refuses_unmanaged_closed_and_out_of_bounds_reads() {
    let (_root, source, cleanup) = source(b"exact").await;
    assert!(matches!(
        open_retained_source_reader(source.clone(), 0, 5).await,
        Err(FfprobeError::Cancelled { .. })
    ));
    let scope = LocalProbeCleanupScope::new();
    for (offset, length) in [(u64::MAX, 1), (5, 1), (0, 6)] {
        assert!(matches!(
            scope
                .scope(open_retained_source_reader(source.clone(), offset, length))
                .await,
            Err(FfprobeError::InvalidOutput { .. })
        ));
    }
    scope.close();
    assert!(matches!(
        scope
            .scope(open_retained_source_reader(source.clone(), 0, 5))
            .await,
        Err(FfprobeError::Cancelled { .. })
    ));
    drop(source);
    scope.wait_cleanup().await.unwrap();
    cleanup.wait().await.unwrap();
}

#[tokio::test]
async fn reader_body_drop_and_scope_close_join_actual_file_owners_before_storage_cleanup() {
    for close_scope in [false, true] {
        let (_root, source, cleanup) = source(&vec![42; BUFFER_BYTES * 4]).await;
        let path = source.path().to_owned();
        let weak = Arc::downgrade(&source);
        let scope = LocalProbeCleanupScope::new();
        let reader = scope
            .scope(open_retained_source_reader(
                source.clone(),
                0,
                source.length,
            ))
            .await
            .unwrap();
        drop(source);
        assert!(weak.upgrade().is_some() && path.exists());
        assert_eq!(scope.cleanup_status(), Ok(false));
        assert!(cleanup.wait().now_or_never().is_none());
        if close_scope {
            scope.close();
        } else {
            drop(reader);
        }
        scope.close();
        tokio::time::timeout(Duration::from_secs(3), scope.wait_cleanup())
            .await
            .unwrap()
            .unwrap();
        cleanup.wait().await.unwrap();
        assert!(weak.upgrade().is_none());
        assert!(!path.exists());
    }
}

#[tokio::test]
async fn aborted_response_owner_cancels_the_detached_reader_and_releases_its_actual_charge() {
    let (_root, source, cleanup) = source(&vec![7; BUFFER_BYTES * 4]).await;
    let scope = LocalProbeCleanupScope::new();
    let caller_scope = scope.clone();
    let (started, ready) = oneshot::channel();
    let caller = tokio::spawn(async move {
        let reader = caller_scope
            .scope(open_retained_source_reader(
                source.clone(),
                0,
                source.length,
            ))
            .await
            .unwrap();
        drop(source);
        started.send(()).unwrap();
        std::future::pending::<()>().await;
        drop(reader);
    });
    ready.await.unwrap();
    assert_eq!(scope.cleanup_status(), Ok(false));
    caller.abort();
    assert!(caller.await.unwrap_err().is_cancelled());
    scope.close();
    scope.wait_cleanup().await.unwrap();
    cleanup.wait().await.unwrap();
}

#[tokio::test]
async fn actual_range_read_failure_is_an_io_error_instead_of_successful_truncated_eof() {
    let (_root, source, cleanup) = source(&vec![9; BUFFER_BYTES * 4]).await;
    let scope = LocalProbeCleanupScope::new();
    let mut reader = scope
        .scope(open_retained_source_reader(
            source.clone(),
            0,
            source.length,
        ))
        .await
        .unwrap();
    // The bounded duplex permits at most two chunks to be prefetched. Truncate
    // behind that boundary, forcing the actual later job to encounter EOF.
    source.file.as_file().set_len(1).unwrap();
    let mut bytes = Vec::new();
    assert!(reader.read_to_end(&mut bytes).await.is_err());
    assert!(bytes.len() < BUFFER_BYTES * 4);
    drop(reader);
    drop(source);
    scope.close();
    scope.wait_cleanup().await.unwrap();
    cleanup.wait().await.unwrap();
}
