use super::*;

#[tokio::test]
async fn refused_probe_registration_releases_unstarted_retention_without_quarantine() {
    use crate::ffprobe::LocalProbeCleanupScope;
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    let (_directory, path, revision) = source();
    let guard = Arc::new(LocalTeletextSourceGuard::open(&path, &revision).unwrap());
    let locker = EntityLocker::new();
    let admitted = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let retention = local::ProbeRetention::new(admitted.hold_work(), guard);
    drop(admitted);
    let scope = LocalProbeCleanupScope::new();
    scope.close();
    let result = scope
        .scope(local::capture_retained(
            FfprobeCommand {
                program: "must-never-start".into(),
                args: Vec::new(),
            },
            PathBuf::from("source association"),
            None,
            Some(retention),
        ))
        .await;
    assert!(matches!(result, Err(FfprobeError::Cancelled { .. })));
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    assert_eq!(locker.active_teletext_work(), 0);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_some());
}
use crate::ffprobe::LocalProbeCleanupScope;
use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};

fn source() -> (
    tempfile::TempDir,
    std::path::PathBuf,
    SourceTeletextRevision,
) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-source.ts");
    std::fs::write(&path, b"original source").unwrap();
    let revision = LocalFileRevision::open(&path).unwrap().revision().unwrap();
    (directory, path, revision)
}

async fn wait_for_released_work(locker: &EntityLocker) {
    tokio::time::timeout(Duration::from_secs(1), async {
        while locker.active_teletext_work() != 0 {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("completed blocking worker must release its teletext admission");
}

#[test]
fn acquired_revision_opens_and_holds_the_current_source() {
    let (_directory, path, revision) = source();
    let guard = LocalTeletextSourceGuard::open(&path, &revision).unwrap();
    assert!(guard.is_current(&path));
    assert_eq!(
        format!("{guard:?}"),
        "LocalTeletextSourceGuard(<held association>)"
    );
}

#[test]
fn unrelated_expected_revision_is_rejected_without_private_diagnostics() {
    let (_directory, path, revision) = source();
    let expected = SourceTeletextRevision::new("private-stale-revision".into()).unwrap();
    let error = LocalTeletextSourceGuard::open(&path, &expected).unwrap_err();
    assert_eq!(error, LocalTeletextSourceGuardError::Changed);
    assert_eq!(
        error.to_string(),
        "local teletext source association has changed"
    );
    assert_eq!(format!("{error:?}"), "Changed");
    assert!(!error.to_string().contains(revision.as_str()));
    assert!(!error.to_string().contains(expected.as_str()));
    assert!(!error.to_string().contains("private-source"));
}

#[test]
fn missing_and_nonregular_locators_are_unavailable() {
    let (directory, _path, revision) = source();
    for path in [
        directory.path().to_owned(),
        directory.path().join("missing.ts"),
    ] {
        let error = LocalTeletextSourceGuard::open(&path, &revision).unwrap_err();
        assert_eq!(error, LocalTeletextSourceGuardError::Unavailable);
        assert_eq!(
            error.to_string(),
            "local teletext source association is unavailable"
        );
        assert_eq!(format!("{error:?}"), "Unavailable");
    }
}

#[test]
fn observed_mutation_invalidates_held_and_persisted_associations() {
    let (_directory, path, revision) = source();
    let guard = LocalTeletextSourceGuard::open(&path, &revision).unwrap();
    std::fs::write(&path, b"source with changed size").unwrap();
    assert!(!guard.is_current(&path));
    assert_eq!(
        LocalTeletextSourceGuard::open(&path, &revision).unwrap_err(),
        LocalTeletextSourceGuardError::Changed
    );
}

#[test]
fn replacement_with_identical_size_and_filesystem_mtime_is_rejected() {
    let (directory, path, revision) = source();
    let original = LocalFileRevision::open(&path).unwrap();
    let guard = LocalTeletextSourceGuard::open(&path, &revision).unwrap();
    let replacement = directory.path().join("replacement.ts");
    std::fs::write(&replacement, b"different bytes").unwrap();
    File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original.modified))
        .unwrap();
    std::fs::rename(&path, directory.path().join("retired.ts")).unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    assert_eq!(path.metadata().unwrap().len(), original.len);
    assert_eq!(
        path.metadata().unwrap().modified().unwrap(),
        original.modified
    );
    assert!(!guard.is_current(&path));
    assert_eq!(
        LocalTeletextSourceGuard::open(&path, &revision).unwrap_err(),
        LocalTeletextSourceGuardError::Changed
    );
}

#[test]
fn resolved_hard_link_must_name_the_same_held_source() {
    let (directory, path, revision) = source();
    let guard = LocalTeletextSourceGuard::open(&path, &revision).unwrap();
    let resolved = directory.path().join("resolved.ts");
    std::fs::hard_link(&path, &resolved).unwrap();
    assert!(guard.is_current(&resolved));
    assert!(LocalTeletextSourceGuard::open(&resolved, &revision).is_ok());

    let unrelated = directory.path().join("unrelated.ts");
    std::fs::copy(&path, &unrelated).unwrap();
    File::options()
        .write(true)
        .open(&unrelated)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new().set_modified(path.metadata().unwrap().modified().unwrap()),
        )
        .unwrap();
    assert!(!guard.is_current(&unrelated));
    assert_eq!(
        LocalTeletextSourceGuard::open(&unrelated, &revision).unwrap_err(),
        LocalTeletextSourceGuardError::Changed
    );

    std::fs::write(&resolved, b"changed through resolved path").unwrap();
    assert!(!guard.is_current(&path));
    assert!(!guard.is_current(&resolved));
}

