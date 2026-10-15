use super::*;

#[test]
fn source_revision_is_stable_and_detects_observed_mutation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.ts");
    std::fs::write(&path, b"first").unwrap();
    let guard = LocalFileRevision::open(&path).unwrap();
    let repeated = LocalFileRevision::open(&path).unwrap();
    assert_eq!(guard.revision(), repeated.revision());
    assert!(guard.is_current(&path));
    std::fs::write(&path, b"longer").unwrap();
    assert!(!guard.is_current(&path));
    assert_ne!(
        guard.revision(),
        LocalFileRevision::open(&path).unwrap().revision()
    );
}

#[test]
fn held_original_detects_replacement_despite_identical_size_and_filesystem_mtime() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.ts");
    let replacement = directory.path().join("replacement.ts");
    std::fs::write(&path, b"first").unwrap();
    let guard = LocalFileRevision::open(&path).unwrap();
    std::fs::write(&replacement, b"other").unwrap();
    File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(guard.modified))
        .unwrap();
    std::fs::rename(&path, directory.path().join("retired.ts")).unwrap();
    std::fs::rename(&replacement, &path).unwrap();
    assert_eq!(path.metadata().unwrap().modified().unwrap(), guard.modified);
    assert_eq!(path.metadata().unwrap().len(), guard.len);
    assert!(!guard.is_current(&path));
    assert_ne!(
        guard.revision(),
        LocalFileRevision::open(&path).unwrap().revision()
    );
}

#[test]
fn nonregular_source_cannot_gain_an_auxiliary_association() {
    let directory = tempfile::tempdir().unwrap();
    assert!(LocalFileRevision::open(directory.path()).is_err());
}

#[test]
fn secondary_dump_options_never_change_the_ordinary_command() {
    let extractor = FfprobeExtractor::new("synthetic-ffprobe");
    let path = Path::new("source.ts");
    let ordinary = extractor.command_for(path);
    assert_eq!(ordinary.args, super::super::ffprobe_args(path));
    assert!(!ordinary.args.iter().any(|arg| arg == "-show_data"));
    let auxiliary = auxiliary_command(&extractor, path);
    assert_eq!(auxiliary.program, ordinary.program);
    for expected in [
        &["-select_streams", "s"][..],
        &["-data_dump_format", "base64"][..],
        &["-i", "source.ts"][..],
    ] {
        assert!(auxiliary.args.windows(2).any(|args| args == expected));
    }
    assert!(!auxiliary.args.iter().any(|arg| arg == "-show_frames"));
}

const PRIMARY: &str = r#"{"format":{"format_name":"mpegts","duration":"60"},"streams":[{"index":0,"id":"0x100","codec_type":"video","codec_name":"mpeg2video","width":320,"height":180},{"index":1,"id":"0x101","codec_type":"audio","codec_name":"aac","tags":{"language":"nld"},"disposition":{"default":1}},{"index":2,"id":"0x102","codec_type":"subtitle","codec_name":"dvb_teletext","tags":{"language":"eng,eng"}}]}"#;
const AUXILIARY: &str = r#"{"format":{"format_name":"mpegts"},"streams":[{"index":2,"id":"0x102","codec_type":"subtitle","codec_name":"dvb_teletext","extradata":"CgAQiA==","extradata_size":4,"tags":{"language":"eng,eng"}}],"programs":[{"program_id":1,"program_num":1,"pmt_pid":4096,"pcr_pid":256,"streams":[{"index":2}]}]}"#;

fn snapshot(json: &str) -> TeletextProbeSnapshot {
    let parsed = serde_json::from_str(json).unwrap();
    teletext_probe::project(&parsed, TeletextProbeEncoding::Ffprobe9Base64)
}

#[test]
fn identities_match_global_stream_indexes_and_pids_not_ordinal_or_language() {
    let primary = identities(&snapshot(PRIMARY)).unwrap();
    assert_eq!(primary, vec![(2, 0x102)]);
    assert_eq!(identities(&snapshot(AUXILIARY)).unwrap(), primary);
    for edited in [
        AUXILIARY.replace("0x102", "0x103"),
        AUXILIARY.replace("\"index\":2", "\"index\":3"),
    ] {
        assert_ne!(identities(&snapshot(&edited)), Some(primary.clone()));
    }
    let missing = AUXILIARY.replace("\"id\":\"0x102\",", "");
    assert!(identities(&snapshot(&missing)).is_none());
}

#[test]
fn optional_failure_cannot_mask_cancellation_or_uncertain_cleanup() {
    for error in [
        FfprobeError::Cancelled {
            path: "synthetic".into(),
        },
        FfprobeError::CleanupUnconfirmed {
            path: "synthetic".into(),
        },
    ] {
        assert!(optional_failure(error).is_err());
    }
    assert!(optional_failure(FfprobeError::Timeout {
        path: "synthetic".into()
    })
    .unwrap()
    .is_none());
}

// Unix process-group fixtures use a shell to model FFprobe outcomes. Windows
// retains the portable association tests above; actual job cleanup is a
// separate supported-platform acceptance gate, not claimed by these fixtures.
#[cfg(unix)]
mod unix {
    use super::*;
    use crate::ffprobe::LocalProbeCleanupScope;
    use std::os::unix::fs::PermissionsExt as _;

