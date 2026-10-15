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
    capture_bytes_with_span(bytes, pid, program, None).await
}

async fn capture_bytes_with_span(
    bytes: &[u8],
    pid: u16,
    program: u16,
    span: Option<SourceTeletextPacketSpan>,
) -> Result<LocalTeletextSourceWindow> {
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
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let result = scope
        .scope(async {
            match span {
                Some(span) => {
                    capture_source_packet_span_owned(
                        guard,
                        (path.clone(), path),
                        pid,
                        program,
                        span,
                        deadline,
                        work.hold_work(),
                    )
                    .await
                }
                None => {
                    capture_source_window_owned(
                        guard,
                        path.clone(),
                        path,
                        pid,
                        program,
                        deadline,
                        work.hold_work(),
                    )
                    .await
                }
            }
        })
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
    assert_eq!(window.evidence.first_source_packet, 0);
    assert_eq!(window.evidence.trimmed_source_tail_packets, 0);
    assert_eq!(window.evidence.decoder_packets, 15);
    assert_eq!(window.evidence.skipped_selected_prefix_packets, 1);
    assert!(window.evidence.ended_at_eof);
    assert!(!window.evidence.covers_entire_source_bytes());
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
    assert_eq!(window.evidence.trimmed_source_tail_packets, 32752);
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

#[test]
fn packet_span_checks_count_exclusive_end_and_byte_arithmetic() {
    let span = SourceTeletextPacketSpan::new(37, 123).unwrap();
    assert_eq!(span.first_packet(), 37);
    assert_eq!(span.packet_count(), 123);
    assert_eq!(span.end_packet_exclusive(), 160);
    assert_eq!(span.byte_offset(), 37 * 188);
    assert_eq!(span.byte_end_exclusive(), 160 * 188);
    for count in [0, 32769, u32::MAX] {
        assert_eq!(
            SourceTeletextPacketSpan::new(0, count),
            Err(SourceTeletextPacketSpanError::Count)
        );
    }
    for first in [u64::MAX, u64::MAX / 188] {
        assert_eq!(
            SourceTeletextPacketSpan::new(first, 1),
            Err(SourceTeletextPacketSpanError::Offset)
        );
    }
    let final_valid = SourceTeletextPacketSpan::new(u64::MAX / 188 - 1, 1).unwrap();
    assert_eq!(final_valid.byte_end_exclusive(), (u64::MAX / 188) * 188);
    assert!(SourceTeletextPacketSpan::new(0, 32768).is_ok());
}

fn complete_span_bytes() -> Vec<u8> {
    let bytes = fixture();
    // Omit the fixture's adaptation-only selected-PID prefix in the SOURCE
    // itself, so this independent raw span needs no decoder filtering.
    [&bytes[..2 * PACKET_BYTES], &bytes[3 * PACKET_BYTES..]].concat()
}

#[tokio::test]
async fn nonzero_span_reads_exact_held_bytes_without_using_invalid_earlier_source() {
    let actual = complete_span_bytes();
    let invalid = [0x55; 2 * PACKET_BYTES];
    let bytes = [invalid.as_slice(), &null_packet(), &actual].concat();
    let span = SourceTeletextPacketSpan::new(3, 15).unwrap();
    let captured = capture_bytes_with_span(&bytes, 0x100, 1, Some(span))
        .await
        .unwrap();
    assert_eq!(
        snapshot_bytes(&captured),
        bytes[usize::try_from(span.byte_offset()).unwrap()
            ..usize::try_from(span.byte_end_exclusive()).unwrap()]
    );
    assert_eq!(captured.evidence.first_source_packet, 3);
    assert_eq!(captured.evidence.source_packets, 15);
    assert_eq!(captured.evidence.decoder_packets, 15);
    assert_eq!(captured.evidence.skipped_selected_prefix_packets, 0);
    assert_eq!(captured.evidence.trimmed_source_tail_packets, 0);
    assert!(captured.evidence.ended_at_eof);
    assert!(!captured.evidence.covers_entire_source_bytes());
    assert_eq!(
        captured.evidence.decoder_sha256,
        <[u8; 32]>::from(Sha256::digest(&actual))
    );
    assert!(
        capture_bytes(&bytes, 0x100, 1).await.is_err(),
        "prefix API still validates its own actual source range"
    );
}

#[tokio::test]
async fn explicit_span_ignores_later_bytes_but_does_not_claim_source_eof() {
    let actual = complete_span_bytes();
    let bytes = [&null_packet()[..], &actual, &[0x55; PACKET_BYTES]].concat();
    let captured = capture_bytes_with_span(
        &bytes,
        0x100,
        1,
        Some(SourceTeletextPacketSpan::new(1, 15).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(snapshot_bytes(&captured), actual);
    assert_eq!(captured.evidence.first_source_packet, 1);
    assert!(!captured.evidence.ended_at_eof);
    assert_eq!(captured.evidence.trimmed_source_tail_packets, 0);
}

#[tokio::test]
async fn full_unfiltered_prefix_covers_only_source_bytes_without_temporal_claim() {
    let bytes = complete_span_bytes();
    let captured = capture_bytes(&bytes, 0x100, 1).await.unwrap();
    assert_eq!(snapshot_bytes(&captured), bytes);
    assert!(captured.evidence.covers_entire_source_bytes());
    assert_eq!(captured.evidence.first_source_packet, 0);
    assert_eq!(captured.evidence.source_packets, 15);
    assert_eq!(captured.evidence.decoder_packets, 15);
}

#[tokio::test]
async fn explicit_span_rejects_out_of_file_extent_without_reducing_requested_count() {
    let bytes = fixture();
    for span in [
        SourceTeletextPacketSpan::new(16, 1).unwrap(),
        SourceTeletextPacketSpan::new(1, 16).unwrap(),
    ] {
        assert!(capture_bytes_with_span(&bytes, 0x100, 1, Some(span))
            .await
            .is_err());
    }
}

#[tokio::test]
async fn explicit_span_cannot_borrow_current_tables_from_before_its_first_packet() {
    let bytes = fixture();
    let span = SourceTeletextPacketSpan::new(3, 13).unwrap();
    assert!(capture_bytes_with_span(&bytes, 0x100, 1, Some(span))
        .await
        .is_err());
}

#[tokio::test]
async fn nonzero_span_rejects_wrong_selection_and_late_current_pmt_change() {
    let actual = fixture();
    let bytes = [&null_packet()[..], &actual].concat();
    let span = SourceTeletextPacketSpan::new(1, 16).unwrap();
    for (pid, program) in [(0x101, 1), (0x100, 2)] {
        assert!(capture_bytes_with_span(&bytes, pid, program, Some(span))
            .await
            .is_err());
    }
    let changed = [bytes, advanced_current_pmt(&actual)].concat();
    assert!(capture_bytes_with_span(
        &changed,
        0x100,
        1,
        Some(SourceTeletextPacketSpan::new(1, 17).unwrap())
    )
    .await
    .is_err());
}

#[tokio::test]
async fn nonzero_capped_span_reports_exact_selected_prefix_and_partial_tail_omissions() {
    let actual = fixture();
    let mut pending: [u8; PACKET_BYTES] = actual[15 * PACKET_BYTES..].try_into().unwrap();
    pending[3] = 0x17;
    pending[8..10].copy_from_slice(&362_u16.to_be_bytes());
    let bytes = [&null_packet()[..], &actual, &pending, &null_packet()].concat();
    let captured = capture_bytes_with_span(
        &bytes,
        0x100,
        1,
        Some(SourceTeletextPacketSpan::new(1, 17).unwrap()),
    )
    .await
    .unwrap();
    let expected = [&actual[..2 * PACKET_BYTES], &actual[3 * PACKET_BYTES..]].concat();
    assert_eq!(snapshot_bytes(&captured), expected);
    assert_eq!(captured.evidence.first_source_packet, 1);
    assert_eq!(captured.evidence.source_packets, 17);
    assert_eq!(captured.evidence.decoder_packets, 15);
    assert_eq!(captured.evidence.skipped_selected_prefix_packets, 1);
    assert_eq!(captured.evidence.trimmed_source_tail_packets, 1);
    assert!(!captured.evidence.ended_at_eof);
}

fn separate_program_pcr_tail() -> (Vec<u8>, [u8; PACKET_BYTES], [u8; PACKET_BYTES]) {
    let fixture = fixture();
    let mut complete = complete_span_bytes();
    // Independently calculated MPEG CRC for this existing source PMT with
    // PCR_PID changed to 0x101; selected teletext remains PID 0x100.
    complete[PACKET_BYTES + 14] = 1;
    complete[PACKET_BYTES + 34..PACKET_BYTES + 38].copy_from_slice(&[0x58, 0x88, 0xd5, 0xe0]);
    let mut first: [u8; PACKET_BYTES] = fixture[2 * PACKET_BYTES..3 * PACKET_BYTES]
        .try_into()
        .unwrap();
    let mut second: [u8; PACKET_BYTES] = fixture[4 * PACKET_BYTES..5 * PACKET_BYTES]
        .try_into()
        .unwrap();
    first[2] = 1;
    second[2] = 1;
    (complete, first, second)
}

#[tokio::test]
async fn non_eof_span_retains_complete_nonselected_program_pcr_tail_byte_exactly() {
    let (complete, first_pcr, last_pcr) = separate_program_pcr_tail();
    let actual = [complete.as_slice(), &first_pcr, &last_pcr].concat();
    let bytes = [&null_packet()[..], &actual, &null_packet()].concat();
    let captured = capture_bytes_with_span(
        &bytes,
        0x100,
        1,
        Some(SourceTeletextPacketSpan::new(1, 17).unwrap()),
    )
    .await
    .unwrap();
    assert_eq!(snapshot_bytes(&captured), actual);
    assert_eq!(captured.evidence.pcr_pid, 0x101);
    assert_eq!(captured.evidence.first_source_packet, 1);
    assert_eq!(captured.evidence.source_packets, 17);
    assert_eq!(captured.evidence.decoder_packets, 17);
    assert_eq!(captured.evidence.skipped_selected_prefix_packets, 0);
    assert_eq!(captured.evidence.trimmed_source_tail_packets, 0);
    assert!(!captured.evidence.ended_at_eof);
    assert!(!captured.evidence.covers_entire_source_bytes());
    assert_eq!(
        captured.evidence.decoder_sha256,
        <[u8; 32]>::from(Sha256::digest(&actual))
    );
}

#[tokio::test]
async fn partial_selected_pes_still_trims_following_pcr_tail_only_before_real_eof() {
    let (complete, first_pcr, last_pcr) = separate_program_pcr_tail();
    let fixture = fixture();
    let mut pending: [u8; PACKET_BYTES] = fixture[15 * PACKET_BYTES..].try_into().unwrap();
    pending[3] = 0x17;
    pending[8..10].copy_from_slice(&362_u16.to_be_bytes());
    let actual = [complete.as_slice(), &pending, &first_pcr, &last_pcr].concat();
    let bytes = [&null_packet()[..], &actual, &null_packet()].concat();
    let span = SourceTeletextPacketSpan::new(1, 18).unwrap();
    let captured = capture_bytes_with_span(&bytes, 0x100, 1, Some(span))
        .await
        .unwrap();
    assert_eq!(snapshot_bytes(&captured), complete);
    assert_eq!(captured.evidence.decoder_packets, 15);
    assert_eq!(captured.evidence.trimmed_source_tail_packets, 3);
    assert!(!captured.evidence.ended_at_eof);
    let eof = [&null_packet()[..], &actual].concat();
    assert!(capture_bytes_with_span(&eof, 0x100, 1, Some(span))
        .await
        .is_err());
}

#[tokio::test]
async fn malformed_selected_tail_never_becomes_an_allowed_incomplete_trim() {
    let (complete, first_pcr, _) = separate_program_pcr_tail();
    let fixture = fixture();
    let mut pending: [u8; PACKET_BYTES] = fixture[15 * PACKET_BYTES..].try_into().unwrap();
    pending[3] = 0x17;
    pending[8..10].copy_from_slice(&362_u16.to_be_bytes());
    let mut wrong_counter = pending;
    wrong_counter[3] = 0x10;
    let mut bad_pes = pending;
    bad_pes[4] = 0xff;
    for malformed in [wrong_counter, bad_pes] {
        let bytes = [complete.as_slice(), &malformed, &first_pcr, &null_packet()].concat();
        assert!(capture_bytes_with_span(
            &bytes,
            0x100,
            1,
            Some(SourceTeletextPacketSpan::new(0, 17).unwrap()),
        )
        .await
        .is_err());
    }
}
