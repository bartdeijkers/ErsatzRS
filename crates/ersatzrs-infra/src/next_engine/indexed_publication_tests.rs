use std::{
    fs::File,
    time::{Duration as WallDuration, SystemTime},
};

use chrono::Duration;

use super::*;
use crate::next_engine::protocol::{PlayoutItem, Source, Track, Tracks, PLAYOUT_VERSION};

pub(super) fn at(value: &str) -> DateTime<Utc> {
    value.parse().unwrap()
}

pub(super) fn item(id: &str, start: DateTime<Utc>, duration: Duration) -> PlayoutItem {
    PlayoutItem {
        id: id.into(),
        start,
        finish: start + duration,
        source: Some(Source::Local {
            path: "synthetic/selected.mkv".into(),
            in_point_ms: Some(123),
            out_point_ms: Some(456),
            probe_hint: None,
        }),
        tracks: Some(Tracks {
            audio: Some(Track {
                source: None,
                stream_index: Some(2),
            }),
            subtitle: Some(Track {
                source: None,
                stream_index: Some(9),
            }),
            ..Default::default()
        }),
        watermark: None,
        graphics: Vec::new(),
    }
}

pub(super) fn schedule(items: Vec<PlayoutItem>) -> Playout {
    Playout {
        version: PLAYOUT_VERSION.into(),
        items,
    }
}
pub(super) fn utc() -> ProgrammeClock {
    ProgrammeClock::named("UTC").unwrap()
}
pub(super) fn publish(
    folder: &Path,
    playout: &Playout,
    clock: &ProgrammeClock,
    now: DateTime<Utc>,
) {
    if !folder.join(INDEX_FILENAME).exists() {
        recover(folder, playout, clock, now, &|| true, None, || {}).unwrap();
        return;
    }
    prepare(folder, playout, clock, now, &|| true, None)
        .unwrap()
        .publish(|| true)
        .unwrap();
}
pub(super) fn read_index(folder: &Path) -> PlayoutIndex {
    serde_json::from_slice(&std::fs::read(folder.join(INDEX_FILENAME)).unwrap()).unwrap()
}
pub(super) fn stamp(path: &Path) -> SystemTime {
    std::fs::OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new()
                .set_modified(SystemTime::UNIX_EPOCH + WallDuration::from_secs(1_600_000_000)),
        )
        .unwrap();
    path.metadata().unwrap().modified().unwrap()
}
pub(super) fn chunks(folder: &Path) -> usize {
    std::fs::read_dir(folder)
        .unwrap()
        .map(Result::unwrap)
        .filter(|entry| entry.file_name().to_string_lossy().starts_with("day-"))
        .count()
}

#[test]
fn unchanged_days_and_index_keep_metadata_while_active_or_future_days_change() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let mut playout = schedule(
        (0..3)
            .map(|day| {
                item(
                    &format!("item-{day}"),
                    now + Duration::days(day),
                    Duration::hours(1),
                )
            })
            .collect(),
    );
    publish(folder.path(), &playout, &utc(), now);
    let old = read_index(folder.path());
    let index_time = stamp(&folder.path().join(INDEX_FILENAME));
    let times = old
        .days
        .iter()
        .map(|day| stamp(&folder.path().join(&day.file)))
        .collect::<Vec<_>>();
    publish(folder.path(), &playout, &utc(), now);
    assert_eq!(
        folder
            .path()
            .join(INDEX_FILENAME)
            .metadata()
            .unwrap()
            .modified()
            .unwrap(),
        index_time
    );
    for changed in [2, 0] {
        playout.items[changed].id.push_str("-edited");
        publish(folder.path(), &playout, &utc(), now);
        let updated = read_index(folder.path());
        assert_ne!(updated.days[changed].file, old.days[changed].file);
        for (index, time) in times.iter().enumerate() {
            if index != 0 && index != 2 || (changed == 2 && index == 0) {
                assert_eq!(updated.days[index], old.days[index]);
                assert_eq!(
                    folder
                        .path()
                        .join(&updated.days[index].file)
                        .metadata()
                        .unwrap()
                        .modified()
                        .unwrap(),
                    *time
                );
            }
        }
    }
    let updated = read_index(folder.path());
    let metadata = stamp(&folder.path().join(INDEX_FILENAME));
    publish(folder.path(), &playout, &utc(), now);
    assert_eq!(read_index(folder.path()), updated);
    assert_eq!(
        folder
            .path()
            .join(INDEX_FILENAME)
            .metadata()
            .unwrap()
            .modified()
            .unwrap(),
        metadata
    );
    assert_eq!(
        chunks(folder.path()),
        3,
        "obsolete chunks reclaimed without rewriting live days"
    );
}

