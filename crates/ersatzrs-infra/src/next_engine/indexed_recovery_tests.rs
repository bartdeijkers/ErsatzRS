use super::super::{
    planner,
    tests::{at, chunks, item, publish, read_index, schedule, stamp, utc},
    READER_LOCK_FILENAME,
};
use super::*;
use chrono::Duration;
use std::fs::File;

#[test]
fn recovery_replaces_stranded_malformed_and_unknown_indexes_from_the_trusted_projection() {
    for old_format in ["candidate", "malformed", "unknown"] {
        let root = tempfile::tempdir().unwrap();
        let now = at("2026-11-06T12:00:00Z");
        let accepted = schedule(vec![item("accepted", now, Duration::hours(1))]);
        publish(root.path(), &accepted, &utc(), now);
        let old = read_index(root.path());
        let modified = stamp(&root.path().join(&old.days[0].file));
        let stranded = schedule(vec![item("uncommitted", now, Duration::hours(1))]);
        publish(root.path(), &stranded, &utc(), now);
        let backup = root.path().join(".next-rollback-Stranded");
        std::fs::write(&backup, serde_json::to_vec(&old).unwrap()).unwrap();
        match old_format {
            "malformed" => std::fs::write(root.path().join(INDEX_FILENAME), b"{broken").unwrap(),
            "unknown" => {
                let mut future = old.clone();
                future.version = "unknown-version".into();
                std::fs::write(
                    root.path().join(INDEX_FILENAME),
                    serde_json::to_vec(&future).unwrap(),
                )
                .unwrap();
            }
            _ => {}
        }
        let foreign = root.path().join("foreign.json");
        std::fs::write(&foreign, b"preserve").unwrap();
        recover(root.path(), &accepted, &utc(), now, &|| true, None, || {}).unwrap();
        assert_eq!(read_index(root.path()), old);
        assert_eq!(
            root.path()
                .join(&old.days[0].file)
                .metadata()
                .unwrap()
                .modified()
                .unwrap(),
            modified
        );
        assert!(!backup.exists());
        assert_eq!(chunks(root.path()), 1);
        assert_eq!(std::fs::read(foreign).unwrap(), b"preserve");
    }
}

#[test]
fn recovery_repairs_missing_and_corrupt_chunks_without_trusting_the_old_index() {
    for missing in [true, false] {
        let root = tempfile::tempdir().unwrap();
        let now = at("2026-11-06T12:00:00Z");
        let accepted = schedule(vec![item("accepted", now, Duration::hours(1))]);
        publish(root.path(), &accepted, &utc(), now);
        let index = read_index(root.path());
        let path = root.path().join(&index.days[0].file);
        let expected = std::fs::read(&path).unwrap();
        if missing {
            std::fs::remove_file(&path).unwrap();
        } else {
            std::fs::write(&path, b"corrupt payload").unwrap();
        }
        recover(root.path(), &accepted, &utc(), now, &|| true, None, || {}).unwrap();
        assert_eq!(std::fs::read(path).unwrap(), expected);
        assert_eq!(read_index(root.path()), index);
    }
}

#[test]
fn recovery_cannot_mutate_a_readers_snapshot_and_succeeds_after_lease_release() {
    let root = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let old = schedule(vec![item("old", now, Duration::hours(1))]);
    publish(root.path(), &old, &utc(), now);
    let index = read_index(root.path());
    let reader = File::open(root.path().join(READER_LOCK_FILENAME)).unwrap();
    reader.try_lock_shared().unwrap();
    let accepted = schedule(vec![item("accepted", now, Duration::hours(1))]);
    assert!(matches!(
        recover(root.path(), &accepted, &utc(), now, &|| true, None, || {}),
        Err(PublicationError::ReadersBusy)
    ));
    assert_eq!(read_index(root.path()), index);
    assert!(root.path().join(&index.days[0].file).exists());
    drop(reader);
    recover(root.path(), &accepted, &utc(), now, &|| true, None, || {}).unwrap();
    assert_ne!(read_index(root.path()), index);
}

