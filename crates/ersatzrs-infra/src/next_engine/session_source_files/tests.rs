use super::*;
use crate::provider_source_storage::{Policy, ProviderSourceStore};
use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use std::io::Write as _;
use tokio::io::AsyncReadExt as _;

fn binding(id: i64) -> TransferBinding {
    let start = chrono::Utc::now();
    TransferBinding {
        media_item_id: 1,
        playout_item_id: id,
        media_version_id: 2,
        media_file_id: 3,
        publication: "exact-publication".into(),
        start,
        finish: start + chrono::Duration::seconds(10),
    }
}

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
async fn exact_binding_reuses_one_gate_while_other_occurrences_and_sessions_are_isolated_and_bounded(
) {
    let arena = SessionSourceFiles::default();
    let other = SessionSourceFiles::default();
    let original = binding(1);
    let expires = Instant::now() + Duration::from_secs(10);
    let (id, gate) = arena.gate(copy_binding(&original), expires).unwrap();
    let (again, same) = arena.gate(copy_binding(&original), expires).unwrap();
    assert_eq!(id, again);
    assert!(Arc::ptr_eq(&gate, &same));
    assert!(other.ready(id).is_none());
    let (foreign, foreign_gate) = other.gate(copy_binding(&original), expires).unwrap();
    assert_ne!(foreign, id);
    assert!(!Arc::ptr_eq(&gate, &foreign_gate));
    for number in 2..=8 {
        arena.gate(binding(number), expires).unwrap();
    }
    assert!(matches!(
        arena.gate(binding(9), expires),
        Err(RemoteStreamError::PreparedRequestTooLarge)
    ));
    arena.shutdown().await.unwrap();
    other.shutdown().await.unwrap();
    assert!(arena.gate(original, expires).is_err());
}

#[tokio::test]
async fn installed_source_description_and_ranges_keep_exact_identity_and_refuse_replacement() {
    let arena = SessionSourceFiles::default();
    let original = binding(1);
    let (id, _) = arena
        .gate(
            copy_binding(&original),
            Instant::now() + Duration::from_secs(10),
        )
        .unwrap();
    assert!(arena.ready(id).is_none());
    assert!(arena.describe(id).is_err());
    let (_root, source, cleanup) = source(b"0123456789").await;
    arena.track_cleanup(cleanup.clone()).unwrap();
    arena.install(id, source.clone()).unwrap();
    arena.install(id, source.clone()).unwrap();
    let (length, retained) = arena.describe(id).unwrap();
    assert_eq!(length, 10);
    assert!(same_binding(&original, &retained));
    let mut reader = arena.open(id, 3, 4).await.unwrap();
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, b"3456");
    let (_other_root, replacement, replacement_cleanup) = self::source(b"other").await;
    assert!(arena.install(id, replacement.clone()).is_err());
    drop(replacement);
    replacement_cleanup.wait().await.unwrap();
    drop(source);
    arena.shutdown().await.unwrap();
    cleanup.wait().await.unwrap();
}

#[tokio::test]
async fn expiry_blocks_new_opens_but_existing_reader_retains_bytes_and_storage_until_actual_eof() {
    let arena = SessionSourceFiles::default();
    let (id, _) = arena
        .gate(binding(1), Instant::now() + Duration::from_mins(1))
        .unwrap();
    let expected = vec![61; 256 * 1024];
    let (_root, source, cleanup) = source(&expected).await;
    arena.track_cleanup(cleanup.clone()).unwrap();
    arena.install(id, source.clone()).unwrap();
    let path = source.path().to_owned();
    let mut reader = arena.open(id, 0, source.length).await.unwrap();
    drop(source);
    arena
        .state
        .lock()
        .unwrap()
        .entries
        .get_mut(&id)
        .unwrap()
        .expires = Instant::now();
    assert!(arena.ready(id).is_none());
    assert!(arena.describe(id).is_err());
    assert!(arena.open(id, 0, 1).await.is_err());
    assert!(path.exists() && cleanup.wait().now_or_never().is_none());
    let mut bytes = Vec::new();
    reader.read_to_end(&mut bytes).await.unwrap();
    assert_eq!(bytes, expected);
    cleanup.wait().await.unwrap();
    assert!(!path.exists());
    arena.shutdown().await.unwrap();
}

#[tokio::test]
async fn shutdown_cancels_blocked_reader_and_waits_real_file_and_receipt_cleanup() {
    let arena = SessionSourceFiles::default();
    let (id, _) = arena
        .gate(binding(1), Instant::now() + Duration::from_mins(1))
        .unwrap();
    let (root, source, cleanup) = source(&vec![83; 256 * 1024]).await;
    arena.track_cleanup(cleanup.clone()).unwrap();
    arena.install(id, source.clone()).unwrap();
    let mut reader = arena.open(id, 0, source.length).await.unwrap();
    let path = source.path().to_owned();
    drop(source);
    assert!(std::fs::read_dir(root.path()).unwrap().any(|entry| entry
        .unwrap()
        .path()
        .extension()
        .is_some_and(|extension| extension == "receipt")));
    arena.shutdown().await.unwrap();
    assert!(!path.exists());
    assert!(arena.ready(id).is_none());
    assert!(arena.open(id, 0, 1).await.is_err());
    let mut remaining = Vec::new();
    assert!(reader.read_to_end(&mut remaining).await.is_err());
    cleanup.wait().await.unwrap();
    assert!(!std::fs::read_dir(root.path()).unwrap().any(|entry| entry
        .unwrap()
        .path()
        .extension()
        .is_some_and(|extension| extension == "receipt")));
}

#[tokio::test]
async fn completed_cleanup_witnesses_are_pruned_without_keeping_leases_alive() {
    let arena = SessionSourceFiles::default();
    for _ in 0..=MAX_CLEANUPS {
        let (_root, source, cleanup) = source(b"x").await;
        arena.track_cleanup(cleanup.clone()).unwrap();
        drop(source);
        cleanup.wait().await.unwrap();
    }
    assert!(arena.state.lock().unwrap().cleanups.len() <= 1);
    arena.shutdown().await.unwrap();
    let full = SessionSourceFiles::default();
    let (_root, source, cleanup) = source(b"pending").await;
    for _ in 0..MAX_CLEANUPS {
        full.track_cleanup(cleanup.clone()).unwrap();
    }
    assert!(matches!(
        full.track_cleanup(cleanup.clone()),
        Err(RemoteStreamError::PreparedTerminationUnconfirmed)
    ));
    drop(source);
    cleanup.wait().await.unwrap();
    assert!(matches!(
        full.shutdown().await,
        Err(RemoteStreamError::PreparedTerminationUnconfirmed)
    ));
}

#[tokio::test]
async fn shutdown_joins_preparation_gate_before_snapshotting_its_late_cleanup_witness() {
    let arena = Arc::new(SessionSourceFiles::default());
    let (id, gate) = arena
        .gate(binding(1), Instant::now() + Duration::from_mins(1))
        .unwrap();
    let pending = gate.lock_owned().await;
    let (_root, source, cleanup) = source(b"pending-preparation").await;
    let stopping = arena.clone();
    let shutdown = tokio::spawn(async move { stopping.shutdown().await });
    tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if arena.state.lock().unwrap().closed_at.is_some() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert!(!shutdown.is_finished());
    assert!(arena.ready(id).is_none());
    assert!(matches!(
        arena.track_cleanup(cleanup.clone()),
        Err(RemoteStreamError::PreparedCancelled)
    ));
    drop(source);
    drop(pending);
    shutdown.await.unwrap().unwrap();
    cleanup.wait().await.unwrap();
}