    #[test]
    fn lossy_command_locator_cannot_bind_a_different_file() {
        use std::os::unix::ffi::OsStringExt as _;
        let directory = tempfile::tempdir().unwrap();
        let original = directory
            .path()
            .join(std::ffi::OsString::from_vec(b"source-\xff.ts".to_vec()));
        let lossy = std::path::PathBuf::from(original.to_string_lossy().into_owned());
        std::fs::write(&original, b"original").unwrap();
        std::fs::write(&lossy, b"different").unwrap();
        assert!(LocalFileRevision::open(&original).is_err());
        assert!(LocalFileRevision::open(&lossy).is_ok());
    }

    struct Probe {
        _directory: tempfile::TempDir,
        extractor: FfprobeExtractor,
        source: std::path::PathBuf,
        pid_file: std::path::PathBuf,
    }

    impl Probe {
        fn new(auxiliary: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let program = directory.path().join("probe");
            let script = format!(
                "#!/bin/sh\nauxiliary=false\nfor arg in \"$@\"; do\n  if [ \"$arg\" = -select_streams ]; then auxiliary=true; fi\ndone\nif $auxiliary; then\n{auxiliary}\nelse\ncat <<'PRIMARY_JSON'\n{PRIMARY}\nPRIMARY_JSON\nfi\n"
            );
            std::fs::write(&program, script).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            let source = directory.path().join("source.ts");
            std::fs::write(&source, b"synthetic source association").unwrap();
            Self {
                extractor: FfprobeExtractor::new(program.to_string_lossy().into_owned()),
                source,
                pid_file: program.with_extension("pid"),
                _directory: directory,
            }
        }

        async fn pid(&self) -> nix::unistd::Pid {
            tokio::time::timeout(Duration::from_secs(3), async {
                loop {
                    if let Ok(text) = tokio::fs::read_to_string(&self.pid_file).await {
                        if let Ok(pid) = text.trim().parse() {
                            break nix::unistd::Pid::from_raw(pid);
                        }
                    }
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            })
            .await
            .expect("auxiliary owner must report its process")
        }
    }

    #[tokio::test]
    async fn successful_auxiliary_preserves_the_exact_primary_projection() {
        let probe = Probe::new(&format!("printf '%s' '{AUXILIARY}'"));
        let scope = LocalProbeCleanupScope::new();
        let mut expected = scope
            .scope(probe.extractor.extract(&probe.source))
            .await
            .unwrap();
        let acquired = scope
            .scope(probe.extractor.extract_local_with_teletext(&probe.source))
            .await
            .unwrap();
        expected.version.date_added = acquired.media.version.date_added;
        expected.version.date_updated = acquired.media.version.date_updated;
        assert_eq!(acquired.media, expected);
        let acquisition = acquired.teletext.unwrap();
        assert_eq!(acquisition.snapshot(), &snapshot(AUXILIARY));
        assert!(acquisition.is_current(&probe.source));
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
    }

    #[tokio::test]
    async fn failed_malformed_oversized_and_mismatched_auxiliary_preserve_ordinary_tracks() {
        for script in [
            "exit 7".to_owned(),
            "printf malformed".to_owned(),
            "head -c 8388609 /dev/zero; exec sleep 30".to_owned(),
            format!("printf '%s' '{}'", AUXILIARY.replace("0x102", "0x103")),
        ] {
            let probe = Probe::new(&script);
            let scope = LocalProbeCleanupScope::new();
            let acquired = scope
                .scope(probe.extractor.extract_local_with_teletext(&probe.source))
                .await
                .unwrap();
            assert!(acquired.teletext.is_none());
            assert_eq!(acquired.media.streams.len(), 3);
            assert_eq!(acquired.media.streams[1].language.as_deref(), Some("nld"));
            assert_eq!(
                acquired.media.streams[2].language.as_deref(),
                Some("eng,eng")
            );
            scope.close();
            assert_eq!(scope.wait_cleanup().await, Ok(()));
        }
    }

    #[tokio::test]
    async fn auxiliary_timeout_returns_optional_metadata_only_after_process_cleanup() {
        let probe = Probe::new("printf '%s' \"$$\" >\"${0}.pid\"; exec sleep 30");
        let scope = LocalProbeCleanupScope::new();
        let acquired = scope
            .scope(extract_with_timeout(
                &probe.extractor,
                &probe.source,
                Duration::from_millis(500),
            ))
            .await
            .unwrap();
        assert!(acquired.teletext.is_none());
        let pid = probe.pid().await;
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH)
        );
        assert_eq!(scope.cleanup_status(), Ok(true));
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
    }

    #[tokio::test]
    async fn auxiliary_scope_cancellation_is_fatal_and_cleanup_precedes_response() {
        let probe = Arc::new(Probe::new(
            "printf '%s' \"$$\" >\"${0}.pid\"; exec sleep 30",
        ));
        let scope = LocalProbeCleanupScope::new();
        let owned = Arc::clone(&probe);
        let caller_scope = scope.clone();
        let caller = tokio::spawn(async move {
            caller_scope
                .scope(owned.extractor.extract_local_with_teletext(&owned.source))
                .await
        });
        let pid = probe.pid().await;
        scope.close();
        assert!(matches!(
            caller.await.unwrap(),
            Err(FfprobeError::Cancelled { .. })
        ));
        assert_eq!(scope.cleanup_status(), Ok(true));
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH)
        );
    }
}