#[tokio::test]
async fn owned_open_and_postcheck_share_actual_source_and_release_worker_holds() {
    let (directory, path, revision) = source();
    let resolved = directory.path().join("resolved.ts");
    std::fs::hard_link(&path, &resolved).unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let deadline = Instant::now() + Duration::from_secs(2);
    let guard = scope
        .scope(open_source_guard_owned(
            path.clone(),
            resolved.clone(),
            revision,
            deadline,
            work.hold_work(),
        ))
        .await
        .unwrap();
    scope
        .scope(check_source_guard_owned(
            guard.clone(),
            path.clone(),
            resolved.clone(),
            deadline,
            work.hold_work(),
        ))
        .await
        .unwrap();
    std::fs::write(&resolved, b"changed physical source").unwrap();
    let error = scope
        .scope(check_source_guard_owned(
            guard,
            path,
            resolved,
            deadline,
            work.hold_work(),
        ))
        .await
        .unwrap_err();
    assert!(
        matches!(error, FfprobeError::InvalidOutput { ref path } if path == "local teletext source association")
    );
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test]
async fn owned_open_rejects_unrelated_resolved_file_and_has_opaque_errors() {
    let (directory, path, revision) = source();
    let resolved = directory.path().join("private-other-source.ts");
    std::fs::copy(&path, &resolved).unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let error = scope
        .scope(open_source_guard_owned(
            path,
            resolved,
            revision,
            Instant::now() + Duration::from_secs(2),
            work.hold_work(),
        ))
        .await
        .unwrap_err();
    assert!(matches!(error, FfprobeError::InvalidOutput { .. }));
    assert!(!format!("{error:?}").contains("private-"));
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test]
async fn owned_open_requires_the_actual_cleanup_scope() {
    let (_directory, path, revision) = source();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let error = open_source_guard_owned(
        path.clone(),
        path,
        revision,
        Instant::now() + Duration::from_secs(2),
        work.hold_work(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, FfprobeError::Cancelled { ref path } if path == "local teletext source association")
    );
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test]
async fn expired_caller_deadline_never_launches_new_blocking_work() {
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let result: crate::ffprobe::Result<()> = scope
        .scope(headers::run_source_guard_owned(
            Instant::now(),
            work.hold_work(),
            |_| panic!("expired work must not run"),
        ))
        .await;
    assert!(matches!(result, Err(FfprobeError::Timeout { .. })));
    assert_eq!(scope.cleanup_status(), Ok(true));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test]
async fn abandoned_source_guard_waiter_retains_admission_until_worker_exit() {
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let hold = work.hold_work();
    let scope = LocalProbeCleanupScope::new();
    let task_scope = scope.clone();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        task_scope
            .scope(headers::run_source_guard_owned(
                Instant::now() + Duration::from_secs(2),
                hold,
                move |_| {
                    let _ = started.send(());
                    wait.recv().unwrap();
                    Ok(())
                },
            ))
            .await
    });
    ready.await.unwrap();
    drop(work);
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(scope.cleanup_status(), Ok(false));
    assert_eq!(locker.active_teletext_work(), 1);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
    release.send(()).unwrap();
    scope.wait_cleanup().await.unwrap();
    wait_for_released_work(&locker).await;
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test(start_paused = true)]
async fn caller_deadline_timeout_keeps_admission_until_actual_worker_exit() {
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let hold = work.hold_work();
    let scope = LocalProbeCleanupScope::new();
    let task_scope = scope.clone();
    let (started, ready) = tokio::sync::oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        task_scope
            .scope(headers::run_source_guard_owned(
                Instant::now() + Duration::from_secs(2),
                hold,
                move |_| {
                    let _ = started.send(());
                    wait.recv().unwrap();
                    Ok(())
                },
            ))
            .await
    });
    ready.await.unwrap();
    drop(work);
    tokio::time::advance(Duration::from_secs(4)).await;
    assert!(matches!(
        task.await.unwrap(),
        Err(FfprobeError::CleanupUnconfirmed { .. })
    ));
    assert_eq!(scope.cleanup_status(), Ok(false));
    assert_eq!(locker.active_teletext_work(), 1);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
    release.send(()).unwrap();
    scope.wait_cleanup().await.unwrap();
    wait_for_released_work(&locker).await;
    assert_eq!(locker.active_teletext_work(), 0);
}

// Unix symlinks and non-Unicode locators exercise platform-specific unsafe-path
// rejection. Windows retains the portable source-identity tests above; native
// reparse-point acceptance requires its separate supported-platform evidence.
#[cfg(unix)]
mod unix {
    use super::*;

    #[test]
    fn symlink_is_rejected_for_original_and_resolved_locators() {
        let (directory, path, revision) = source();
        let guard = LocalTeletextSourceGuard::open(&path, &revision).unwrap();
        let link = directory.path().join("link.ts");
        std::os::unix::fs::symlink(&path, &link).unwrap();
        assert!(!guard.is_current(&link));
        assert_eq!(
            LocalTeletextSourceGuard::open(&link, &revision).unwrap_err(),
            LocalTeletextSourceGuardError::Unavailable
        );
    }

    #[test]
    fn nonlossless_locator_cannot_bind_its_unicode_replacement() {
        use std::os::unix::ffi::OsStringExt as _;
        let (directory, path, revision) = source();
        let guard = LocalTeletextSourceGuard::open(&path, &revision).unwrap();
        let nonlossless = directory.path().join(std::ffi::OsString::from_vec(
            b"private-source-\xff.ts".to_vec(),
        ));
        std::fs::hard_link(&path, &nonlossless).unwrap();
        assert!(!guard.is_current(&nonlossless));
        assert_eq!(
            LocalTeletextSourceGuard::open(&nonlossless, &revision).unwrap_err(),
            LocalTeletextSourceGuardError::Unavailable
        );
    }
}
