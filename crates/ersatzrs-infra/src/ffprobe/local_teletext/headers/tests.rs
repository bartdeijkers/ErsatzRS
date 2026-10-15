use super::*;
use crate::ffprobe::local_teletext::{LocalTeletextHeaderProblem as Problem, LocalTeletextHeaders};
use crate::ffprobe::teletext_probe::{self, TeletextProbeEncoding};

fn fixture() -> Vec<u8> {
    include_str!("../../../../../ersatzrs-ffmpeg/tests/fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|v| u8::from_str_radix(v, 16).unwrap())
        .collect()
}

fn acquisition(path: &Path) -> LocalTeletextAcquisition {
    let guard = super::super::LocalFileRevision::open(path).unwrap();
    let probe=serde_json::from_str(r#"{"format":{"format_name":"mpegts"},"streams":[{"index":2,"id":"0x100","codec_type":"subtitle","codec_name":"dvb_teletext","extradata":"CgAQiA==","extradata_size":4,"tags":{"language":"eng,eng"}}],"programs":[{"program_id":1,"program_num":1,"pmt_pid":4096,"pcr_pid":256,"streams":[{"index":2}]}]}"#).unwrap();
    LocalTeletextAcquisition {
        snapshot: teletext_probe::project(&probe, TeletextProbeEncoding::Ffprobe9Base64),
        revision: guard.revision().unwrap(),
        guard,
        headers: LocalTeletextHeaders::NotAcquired(Problem::NoManagedScope),
    }
}

#[tokio::test]
async fn held_source_read_keeps_wire_headers_and_exact_current_program() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ts");
    std::fs::write(&path, fixture()).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let before = acquisition(&path);
    let revision = before.revision().clone();
    let actual = scope.scope(acquire(before, &path)).await.unwrap();
    assert_eq!(actual.revision(), &revision);
    assert!(actual.is_current(&path));
    let window = actual.headers().for_stream(2).expect("actual header read");
    assert_eq!(window.packet_count, 16);
    assert_eq!(window.complete_pes, 7);
    assert!(window.ended_at_eof);
    assert_eq!(
        (
            window.programs[0].program_number,
            window.programs[0].pmt_pid,
            window.programs[0].stream_type
        ),
        (1, 0x1000, 6)
    );
    assert_eq!(
        window
            .headers
            .iter()
            .map(|h| (h.wire_magazine, h.wire_page, h.completion_packet))
            .collect::<Vec<_>>(),
        [(2, 0, 3), (2, 1, 7), (0, 0x88, 11), (0, 0x99, 15)]
    );
    assert_eq!(scope.cleanup_status(), Ok(true));
}

#[tokio::test]
async fn held_source_read_bound_is_explicit_without_claiming_eof() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ts");
    let mut bytes = fixture();
    let mut null = [0xff; 188];
    null[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    for _ in bytes.len() / 188..=MAX_SOURCE_OBSERVATION_PACKETS {
        bytes.extend_from_slice(&null);
    }
    std::fs::write(&path, bytes).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let actual = scope
        .scope(acquire(acquisition(&path), &path))
        .await
        .unwrap();
    let window = actual.headers().for_stream(2).unwrap();
    assert_eq!(window.packet_count, 32768);
    assert!(!window.ended_at_eof);
    assert_eq!(window.headers.len(), 4);
}

#[tokio::test]
async fn optional_read_failure_preserves_descriptors_and_source_guard() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ts");
    let mut bytes = fixture();
    bytes.push(0);
    std::fs::write(&path, bytes).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let before = acquisition(&path);
    let snapshot = before.snapshot().clone();
    let actual = scope.scope(acquire(before, &path)).await.unwrap();
    assert!(matches!(
        actual.headers(),
        LocalTeletextHeaders::NotAcquired(Problem::UnsupportedFraming)
    ));
    assert_eq!(actual.snapshot(), &snapshot);
    assert!(actual.is_current(&path));
    assert_eq!(scope.cleanup_status(), Ok(true));
}

#[tokio::test]
async fn changed_source_or_missing_scope_never_acquires_headers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ts");
    std::fs::write(&path, fixture()).unwrap();
    let unmanaged = acquire(acquisition(&path), &path).await.unwrap();
    assert!(matches!(
        unmanaged.headers(),
        LocalTeletextHeaders::NotAcquired(Problem::NoManagedScope)
    ));
    let before = acquisition(&path);
    std::fs::write(&path, b"changed").unwrap();
    let scope = LocalProbeCleanupScope::new();
    let actual = scope.scope(acquire(before, &path)).await.unwrap();
    assert!(matches!(
        actual.headers(),
        LocalTeletextHeaders::NotAcquired(Problem::SourceChanged)
    ));
}