#[test]
fn finished_only_trimming_is_a_noop_but_empty_input_withdraws_and_reclaims() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let mut playout = schedule(vec![
        item("finished", now - Duration::minutes(7), Duration::minutes(1)),
        item("live", now, Duration::hours(1)),
    ]);
    publish(folder.path(), &playout, &utc(), now - Duration::minutes(10));
    let old = read_index(folder.path());
    let metadata = stamp(&folder.path().join(INDEX_FILENAME));
    playout.items.remove(0);
    publish(folder.path(), &playout, &utc(), now);
    assert_eq!(read_index(folder.path()), old);
    assert_eq!(
        folder
            .path()
            .join(INDEX_FILENAME)
            .metadata()
            .unwrap()
            .modified()
            .unwrap(),
        metadata
    );
    let empty = schedule(Vec::new());
    publish(folder.path(), &empty, &utc(), now);
    assert!(read_index(folder.path()).days.is_empty());
    publish(folder.path(), &empty, &utc(), now);
    assert_eq!(chunks(folder.path()), 0);
}

#[test]
fn civil_dst_days_and_cross_midnight_duplicates_preserve_full_items_and_tracks() {
    let clock = ProgrammeClock::named("Europe/Amsterdam").unwrap();
    for (instant, hours) in [("2026-03-29T12:00:00Z", 23), ("2026-10-25T12:00:00Z", 25)] {
        let (start, finish) = clock.day_bounds(at(instant)).unwrap();
        let original = item(
            "full-precision-occurrence",
            start + Duration::nanoseconds(123),
            finish - start + Duration::hours(1),
        );
        let folder = tempfile::tempdir().unwrap();
        publish(
            folder.path(),
            &schedule(vec![original.clone()]),
            &clock,
            start,
        );
        let index = read_index(folder.path());
        assert_eq!(index.days.len(), 2);
        assert_eq!(
            index.days[0].finish - index.days[0].start,
            Duration::hours(hours)
        );
        assert_eq!(index.days[0].finish, index.days[1].start);
        for day in index.days {
            let saved: Playout =
                serde_json::from_slice(&std::fs::read(folder.path().join(day.file)).unwrap())
                    .unwrap();
            assert_eq!(
                serde_json::to_value(&saved.items[0]).unwrap(),
                serde_json::to_value(&original).unwrap()
            );
        }
    }
}

#[test]
fn reclamation_cannot_delete_a_readers_snapshot_across_index_rollback() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let old = schedule(vec![item("accepted", now, Duration::hours(1))]);
    publish(folder.path(), &old, &utc(), now);
    let accepted = std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap();
    let candidate = schedule(vec![item("private-sql-candidate", now, Duration::hours(1))]);
    let mut applied = prepare(folder.path(), &candidate, &utc(), now, &|| true, None)
        .unwrap()
        .publish_reversible(|| true)
        .unwrap()
        .unwrap();
    let reader = File::open(folder.path().join(READER_LOCK_FILENAME)).unwrap();
    reader.try_lock_shared().unwrap();
    let captured = read_index(folder.path());
    applied.restore().unwrap();
    assert_eq!(
        std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap(),
        accepted
    );
    assert!(matches!(
        prepare(folder.path(), &old, &utc(), now, &|| true, None),
        Err(PublicationError::ReadersBusy)
    ));
    let bytes = std::fs::read(folder.path().join(&captured.days[0].file)).unwrap();
    assert_eq!(planner::digest(&bytes), captured.days[0].sha256);
    drop(reader);
    publish(folder.path(), &old, &utc(), now);
    assert!(!folder.path().join(&captured.days[0].file).exists());
    assert_eq!(chunks(folder.path()), 1);
}

