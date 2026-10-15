use super::*;
use crate::ffprobe::LocalProbeCleanupScope;
use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};

fn fixture() -> Vec<u8> {
    include_str!("../../../../../ersatzrs-ffmpeg/tests/fixtures/teletext-source-assembly.hex")
        .split_whitespace()
        .map(|byte| u8::from_str_radix(byte, 16).unwrap())
        .collect()
}

fn null_packet() -> [u8; PACKET_BYTES] {
    let mut raw = [0xff; PACKET_BYTES];
    raw[..4].copy_from_slice(&[0x47, 0x1f, 0xff, 0x10]);
    raw
}

fn advanced_current_pmt(bytes: &[u8]) -> Vec<u8> {
    let mut pmt = bytes[PACKET_BYTES..2 * PACKET_BYTES].to_vec();
    pmt[3] = 0x11;
    pmt[10] = 0xc3;
    // Independent calibration CRC, not the production table writer.
    pmt[34..38].copy_from_slice(&[0xc1, 0xd5, 0x61, 0x52]);
    let mut observer = SourceTeletextPsiObserver::new(0x100).unwrap();
    for packet in bytes[..2 * PACKET_BYTES].chunks_exact(PACKET_BYTES) {
        observer.push(packet.try_into().unwrap()).unwrap();
    }
    observer.push(pmt.as_slice().try_into().unwrap()).unwrap();
    let report = observer.observation().unwrap();
    assert!(report.current_tables_changed);
    assert_eq!(report.programs[0].pmt.version, 1);
    pmt
}

async fn capture_bytes(bytes: &[u8], pid: u16, program: u16) -> Result<LocalTeletextSourceWindow> {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-source.ts");
    std::fs::write(&path, bytes).unwrap();
    let revision = super::super::LocalFileRevision::open(&path)
        .unwrap()
        .revision()
        .unwrap();
    let guard = Arc::new(LocalTeletextSourceGuard::open(&path, &revision).unwrap());
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let result = scope
        .scope(capture_source_window_owned(
            guard,
            path.clone(),
            path,
            pid,
            program,
            Instant::now() + std::time::Duration::from_secs(5),
            work.hold_work(),
        ))
        .await;
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    result
}

fn snapshot_bytes(window: &LocalTeletextSourceWindow) -> Vec<u8> {
    std::fs::read(window.snapshot.path()).unwrap()
}

#[tokio::test]
async fn same_byte_snapshot_retains_psi_and_completed_pes_with_exact_evidence() {
    let bytes = fixture();
    let window = capture_bytes(&bytes, 0x100, 1).await.unwrap();
    // The independent source has one selected adaptation-only clock before
    // its first PES start. It belongs to the explicitly omitted PID prefix.
    let expected = [&bytes[..2 * PACKET_BYTES], &bytes[3 * PACKET_BYTES..]].concat();
    assert_eq!(snapshot_bytes(&window), expected);
    assert_eq!(window.evidence.source_packets, 16);
    assert_eq!(window.evidence.decoder_packets, 15);
    assert_eq!(window.evidence.skipped_selected_prefix_packets, 1);
    assert!(window.evidence.ended_at_eof);
    assert_eq!(
        window.evidence.decoder_sha256,
        <[u8; 32]>::from(Sha256::digest(&expected))
    );
    assert_eq!(window.evidence.program_number, 1);
    assert_eq!(window.evidence.pmt_pid, 0x1000);
    assert_eq!(window.evidence.pcr_pid, 0x100);
    assert!(!format!("{window:?}").contains("private-source"));
    assert!(!format!("{window:?}").contains("ersatzrs-teletext-window"));
    let temporary = window.snapshot.path().to_owned();
    drop(window);
    assert!(!temporary.exists());
}

#[tokio::test]
async fn selected_payload_before_complete_psi_cannot_reach_the_decoder() {
    let bytes = fixture();
    let early = &bytes[3 * PACKET_BYTES..4 * PACKET_BYTES];
    let supplied = [early, bytes.as_slice()].concat();
    let window = capture_bytes(&supplied, 0x100, 1).await.unwrap();
    let expected = [&bytes[..2 * PACKET_BYTES], &bytes[3 * PACKET_BYTES..]].concat();
    assert_eq!(snapshot_bytes(&window), expected);
    assert_eq!(window.evidence.source_packets, 17);
    assert_eq!(window.evidence.skipped_selected_prefix_packets, 2);
}

#[tokio::test]
async fn a_late_current_pmt_change_fences_previously_valid_pes() {
    let mut bytes = fixture();
    let pmt = advanced_current_pmt(&bytes);
    bytes.extend_from_slice(&pmt);
    let error = capture_bytes(&bytes, 0x100, 1).await.unwrap_err();
    assert!(matches!(error, FfprobeError::InvalidOutput { ref path } if path == LABEL));
}

