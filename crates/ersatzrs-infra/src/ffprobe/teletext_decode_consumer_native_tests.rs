//! Real native normalization with staggered AV and an earlier teletext stream.

use super::*;
use crate::ffprobe::teletext_consumer_origin::{
    NativeSourceConsumerOrigin, NativeSourceConsumerStream, NativeSourceConsumerTiming,
};
use crate::ffprobe::teletext_decode::PreparedLocalTeletextSource;
use ersatzrs_ffmpeg::{
    build_pipeline_command_with_source_teletext_plan, DesiredVideoState, InputFile,
    PipelineRequest, SourceAudioCodec, SourceAudioStream, SourceTeletextInputPlan,
    SourceVideoCodec, SourceVideoStream,
};

#[path = "teletext_decode_consumer_burn_tests.rs"]
mod burn;

async fn staggered_source(
    scope: &LocalProbeCleanupScope,
    ffmpeg: &str,
    directory: &std::path::Path,
) -> PathBuf {
    let path = directory.join("staggered-source.ts");
    let seed = directory.join("teletext.ts");
    std::fs::write(
        &seed,
        include_bytes!("../../../ersatzrs-ffmpeg/src/fixtures/teletext.ts"),
    )
    .unwrap();
    let mut args: Vec<_> = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-itsoffset",
        "3",
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=492x250:r=25",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=440:sample_rate=48000",
        "-itsoffset",
        "2",
        "-f",
        "lavfi",
        "-i",
        "sine=frequency=660:sample_rate=48000",
        "-f",
        "mpegts",
        "-i",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    args.push(seed.to_str().unwrap().into());
    args.extend(
        [
            "-t",
            "8",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-map",
            "3:s:0",
            "-map",
            "2:a:0",
            "-c:v",
            "mpeg2video",
            "-b:v",
            "200k",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-c:s",
            "copy",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:1",
            "language=nld",
            "-muxrate",
            "2000000",
            "-pcr_period",
            "20",
            "-mpegts_start_pid",
            "256",
            "-mpegts_pmt_start_pid",
            "4096",
            "-f",
            "mpegts",
            "-n",
        ]
        .into_iter()
        .map(str::to_owned),
    );
    args.push(path.to_str().unwrap().into());
    let output = scope
        .scope(local::capture(
            FfprobeCommand {
                program: ffmpeg.into(),
                args,
            },
            "synthetic staggered consumer source".into(),
            Some(Instant::now() + Duration::from_secs(30)),
        ))
        .await
        .unwrap();
    assert!(output.is_empty());
    path
}