#[test]
fn cancellation_before_index_replacement_keeps_rollback_evidence_and_discards_new_chunks() {
    let root = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let old = schedule(vec![item("old", now, Duration::hours(1))]);
    publish(root.path(), &old, &utc(), now);
    let before = std::fs::read(root.path().join(INDEX_FILENAME)).unwrap();
    let backup = root.path().join(".next-rollback-Stranded");
    std::fs::write(&backup, &before).unwrap();
    let accepted = schedule(
        (0..2)
            .map(|day| {
                item(
                    &format!("new-{day}"),
                    now + Duration::days(day),
                    Duration::hours(1),
                )
            })
            .collect(),
    );
    let plan = planner::plan(&accepted, None, &utc(), now, &|| true, None)
        .unwrap()
        .unwrap();
    let first = root.path().join(&plan.chunks[0].0.file);
    let is_current = || !first.exists();
    assert!(matches!(
        recover(
            root.path(),
            &accepted,
            &utc(),
            now,
            &is_current,
            None,
            || {}
        ),
        Err(PublicationError::Cancelled)
    ));
    assert!(!first.exists());
    assert_eq!(
        std::fs::read(root.path().join(INDEX_FILENAME)).unwrap(),
        before
    );
    assert_eq!(std::fs::read(&backup).unwrap(), before);
    assert_eq!(chunks(root.path()), 1);
    assert!(matches!(
        recover(
            root.path(),
            &accepted,
            &utc(),
            now,
            &|| true,
            Some(Instant::now()),
            || {}
        ),
        Err(PublicationError::Expired)
    ));
    assert!(backup.exists());
    recover(root.path(), &accepted, &utc(), now, &|| true, None, || {}).unwrap();
    assert!(!backup.exists());
}

#[test]
fn cleanup_failure_keeps_published_chunks_and_retry_reuses_them() {
    let root = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let old = schedule(vec![item("old", now, Duration::hours(1))]);
    publish(root.path(), &old, &utc(), now);
    let backup = root.path().join(".next-rollback-Stranded");
    std::fs::write(
        &backup,
        serde_json::to_vec(&read_index(root.path())).unwrap(),
    )
    .unwrap();
    let accepted = schedule(vec![item("accepted", now, Duration::hours(1))]);
    let plan = planner::plan(&accepted, None, &utc(), now, &|| true, None)
        .unwrap()
        .unwrap();
    let expected = serde_json::to_vec(&plan.index).unwrap();
    let injected = std::cell::Cell::new(false);
    let is_current = || {
        if !injected.get()
            && std::fs::read(root.path().join(INDEX_FILENAME)).is_ok_and(|bytes| bytes == expected)
        {
            std::fs::remove_file(&backup).unwrap();
            std::fs::create_dir(&backup).unwrap();
            injected.set(true);
        }
        true
    };
    assert!(recover(
        root.path(),
        &accepted,
        &utc(),
        now,
        &is_current,
        None,
        || {}
    )
    .is_err());
    assert!(injected.get());
    assert_eq!(
        std::fs::read(root.path().join(INDEX_FILENAME)).unwrap(),
        expected
    );
    let chunk = root.path().join(&plan.index.days[0].file);
    assert!(
        chunk.exists(),
        "post-publication cleanup failure must not delete referenced chunks"
    );
    let modified = stamp(&chunk);
    std::fs::remove_dir(&backup).unwrap();
    recover(root.path(), &accepted, &utc(), now, &|| true, None, || {}).unwrap();
    assert_eq!(chunk.metadata().unwrap().modified().unwrap(), modified);
    assert_eq!(chunks(root.path()), 1);
}

#[test]
fn recovery_rejects_unsafe_owned_objects_and_budget_overflow_before_publication() {
    let root = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let accepted = schedule(vec![item("accepted", now, Duration::hours(1))]);
    publish(root.path(), &accepted, &utc(), now);
    let before = std::fs::read(root.path().join(INDEX_FILENAME)).unwrap();
    let foreign_object = root.path().join(".next-rollback-Foreign");
    std::fs::create_dir(&foreign_object).unwrap();
    assert!(matches!(
        recover(root.path(), &accepted, &utc(), now, &|| true, None, || {}),
        Err(PublicationError::InvalidIndex)
    ));
    assert_eq!(
        std::fs::read(root.path().join(INDEX_FILENAME)).unwrap(),
        before
    );
    assert!(foreign_object.is_dir());
    for (footprint, additional) in [
        (MAX_RECOVERY_BYTES, 0),
        (u64::MAX, 1),
        (MAX_RECOVERY_BYTES - MAX_INDEX_BYTES as u64, 1),
    ] {
        assert!(matches!(
            peak_bytes(footprint, additional),
            Err(PublicationError::Limit)
        ));
    }
    assert_eq!(
        peak_bytes(0, MAX_TOTAL_BYTES).unwrap(),
        MAX_TOTAL_BYTES as u64
    );
}
