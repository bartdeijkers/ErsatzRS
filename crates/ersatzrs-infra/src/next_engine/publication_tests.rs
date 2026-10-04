use std::io::Write;

use chrono::{TimeZone, Utc};

use super::*;
use crate::next_engine::protocol::{PlayoutItem, Source};

fn schedule(id: &str) -> Playout {
    Playout {
        version: PLAYOUT_VERSION.to_owned(),
        items: vec![PlayoutItem {
            watermark: None,
            graphics: Vec::new(),
            id: id.to_owned(),
            start: Utc.with_ymd_and_hms(2026, 9, 22, 12, 0, 0).unwrap(),
            finish: Utc.with_ymd_and_hms(2026, 9, 22, 12, 1, 0).unwrap(),
            source: Some(Source::Lavfi {
                params: "testsrc=size=320x180:rate=25".to_owned(),
                probe_hint: None,
            }),
            tracks: None,
        }],
    }
}

#[test]
fn replacement_and_empty_publication_withdraw_old_items() {
    let temp = tempfile::tempdir().unwrap();
    let folder = temp.path().join("current");
    publish_playout(&folder, &schedule("old")).unwrap();
    publish_playout(&folder, &schedule("new")).unwrap();
    let read = || -> Playout {
        serde_json::from_slice(&std::fs::read(folder.join(PLAYOUT_FILENAME)).unwrap()).unwrap()
    };
    assert_eq!(read().items[0].id, "new");
    publish_playout(
        &folder,
        &Playout {
            version: PLAYOUT_VERSION.to_owned(),
            items: Vec::new(),
        },
    )
    .unwrap();
    assert!(read().items.is_empty());
    assert_eq!(std::fs::read_dir(folder).unwrap().count(), 1);
}

