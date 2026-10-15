use serde_json::{json, Value};

use super::*;
use ersatzrs_domain::SourceTeletextRevision;

fn request() -> SourceTeletextPageDecodeRequest {
    SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(
            11,
            MediaSourceKind::Local,
            12,
            13,
            14,
            SourceTeletextRevision::new("synthetic-observed-revision".into()).unwrap(),
        )
        .unwrap(),
        stream_index: 3,
        pid: 0x103,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    }
}

fn output() -> Value {
    let stream = json!({
        "index": 3, "id": "0x103", "codec_name": "dvb_teletext",
        "codec_type": "subtitle"
    });
    json!({
        "streams": [stream.clone()],
        "programs": [{"program_num": 1, "streams": [stream]}],
        "frames": [
            {"media_type": "subtitle", "pts": 200_000, "format": 0, "num_rects": 1},
            {"media_type": "subtitle", "pts": 500_000, "format": 0, "num_rects": 0},
            {"media_type": "subtitle", "pts": 900_000, "format": 0, "num_rects": 2}
        ]
    })
}

fn parse(value: &Value) -> Result<SourceTeletextBitmapEvents, SourceTeletextPageDecodeError> {
    parse_bitmap_events(&request(), &serde_json::to_vec(value).unwrap())
}

#[test]
fn request_requires_actual_local_parent_and_transport_identities() {
    assert_eq!(request().validate(), Ok(()));
    let mut cases = Vec::new();
    for field in 0..4 {
        for value in [0, -1] {
            let mut candidate = request();
            match field {
                0 => candidate.binding.media_source_id = value,
                1 => candidate.binding.media_version_id = value,
                2 => candidate.binding.media_file_id = value,
                _ => candidate.binding.media_stream_id = value,
            }
            cases.push(candidate);
        }
    }
    for provider in [
        MediaSourceKind::Plex,
        MediaSourceKind::Jellyfin,
        MediaSourceKind::Emby,
    ] {
        let mut candidate = request();
        candidate.binding.provider = provider;
        cases.push(candidate);
    }
    let mut candidate = request();
    candidate.stream_index = -1;
    cases.push(candidate);
    let mut candidate = request();
    candidate.program_number = 0;
    cases.push(candidate);
    for pid in [0, 0x1f, 0x1fff, 0x2000, u16::MAX] {
        let mut candidate = request();
        candidate.pid = pid;
        cases.push(candidate);
    }
    for candidate in cases {
        assert_eq!(
            candidate.validate(),
            Err(SourceTeletextPageDecodeError::InvalidRequest)
        );
    }
}

#[test]
fn only_actual_nonempty_bitmap_events_contribute_to_the_receipt() {
    assert_eq!(
        parse(&output()),
        Ok(SourceTeletextBitmapEvents {
            count: 2,
            first_pts_microseconds: Some(200_000),
            last_pts_microseconds: Some(900_000),
        })
    );
    for frames in [
        json!([]),
        json!([
            {"media_type": "subtitle", "pts": 500_000, "format": 0, "num_rects": 0}
        ]),
    ] {
        let mut candidate = output();
        candidate["frames"] = frames;
        assert_eq!(
            parse(&candidate),
            Err(SourceTeletextPageDecodeError::NoBitmap)
        );
    }
}

#[test]
fn matching_language_or_pid_cannot_substitute_for_the_exact_global_stream() {
    for (field, value) in [
        ("index", json!(0)),
        ("id", json!("0x104")),
        ("codec_name", json!("dvb_subtitle")),
        ("codec_type", json!("data")),
    ] {
        for path in ["/streams/0", "/programs/0/streams/0"] {
            let mut candidate = output();
            candidate.pointer_mut(path).unwrap()[field] = value.clone();
            assert_eq!(
                parse(&candidate),
                Err(SourceTeletextPageDecodeError::StreamChanged)
            );
        }
    }
    for id in ["0x+103", "0x-103", "0x", "+0x103", "0x103 ", "0x10g", "259"] {
        let mut candidate = output();
        candidate["streams"][0]["id"] = json!(id);
        assert_eq!(
            parse(&candidate),
            Err(SourceTeletextPageDecodeError::StreamChanged)
        );
    }
}

