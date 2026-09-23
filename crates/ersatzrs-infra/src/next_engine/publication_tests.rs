use std::io::Write;

use chrono::{TimeZone, Utc};

use super::*;
use crate::next_engine::protocol::{PlayoutItem, Source};

fn schedule(id: &str) -> Playout {
    Playout {
        version: PLAYOUT_VERSION.to_owned(),
        items: vec![PlayoutItem {
            watermark: None,
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
fn interrupted_write_preserves_previous_publication_and_cleans_temporary_file() {
    let folder = tempfile::tempdir().unwrap();
    publish_playout(folder.path(), &schedule("old")).unwrap();
    let before = std::fs::read(folder.path().join(PLAYOUT_FILENAME)).unwrap();
    let error = replace_with(folder.path(), |file| {
        file.write_all(b"{\"version\":").unwrap();
        Err(io_error("write", &io::Error::other("private diagnostic")))
    })
    .unwrap_err();
    assert!(!error.to_string().contains("private diagnostic"));
    assert_eq!(
        std::fs::read(folder.path().join(PLAYOUT_FILENAME)).unwrap(),
        before
    );
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