#[test]
fn transient_replacement_failure_retries_the_same_prepared_bytes() {
    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let path = folder.path().join(PLAYOUT_FILENAME);
    let old = std::fs::read(&path).unwrap();
    let prepared = prepare_playout_checked(
        folder.path(),
        &schedule("new"),
        || true,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
    )
    .unwrap();
    let temporary = prepared.pending.path().to_owned();
    let candidate = std::fs::read(&temporary).unwrap();
    let mut attempts = 0;
    let mut waits = Vec::new();
    prepared
        .publish_with(
            || true,
            |file, destination| {
                attempts += 1;
                assert_eq!(file.path(), temporary);
                assert_eq!(std::fs::read(file.path()).unwrap(), candidate);
                assert_eq!(std::fs::read(destination).unwrap(), old);
                if attempts <= 2 {
                    Err(tempfile::PersistError {
                        error: io::ErrorKind::WouldBlock.into(),
                        file,
                    })
                } else {
                    file.persist(destination)
                }
            },
            |delay| waits.push(delay),
        )
        .unwrap();
    assert_eq!(attempts, 3);
    assert_eq!(waits.len(), 2);
    assert!(waits[0] < waits[1]);
    assert_eq!(std::fs::read(path).unwrap(), candidate);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

#[test]
fn replacement_retry_remains_bounded_and_does_not_retry_permanent_errors() {
    for (kind, expected_attempts) in [(io::ErrorKind::WouldBlock, 8), (io::ErrorKind::NotFound, 1)]
    {
        let folder = tempfile::tempdir().unwrap();
        publish_playout(folder.path(), &schedule("old")).unwrap();
        let path = folder.path().join(PLAYOUT_FILENAME);
        let old = std::fs::read(&path).unwrap();
        let prepared = prepare_with(
            folder.path(),
            |file| {
                serde_json::to_writer(file, &schedule("new")).unwrap();
                Ok(())
            },
            || true,
            None,
        )
        .unwrap();
        let mut attempts = 0;
        let mut delays = Vec::new();
        let error = prepared
            .publish_with(
                || true,
                |file, _| {
                    attempts += 1;
                    Err(tempfile::PersistError {
                        error: kind.into(),
                        file,
                    })
                },
                |delay| delays.push(delay),
            )
            .unwrap_err();
        assert!(
            matches!(error, PublicationError::Io { operation: "replace", kind: observed } if observed == kind)
        );
        assert_eq!(attempts, expected_attempts);
        assert_eq!(delays.len(), attempts - 1);
        assert!(delays.iter().sum::<std::time::Duration>() <= std::time::Duration::from_secs(2));
        assert_eq!(std::fs::read(path).unwrap(), old);
        assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    }
}

#[test]
fn cancellation_during_replacement_backoff_cleans_staging_without_late_write() {
    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let path = folder.path().join(PLAYOUT_FILENAME);
    let old = std::fs::read(&path).unwrap();
    let current = std::cell::Cell::new(true);
    let prepared = prepare_playout_checked(
        folder.path(),
        &schedule("new"),
        || true,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
    )
    .unwrap();
    let mut attempts = 0;
    let error = prepared
        .publish_with(
            || current.get(),
            |file, _| {
                attempts += 1;
                Err(tempfile::PersistError {
                    error: io::ErrorKind::Interrupted.into(),
                    file,
                })
            },
            |_| current.set(false),
        )
        .unwrap_err();
    assert!(matches!(error, PublicationError::Cancelled));
    assert_eq!(attempts, 1);
    assert_eq!(std::fs::read(path).unwrap(), old);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

#[test]
fn replacement_backoff_cannot_extend_the_prepared_deadline() {
    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let path = folder.path().join(PLAYOUT_FILENAME);
    let old = std::fs::read(&path).unwrap();
    let prepared = prepare_playout_checked(
        folder.path(),
        &schedule("new"),
        || true,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
    )
    .unwrap();
    // Start the short deadline only after staging, avoiding fsync timing in the oracle.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(50);
    let prepared = PreparedPlayout {
        deadline: Some(deadline),
        ..prepared
    };
    let mut attempts = 0;
    let error = prepared
        .publish_with(
            || true,
            |file, _| {
                attempts += 1;
                Err(tempfile::PersistError {
                    error: io::ErrorKind::WouldBlock.into(),
                    file,
                })
            },
            |delay| {
                assert!(delay <= std::time::Duration::from_millis(25));
                std::thread::sleep(deadline.saturating_duration_since(std::time::Instant::now()));
            },
        )
        .unwrap_err();
    assert!(matches!(error, PublicationError::Expired));
    assert_eq!(attempts, 1);
    assert_eq!(std::fs::read(path).unwrap(), old);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

#[test]
fn replacement_retry_classifies_windows_codes_only_on_windows() {
    for code in [5, 32, 33] {
        assert_eq!(
            retryable_replacement(&io::Error::from_raw_os_error(code)),
            cfg!(windows)
        );
    }
    assert!(!retryable_replacement(&io::ErrorKind::NotFound.into()));
    assert!(!retryable_replacement(
        &io::ErrorKind::PermissionDenied.into()
    ));
}

#[test]
fn interrupted_write_preserves_previous_publication_and_cleans_temporary_file() {
    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let before = std::fs::read(folder.path().join(PLAYOUT_FILENAME)).unwrap();
    let error = replace_with(
        folder.path(),
        |file| {
            file.write_all(b"{\"version\":").unwrap();
            Err(io_error("write", &io::Error::other("private diagnostic")))
        },
        || true,
        None,
    )
    .unwrap_err();
    assert!(!error.to_string().contains("private diagnostic"));
    assert_eq!(
        std::fs::read(folder.path().join(PLAYOUT_FILENAME)).unwrap(),
        before
    );
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

#[test]
fn cancellation_during_serialization_preserves_old_bytes_and_removes_temporary_file() {
    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let filename = folder.path().join(PLAYOUT_FILENAME);
    let old = std::fs::read(&filename).unwrap();
    let active = std::cell::Cell::new(true);
    let error = replace_with(
        folder.path(),
        |file| {
            serde_json::to_writer(file, &schedule("new")).unwrap();
            active.set(false);
            Ok(())
        },
        || active.get(),
        None,
    )
    .unwrap_err();
    assert!(matches!(error, PublicationError::Cancelled));
    assert_eq!(std::fs::read(&filename).unwrap(), old);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    publish_playout(folder.path(), &schedule("retry")).unwrap();
    let retry: Playout = serde_json::from_slice(&std::fs::read(filename).unwrap()).unwrap();
    assert_eq!(retry.items[0].id, "retry");
}

#[test]
fn expired_publication_preserves_previous_file_before_and_after_serialization() {
    use std::time::{Duration, Instant};

    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let filename = folder.path().join(PLAYOUT_FILENAME);
    let old = std::fs::read(&filename).unwrap();
    let error = replace_with(
        folder.path(),
        |_| panic!("expired requests must not start serialization"),
        || true,
        Some(Instant::now()),
    )
    .unwrap_err();
    assert!(matches!(error, PublicationError::Expired));

    let deadline = Instant::now() + Duration::from_millis(100);
    let entered = std::cell::Cell::new(false);
    let error = replace_with(
        folder.path(),
        |file| {
            entered.set(true);
            serde_json::to_writer(file, &schedule("new")).unwrap();
            std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
            Ok(())
        },
        || true,
        Some(deadline),
    )
    .unwrap_err();
    assert!(entered.get());
    assert!(matches!(error, PublicationError::Expired));
    assert_eq!(std::fs::read(filename).unwrap(), old);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

#[test]
fn staged_file_rechecks_deadline_and_ownership_before_replacement() {
    use std::time::{Duration, Instant};

    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let filename = folder.path().join(PLAYOUT_FILENAME);
    let old = std::fs::read(&filename).unwrap();
    let deadline = Instant::now() + Duration::from_millis(100);
    let staged =
        prepare_playout_checked(folder.path(), &schedule("new"), || true, deadline).unwrap();
    assert_eq!(std::fs::read(&filename).unwrap(), old);
    std::thread::sleep(deadline.saturating_duration_since(Instant::now()));
    assert!(matches!(
        staged.publish(|| true),
        Err(PublicationError::Expired)
    ));
    let staged = prepare_playout_checked(
        folder.path(),
        &schedule("new"),
        || true,
        Instant::now() + Duration::from_secs(5),
    )
    .unwrap();
    assert!(matches!(
        staged.publish(|| false),
        Err(PublicationError::Cancelled)
    ));
    assert_eq!(std::fs::read(filename).unwrap(), old);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

#[test]
fn invalid_schedule_cannot_replace_last_complete_generation() {
    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let before = std::fs::read(folder.path().join(PLAYOUT_FILENAME)).unwrap();
    let mut invalid = schedule("invalid");
    invalid.items[0].finish = invalid.items[0].start;
    assert!(matches!(
        publish_playout(folder.path(), &invalid),
        Err(PublicationError::InvalidWindow)
    ));
    invalid = schedule("invalid");
    invalid.version = "unknown schema".to_owned();
    assert!(matches!(
        publish_playout(folder.path(), &invalid),
        Err(PublicationError::UnsupportedVersion)
    ));
    invalid = schedule("invalid");
    let mut earlier = invalid.items[0].clone();
    earlier.start -= chrono::Duration::seconds(30);
    invalid.items.push(earlier);
    assert!(matches!(
        publish_playout(folder.path(), &invalid),
        Err(PublicationError::UnorderedItems)
    ));
    assert_eq!(
        std::fs::read(folder.path().join(PLAYOUT_FILENAME)).unwrap(),
        before
    );
}

#[test]
fn failed_replacement_is_redacted_and_leaves_no_temporary_file() {
    let folder = tempfile::tempdir().unwrap();
    std::fs::create_dir(folder.path().join(PLAYOUT_FILENAME)).unwrap();
    let error = publish_playout(folder.path(), &schedule("new")).unwrap_err();
    assert!(matches!(
        error,
        PublicationError::Io {
            operation: "replace",
            ..
        }
    ));
    assert!(!error
        .to_string()
        .contains(&folder.path().display().to_string()));
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

#[test]
fn filename_bounds_cover_every_accepted_item() {
    let mut playout = schedule("bounds");
    playout.items[0].start = Utc.with_ymd_and_hms(1, 1, 1, 0, 0, 0).unwrap();
    playout.items[0].finish = Utc.with_ymd_and_hms(9999, 12, 31, 23, 59, 59).unwrap();
    assert!(validate(&playout).is_ok());
    playout.items[0].start -= chrono::Duration::nanoseconds(1);
    assert!(matches!(
        validate(&playout),
        Err(PublicationError::InvalidWindow)
    ));
    playout.items[0].start += chrono::Duration::nanoseconds(1);
    playout.items[0].finish += chrono::Duration::nanoseconds(1);
    assert!(matches!(
        validate(&playout),
        Err(PublicationError::InvalidWindow)
    ));
}

#[test]
fn simultaneous_readers_observe_only_complete_generations() {
    use std::sync::Barrier;

    let folder = tempfile::tempdir().unwrap();
    let mut old = schedule("old");
    old.items = vec![old.items[0].clone(); 256];
    let mut new = old.clone();
    for item in &mut new.items {
        item.id = "new".to_owned();
    }
    publish_playout(folder.path(), &old).unwrap();
    let barrier = Barrier::new(2);
    std::thread::scope(|scope| {
        let reader = scope.spawn(|| {
            let mut reads = Vec::new();
            for _ in 0..32 {
                barrier.wait();
                reads.push(std::fs::read(folder.path().join(PLAYOUT_FILENAME)));
                barrier.wait();
            }
            reads
        });
        let mut writes = Vec::new();
        for i in 0..32 {
            barrier.wait();
            writes.push(publish_playout(
                folder.path(),
                if i % 2 == 0 { &new } else { &old },
            ));
            barrier.wait();
        }
        // Assert only after both sides leave the barriers: a failure must report
        // the broken invariant rather than deadlocking its companion thread.
        for write in writes {
            write.unwrap();
        }
        for bytes in reader.join().unwrap() {
            let read: Playout = serde_json::from_slice(&bytes.unwrap()).unwrap();
            assert_eq!(read.items.len(), 256);
            let id = &read.items[0].id;
            assert!(id == "old" || id == "new");
            assert!(read.items.iter().all(|item| &item.id == id));
        }
    });
}

// Windows sharing flags can forbid replacement even when the file is writable.
// Unix keeps open inode handles valid across rename; the portable reader test
// above covers its counterpart without imposing Windows sharing semantics.
#[cfg(windows)]
#[test]
fn windows_sharing_denial_preserves_generation_until_reader_releases_file() {
    use std::os::windows::fs::OpenOptionsExt;

    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join(PLAYOUT_FILENAME);
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let previous = std::fs::read(&path).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1) // FILE_SHARE_READ, deliberately excludes FILE_SHARE_DELETE.
        .open(&path)
        .unwrap();
    let error = publish_playout(folder.path(), &schedule("new")).unwrap_err();
    assert!(matches!(
        error,
        PublicationError::Io {
            operation: "replace",
            ..
        }
    ));
    assert_eq!(std::fs::read(&path).unwrap(), previous);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
    drop(held);
    publish_playout(folder.path(), &schedule("new")).unwrap();
    let read: Playout = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
    assert_eq!(read.items[0].id, "new");
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}

// This real sharing-handle test complements the portable injected-I/O tests;
// Unix's open-handle counterpart is simultaneous_readers_observe_only_complete_generations.
#[cfg(windows)]
#[test]
fn windows_transient_sharing_denial_recovers_without_rebuilding_the_candidate() {
    use std::os::windows::fs::OpenOptionsExt;

    let folder = tempfile::tempdir().unwrap();
    let path = folder.path().join(PLAYOUT_FILENAME);
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let held = std::fs::OpenOptions::new()
        .read(true)
        .share_mode(1)
        .open(&path)
        .unwrap();
    let prepared = prepare_playout_checked(
        folder.path(),
        &schedule("new"),
        || true,
        std::time::Instant::now() + std::time::Duration::from_secs(5),
    )
    .unwrap();
    let candidate = std::fs::read(prepared.pending.path()).unwrap();
    let mut held = Some(held);
    let mut waits = 0;
    prepared
        .publish_with(
            || true,
            |file, destination| file.persist(destination),
            |_| {
                waits += 1;
                drop(held.take());
            },
        )
        .unwrap();
    assert_eq!(waits, 1);
    assert_eq!(std::fs::read(path).unwrap(), candidate);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 1);
}
