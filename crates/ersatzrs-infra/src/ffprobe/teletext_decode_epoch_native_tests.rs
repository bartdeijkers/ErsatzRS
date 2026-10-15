//! A native corrupt first PES must not certify ordinary wrap initialization.

use super::{
    packet_pid, Duration, EntityLocker, Instant, LaterSource, LocalProbeCleanupScope,
    TeletextWorkGuard, PACKET_BYTES,
};

fn corrupt_first_video_pes(bytes: &mut [u8]) -> u64 {
    let (ordinal, packet) = bytes
        .chunks_exact_mut(PACKET_BYTES)
        .enumerate()
        .find(|(_, packet)| packet_pid(packet) == 256 && packet[1] & 0x40 != 0)
        .expect("actual first video PES packet");
    assert_eq!(packet[0], 0x47);
    assert_ne!(packet[3] & 0x10, 0, "PUSI must carry payload");
    let payload = if packet[3] & 0x20 != 0 {
        5 + usize::from(packet[4])
    } else {
        4
    };
    assert!(payload + 6 <= PACKET_BYTES);
    assert_eq!(&packet[payload..payload + 4], &[0, 0, 1, 0xe0]);
    assert_eq!(&packet[payload + 4..payload + 6], &[0, 0]);
    // n9.0 mpegts.c::new_pes_packet flags a nonzero declared length
    // mismatch corrupt when the next PUSI flushes this actual short PES.
    // Keep its timestamps, payload, transport framing and all other bytes.
    packet[payload + 4..payload + 6].copy_from_slice(&u16::MAX.to_be_bytes());
    u64::try_from(ordinal).unwrap()
}

#[tokio::test]
#[ignore = "requires explicit FFmpeg/FFprobe 9 for actual corrupt-first-PES wrap rejection"]
async fn native_corrupt_first_pes_does_not_qualify_ordinary_source_epoch() {
    let scope = LocalProbeCleanupScope::new();
    let mut source = LaterSource::new(&scope).await;
    let ordinary = scope
        .scope(source.extractor.extract(&source.path))
        .await
        .unwrap();
    let original_bytes = source.bytes.clone();
    let original_revision = source.request.binding.source_revision.clone();
    let first_pusi = corrupt_first_video_pes(&mut source.bytes);
    assert_eq!(first_pusi, 3);
    assert_eq!(source.bytes.len(), original_bytes.len());
    assert_eq!(
        source
            .bytes
            .iter()
            .zip(&original_bytes)
            .filter(|(actual, original)| actual != original)
            .count(),
        2,
        "only the two originally-zero PES length bytes change"
    );
    std::fs::write(&source.path, &source.bytes).unwrap();
    let acquired = scope
        .scope(source.extractor.extract_local_with_teletext(&source.path))
        .await
        .unwrap();
    assert_eq!(acquired.media.streams, ordinary.streams);
    let acquisition = acquired.teletext.expect("fresh actual source association");
    assert!(acquisition.is_current(&source.path));
    assert_ne!(acquisition.revision(), &original_revision);
    source.request.binding.source_revision = acquisition.revision().clone();

    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let prepared = source.prepare(&scope, &work, 888, 1).await;
    let trace = scope
        .scope(source.extractor.observe_prepared_local_demux_trace(
            &prepared,
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap();
    let first = trace.packets().first().expect("actual first native packet");
    assert_eq!(first.stream_index(), 0);
    assert_eq!(first.source_packet(), Some(first_pusi));
    assert_eq!(first.is_corrupt(), Some(true));
    assert_eq!(first.is_discard(), Some(false));
    assert!(scope
        .scope(source.extractor.observe_prepared_local_native_clock(
            &prepared,
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap()
        .is_none());
    assert_eq!(std::fs::read(&source.path).unwrap(), source.bytes);
    assert!(acquisition.is_current(&source.path));
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    eprintln!(
        "native first PES: observed corrupt flag at actual source packet {first_pusi}; ordinary tracks preserved, no ordinary epoch certificate, owned cleanup drained"
    );
}