#[test]
fn a_real_bitmap_with_no_decoder_timestamp_retains_unknown_instead_of_zero() {
    let mut candidate = output();
    candidate["frames"][0]
        .as_object_mut()
        .unwrap()
        .remove("pts");
    candidate["frames"][2]["pts"] = Value::Null;
    let events = parse(&candidate).unwrap();
    assert_eq!(events.count, 2);
    assert_eq!(events.first_pts_microseconds, None);
    assert_eq!(events.last_pts_microseconds, None);
    candidate["frames"][0]["pts"] = json!(0);
    assert_eq!(parse(&candidate).unwrap().first_pts_microseconds, Some(0));
}

#[test]
fn requested_program_must_have_one_unambiguous_matching_membership() {
    for programs in [
        json!([]),
        json!([{"program_num": 2, "streams": output()["streams"]}]),
        json!([{"program_num": 1, "streams": []}]),
        json!([output()["programs"][0], output()["programs"][0]]),
        json!([output()["programs"][0], {"program_num": 1, "streams": []}]),
        json!([output()["programs"][0], {"program_num": 1, "streams": [{
            "index": 3, "id": "0x104", "codec_name": "dvb_teletext", "codec_type": "subtitle"
        }]}]),
    ] {
        let mut candidate = output();
        candidate["programs"] = programs;
        assert_eq!(
            parse(&candidate),
            Err(SourceTeletextPageDecodeError::StreamChanged)
        );
    }
    let mut candidate = output();
    candidate["programs"]
        .as_array_mut()
        .unwrap()
        .insert(0, json!({"program_num": 2, "streams": output()["streams"]}));
    // A matching second membership is selected explicitly, never by position.
    assert!(parse(&candidate).is_ok());
}

#[test]
fn a_valid_bitmap_prefix_cannot_hide_a_later_invalid_decode_event() {
    for frame in [
        json!({"media_type": "video", "pts": 1_000_000, "format": 0, "num_rects": 1}),
        json!({"media_type": "subtitle", "pts": 1_000_000, "format": 1, "num_rects": 1}),
        json!({"media_type": "subtitle", "pts": 1_000_000, "format": 0, "num_rects": 4097}),
        json!({"media_type": "subtitle", "pts": 1_000_000, "format": 0, "num_rects": -1}),
        json!({"media_type": "subtitle", "pts": "1000000", "format": 0, "num_rects": 1}),
        json!({"media_type": "subtitle", "pts": 1_000_000, "format": 0}),
    ] {
        let mut candidate = output();
        candidate["frames"].as_array_mut().unwrap().push(frame);
        assert_eq!(
            parse(&candidate),
            Err(SourceTeletextPageDecodeError::InvalidOutput)
        );
    }
}

#[test]
fn malformed_missing_and_excessive_output_never_returns_a_partial_receipt() {
    for bytes in [b"{".as_slice(), b"null", b"[]", b"{}"] {
        assert_eq!(
            parse_bitmap_events(&request(), bytes),
            Err(SourceTeletextPageDecodeError::InvalidOutput)
        );
    }
    for field in ["streams", "programs", "frames"] {
        let mut candidate = output();
        candidate.as_object_mut().unwrap().remove(field);
        assert_eq!(
            parse(&candidate),
            Err(SourceTeletextPageDecodeError::InvalidOutput)
        );
    }
    let mut duplicate_stream = output();
    duplicate_stream["streams"]
        .as_array_mut()
        .unwrap()
        .push(output()["streams"][0].clone());
    assert_eq!(
        parse(&duplicate_stream),
        Err(SourceTeletextPageDecodeError::InvalidOutput)
    );
    let mut frames = output();
    frames["frames"] = Value::Array(vec![output()["frames"][0].clone(); MAX_FRAMES + 1]);
    assert_eq!(
        parse(&frames),
        Err(SourceTeletextPageDecodeError::InvalidOutput)
    );
    let mut programs = output();
    programs["programs"] = Value::Array(vec![output()["programs"][0].clone(); 129]);
    assert_eq!(
        parse(&programs),
        Err(SourceTeletextPageDecodeError::InvalidOutput)
    );
}

