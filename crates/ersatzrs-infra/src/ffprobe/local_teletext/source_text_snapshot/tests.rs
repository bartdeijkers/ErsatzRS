use super::*;
use crate::ffprobe::{local_teletext::LocalFileRevision, LocalProbeCleanupScope};
use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
use ersatzrs_domain::{
    MediaSourceKind, SourceTeletextBindingIdentity, SourceTeletextRevision, TeletextPageNumber,
};
use std::time::Duration;

fn request() -> SourceTeletextPageDecodeRequest {
    SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(
            1,
            MediaSourceKind::Local,
            2,
            3,
            4,
            SourceTeletextRevision::new("synthetic-revision".into()).unwrap(),
        )
        .unwrap(),
        stream_index: 2,
        pid: 0x100,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    }
}

fn original() -> Vec<u8> {
    include_str!("../../../../../ersatzrs-ffmpeg/tests/fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn complete_source() -> Vec<u8> {
    let bytes = original();
    // Remove the independent fixture's adaptation-only selected prefix from
    // the source itself; complete text capture never silently filters it.
    [&bytes[..2 * 188], &bytes[3 * 188..]].concat()
}

fn padded_source() -> Vec<u8> {
    let original = complete_source();
    let mut bytes = original[..2 * 188].to_vec();
    let mut null = [0xff; 188];
    null[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    while bytes.len() / 188 < MAX_SOURCE_OBSERVATION_PACKETS + 1 {
        bytes.extend_from_slice(&null);
    }
    bytes.extend_from_slice(&original[2 * 188..]);
    bytes
}

async fn capture_bytes(
    bytes: &[u8],
    request: SourceTeletextPageDecodeRequest,
) -> Result<CompleteTextSnapshot> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic-source.ts");
    std::fs::write(&path, bytes).unwrap();
    let revision = LocalFileRevision::open(&path).unwrap().revision().unwrap();
    let guard = Arc::new(LocalTeletextSourceGuard::open(&path, &revision).unwrap());
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let result = scope
        .scope(capture_owned(
            guard,
            (path.clone(), path),
            request,
            Instant::now() + Duration::from_secs(5),
            work.hold_work(),
        ))
        .await;
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    result
}

#[tokio::test]
async fn complete_text_keeps_exact_bytes_and_page_after_observation_boundary() {
    for bytes in [complete_source(), padded_source()] {
        let captured = capture_bytes(&bytes, request()).await.unwrap();
        assert!(captured.selected_page_observed);
        assert_eq!(std::fs::read(captured.snapshot.path()).unwrap(), bytes);
        let mut psi = SourceTeletextPsiObserver::new(0x100).unwrap();
        for raw in bytes[..2 * 188].chunks_exact(188) {
            psi.push(raw.try_into().unwrap()).unwrap();
        }
        assert_eq!(
            captured.psi_fingerprint,
            headers::fingerprint(&psi.observation().unwrap())
        );
        let temporary = captured.snapshot.path().to_owned();
        drop(captured);
        assert!(!temporary.exists());
    }
}

#[tokio::test]
async fn complete_text_retains_one_pes_across_the_psi_continuation_boundary() {
    let fixture = original();
    let mut bytes = fixture[..2 * 188].to_vec();
    let mut null = [0xff; 188];
    null[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    while bytes.len() / 188 < MAX_SOURCE_OBSERVATION_PACKETS - 1 {
        bytes.extend_from_slice(&null);
    }
    let mut first = fixture[3 * 188..4 * 188].to_vec();
    first[8..10].copy_from_slice(&362_u16.to_be_bytes());
    let mut last = [0xff; 188];
    last[..4].copy_from_slice(&[0x47, 0x01, 0x00, 0x11]);
    for unit in last[4..].chunks_exact_mut(46) {
        unit[1] = 44;
    }
    bytes.extend_from_slice(&first);
    bytes.extend_from_slice(&last);
    let mut selected = request();
    selected.page = TeletextPageNumber::new(200).unwrap();
    let captured = capture_bytes(&bytes, selected).await.unwrap();
    assert!(captured.selected_page_observed);
    assert_eq!(std::fs::read(captured.snapshot.path()).unwrap(), bytes);
}

#[tokio::test]
async fn complete_text_never_repairs_prefix_or_incomplete_eof() {
    let bytes = complete_source();
    let mut incomplete = bytes.clone();
    let end = incomplete.len() - 188;
    incomplete[end + 8..end + 10].copy_from_slice(&362_u16.to_be_bytes());
    let early = [&bytes[2 * 188..3 * 188], &bytes].concat();
    for invalid in [
        original(),
        bytes[..bytes.len() - 1].to_vec(),
        incomplete,
        early,
        Vec::new(),
    ] {
        assert!(capture_bytes(&invalid, request()).await.is_err());
    }
}

#[tokio::test]
async fn complete_text_validates_late_transport_and_current_psi() {
    let bytes = padded_source();
    let mut changed = bytes.clone();
    let mut pmt = bytes[188..2 * 188].to_vec();
    pmt[3] = 0x11;
    pmt[10] = 0xc3;
    pmt[34..38].copy_from_slice(&[0xc1, 0xd5, 0x61, 0x52]);
    changed.extend_from_slice(&pmt);
    assert!(capture_bytes(&changed, request()).await.is_err());
    let mut discontinuity = bytes.clone();
    let last = discontinuity.len() - 188;
    discontinuity[last + 3] ^= 1;
    assert!(capture_bytes(&discontinuity, request()).await.is_err());
    for (pid, program) in [(0x101, 1), (0x100, 2), (0x100, 0)] {
        let mut wrong = request();
        wrong.pid = pid;
        wrong.program_number = program;
        assert!(capture_bytes(&bytes, wrong).await.is_err());
    }
    let mut absent = request();
    absent.page = TeletextPageNumber::new(777).unwrap();
    assert!(
        !capture_bytes(&bytes, absent)
            .await
            .unwrap()
            .selected_page_observed
    );
}

#[tokio::test]
async fn complete_text_reuses_source_fences_scope_and_deadline() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("synthetic-source.ts");
    let bytes = complete_source();
    std::fs::write(&path, &bytes).unwrap();
    let revision = LocalFileRevision::open(&path).unwrap().revision().unwrap();
    let guard = Arc::new(LocalTeletextSourceGuard::open(&path, &revision).unwrap());
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    assert!(capture_owned(
        guard.clone(),
        (path.clone(), path.clone()),
        request(),
        Instant::now() + Duration::from_secs(5),
        work.hold_work()
    )
    .await
    .is_err());
    let scope = LocalProbeCleanupScope::new();
    assert!(scope
        .scope(capture_owned(
            guard.clone(),
            (path.clone(), path.clone()),
            request(),
            Instant::now() - Duration::from_secs(1),
            work.hold_work()
        ))
        .await
        .is_err());
    std::fs::write(&path, [bytes, vec![0x47]].concat()).unwrap();
    assert!(scope
        .scope(capture_owned(
            guard,
            (path.clone(), path),
            request(),
            Instant::now() + Duration::from_secs(5),
            work.hold_work()
        ))
        .await
        .is_err());
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}