#[tokio::test]
async fn explicit_program_and_pid_must_match_validated_current_psi() {
    let bytes = fixture();
    for (pid, program) in [(0x100, 2), (0x101, 1), (0x100, 0), (0x1fff, 1)] {
        assert!(capture_bytes(&bytes, pid, program).await.is_err());
    }
}

#[tokio::test]
async fn eof_rejects_partial_ts_or_selected_pes_instead_of_trimming() {
    let bytes = fixture();
    assert!(capture_bytes(&bytes[..bytes.len() - 1], 0x100, 1)
        .await
        .is_err());
    let mut incomplete = bytes.clone();
    // Enlarge the last PES to two TS payloads while retaining only the first.
    incomplete[15 * PACKET_BYTES + 8..15 * PACKET_BYTES + 10]
        .copy_from_slice(&362_u16.to_be_bytes());
    assert!(capture_bytes(&incomplete, 0x100, 1).await.is_err());
}

#[tokio::test]
async fn capped_window_trims_an_incomplete_tail_to_the_last_complete_pes() {
    let bytes = fixture();
    let mut capped = bytes.clone();
    let null = null_packet();
    while capped.len() / PACKET_BYTES < MAX_SOURCE_OBSERVATION_PACKETS - 1 {
        capped.extend_from_slice(&null);
    }
    let mut pending: [u8; PACKET_BYTES] = bytes[15 * PACKET_BYTES..].try_into().unwrap();
    pending[3] = 0x17;
    pending[8..10].copy_from_slice(&362_u16.to_be_bytes());
    capped.extend_from_slice(&pending);
    capped.extend_from_slice(&null);
    let window = capture_bytes(&capped, 0x100, 1).await.unwrap();
    let expected = [&bytes[..2 * PACKET_BYTES], &bytes[3 * PACKET_BYTES..]].concat();
    assert_eq!(snapshot_bytes(&window), expected);
    assert_eq!(window.evidence.source_packets, 32_768);
    assert_eq!(window.evidence.decoder_packets, 15);
    assert!(!window.evidence.ended_at_eof);
}

#[tokio::test]
async fn a_late_pmt_change_is_not_hidden_by_capped_tail_trimming() {
    let mut bytes = fixture();
    let pmt = advanced_current_pmt(&bytes);
    let null = null_packet();
    while bytes.len() / PACKET_BYTES < MAX_SOURCE_OBSERVATION_PACKETS - 1 {
        bytes.extend_from_slice(&null);
    }
    bytes.extend_from_slice(&pmt);
    bytes.extend_from_slice(&null);
    assert!(capture_bytes(&bytes, 0x100, 1).await.is_err());
}

#[tokio::test]
async fn captured_file_uses_the_held_handle_at_explicit_offsets() {
    use std::io::{Seek as _, SeekFrom};
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.ts");
    let bytes = fixture();
    std::fs::write(&path, &bytes).unwrap();
    let revision = super::super::LocalFileRevision::open(&path)
        .unwrap()
        .revision()
        .unwrap();
    let guard = Arc::new(LocalTeletextSourceGuard::open(&path, &revision).unwrap());
    let mut held = guard.guard.original.as_file();
    held.seek(SeekFrom::End(0)).unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let window = scope
        .scope(capture_source_window_owned(
            guard,
            path.clone(),
            path,
            0x100,
            1,
            Instant::now() + std::time::Duration::from_secs(2),
            work.hold_work(),
        ))
        .await
        .unwrap();
    let expected = [&bytes[..2 * PACKET_BYTES], &bytes[3 * PACKET_BYTES..]].concat();
    assert_eq!(snapshot_bytes(&window), expected);
    scope.close();
    scope.wait_cleanup().await.unwrap();
}

#[tokio::test]
async fn missing_scope_never_reads_or_leaks_a_source_snapshot() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("private-source.ts");
    std::fs::write(&path, fixture()).unwrap();
    let revision = super::super::LocalFileRevision::open(&path)
        .unwrap()
        .revision()
        .unwrap();
    let guard = Arc::new(LocalTeletextSourceGuard::open(&path, &revision).unwrap());
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let error = capture_source_window_owned(
        guard,
        path.clone(),
        path,
        0x100,
        1,
        Instant::now() + std::time::Duration::from_secs(2),
        work.hold_work(),
    )
    .await
    .unwrap_err();
    assert!(matches!(error, FfprobeError::Cancelled { ref path } if path == LABEL));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}