#[test]
fn cancelled_or_expired_candidates_keep_the_accepted_index_and_remove_staging() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let old = schedule(vec![item("old", now, Duration::hours(1))]);
    publish(folder.path(), &old, &utc(), now);
    let bytes = std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap();
    let candidate = schedule(vec![item("candidate", now, Duration::hours(1))]);
    let staged = prepare(folder.path(), &candidate, &utc(), now, &|| true, None).unwrap();
    assert!(matches!(
        staged.publish(|| false),
        Err(PublicationError::Cancelled)
    ));
    let mut staged = prepare(folder.path(), &candidate, &utc(), now, &|| true, None).unwrap();
    staged.restrict_deadline(Instant::now());
    assert!(matches!(
        staged.publish(|| true),
        Err(PublicationError::Expired)
    ));
    assert_eq!(
        std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap(),
        bytes
    );
    assert_eq!(chunks(folder.path()), 1);
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 3);
}

#[test]
fn acceptance_keeps_new_index_and_repeated_edits_bound_obsolete_chunks() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    publish(folder.path(), &schedule(Vec::new()), &utc(), now);
    for revision in 0..40 {
        let playout = schedule(vec![item(
            &format!("revision-{revision}"),
            now,
            Duration::hours(1),
        )]);
        let mut applied = prepare(folder.path(), &playout, &utc(), now, &|| true, None)
            .unwrap()
            .publish_reversible(|| true)
            .unwrap()
            .unwrap();
        applied.accept();
        drop(applied);
        assert!(
            chunks(folder.path()) <= 2,
            "only accepted and preceding generations remain"
        );
        let index = read_index(folder.path());
        let data: Playout = serde_json::from_slice(
            &std::fs::read(folder.path().join(&index.days[0].file)).unwrap(),
        )
        .unwrap();
        assert_eq!(data.items[0].id, format!("revision-{revision}"));
    }
}

#[test]
fn bad_index_references_payload_hashes_and_budgets_fail_closed() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let playout = schedule(vec![item("valid", now, Duration::hours(1))]);
    publish(folder.path(), &playout, &utc(), now);
    let index = read_index(folder.path());
    for invalid in [
        "../outside.json",
        "C:\\outside.json",
        "/outside.json",
        "day-INVALID.json",
    ] {
        let mut bad = index.clone();
        bad.days[0].file = invalid.into();
        assert!(matches!(
            bad.validate(),
            Err(PublicationError::InvalidIndex)
        ));
    }
    let mut bad = index.clone();
    bad.days.push(bad.days[0].clone());
    assert!(bad.validate().is_err());
    bad = index.clone();
    bad.version = "future-index".into();
    assert!(bad.validate().is_err());
    std::fs::write(folder.path().join(&index.days[0].file), b"corrupt").unwrap();
    assert!(matches!(
        prepare(folder.path(), &playout, &utc(), now, &|| true, None),
        Err(PublicationError::InvalidIndex)
    ));
    assert_eq!(read_index(folder.path()), index);
    assert!(matches!(
        planner::encode(&"oversized", 4),
        Err(PublicationError::Limit)
    ));
    let huge = schedule(vec![item("too-many-days", now, Duration::days(368))]);
    assert!(matches!(
        planner::plan(&huge, None, &utc(), now, &|| true, None),
        Err(PublicationError::Limit)
    ));
}

#[test]
fn reclamation_preserves_foreign_files_and_never_follows_an_index_symlink() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let playout = schedule(vec![item("valid", now, Duration::hours(1))]);
    publish(folder.path(), &playout, &utc(), now);
    let foreign = folder.path().join("foreign.json");
    std::fs::write(&foreign, b"preserve").unwrap();
    publish(folder.path(), &playout, &utc(), now);
    assert_eq!(std::fs::read(&foreign).unwrap(), b"preserve");
    // Unix symlink creation needs no extra privilege. The same regular-file
    // validation executes on Windows; native symlink creation is not required.
    #[cfg(unix)]
    {
        std::fs::remove_file(folder.path().join(INDEX_FILENAME)).unwrap();
        std::os::unix::fs::symlink(&foreign, folder.path().join(INDEX_FILENAME)).unwrap();
        assert!(matches!(
            prepare(folder.path(), &playout, &utc(), now, &|| true, None),
            Err(PublicationError::InvalidIndex)
        ));
        assert_eq!(std::fs::read(foreign).unwrap(), b"preserve");
    }
}

