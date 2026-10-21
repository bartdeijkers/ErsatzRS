//! Preparation boundaries, not native decoding or transport readiness evidence.

use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard, TeletextWorkHold};
use ersatzrs_domain::SourceTeletextRevision;

use super::*;

fn request() -> SourceTeletextPageDecodeRequest {
    SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(
            11,
            MediaSourceKind::Local,
            12,
            13,
            14,
            SourceTeletextRevision::new("synthetic-unopened-revision".into()).unwrap(),
        )
        .unwrap(),
        stream_index: 3,
        pid: 0x103,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    }
}

fn admitted_hold(locker: &EntityLocker) -> TeletextWorkHold {
    TeletextWorkGuard::try_acquire(locker, 1)
        .unwrap()
        .hold_work()
}

#[tokio::test]
async fn unowned_preparation_starts_no_source_or_process_work() {
    let locker = EntityLocker::new();
    let result = FfprobeExtractor::new("must-not-start-synthetic-probe")
        .prepare_local_teletext_source(
            request(),
            "missing-original.ts".into(),
            "missing-resolved.ts".into(),
            Instant::now() + Duration::from_secs(5),
            admitted_hold(&locker),
        )
        .await;
    assert_eq!(
        result.unwrap_err(),
        SourceTeletextPageDecodeError::NoCleanupScope
    );
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test]
async fn preparation_rejects_expired_and_unbounded_deadlines_before_source_open() {
    let locker = EntityLocker::new();
    let scope = LocalProbeCleanupScope::new();
    let now = Instant::now();
    for (deadline, expected) in [
        (now, SourceTeletextPageDecodeError::Deadline),
        (
            now + Duration::from_mins(1),
            SourceTeletextPageDecodeError::InvalidRequest,
        ),
    ] {
        let result = scope
            .scope(
                FfprobeExtractor::new("must-not-start-synthetic-probe")
                    .prepare_local_teletext_source(
                        request(),
                        "missing-original.ts".into(),
                        "missing-resolved.ts".into(),
                        deadline,
                        admitted_hold(&locker),
                    ),
            )
            .await;
        assert_eq!(result.unwrap_err(), expected);
        assert_eq!(locker.active_teletext_work(), 0);
        assert_eq!(scope.cleanup_status(), Ok(true));
    }
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
}

#[tokio::test]
async fn invalid_preparation_request_cannot_open_source_or_start_probe() {
    let locker = EntityLocker::new();
    let scope = LocalProbeCleanupScope::new();
    let mut invalid = request();
    invalid.binding.media_source_id = 0;
    let result = scope
        .scope(
            FfprobeExtractor::new("must-not-start-synthetic-probe").prepare_local_teletext_source(
                invalid,
                "missing-original.ts".into(),
                "missing-resolved.ts".into(),
                Instant::now() + Duration::from_secs(5),
                admitted_hold(&locker),
            ),
        )
        .await;
    assert_eq!(
        result.unwrap_err(),
        SourceTeletextPageDecodeError::InvalidRequest
    );
    assert_eq!(locker.active_teletext_work(), 0);
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
}

// POSIX fake-tool process ownership complements separate native/all-platform proof.
#[cfg(unix)]
mod unix {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt as _;
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[test]
    fn native_origin_command_cannot_silently_probe_a_lossy_sibling_path() {
        let path = PathBuf::from(OsString::from_vec(b"synthetic-\xff.ts".to_vec()));
        let sibling = PathBuf::from(path.to_string_lossy().into_owned());
        assert_ne!(path, sibling);
        let extractor = FfprobeExtractor::new("must-not-start-synthetic-probe");
        assert_eq!(
            extractor.source_origin_command(&path),
            Err(SourceTeletextPageDecodeError::SourceChanged)
        );
        assert_eq!(
            extractor.source_demux_command(&path),
            Err(SourceTeletextPageDecodeError::SourceChanged)
        );
        let command = extractor.source_origin_command(&sibling).unwrap();
        assert_eq!(command.args.last().unwrap(), sibling.to_str().unwrap());
    }

    const PRIMARY: &str = r#"{"format":{"format_name":"mpegts","duration":"1"},"streams":[{"index":3,"id":"0x103","codec_type":"subtitle","codec_name":"dvb_teletext"}]}"#;
    const AUXILIARY: &str = r#"{"format":{"format_name":"mpegts"},"streams":[{"index":3,"id":"0x103","codec_type":"subtitle","codec_name":"dvb_teletext","extradata":"CgAQiA==","extradata_size":4}],"programs":[{"program_id":1,"program_num":1,"pmt_pid":4096,"pcr_pid":256,"streams":[{"index":3}]}]}"#;
    const NATIVE: &str = r#"{"format":{"start_time":"1.418667"},"streams":[{"index":0,"id":"0x100","codec_type":"video","start_pts":129600,"time_base":"1/90000"}]}"#;

