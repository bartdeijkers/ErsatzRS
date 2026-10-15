//! Requested native-format interval -> held PCR span -> same-byte page decoding.

use super::*;
use crate::{
    ffprobe::teletext_decode::{
        PreparedLocalTeletextSource, SourceTeletextCueEvidence, SourceTeletextCueRequest,
    },
    playback::source_teletext::runtime::SourceTeletextPlaybackInterval,
};

async fn observe(
    source: &LaterSource,
    prepared: &PreparedLocalTeletextSource,
    scope: &LocalProbeCleanupScope,
    work: &TeletextWorkGuard,
    request: SourceTeletextCueRequest,
) -> Option<SourceTeletextCueEvidence> {
    scope
        .scope(source.extractor.observe_prepared_local_native_cue(
            prepared,
            request,
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap()
}

fn assert_bookends(source: &LaterSource, evidence: &SourceTeletextCueEvidence) {
    let clock = evidence.native_clock();
    assert_eq!(
        clock.av_origin().clock().source_sha256(),
        <[u8; 32]>::from(Sha256::digest(&source.bytes))
    );
    let first = clock.first_pcr_ticks();
    let before = first + i64::try_from(evidence.before().elapsed_ticks).unwrap();
    let after = first + i64::try_from(evidence.after().elapsed_ticks).unwrap();
    assert!(before <= evidence.target_ticks().0);
    assert!(after >= evidence.target_ticks().1);
    let span = evidence.span();
    assert_eq!(span.first_packet(), evidence.before().source_packet);
    assert_eq!(
        span.end_packet_exclusive(),
        evidence.after().source_packet + 1
    );
    assert!(span.first_packet() > MAX_SOURCE_OBSERVATION_PACKETS as u64);
    assert!(span.byte_end_exclusive() < u64::try_from(source.bytes.len()).unwrap());
}

#[tokio::test]
#[ignore = "requires explicit FFmpeg/FFprobe 9 with libzvbi for actual requested-cue page decoding"]
async fn native_requested_format_cue_locates_and_decodes_later_pages_without_packet_rate_guesses() {
    let scope = LocalProbeCleanupScope::new();
    let source = LaterSource::new(&scope).await;
    let ordinary = scope
        .scope(source.extractor.extract(&source.path))
        .await
        .unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let requested = SourceTeletextPlaybackInterval::from_start_and_duration(
        Duration::new(40, 900),
        Duration::new(12, 900),
    )
    .unwrap();
    let effective = requested.effective_native_interval().unwrap();
    assert_eq!(effective.end_microseconds(), 52_000_000);
    assert_eq!(requested.end().as_micros(), 52_000_001);
    let cue = SourceTeletextCueRequest::new(
        effective.start_microseconds(),
        effective.end_microseconds(),
        134,
        2_700_000,
    )
    .unwrap();
    for page in [888, 200] {
        let prepared = source.prepare(&scope, &work, page, 1).await;
        let evidence = observe(&source, &prepared, &scope, &work, cue)
            .await
            .expect("actual finite native-format bookends");
        assert_bookends(&source, &evidence);
        let decoded = scope
            .scope(source.extractor.decode_prepared_local_teletext_cue(
                &prepared,
                &evidence,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await
            .expect("requested-clock-derived span contains actual requested-page headers/bitmaps");
        assert!(std::sync::Arc::ptr_eq(&prepared.guard, &decoded.guard));
        assert!(decoded.events().count > 0);
        assert!(decoded.headers().unwrap().headers().iter().any(|item| item
            .header
            .display_page()
            .map(TeletextPageNumber::get)
            == Some(page)));
        assert!(!decoded.window().covers_entire_source_bytes());
        eprintln!("requested native-format cue: page={page}, exact={requested:?}, effective={effective:?}, targets={:?}, before={:?}, after={:?}, span={:?}, bitmaps={}, decoder_sha256={}; no enabled-stream consumer/initial-page-state admission", evidence.target_ticks(), evidence.before(), evidence.after(), evidence.span(), decoded.events().count, hex_text(&decoded.window().decoder_sha256));
    }
    let prepared = source.prepare(&scope, &work, 888, 1).await;
    for rejected in [
        SourceTeletextCueRequest::new(70_000_000, 75_000_000, 134, 2_700_000).unwrap(),
        SourceTeletextCueRequest::new(40_000_000, 52_000_000, 1, 2_700_000).unwrap(),
        SourceTeletextCueRequest::new(20_000_000, 52_000_000, 134, 2_700_000).unwrap(),
    ] {
        assert!(observe(&source, &prepared, &scope, &work, rejected)
            .await
            .is_none());
    }
    let early = SourceTeletextCueRequest::new(0, 2_000_000, 134, 2_700_000).unwrap();
    let early = observe(&source, &prepared, &scope, &work, early)
        .await
        .unwrap();
    assert!(matches!(
        scope
            .scope(source.extractor.decode_prepared_local_teletext_cue(
                &prepared,
                &early,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await,
        Err(SourceTeletextPageDecodeError::InvalidTransportWindow)
    ));
    let evidence = observe(&source, &prepared, &scope, &work, cue)
        .await
        .unwrap();
    assert_changed_tail_rejected(&source, &prepared, &evidence, &scope, &work).await;
    assert_eq!(
        scope
            .scope(source.extractor.extract(&source.path))
            .await
            .unwrap()
            .streams,
        ordinary.streams
    );
    assert_eq!(std::fs::read(&source.path).unwrap(), source.bytes);
    drop(prepared);
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}

async fn assert_changed_tail_rejected(
    source: &LaterSource,
    prepared: &PreparedLocalTeletextSource,
    cue: &SourceTeletextCueEvidence,
    scope: &LocalProbeCleanupScope,
    work: &TeletextWorkGuard,
) {
    let modified = std::fs::metadata(&source.path).unwrap().modified().unwrap();
    let file = std::fs::File::options()
        .write(true)
        .open(&source.path)
        .unwrap();
    let ordinal = source
        .bytes
        .chunks_exact(PACKET_BYTES)
        .enumerate()
        .find(|(i, packet)| {
            u64::try_from(*i).unwrap() >= cue.span().end_packet_exclusive()
                && packet_pid(packet) == 0x1fff
        })
        .map(|(i, _)| i)
        .expect("actual nonselected null tail outside decoded cue");
    let mut changed = source.bytes.clone();
    changed[ordinal * PACKET_BYTES + 100] ^= 1;
    std::fs::write(&source.path, changed).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    scope
        .scope(prepared.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()))
        .await
        .unwrap();
    assert!(matches!(scope.scope(source.extractor.decode_prepared_local_teletext_cue(
        prepared, cue, Instant::now() + Duration::from_secs(15), work.hold_work(),
    )).await, Err(SourceTeletextPageDecodeError::SourceChanged)),
    "unchanged decoder bytes and restored metadata cannot reuse changed full-source cue evidence");
    std::fs::write(&source.path, &source.bytes).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let mismatched = source.prepare(scope, work, 200, 1).await;
    assert!(matches!(
        scope
            .scope(source.extractor.decode_prepared_local_teletext_cue(
                &mismatched,
                cue,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await,
        Err(SourceTeletextPageDecodeError::SourceChanged)
    ));
    eprintln!("native cue decoder/source join rejects changed actual null tail at packet{ordinal} despite unchanged decoder span and restored mtime; exact request mismatch rejects before decoding");
}