#[test]
fn stale_staging_is_reclaimed_but_retained_rollback_protects_every_chunk() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let old = schedule(vec![item("accepted", now, Duration::hours(1))]);
    publish(folder.path(), &old, &utc(), now);
    let old_index = read_index(folder.path());
    let backup = serde_json::to_vec(&old_index).unwrap();
    let new = schedule(vec![item("candidate", now, Duration::hours(1))]);
    publish(folder.path(), &new, &utc(), now);
    let new_index = std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap();
    let retained = folder.path().join(".next-rollback-Retained");
    std::fs::write(&retained, &backup).unwrap();
    let stale = folder.path().join(".next-day-Stale123");
    std::fs::write(&stale, b"staged-before-crash").unwrap();
    assert!(matches!(
        prepare(folder.path(), &new, &utc(), now, &|| true, None),
        Err(PublicationError::RecoveryRequired)
    ));
    assert!(
        stale.exists(),
        "preflight detects recovery before any deletion"
    );
    assert!(folder.path().join(&old_index.days[0].file).exists());
    assert_eq!(std::fs::read(&retained).unwrap(), backup);
    assert_eq!(
        std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap(),
        new_index
    );
    std::fs::remove_file(retained).unwrap();
    publish(folder.path(), &new, &utc(), now);
    assert!(!stale.exists());
    assert_eq!(chunks(folder.path()), 1);
}

#[test]
fn crossing_a_finished_day_does_not_rewrite_a_still_running_occurrence() {
    let folder = tempfile::tempdir().unwrap();
    let midnight = at("2026-11-07T00:00:00Z");
    let playout = schedule(vec![item(
        "cross-midnight",
        midnight - Duration::hours(1),
        Duration::hours(3),
    )]);
    publish(
        folder.path(),
        &playout,
        &utc(),
        midnight - Duration::minutes(10),
    );
    let old = read_index(folder.path());
    let modified = stamp(&folder.path().join(INDEX_FILENAME));
    publish(
        folder.path(),
        &playout,
        &utc(),
        midnight + Duration::minutes(10),
    );
    assert_eq!(read_index(folder.path()), old);
    assert_eq!(
        folder
            .path()
            .join(INDEX_FILENAME)
            .metadata()
            .unwrap()
            .modified()
            .unwrap(),
        modified
    );
}

#[test]
fn cancellation_during_bounded_day_planning_never_touches_publication() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    publish(folder.path(), &schedule(Vec::new()), &utc(), now);
    let accepted = std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap();
    let playout = schedule(vec![item("many-days", now, Duration::days(30))]);
    let checks = std::cell::Cell::new(0);
    let is_current = || {
        checks.set(checks.get() + 1);
        checks.get() < 4
    };
    assert!(matches!(
        prepare(folder.path(), &playout, &utc(), now, &is_current, None),
        Err(PublicationError::Cancelled)
    ));
    assert!(checks.get() >= 4);
    assert_eq!(
        std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap(),
        accepted
    );
    assert_eq!(std::fs::read_dir(folder.path()).unwrap().count(), 2);
}

#[test]
fn a_later_day_staging_failure_discards_new_chunks_and_keeps_the_accepted_generation() {
    let folder = tempfile::tempdir().unwrap();
    let now = at("2026-11-06T12:00:00Z");
    let old = schedule(
        (0..2)
            .map(|day| {
                item(
                    &format!("old-{day}"),
                    now + Duration::days(day),
                    Duration::hours(1),
                )
            })
            .collect(),
    );
    publish(folder.path(), &old, &utc(), now);
    let accepted = std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap();
    let candidate = schedule(
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
    let plan = planner::plan(&candidate, None, &utc(), now, &|| true, None)
        .unwrap()
        .unwrap();
    let first = folder.path().join(&plan.chunks[0].0.file);
    let second = folder.path().join(&plan.chunks[1].0.file);
    let injected = std::cell::Cell::new(false);
    // Inject a filesystem conflict after the first chunk has been staged,
    // rather than adding a production-only fault-injection interface.
    let is_current = || {
        if first.is_file() && !injected.get() {
            std::fs::create_dir(&second).unwrap();
            injected.set(true);
        }
        true
    };
    assert!(matches!(
        prepare(folder.path(), &candidate, &utc(), now, &is_current, None),
        Err(PublicationError::InvalidIndex)
    ));
    assert!(injected.get());
    assert!(
        !first.exists(),
        "all staged candidate chunks must be discarded"
    );
    assert!(
        second.is_dir(),
        "the conflicting foreign object must be preserved"
    );
    assert_eq!(
        std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap(),
        accepted
    );
    std::fs::remove_dir(second).unwrap();
    publish(folder.path(), &candidate, &utc(), now);
    assert_ne!(
        std::fs::read(folder.path().join(INDEX_FILENAME)).unwrap(),
        accepted
    );
}