#[test]
fn command_limits_one_exact_page_and_global_stream_without_charset_guessing() {
    let extractor = FfprobeExtractor::new("synthetic-ffprobe");
    let command = extractor
        .teletext_page_decode_command(&request(), std::path::Path::new("synthetic-source.ts"))
        .unwrap();
    for pair in [
        ["-txt_page:3", "888"],
        ["-txt_format:3", "bitmap"],
        ["-txt_default_region:3", "-1"],
        ["-select_streams", "3"],
        ["-read_intervals", "%+8"],
    ] {
        assert!(command.args.windows(2).any(|args| args == pair));
    }
    assert_eq!(command.program, "synthetic-ffprobe");
    assert_eq!(command.args.last().unwrap(), "synthetic-source.ts");
    assert!(!command
        .args
        .iter()
        .any(|arg| arg == "-show_data" || arg == "-txt_page:0"));
    // ADR 0121: scope decoder settings to the original absolute stream before input.
    insta::assert_snapshot!(command.args.join("\n"), @r"
    -v
    error
    -txt_page:3
    888
    -txt_format:3
    bitmap
    -txt_default_region:3
    -1
    -select_streams
    3
    -read_intervals
    %+8
    -show_frames
    -show_streams
    -show_programs
    -show_entries
    stream=index,codec_name,codec_type,id:program=program_id,program_num,pmt_pid,pcr_pid:program_stream=index,codec_name,codec_type,id:frame=media_type,pts,format,num_rects
    -of
    json
    synthetic-source.ts
    ");
}

#[test]
fn deadline_and_probe_errors_keep_timeout_cancellation_and_cleanup_distinct() {
    assert_eq!(
        check_deadline(Instant::now()),
        Err(SourceTeletextPageDecodeError::Deadline)
    );
    assert_eq!(
        check_deadline(Instant::now() + Duration::from_secs(16)),
        Err(SourceTeletextPageDecodeError::InvalidRequest)
    );
    assert_eq!(
        probe_error(FfprobeError::Cancelled {
            path: "synthetic".into()
        }),
        SourceTeletextPageDecodeError::Cancelled
    );
    assert_eq!(
        probe_error(FfprobeError::CleanupUnconfirmed {
            path: "synthetic".into()
        }),
        SourceTeletextPageDecodeError::CleanupUnconfirmed
    );
    assert_eq!(
        probe_error(FfprobeError::Timeout {
            path: "synthetic".into()
        }),
        SourceTeletextPageDecodeError::Deadline
    );
}

#[tokio::test]
async fn unowned_decode_starts_no_process_and_releases_admitted_work() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    let locker = EntityLocker::new();
    let admitted = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let hold = admitted.hold_work();
    drop(admitted);
    let result = FfprobeExtractor::new("nonexistent-synthetic-decoder")
        .decode_local_teletext_page(
            request(),
            PathBuf::from("synthetic-source.ts"),
            PathBuf::from("synthetic-source.ts"),
            Instant::now() + Duration::from_secs(5),
            hold,
        )
        .await;
    assert!(matches!(
        result,
        Err(SourceTeletextPageDecodeError::NoCleanupScope)
    ));
    assert_eq!(locker.active_teletext_work(), 0);
}

// These shell fixtures prove the actual Unix owner boundary. Portable parser
// tests remain above; native Windows job and real decoder acceptance are separate.
#[cfg(unix)]
mod unix {
    use super::*;
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    use std::os::unix::fs::PermissionsExt as _;

    const PRIMARY: &str = r#"{"format":{"format_name":"mpegts","duration":"1"},"streams":[{"index":0,"id":"0x100","codec_type":"video","codec_name":"mpeg2video","width":320,"height":180},{"index":3,"id":"0x103","codec_type":"subtitle","codec_name":"dvb_teletext","tags":{"language":"eng,eng"}}]}"#;
    const AUXILIARY: &str = r#"{"format":{"format_name":"mpegts"},"streams":[{"index":3,"id":"0x103","codec_type":"subtitle","codec_name":"dvb_teletext","extradata":"CgAQiA==","extradata_size":4,"tags":{"language":"eng,eng"}}],"programs":[{"program_id":1,"program_num":1,"pmt_pid":4096,"pcr_pid":256,"streams":[{"index":3}]}]}"#;