#[tokio::test]
async fn abandoned_caller_retains_capacity_until_the_actual_blocking_worker_ends() {
    let scope = LocalProbeCleanupScope::new();
    let task_scope = scope.clone();
    let (started, ready) = oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        task_scope
            .scope(run_owned(Path::new("synthetic"), READ_TIMEOUT, move |_| {
                let _ = started.send(());
                wait.recv().unwrap();
                Ok(())
            }))
            .await
    });
    ready.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(scope.cleanup_status(), Ok(false));
    scope
        .scope(async {
            let owners = (0..63)
                .map(|_| local_cleanup_scope::register_current().unwrap().unwrap())
                .collect::<Vec<_>>();
            assert!(matches!(
                local_cleanup_scope::register_current(),
                Err(AdmissionError::Capacity)
            ));
            for owner in owners {
                owner.complete(true);
            }
        })
        .await;
    release.send(()).unwrap();
    scope.wait_cleanup().await.unwrap();
    assert_eq!(scope.cleanup_status(), Ok(true));
}

#[tokio::test(start_paused = true)]
async fn early_timeout_reply_retains_the_registered_owner_until_actual_join() {
    let scope = LocalProbeCleanupScope::new();
    let task_scope = scope.clone();
    let (started, ready) = oneshot::channel();
    let (release, wait) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        task_scope
            .scope(run_owned(
                Path::new("synthetic"),
                Duration::ZERO,
                move |_| {
                    let _ = started.send(());
                    wait.recv().unwrap();
                    Ok(())
                },
            ))
            .await
    });
    ready.await.unwrap();
    tokio::time::advance(SETTLEMENT_TIMEOUT + Duration::from_secs(1)).await;
    assert!(matches!(
        task.await.unwrap(),
        Err(FfprobeError::CleanupUnconfirmed { .. })
    ));
    assert_eq!(scope.cleanup_status(), Ok(false));
    release.send(()).unwrap();
    scope.wait_cleanup().await.unwrap();
    assert_eq!(scope.cleanup_status(), Ok(true));
}

#[tokio::test]
async fn closed_scope_never_launches_a_blocking_source_read() {
    let scope = LocalProbeCleanupScope::new();
    scope.close();
    let result: crate::ffprobe::Result<()> = scope
        .scope(run_owned(Path::new("synthetic"), READ_TIMEOUT, |_| {
            panic!("must not run")
        }))
        .await;
    assert!(matches!(result, Err(FfprobeError::Cancelled { .. })));
    assert_eq!(scope.cleanup_status(), Ok(true));
}

#[tokio::test]
async fn current_psi_change_discards_headers_instead_of_binding_them_to_final_tables() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ts");
    let mut bytes = fixture();
    let mut pmt = bytes[188..376].to_vec();
    pmt[3] = 0x11;
    pmt[10] = 0xc3;
    // MPEG CRC for this exact independent PMT with only its version advanced.
    pmt[34..38].copy_from_slice(&[0xc1, 0xd5, 0x61, 0x52]);
    bytes.extend_from_slice(&pmt);
    std::fs::write(&path, bytes).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let actual = scope
        .scope(acquire(acquisition(&path), &path))
        .await
        .unwrap();
    assert!(matches!(
        actual.headers(),
        LocalTeletextHeaders::NotAcquired(Problem::ChangedPsi)
    ));
    assert!(actual.is_current(&path));
    assert_eq!(scope.cleanup_status(), Ok(true));
}

#[tokio::test]
async fn headers_before_complete_current_psi_are_never_assigned_retroactively() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ts");
    let cycle = fixture();
    let mut bytes = cycle[2 * 188..].to_vec();
    bytes.extend_from_slice(&cycle[..2 * 188]);
    std::fs::write(&path, bytes).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let actual = scope
        .scope(acquire(acquisition(&path), &path))
        .await
        .unwrap();
    let window = actual.headers().for_stream(2).unwrap();
    assert!(window.headers.is_empty());
    assert_eq!(window.complete_pes, 0);
    assert!(window.unobserved_prefix);
}
