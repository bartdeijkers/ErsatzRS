//! Dedicated reservation policy, process-shared ledger and actual-owner cleanup.
use super::*;
use crate::repositories::SqliteConfigRepository;
use ersatzrs_core::{
    locking::{EntityLocker, TeletextWorkGuard},
    repositories::ConfigRepository as _,
};

fn policy() -> Policy {
    Policy {
        total_bytes: 100,
        maximum_source_bytes: 80,
        minimum_free_bytes: 0,
    }
}
fn work() -> (EntityLocker, TeletextWorkGuard) {
    let locker = EntityLocker::new();
    let guard = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    (locker, guard)
}
async fn config() -> (crate::db::DbPool, SqliteConfigRepository) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::query(r#"CREATE TABLE "ConfigElement" ("Key" TEXT PRIMARY KEY,"Value" TEXT NOT NULL)"#)
        .execute(&pool)
        .await
        .unwrap();
    (pool.clone(), SqliteConfigRepository(pool))
}
fn receipts(root: &Path) -> Vec<PathBuf> {
    fs::read_dir(root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|value| value == "receipt"))
        .collect()
}
#[tokio::test]
async fn source_storage_defaults_and_exact_configuration_are_independent_from_addon_limits() {
    let (pool, configuration) = config().await;
    configuration
        .set(
            "addons.media_lists.processed_artifacts.global_byte_limit",
            "1",
        )
        .await
        .unwrap();
    assert_eq!(Policy::load(&pool).await.unwrap(), Policy::default());
    assert_eq!(Policy::default().total_bytes, 20 * 1024 * 1024 * 1024);
    assert_eq!(
        Policy::default().maximum_source_bytes,
        8 * 1024 * 1024 * 1024
    );
    assert_eq!(Policy::default().minimum_free_bytes, 2 * 1024 * 1024 * 1024);
    for (key, value) in [
        (TOTAL_BYTES_KEY, "100"),
        (MAXIMUM_SOURCE_BYTES_KEY, "80"),
        (MINIMUM_FREE_BYTES_KEY, "0"),
    ] {
        configuration.set(key, value).await.unwrap();
    }
    assert_eq!(Policy::load(&pool).await.unwrap(), policy());
    for (key, bad) in [
        (TOTAL_BYTES_KEY, "0"),
        (MAXIMUM_SOURCE_BYTES_KEY, "0"),
        (MAXIMUM_SOURCE_BYTES_KEY, "101"),
        (TOTAL_BYTES_KEY, ""),
        (TOTAL_BYTES_KEY, "+100"),
        (TOTAL_BYTES_KEY, " 100"),
        (TOTAL_BYTES_KEY, "-1"),
        (TOTAL_BYTES_KEY, "18446744073709551616"),
        (MINIMUM_FREE_BYTES_KEY, "invalid"),
    ] {
        let old = configuration.get(key).await.unwrap().unwrap();
        configuration.set(key, bad).await.unwrap();
        assert_eq!(
            Policy::load(&pool).await.unwrap_err(),
            StorageError::Policy,
            "{key}"
        );
        configuration.set(key, &old).await.unwrap();
    }
    pool.close().await;
    assert_eq!(
        Policy::load(&pool).await.unwrap_err(),
        StorageError::Unavailable
    );
}
#[test]
fn admission_checks_source_copy_peaks_total_future_writes_and_free_floor_without_overflow() {
    let policy = Policy {
        total_bytes: 20,
        maximum_source_bytes: 8,
        minimum_free_bytes: 2,
    };
    assert_eq!(validate_limits(policy, 8, 16), Ok(()));
    for (source, peak, error) in [
        (0, 1, StorageError::Policy),
        (9, 9, StorageError::Policy),
        (8, 7, StorageError::Policy),
        (8, 21, StorageError::Capacity),
    ] {
        assert_eq!(validate_limits(policy, source, peak), Err(error));
    }
    let retained = Accounting {
        charged: 8,
        physical: 8,
        reservations: 1,
    };
    assert_eq!(
        check_capacity(policy, retained, 8, 10),
        Ok(()),
        "retained bytes already reduced free space"
    );
    assert_eq!(
        check_capacity(
            policy,
            Accounting {
                physical: 0,
                ..retained
            },
            8,
            10
        ),
        Err(StorageError::Capacity),
        "outstanding admitted writes must also fit"
    );
    assert_eq!(
        check_capacity(policy, retained, 8, 9),
        Err(StorageError::Capacity)
    );
    assert_eq!(
        check_capacity(
            policy,
            Accounting {
                charged: 16,
                ..retained
            },
            8,
            u64::MAX
        ),
        Err(StorageError::Capacity)
    );
    assert_eq!(
        check_capacity(
            Policy {
                minimum_free_bytes: u64::MAX,
                ..policy
            },
            Accounting::default(),
            1,
            u64::MAX
        ),
        Err(StorageError::Capacity)
    );
    assert_eq!(
        check_capacity(
            policy,
            Accounting {
                reservations: MAX_RESERVATIONS,
                ..Accounting::default()
            },
            1,
            u64::MAX
        ),
        Err(StorageError::Capacity)
    );
    assert_eq!(
        check_capacity(
            policy,
            Accounting {
                physical: 9,
                ..retained
            },
            1,
            u64::MAX
        ),
        Err(StorageError::Invalid)
    );
}
#[tokio::test]
async fn independent_stores_share_receipts_and_only_last_actual_holder_releases_files_and_charge() {
    let root = tempfile::tempdir().unwrap();
    let first = ProviderSourceStore::new(root.path().to_owned());
    let second = ProviderSourceStore::new(root.path().to_owned());
    let (locker, work) = work();
    let lease = first
        .reserve(policy(), 40, 80, work.hold_work())
        .await
        .unwrap();
    let completion = lease.cleanup();
    let other_completion = completion.clone();
    fs::write(lease.directory().join("source.ts"), [1; 25]).unwrap();
    assert!(matches!(
        second.reserve(policy(), 40, 40, work.hold_work()).await,
        Err(StorageError::Capacity)
    ));
    let next = second
        .reserve(policy(), 20, 20, work.hold_work())
        .await
        .unwrap();
    assert_eq!(receipts(root.path()).len(), 2);
    assert_eq!(accounting(root.path()).unwrap().charged, 100);
    let retained = lease.clone();
    let directory = lease.directory().to_owned();
    drop(lease);
    drop(work);
    assert_eq!(
        locker.active_teletext_work(),
        0,
        "retained AV does not hold preparation capacity"
    );
    assert!(completion.completion.clone().now_or_never().is_none());
    assert_eq!(fs::read(directory.join("source.ts")).unwrap(), [1; 25]);
    drop(retained);
    completion.wait().await.unwrap();
    other_completion.wait().await.unwrap();
    assert!(!directory.exists());
    assert_eq!(accounting(root.path()).unwrap().charged, 20);
    let cleanup = next.cleanup();
    drop(next);
    cleanup.wait().await.unwrap();
    assert!(receipts(root.path()).is_empty());
}
#[tokio::test]
async fn busy_admission_and_malformed_state_are_rejected_without_reclaiming_unlocked_receipts() {
    let root = tempfile::tempdir().unwrap();
    let store = ProviderSourceStore::new(root.path().to_owned());
    let (_, work) = work();
    let lock = admission_lock(root.path()).unwrap();
    assert!(matches!(
        store.reserve(policy(), 1, 1, work.hold_work()).await,
        Err(StorageError::Busy)
    ));
    drop(lock);
    let receipt = Receipt {
        version: 1,
        id: Uuid::new_v4(),
        source_limit: 80,
        charged_bytes: 80,
    };
    write_receipt(root.path(), &receipt).unwrap();
    // A stale/crashed receipt remains charged even without a live lease or file.
    assert!(matches!(
        store.reserve(policy(), 21, 21, work.hold_work()).await,
        Err(StorageError::Capacity)
    ));
    assert_eq!(
        read_receipt(&record_path(root.path(), receipt.id, "receipt"), receipt.id).unwrap(),
        receipt
    );
    fs::write(
        record_path(root.path(), receipt.id, "receipt"),
        br#"{"version":1,"forged":true}"#,
    )
    .unwrap();
    assert!(matches!(
        store.reserve(policy(), 1, 1, work.hold_work()).await,
        Err(StorageError::Invalid)
    ));
    assert_eq!(receipts(root.path()).len(), 1);
}
#[tokio::test]
async fn cleanup_failure_preserves_the_receipt_instead_of_releasing_unconfirmed_charge() {
    let root = tempfile::tempdir().unwrap();
    let store = ProviderSourceStore::new(root.path().to_owned());
    let (_, work) = work();
    let lease = store
        .reserve(policy(), 40, 80, work.hold_work())
        .await
        .unwrap();
    let cleanup = lease.cleanup();
    let receipt_path = receipts(root.path()).pop().unwrap();
    let original = fs::read(&receipt_path).unwrap();
    fs::write(&receipt_path, b"invalid replacement").unwrap();
    let directory = lease.directory().to_owned();
    fs::write(directory.join("source.ts"), b"retained").unwrap();
    drop(lease);
    assert_eq!(cleanup.wait().await, Err(StorageError::Invalid));
    assert_eq!(fs::read(directory.join("source.ts")).unwrap(), b"retained");
    fs::write(&receipt_path, original).unwrap();
    assert_eq!(accounting(root.path()).unwrap().charged, 80);
    assert!(matches!(
        store.reserve(policy(), 21, 21, work.hold_work()).await,
        Err(StorageError::Capacity)
    ));
}
// Unix symlink creation is unprivileged; Windows filesystem type rejection is
// covered by the ordinary-file/root tests without requiring Developer Mode.
#[cfg(unix)]
#[tokio::test]
async fn symlink_root_lock_and_owned_content_never_grant_or_release_storage_authority() {
    use std::os::unix::fs::symlink;
    let root = tempfile::tempdir().unwrap();
    let elsewhere = tempfile::tempdir().unwrap();
    let (_, work) = work();
    let alias = root.path().join("alias");
    symlink(elsewhere.path(), &alias).unwrap();
    assert!(matches!(
        ProviderSourceStore::new(alias)
            .reserve(policy(), 1, 1, work.hold_work())
            .await,
        Err(StorageError::Invalid)
    ));
    let store = ProviderSourceStore::new(root.path().join("store"));
    fs::create_dir(root.path().join("store")).unwrap();
    symlink(
        elsewhere.path().join("outside"),
        root.path().join("store").join(BUDGET_LOCK),
    )
    .unwrap();
    assert!(matches!(
        store.reserve(policy(), 1, 1, work.hold_work()).await,
        Err(StorageError::Invalid)
    ));
    fs::remove_file(root.path().join("store").join(BUDGET_LOCK)).unwrap();
    let lease = store
        .reserve(policy(), 40, 80, work.hold_work())
        .await
        .unwrap();
    let cleanup = lease.cleanup();
    symlink(elsewhere.path(), lease.directory().join("outside")).unwrap();
    drop(lease);
    assert_eq!(cleanup.wait().await, Err(StorageError::Invalid));
    assert_eq!(receipts(&root.path().join("store")).len(), 1);
    assert!(elsewhere.path().exists());
}
#[tokio::test]
async fn non_directory_root_and_oversized_owned_content_are_rejected_on_all_platforms() {
    let root = tempfile::tempdir().unwrap();
    let file = root.path().join("file");
    fs::write(&file, b"not directory").unwrap();
    let (_, work) = work();
    assert!(ProviderSourceStore::new(file)
        .reserve(policy(), 1, 1, work.hold_work())
        .await
        .is_err());
    let store = ProviderSourceStore::new(root.path().join("store"));
    let lease = store
        .reserve(policy(), 1, 2, work.hold_work())
        .await
        .unwrap();
    fs::write(lease.directory().join("too-big"), b"oversized").unwrap();
    assert!(matches!(
        store.reserve(policy(), 1, 1, work.hold_work()).await,
        Err(StorageError::Invalid)
    ));
    let cleanup = lease.cleanup();
    drop(lease);
    cleanup.wait().await.unwrap();
}
#[test]
fn lost_reservation_awaiter_keeps_real_worker_admission_and_cleans_its_completed_reservation() {
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(1)
        .max_blocking_threads(1)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let root = tempfile::tempdir().unwrap();
            let store = ProviderSourceStore::new(root.path().to_owned());
            let (locker, work) = work();
            let (release, blocked) = std::sync::mpsc::channel();
            let (started, ready) = oneshot::channel();
            let occupier = tokio::task::spawn_blocking(move || {
                started.send(()).unwrap();
                blocked.recv().unwrap();
            });
            ready.await.unwrap();
            let mut future = Box::pin(store.reserve(policy(), 40, 80, work.hold_work()));
            assert!(futures_util::poll!(future.as_mut()).is_pending());
            drop(future);
            drop(work);
            assert_eq!(
                locker.active_teletext_work(),
                1,
                "queued actual filesystem worker still owns admission"
            );
            release.send(()).unwrap();
            occupier.await.unwrap();
            tokio::time::timeout(Duration::from_secs(5), async {
                loop {
                    if locker.active_teletext_work() == 0
                        && root.path().join(BUDGET_LOCK).is_file()
                        && receipts(root.path()).is_empty()
                        && fs::read_dir(root.path()).unwrap().count() == 1
                    {
                        break;
                    }
                    tokio::task::yield_now().await;
                }
            })
            .await
            .expect("abandoned reservation worker and independent cleanup must both finish");
        });
}
