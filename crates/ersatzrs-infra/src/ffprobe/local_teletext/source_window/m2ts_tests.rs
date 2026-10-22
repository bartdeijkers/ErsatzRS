//! Physical M2TS bytes remain distinct from the unchanged inner transport.

use super::*;
use crate::ffprobe::local_teletext::{source_clock, source_text_snapshot, LocalFileRevision};

const FORMAT: SourceTeletextPacketFormat = SourceTeletextPacketFormat::M2ts192;
const STRIDE: usize = 192;

fn m2ts(bytes: &[u8]) -> Vec<u8> {
    let mut framed = Vec::new();
    for (ordinal, packet) in bytes.chunks_exact(PACKET_BYTES).enumerate() {
        framed.extend_from_slice(&(0x2000_0000 | u32::try_from(ordinal).unwrap()).to_be_bytes());
        framed.extend_from_slice(packet);
    }
    framed
}

#[tokio::test]
async fn m2ts_full_snapshot_clock_hash_and_complete_text_keep_every_source_byte() {
    use ersatzrs_domain::{MediaSourceKind, SourceTeletextBindingIdentity, TeletextPageNumber};
    let inner = complete_span_bytes();
    let bytes = m2ts(&inner);
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.m2ts");
    std::fs::write(&path, &bytes).unwrap();
    let revision = LocalFileRevision::open(&path).unwrap().revision().unwrap();
    let guard = Arc::new(LocalTeletextSourceGuard::open(&path, &revision).unwrap());
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    let window = scope
        .scope(capture_source_window_owned(
            guard.clone(),
            path.clone(),
            path.clone(),
            0x100,
            1,
            deadline,
            work.hold_work(),
        ))
        .await
        .unwrap();
    assert_eq!(snapshot_bytes(&window), bytes);
    assert_eq!(window.evidence.format, FORMAT);
    assert_eq!(
        (
            window.evidence.source_packets,
            window.evidence.decoder_packets
        ),
        (15, 15)
    );
    assert!(window.evidence.covers_entire_source_bytes());
    let expected: [u8; 32] = Sha256::digest(&bytes).into();
    assert_eq!(window.evidence.decoder_sha256, expected);
    assert_ne!(expected, <[u8; 32]>::from(Sha256::digest(&inner)));
    let clock = scope
        .scope(source_clock::observe_owned(
            guard.clone(),
            (path.clone(), path.clone()),
            0x100,
            window.evidence,
            deadline,
            work.hold_work(),
        ))
        .await
        .unwrap();
    assert_eq!(clock.format(), FORMAT);
    assert_eq!(
        clock.source_byte_length(),
        u64::try_from(bytes.len()).unwrap()
    );
    assert_eq!(clock.source_sha256(), expected);
    assert_eq!(clock.clock().packets_seen, 15);
    // A normalized TS188 digest cannot masquerade as this decoder snapshot.
    let mut wrong = window.evidence;
    wrong.decoder_sha256 = Sha256::digest(&inner).into();
    assert!(scope
        .scope(source_clock::observe_owned(
            guard.clone(),
            (path.clone(), path.clone()),
            0x100,
            wrong,
            deadline,
            work.hold_work(),
        ))
        .await
        .is_err());
    let request = crate::ffprobe::teletext_decode::SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(1, MediaSourceKind::Local, 2, 3, 4, revision)
            .unwrap(),
        stream_index: 2,
        pid: 0x100,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    };
    let text = scope
        .scope(source_text_snapshot::capture_owned(
            guard,
            (path.clone(), path),
            request,
            deadline,
            work.hold_work(),
        ))
        .await
        .unwrap();
    assert!(text.selected_page_observed);
    assert_eq!(std::fs::read(text.snapshot.path()).unwrap(), bytes);
    let window_path = window.snapshot.path().to_owned();
    let text_path = text.snapshot.path().to_owned();
    drop((window, text));
    assert!(!window_path.exists() && !text_path.exists());
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}

#[tokio::test]
async fn m2ts_selected_prefix_omission_and_partial_tail_trim_remove_whole_frames() {
    let original = fixture();
    let mut pending = original[15 * PACKET_BYTES..].to_vec();
    pending[3] = 0x17;
    pending[8..10].copy_from_slice(&362_u16.to_be_bytes());
    let inner = [original.as_slice(), &pending, &null_packet()].concat();
    let bytes = m2ts(&inner);
    let span = SourceTeletextPacketSpan::new_with_format(0, 17, FORMAT).unwrap();
    let window = capture_bytes_with_span(&bytes, 0x100, 1, Some(span))
        .await
        .unwrap();
    let expected = [&bytes[..2 * STRIDE], &bytes[3 * STRIDE..16 * STRIDE]].concat();
    assert_eq!(snapshot_bytes(&window), expected);
    assert_eq!(window.evidence.format, FORMAT);
    assert_eq!(window.evidence.source_packets, 17);
    assert_eq!(window.evidence.decoder_packets, 15);
    assert_eq!(window.evidence.skipped_selected_prefix_packets, 1);
    assert_eq!(window.evidence.trimmed_source_tail_packets, 1);
    assert!(!window.evidence.ended_at_eof);
    assert!(!window.evidence.covers_entire_source_bytes());
    assert_eq!(
        window.evidence.decoder_sha256,
        <[u8; 32]>::from(Sha256::digest(&expected))
    );
}

#[tokio::test]
async fn m2ts_nonzero_span_uses_its_declared_framing_without_borrowing_invalid_prefix() {
    let actual = m2ts(&complete_span_bytes());
    let bytes = [&[0x55; 2 * STRIDE][..], &m2ts(&null_packet()), &actual].concat();
    let span = SourceTeletextPacketSpan::new_with_format(3, 15, FORMAT).unwrap();
    assert_eq!(span.byte_offset(), 3 * STRIDE as u64);
    assert_eq!(span.byte_end_exclusive(), 18 * STRIDE as u64);
    let captured = capture_bytes_with_span(&bytes, 0x100, 1, Some(span))
        .await
        .unwrap();
    assert_eq!(snapshot_bytes(&captured), actual);
    assert_eq!(captured.evidence.format, FORMAT);
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
    assert!(capture_bytes(&bytes, 0x100, 1).await.is_err());
    assert!(capture_bytes_with_span(
        &bytes,
        0x100,
        1,
        Some(SourceTeletextPacketSpan::new(3, 15).unwrap())
    )
    .await
    .is_err());
}

#[tokio::test]
async fn m2ts_wrong_framing_malformed_frame_and_partial_eof_never_publish_snapshots() {
    let bytes = m2ts(&complete_span_bytes());
    let mut malformed = bytes.clone();
    malformed[4 * STRIDE + 4] = 0;
    for candidate in [&bytes[..bytes.len() - 1], malformed.as_slice()] {
        assert!(capture_bytes(candidate, 0x100, 1).await.is_err());
    }
    let wrong = SourceTeletextPacketSpan::new(0, 15).unwrap();
    assert!(capture_bytes_with_span(&bytes, 0x100, 1, Some(wrong))
        .await
        .is_err());
}