    struct Probe {
        _directory: tempfile::TempDir,
        extractor: FfprobeExtractor,
        source: PathBuf,
        pid_file: PathBuf,
    }

    impl Probe {
        fn new(native: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let program = directory.path().join("probe");
            let script = format!(
                "#!/bin/sh\nauxiliary=false\nnative=false\nfor arg in \"$@\"; do\n  if [ \"$arg\" = -data_dump_format ]; then auxiliary=true; fi\n  case \"$arg\" in format=start_time*) native=true ;; esac\ndone\nif $native; then\n{native}\nelif $auxiliary; then\ncat <<'AUXILIARY_JSON'\n{AUXILIARY}\nAUXILIARY_JSON\nelse\ncat <<'PRIMARY_JSON'\n{PRIMARY}\nPRIMARY_JSON\nfi\n"
            );
            std::fs::write(&program, script).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            let source = directory.path().join("private-source.ts");
            // Deliberately no valid TS prefix: this owner grants no page/window
            // readiness and uses only the real regular-file association.
            std::fs::write(&source, b"synthetic non-transport source").unwrap();
            Self {
                extractor: FfprobeExtractor::new(program.to_string_lossy().into_owned()),
                source,
                pid_file: program.with_extension("pid"),
                _directory: directory,
            }
        }

        async fn request(&self, scope: &LocalProbeCleanupScope) -> SourceTeletextPageDecodeRequest {
            let acquired = scope
                .scope(self.extractor.extract_local_with_teletext(&self.source))
                .await
                .unwrap();
            let mut requested = request();
            requested.binding.source_revision = acquired.teletext.unwrap().revision().clone();
            requested
        }

        async fn pid(&self) -> nix::unistd::Pid {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Ok(value) = tokio::fs::read_to_string(&self.pid_file).await {
                        if let Ok(pid) = value.trim().parse() {
                            return nix::unistd::Pid::from_raw(pid);
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("actual retained native probe must report its process identity")
        }
    }

    #[tokio::test]
    async fn preparation_retains_source_but_releases_work_and_rejects_replacement() {
        let probe = Probe::new(&format!("cat <<'NATIVE_JSON'\n{NATIVE}\nNATIVE_JSON"));
        let scope = LocalProbeCleanupScope::new();
        let request = probe.request(&scope).await;
        let locker = EntityLocker::new();
        let prepared = scope
            .scope(probe.extractor.prepare_local_teletext_source(
                request.clone(),
                probe.source.clone(),
                probe.source.clone(),
                Instant::now() + Duration::from_secs(5),
                admitted_hold(&locker),
            ))
            .await
            .unwrap();
        assert_eq!(prepared.request(), &request);
        assert_eq!(prepared.native_origin().streams().len(), 1);
        assert_eq!(
            locker.active_teletext_work(),
            0,
            "prepared playback must not retain short preparation admission"
        );
        let debug = format!("{prepared:?}");
        assert!(!debug.contains(probe.source.to_str().unwrap()));
        assert!(!debug.contains(request.binding.source_revision.as_str()));
        let metadata = probe.source.metadata().unwrap();
        let replacement = probe.source.with_extension("replacement");
        std::fs::write(&replacement, std::fs::read(&probe.source).unwrap()).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_times(std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
            .unwrap();
        std::fs::rename(&probe.source, probe.source.with_extension("retired")).unwrap();
        std::fs::rename(replacement, &probe.source).unwrap();
        assert_eq!(probe.source.metadata().unwrap().len(), metadata.len());
        assert_eq!(
            probe.source.metadata().unwrap().modified().unwrap(),
            metadata.modified().unwrap()
        );
        assert_eq!(
            scope
                .scope(prepared.check_source(
                    Instant::now() + Duration::from_secs(5),
                    admitted_hold(&locker)
                ))
                .await,
            Err(SourceTeletextPageDecodeError::SourceChanged)
        );
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        assert_eq!(locker.active_teletext_work(), 0);
    }

    #[tokio::test]
    async fn aborted_preparation_retains_work_until_native_process_cleanup_is_confirmed() {
        let probe = Probe::new("printf '%s' \"$$\" >\"${0}.pid\"; exec sleep 30");
        let scope = LocalProbeCleanupScope::new();
        let request = probe.request(&scope).await;
        let locker = EntityLocker::new();
        let hold = admitted_hold(&locker);
        let caller_scope = scope.clone();
        let extractor = probe.extractor.clone();
        let source = probe.source.clone();
        let caller = tokio::spawn(async move {
            caller_scope
                .scope(extractor.prepare_local_teletext_source(
                    request,
                    source.clone(),
                    source,
                    Instant::now() + Duration::from_secs(10),
                    hold,
                ))
                .await
        });
        let pid = probe.pid().await;
        assert_eq!(locker.active_teletext_work(), 1);
        assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
        caller.abort();
        assert!(caller.await.unwrap_err().is_cancelled());
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH)
        );
        assert_eq!(locker.active_teletext_work(), 0);
        assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_some());
    }
}
