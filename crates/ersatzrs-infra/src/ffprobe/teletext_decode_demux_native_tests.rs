//! Original-source demux/raw AV association, including a genuine timestamp wrap.

use std::collections::BTreeMap;

use ersatzrs_ffmpeg::teletext::{
    source_av_timestamps::{SourceAvStreamIdentity, SourceAvStreamKind, SourceAvTimestampObserver},
    source_native_epoch::{SourceNativeEpoch, SourceNativeWrapBehavior},
};

use super::*;
use crate::ffprobe::{
    teletext_decode::PreparedLocalTeletextSource, teletext_demux::NativeSourceDemuxTrace,
};

fn assert_raw_av_trace(bytes: &[u8], trace: &NativeSourceDemuxTrace) -> SourceNativeEpoch {
    assert!(!trace.hit_packet_limit());
    let first = trace.packets().first().unwrap();
    assert_eq!(first.stream_index(), 0);
    assert_eq!(first.source_packet(), Some(3));
    let policy = SourceNativeEpoch::from_first_demux_timestamps(first.pts(), first.dts());
    let mut observers = Vec::new();
    let mut packets = BTreeMap::new();
    for stream in trace.streams() {
        let kind = match stream.codec_type() {
            "audio" => SourceAvStreamKind::Audio,
            "video" => SourceAvStreamKind::Video,
            _ => continue,
        };
        let base = stream.time_base().unwrap();
        assert_eq!((base.numerator(), base.denominator()), (1, 90_000));
        observers.push(
            SourceAvTimestampObserver::new_source_epoch(
                SourceAvStreamIdentity {
                    stream_index: stream.index(),
                    pid: stream.pid(),
                    kind,
                },
                0,
            )
            .unwrap(),
        );
        for packet in trace
            .packets()
            .iter()
            .filter(|packet| packet.stream_index() == stream.index())
        {
            assert!(packets
                .insert((stream.index(), packet.source_packet().unwrap()), packet)
                .is_none());
        }
    }
    let expected = packets.len();
    for raw in bytes.chunks_exact(PACKET_BYTES) {
        for observer in &mut observers {
            if let Some(header) = observer.push(raw.try_into().unwrap()).unwrap() {
                let index = observer.report().unwrap().stream.stream_index;
                if let Some(native) = packets.remove(&(index, header.source_pusi_packet)) {
                    assert_eq!(
                        header.pts.and_then(|pts| policy
                            .apply_timestamp(i64::try_from(pts).unwrap())
                            .unwrap()),
                        native.pts()
                    );
                    // MPEG-TS demux copies PTS into native DTS on PTS-only PES;
                    // retain that explicit distinction from raw DTS absence.
                    assert_eq!(
                        header.dts.or(header.pts).and_then(|dts| policy
                            .apply_timestamp(i64::try_from(dts).unwrap())
                            .unwrap()),
                        native.dts()
                    );
                }
            }
        }
    }
    assert!(
        packets.is_empty(),
        "every native AV position must match an actual raw PES header"
    );
    let mut clock =
        SourceTeletextClockObserver::new_source_epoch(trace.programs()[0].pcr_pid(), 0).unwrap();
    for raw in bytes.chunks_exact(PACKET_BYTES) {
        clock.push(raw.try_into().unwrap()).unwrap();
    }
    let clock = clock.report().unwrap();
    let start = policy
        .apply_pcr_ticks(clock.first.unwrap().raw_ticks)
        .unwrap()
        .unwrap();
    let end = policy
        .apply_pcr_ticks(clock.last.unwrap().raw_ticks)
        .unwrap()
        .unwrap();
    assert_eq!(
        start + i64::try_from(clock.last.unwrap().elapsed_ticks).unwrap(),
        end
    );
    eprintln!("native demux association: AV_PES={expected}, all_packets={}, candidate_policy={policy:?}, candidate_PCR_first={start}, candidate_PCR_last={end}, raw_wraps={}; no ordinary-consumer/cue grant", trace.packets().len(), clock.wraps);
    policy
}

async fn observe(
    source: &LaterSource,
    scope: &LocalProbeCleanupScope,
    work: &TeletextWorkGuard,
    prepared: &PreparedLocalTeletextSource,
) -> NativeSourceDemuxTrace {
    assert!(matches!(
        source
            .extractor
            .observe_prepared_local_demux_trace(
                prepared,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            )
            .await,
        Err(SourceTeletextPageDecodeError::NoCleanupScope)
    ));
    scope
        .scope(source.extractor.observe_prepared_local_demux_trace(
            prepared,
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap()
}

#[tokio::test]
#[ignore = "requires explicit FFmpeg/FFprobe 9 for genuine native demux/raw epoch comparison"]
async fn native_demux_trace_matches_original_pes_positions_and_signed_wrap_timestamps() {
    let scope = LocalProbeCleanupScope::new();
    let source = LaterSource::new(&scope).await;
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let prepared = source.prepare(&scope, &work, 888, 1).await;
    let trace = observe(&source, &scope, &work, &prepared).await;
    assert_eq!(trace.streams().len(), 4);
    assert_eq!(trace.programs().len(), 1);
    assert_eq!(trace.programs()[0].number(), 1);
    assert_eq!(trace.programs()[0].pmt_pid(), 4096);
    assert_eq!(trace.programs()[0].pcr_pid(), 256);
    assert_eq!(trace.programs()[0].stream_indices(), [0, 1, 2, 3]);
    let early = assert_raw_av_trace(&source.bytes, &trace);
    assert_eq!(early.behavior(), Some(SourceNativeWrapBehavior::AddOffset));
    assert_eq!(early.reference_90khz(), Some(-5_274_000));

    let path = source.path.parent().unwrap().join("wrapped-source.ts");
    let ffmpeg = tool(&scope, "ERSATZRS_TELETEXT_TEST_FFMPEG", "ffmpeg").await;
    scope
        .scope(local::capture(
            FfprobeCommand {
                program: ffmpeg,
                args: vec![
                    "-v".into(),
                    "error".into(),
                    "-nostdin".into(),
                    "-i".into(),
                    source.path.to_str().unwrap().into(),
                    "-map".into(),
                    "0".into(),
                    "-c".into(),
                    "copy".into(),
                    "-muxrate".into(),
                    "2000000".into(),
                    "-output_ts_offset".into(),
                    "95442".into(),
                    "-f".into(),
                    "mpegts".into(),
                    "-n".into(),
                    path.to_str().unwrap().into(),
                ],
            },
            "synthetic native timestamp-wrap generator".into(),
            Some(Instant::now() + Duration::from_secs(15)),
        ))
        .await
        .unwrap();
    let acquired = scope
        .scope(source.extractor.extract_local_with_teletext(&path))
        .await
        .unwrap();
    let mut request = source.request.clone();
    request.binding.source_revision = acquired.teletext.unwrap().revision().clone();
    let wrapped = scope
        .scope(source.extractor.prepare_local_teletext_source(
            request,
            path.clone(),
            path.clone(),
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap();
    let trace = observe(&source, &scope, &work, &wrapped).await;
    let bytes = std::fs::read(&path).unwrap();
    let policy = assert_raw_av_trace(&bytes, &trace);
    assert_eq!(
        policy.behavior(),
        Some(SourceNativeWrapBehavior::SubtractOffset)
    );
    assert!(trace
        .packets()
        .iter()
        .any(|packet| packet.pts().is_some_and(|pts| pts < 0)));
    assert!(trace
        .packets()
        .iter()
        .any(|packet| packet.pts().is_some_and(|pts| pts > 0)));
    assert_eq!(std::fs::read(&source.path).unwrap(), source.bytes);
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
}