    struct Probe {
        _directory: tempfile::TempDir,
        extractor: FfprobeExtractor,
        source: PathBuf,
        pid_file: PathBuf,
    }

    impl Probe {
        fn new(decode: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let program = directory.path().join("probe");
            let script = format!(
                "#!/bin/sh\nauxiliary=false\ndecode=false\nfor arg in \"$@\"; do\n  if [ \"$arg\" = -data_dump_format ]; then auxiliary=true; fi\n  if [ \"$arg\" = -txt_page:3 ]; then decode=true; fi\ndone\nif $decode; then\n{decode}\nelif $auxiliary; then\ncat <<'AUXILIARY_JSON'\n{AUXILIARY}\nAUXILIARY_JSON\nelse\ncat <<'PRIMARY_JSON'\n{PRIMARY}\nPRIMARY_JSON\nfi\n"
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
            .expect("actual owned decode process must report its identity")
        }
    }

    #[tokio::test]
    async fn decoded_receipt_retains_the_requested_page_and_rejects_later_source_replacement() {
        let probe = Probe::new(&format!("cat <<'DECODE_JSON'\n{}\nDECODE_JSON", output()));
        let scope = LocalProbeCleanupScope::new();
        let requested = probe.request(&scope).await;
        let locker = EntityLocker::new();
        let admitted = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let decoded = scope
            .scope(probe.extractor.decode_local_teletext_page(
                requested.clone(),
                probe.source.clone(),
                probe.source.clone(),
                Instant::now() + Duration::from_secs(5),
                admitted.hold_work(),
            ))
            .await
            .unwrap();
        assert_eq!(decoded.request(), &requested);
        assert_eq!(decoded.events().count, 2);
        let input = decoded.native_burn_input();
        assert_eq!(input.streams[0].index, 3);
        assert_eq!(input.streams[0].source_teletext_page, Some(requested.page));
        assert_eq!(
            input.subtitle_method,
            Some(ersatzrs_ffmpeg::SubtitleMethod::Burn)
        );
        assert!(input.is_image_based_subtitle);
        let replacement = probe.source.with_extension("replacement");
        std::fs::write(&replacement, std::fs::read(&probe.source).unwrap()).unwrap();
        std::fs::File::options()
            .write(true)
            .open(&replacement)
            .unwrap()
            .set_times(
                std::fs::FileTimes::new()
                    .set_modified(probe.source.metadata().unwrap().modified().unwrap()),
            )
            .unwrap();
        std::fs::rename(&probe.source, probe.source.with_extension("retired")).unwrap();
        std::fs::rename(replacement, &probe.source).unwrap();
        assert_eq!(
            scope
                .scope(decoded.check_source(
                    Instant::now() + Duration::from_secs(5),
                    admitted.hold_work(),
                ))
                .await,
            Err(SourceTeletextPageDecodeError::SourceChanged)
        );
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        drop(admitted);
        assert_eq!(locker.active_teletext_work(), 0);
    }

    #[tokio::test]
    async fn abandoned_decode_caller_leaves_process_cleanup_with_the_actual_capacity_owner() {
        let probe = Probe::new("printf '%s' \"$$\" >\"${0}.pid\"; exec sleep 30");
        let scope = LocalProbeCleanupScope::new();
        let requested = probe.request(&scope).await;
        let locker = EntityLocker::new();
        let admitted = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
        let hold = admitted.hold_work();
        drop(admitted);
        let caller_scope = scope.clone();
        let extractor = probe.extractor.clone();
        let source = probe.source.clone();
        let caller = tokio::spawn(async move {
            caller_scope
                .scope(extractor.decode_local_teletext_page(
                    requested,
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
        tokio::time::timeout(Duration::from_secs(8), async {
            while locker.active_teletext_work() != 0 {
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("confirmed cleanup must eventually release admitted work");
        assert_eq!(
            nix::sys::signal::kill(pid, None),
            Err(nix::errno::Errno::ESRCH)
        );
        scope.close();
        assert_eq!(scope.wait_cleanup().await, Ok(()));
        assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_some());
    }
}