async fn prepare(
    scope: &LocalProbeCleanupScope,
    extractor: &FfprobeExtractor,
    path: &std::path::Path,
    work: &TeletextWorkGuard,
) -> PreparedLocalTeletextSource {
    let acquired = scope
        .scope(extractor.extract_local_with_teletext(path))
        .await
        .unwrap();
    let subtitle = acquired
        .media
        .streams
        .iter()
        .find(|stream| stream.codec.as_deref() == Some("dvb_teletext"))
        .unwrap();
    assert_eq!(subtitle.index, 2);
    let request = SourceTeletextPageDecodeRequest {
        binding: SourceTeletextBindingIdentity::new(
            1,
            MediaSourceKind::Local,
            2,
            3,
            4,
            acquired.teletext.unwrap().revision().clone(),
        )
        .unwrap(),
        stream_index: subtitle.index,
        pid: 258,
        program_number: 1,
        page: TeletextPageNumber::new(888).unwrap(),
    };
    scope
        .scope(extractor.prepare_local_teletext_source(
            request,
            path.into(),
            path.into(),
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap()
}

async fn decode(
    scope: &LocalProbeCleanupScope,
    extractor: &FfprobeExtractor,
    source: &PreparedLocalTeletextSource,
    original: &[u8],
    work: &TeletextWorkGuard,
) -> DecodedLocalTeletextPage {
    assert!(original.len().is_multiple_of(PACKET_BYTES));
    let span =
        SourceTeletextPacketSpan::new(0, u32::try_from(original.len() / PACKET_BYTES).unwrap())
            .expect("existing short fixture fits one bounded actual packet span");
    let decoded = scope
        .scope(extractor.decode_prepared_local_teletext_page(
            source,
            span,
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&source.guard, &decoded.guard));
    assert_eq!(decoded.request(), source.request());
    assert!(decoded.events().count > 0);
    assert!(decoded
        .headers()
        .unwrap()
        .headers()
        .iter()
        .any(|occurrence| { occurrence.header.display_page() == Some(decoded.request().page) }));
    assert!(decoded.window().covers_entire_source_bytes());
    assert_eq!(
        decoded.window().decoder_sha256,
        <[u8; 32]>::from(Sha256::digest(original))
    );
    decoded
}

fn generated_plan(
    source: &PreparedLocalTeletextSource,
    decoded: &DecodedLocalTeletextPage,
    audio: usize,
    ffmpeg: &str,
) -> SourceTeletextInputPlan {
    let path = source.resolved.to_str().unwrap();
    let mut request = PipelineRequest::new(InputFile::video(path), DesiredVideoState::h264_1080p())
        .with_source_video_stream(
            SourceVideoStream::new(SourceVideoCodec::Mpeg2Video).with_index(0),
        )
        .with_source_audio_streams(vec![
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(1),
            SourceAudioStream::new(SourceAudioCodec::Aac).with_index(3),
        ]);
    request.inputs[1] = request.inputs[1].clone().with_stream_index(audio);
    request.inputs.push(decoded.native_burn_input());
    request.ffmpeg_executable = Some(ffmpeg.into());
    let (command, plan) = build_pipeline_command_with_source_teletext_plan(
        ersatzrs_domain::HardwareAccelerationKind::None,
        request,
    )
    .unwrap();
    let plan = plan.unwrap();
    assert!(plan.matches_command(&command));
    assert_eq!(plan.caption_input(), &decoded.native_burn_input());
    assert_eq!(plan.caption_physical_input_index(), 0);
    assert_eq!(plan.video_stream(), Some((0, 0)));
    assert_eq!(plan.audio_stream(), Some((0, audio)));
    assert_eq!(plan.enabled_caption_input_streams(), [0, audio, 2]);
    assert_eq!(
        command.arguments.iter().filter(|arg| *arg == "-i").count(),
        1
    );
    assert!(command.arguments.iter().any(|arg| arg == "-txt_page:2"));
    // Command generation only: these graphs are not spawned or timing-qualified.
    plan
}

async fn equal_origins(
    scope: &LocalProbeCleanupScope,
    extractor: &FfprobeExtractor,
    source: &PreparedLocalTeletextSource,
    decoded: &DecodedLocalTeletextPage,
    enabled: &[i32],
    work: &TeletextWorkGuard,
) -> NativeSourceConsumerOrigin {
    let origin = scope
        .scope(extractor.observe_local_teletext_consumer_origin(
            decoded,
            enabled,
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap();
    let prepared = scope
        .scope(extractor.observe_prepared_local_consumer_origin(
            source,
            enabled,
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await
        .unwrap();
    assert_eq!(origin, prepared);
    assert!(std::sync::Arc::ptr_eq(&source.guard, &decoded.guard));
    origin
}

#[derive(Debug)]
struct CrcPacket {
    index: i32,
    pts: i64,
    size: usize,
    checksum: String,
}

fn parse_crc(bytes: &[u8], enabled: &[i32]) -> Vec<CrcPacket> {
    let text = std::str::from_utf8(bytes).unwrap();
    for (index, _) in enabled.iter().enumerate() {
        assert!(text.contains(&format!("#tb {index}: 1/90000\n")));
    }
    let packets: Vec<_> = text
        .lines()
        .filter(|line| !line.starts_with('#'))
        .map(|line| {
            let fields: Vec<_> = line.split(',').map(str::trim).collect();
            CrcPacket {
                index: enabled[fields[0].parse::<usize>().unwrap()],
                pts: fields[2].parse().unwrap(),
                size: fields[4].parse().unwrap(),
                checksum: fields[5].into(),
            }
        })
        .collect();
    for index in enabled {
        assert!(packets.iter().any(|packet| packet.index == *index));
    }
    packets
}

fn crc_command(
    program: &str,
    source: &PreparedLocalTeletextSource,
    enabled: &[i32],
    seek: Option<&str>,
    duration: &str,
) -> FfprobeCommand {
    let mut args: Vec<_> = ["-hide_banner", "-loglevel", "error", "-nostdin"]
        .into_iter()
        .map(str::to_owned)
        .chain(
            ersatzrs_ffmpeg::standard_format_flags()
                .global_options()
                .iter()
                .cloned(),
        )
        .chain(["-probesize", "5000000", "-correct_ts_overflow", "1"].map(str::to_owned))
        .collect();
    if enabled == [0, 1, 2, 3] {
        assert!(seek.is_none());
        args.push("-copyts".into());
    }
    if let Some(seek) = seek {
        args.extend(["-ss".into(), seek.into()]);
    }
    args.extend(["-i".into(), source.resolved.to_str().unwrap().into()]);
    for index in enabled {
        args.extend(["-map".into(), format!("0:{index}")]);
    }
    args.extend(["-c", "copy", "-t", duration, "-f", "framecrc", "-"].map(str::to_owned));
    FfprobeCommand {
        program: program.into(),
        args,
    }
}

async fn capture_crc(
    scope: &LocalProbeCleanupScope,
    program: &str,
    source: &PreparedLocalTeletextSource,
    enabled: &[i32],
    seek: Option<&str>,
    work: &TeletextWorkGuard,
) -> Vec<CrcPacket> {
    let bytes = scope
        .scope(local::capture_retained(
            crc_command(program, source, enabled, seek, "20"),
            "synthetic consumer packet timing".into(),
            Some(Instant::now() + Duration::from_secs(15)),
            Some(local::ProbeRetention::new(
                work.hold_work(),
                source.guard.clone(),
            )),
        ))
        .await
        .unwrap();
    parse_crc(&bytes, enabled)
}

fn assert_normalization(
    reference: &[CrcPacket],
    output: &[CrcPacket],
    timing: NativeSourceConsumerTiming,
) {
    let (lower, upper) = timing.presentation_zero_bounds_microseconds();
    for packet in output {
        let matches: Vec<_> = reference
            .iter()
            .filter(|original| {
                if original.index != packet.index
                    || original.size != packet.size
                    || original.checksum != packet.checksum
                {
                    return false;
                }
                let offset = i128::from(original.pts - packet.pts) * 1_000_000;
                // One 90 kHz tick covers integer rescaling of the microsecond offset.
                i128::from(lower) * 90_000 - 1_000_000 <= offset
                    && offset <= i128::from(upper) * 90_000 + 1_000_000
            })
            .collect();
        assert_eq!(
            matches.len(),
            1,
            "actual packet normalization: {packet:?}, {timing:?}"
        );
    }
}

#[tokio::test]
#[ignore = "requires explicit FFmpeg/FFprobe 9 for genuine consumer normalization trials"]
async fn native_consumer_origin_matches_two_audio_choices_and_enabled_subtitle_with_seek() {
    let scope = LocalProbeCleanupScope::new();
    let ffmpeg = tool(&scope, "ERSATZRS_TELETEXT_TEST_FFMPEG", "ffmpeg").await;
    let extractor = FfprobeExtractor::new(tool(&scope, "FFPROBE_PATH", "ffprobe").await);
    let directory = tempfile::tempdir().unwrap();
    let path = staggered_source(&scope, &ffmpeg, directory.path()).await;
    let original = std::fs::read(&path).unwrap();
    let ordinary = scope.scope(extractor.extract(&path)).await.unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let source = prepare(&scope, &extractor, &path, &work).await;
    let decoded = decode(&scope, &extractor, &source, &original, &work).await;
    let native = source.native_origin().clone();
    let reference = capture_crc(&scope, &ffmpeg, &source, &[0, 1, 2, 3], None, &work).await;
    assert_missing_prefix(&scope, &ffmpeg, &source, &work).await;
    assert_trials(
        &scope, &extractor, &ffmpeg, &source, &decoded, &work, &reference,
    )
    .await;
    assert_eq!(source.native_origin(), &native);
    assert_boundaries(&scope, &extractor, &source, &decoded, &work).await;
    assert_eq!(
        scope.scope(extractor.extract(&path)).await.unwrap().streams,
        ordinary.streams
    );
    assert_eq!(std::fs::read(&path).unwrap(), original);
    scope
        .scope(source.check_source(Instant::now() + Duration::from_secs(15), work.hold_work()))
        .await
        .unwrap();
    std::fs::write(&path, b"synthetic changed consumer source").unwrap();
    for result in [
        scope
            .scope(extractor.observe_prepared_local_consumer_origin(
                &source,
                &[0, 1],
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await,
        scope
            .scope(extractor.observe_local_teletext_consumer_origin(
                &decoded,
                &[0, 1],
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await,
    ] {
        assert_eq!(
            result.unwrap_err(),
            SourceTeletextPageDecodeError::SourceChanged
        );
    }
    scope.close();
    assert_eq!(scope.wait_cleanup().await, Ok(()));
    assert_eq!(locker.active_teletext_work(), 1);
    drop(work);
    assert_eq!(locker.active_teletext_work(), 0);
    eprintln!("consumer source SHA256={}, ordinary metadata/source unchanged before deliberate rejection; actual cleanup drained", hex_text(&Sha256::digest(original)));
}

async fn assert_trials(
    scope: &LocalProbeCleanupScope,
    extractor: &FfprobeExtractor,
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    decoded: &DecodedLocalTeletextPage,
    work: &TeletextWorkGuard,
    reference: &[CrcPacket],
) {
    let english = generated_plan(source, decoded, 1, ffmpeg);
    let dutch = generated_plan(source, decoded, 3, ffmpeg);
    let indices = |plan: &SourceTeletextInputPlan| {
        plan.enabled_caption_input_streams()
            .into_iter()
            .map(|index| i32::try_from(index).unwrap())
            .collect::<Vec<_>>()
    };
    let english = indices(&english);
    let dutch = indices(&dutch);
    let caption_english = equal_origins(scope, extractor, source, decoded, &english, work).await;
    let mut enabled_starts = Vec::new();
    for enabled in [english[..2].to_vec(), dutch[..2].to_vec(), dutch] {
        let origin = equal_origins(scope, extractor, source, decoded, &enabled, work).await;
        assert_eq!(
            origin
                .streams()
                .iter()
                .map(NativeSourceConsumerStream::index)
                .collect::<Vec<_>>(),
            enabled
        );
        enabled_starts.push(origin.timing(0).unwrap().enabled_start_microseconds());
        for (seek_us, seek) in [
            (0, None),
            (500_000, Some("0.500000")),
            (4_000_000, Some("4.000000")),
        ] {
            let timing = origin.timing(seek_us).unwrap();
            if seek_us == 4_000_000 {
                assert!(
                    timing.initial_seek_bounds_microseconds().0
                        > timing.enabled_start_microseconds()
                );
            }
            let output = capture_crc(scope, ffmpeg, source, &enabled, seek, work).await;
            let unknown_prefix = usize::from(enabled == [0, 3, 2] && seek_us == 500_000);
            if unknown_prefix != 0 {
                // The actual pinned -debug_ts trial reports NOPTS for this
                // first subtitle PES after seek. framecrc muxing invents zero;
                // that zero cannot prove source timing or cue admission.
                let first = &output[0];
                assert_eq!((first.index, first.pts, first.size), (2, 0, 139));
                assert_eq!(first.checksum, "0x73766b6c");
            }
            assert_normalization(reference, &output[unknown_prefix..], timing);
            eprintln!("actual consumer normalization: enabled={enabled:?}, seek={seek_us}, timing={timing:?}, matched_packets={}, unknown_prefix_packets={unknown_prefix}; stream-copy timing proof, no bitmap/landing/runtime admission", output.len() - unknown_prefix);
        }
    }
    assert!(enabled_starts[2] < enabled_starts[0]);
    assert!(enabled_starts[0] < enabled_starts[1]);
    assert_eq!(
        caption_english
            .timing(0)
            .unwrap()
            .enabled_start_microseconds(),
        enabled_starts[2]
    );
}

async fn assert_missing_prefix(
    scope: &LocalProbeCleanupScope,
    ffmpeg: &str,
    source: &PreparedLocalTeletextSource,
    work: &TeletextWorkGuard,
) {
    let mut command = crc_command(ffmpeg, source, &[0, 3, 2], Some("0.500000"), "0.000001");
    let level = command
        .args
        .iter()
        .position(|arg| arg == "-loglevel")
        .unwrap();
    command.args[level + 1] = "verbose".into();
    command.args.splice(
        0..0,
        ["-debug_ts", "-abort_on", "empty_output_stream"].map(str::to_owned),
    );
    // Deliberately abort on empty AV: the tiny output retains the first missing
    // subtitle timestamp inside the existing bounded stderr tail. No new runner.
    let error = scope
        .scope(local::capture_retained(
            command,
            "synthetic missing-PTS consumer diagnostic".into(),
            Some(Instant::now() + Duration::from_secs(15)),
            Some(local::ProbeRetention::new(
                work.hold_work(),
                source.guard.clone(),
            )),
        ))
        .await
        .unwrap_err();
    let crate::ffprobe::FfprobeError::Process { stderr, .. } = error else {
        panic!("expected explicit empty-AV diagnostic failure, got {error:?}");
    };
    assert!(stderr.contains("Empty output stream"));
    let first = stderr
        .lines()
        .find(|line| line.contains("demuxer -> ist_index:0:2 type:subtitle"))
        .unwrap();
    assert!(first.contains("pkt_pts:NOPTS pkt_pts_time:NOPTS pkt_dts:NOPTS"));
    eprintln!("actual first post-seek subtitle: native PTS/DTS absent, framecrc zero is not timing evidence; bounded diagnostic confirmed and process reaped");
}

async fn assert_boundaries(
    scope: &LocalProbeCleanupScope,
    extractor: &FfprobeExtractor,
    source: &PreparedLocalTeletextSource,
    decoded: &DecodedLocalTeletextPage,
    work: &TeletextWorkGuard,
) {
    let unavailable = FfprobeExtractor::new("must-not-start-synthetic-consumer-probe");
    for enabled in [vec![], vec![-1], vec![0, 0], vec![0; 8193]] {
        let result = scope
            .scope(unavailable.observe_prepared_local_consumer_origin(
                source,
                &enabled,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await;
        assert_eq!(
            result.unwrap_err(),
            SourceTeletextPageDecodeError::InvalidRequest
        );
        let decoded_result = scope
            .scope(unavailable.observe_local_teletext_consumer_origin(
                decoded,
                &enabled,
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await;
        assert_eq!(
            decoded_result.unwrap_err(),
            SourceTeletextPageDecodeError::InvalidRequest
        );
    }
    let result = unavailable
        .observe_prepared_local_consumer_origin(
            source,
            &[0],
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        )
        .await;
    assert_eq!(
        result.unwrap_err(),
        SourceTeletextPageDecodeError::NoCleanupScope
    );
    let decoded_result = unavailable
        .observe_local_teletext_consumer_origin(
            decoded,
            &[0],
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        )
        .await;
    assert_eq!(
        decoded_result.unwrap_err(),
        SourceTeletextPageDecodeError::NoCleanupScope
    );
    let result = scope
        .scope(extractor.observe_prepared_local_consumer_origin(
            source,
            &[99],
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await;
    assert_eq!(
        result.unwrap_err(),
        SourceTeletextPageDecodeError::InvalidOutput
    );
    let decoded_result = scope
        .scope(extractor.observe_local_teletext_consumer_origin(
            decoded,
            &[99],
            Instant::now() + Duration::from_secs(15),
            work.hold_work(),
        ))
        .await;
    assert_eq!(
        decoded_result.unwrap_err(),
        SourceTeletextPageDecodeError::InvalidOutput
    );
}
