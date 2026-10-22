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
    assert!(span.first_packet() < evidence.before().source_packet);
    assert_eq!(
        span.first_packet(),
        evidence.page_prelude().psi_start_packet()
    );
    assert!(evidence.page_prelude().source_pusi_packet() < evidence.before().source_packet);
    assert!(evidence.page_prelude().header().controls.erase_page());
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
    Box::pin(requested_format_cue(
        crate::ffprobe::SourceTeletextPacketFormat::Ts188,
    ))
    .await;
}

#[tokio::test]
#[ignore = "requires explicit FFmpeg/FFprobe 9 with libzvbi for original M2TS source clock and page decoding"]
async fn native_m2ts_requested_format_cue_retains_original_clock_bytes_and_later_pages() {
    Box::pin(requested_format_cue(
        crate::ffprobe::SourceTeletextPacketFormat::M2ts192,
    ))
    .await;
}

async fn requested_format_cue(format: crate::ffprobe::SourceTeletextPacketFormat) {
    let scope = LocalProbeCleanupScope::new();
    let source = Box::pin(framed_source(&scope, format)).await;
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
        let baseline = source.decode(&scope, &work, page, 1).await.unwrap();
        assert_eq!(decoded.events().count, 8);
        assert_eq!(
            decoded.events(),
            baseline.events(),
            "the initial native bitmap event must survive with its original timestamps"
        );
        assert!(decoded.headers().unwrap().headers().iter().any(|item| item
            .header
            .display_page()
            .map(TeletextPageNumber::get)
            == Some(page)));
        assert!(!decoded.window().covers_entire_source_bytes());
        eprintln!("requested native-format cue: page={page}, exact={requested:?}, effective={effective:?}, targets={:?}, before={:?}, after={:?}, span={:?}, prelude={:?}, events={:?}, decoder_sha256={}; no complete inherited-state/consumer admission", evidence.target_ticks(), evidence.before(), evidence.after(), evidence.span(), evidence.page_prelude(), decoded.events(), hex_text(&decoded.window().decoder_sha256));
    }
    let prepared = source.prepare(&scope, &work, 888, 1).await;
    assert_prior_page_reset(&source, &prepared, &scope, &work).await;
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
    assert!(
        observe(&source, &prepared, &scope, &work, early)
            .await
            .is_none(),
        "PCR coverage without the requested page reset/header grants no cue"
    );
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

async fn framed_source(
    scope: &LocalProbeCleanupScope,
    format: crate::ffprobe::SourceTeletextPacketFormat,
) -> LaterSource {
    let mut source = LaterSource::new(scope).await;
    if format == crate::ffprobe::SourceTeletextPacketFormat::M2ts192 {
        let wrapped = source
            .bytes
            .chunks_exact(PACKET_BYTES)
            .flat_map(|packet| [0; 4].into_iter().chain(packet.iter().copied()))
            .collect::<Vec<_>>();
        std::fs::write(&source.path, &wrapped).unwrap();
        source.bytes = wrapped;
        source.span = SourceTeletextPacketSpan::new_with_format(
            source.span.first_packet(),
            source.span.packet_count(),
            format,
        )
        .unwrap();
        let acquired = scope
            .scope(source.extractor.extract_local_with_teletext(&source.path))
            .await
            .unwrap();
        source.request.binding.source_revision = acquired.teletext.unwrap().revision().clone();
    }
    source
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
        .chunks_exact(cue.span().format().packet_bytes())
        .enumerate()
        .find(|(i, packet)| {
            u64::try_from(*i).unwrap() >= cue.span().end_packet_exclusive()
                && packet_pid(&packet[cue.span().format().sync_offset()..]) == 0x1fff
        })
        .map(|(i, _)| i)
        .expect("actual nonselected null tail outside decoded cue");
    let format = cue.span().format();
    let mut offsets = vec![format.sync_offset() + 100];
    if format == crate::ffprobe::SourceTeletextPacketFormat::M2ts192 {
        offsets.push(0); // An opaque prefix outside the cue remains source bytes.
    }
    for offset in offsets {
        let mut changed = source.bytes.clone();
        changed[ordinal * format.packet_bytes() + offset] ^= 1;
        std::fs::write(&source.path, changed).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        scope
            .scope(
                prepared.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()),
            )
            .await
            .unwrap();
        assert!(matches!(scope.scope(source.extractor.decode_prepared_local_teletext_cue(
            prepared, cue, Instant::now() + Duration::from_secs(15), work.hold_work(),
        )).await, Err(SourceTeletextPageDecodeError::SourceChanged)),
        "unchanged decoder bytes and restored metadata cannot reuse changed full-source cue evidence");
        std::fs::write(&source.path, &source.bytes).unwrap();
        file.set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
    }
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

async fn assert_prior_page_reset(
    source: &LaterSource,
    prepared: &PreparedLocalTeletextSource,
    scope: &LocalProbeCleanupScope,
    work: &TeletextWorkGuard,
) {
    let request = SourceTeletextCueRequest::new(43_000_000, 52_000_000, 134, 2_700_000).unwrap();
    let cue = observe(source, prepared, scope, work, request)
        .await
        .unwrap();
    let prelude = cue.page_prelude();
    assert!(
        prelude.native_pts_ticks() < cue.target_ticks().0,
        "an already active page reset must precede the requested cue, not the next repetition"
    );
    assert!(prelude.source_pusi_packet() < cue.before().source_packet);
    let decoded = scope
        .scope(source.extractor.decode_prepared_local_teletext_cue(
            prepared,
            &cue,
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap();
    assert_eq!(
        decoded.events().count,
        6,
        "retain the active reset and all five later page events"
    );
    assert!(prelude.is_retained_in(
        decoded.headers().unwrap(),
        decoded.window().first_source_packet,
        decoded.window().skipped_selected_prefix_packets
    ));
    eprintln!("native43..52 prior-page state: prelude={prelude:?}, before={:?}, span={:?}, events={:?}; reset anchor survives before the requested start", cue.before(), cue.span(), decoded.events());
}
