//! Unix shared-pipe viability trial, not ordinary HTTP/RTSP acceptance.
//! Prefix observations do not certify a whole source or page readiness.
#![cfg(unix)]

use std::collections::BTreeMap;
use std::panic::AssertUnwindSafe;

use futures_util::FutureExt as _;

use ersatzrs_ffmpeg::{
    teletext::{
        source_assembly::{SuppliedTeletextAssembler, TeletextAssemblyEvent},
        source_clock::SourceTeletextClockObserver,
        source_descriptors::parse_source_teletext_descriptors,
        source_observation::SuppliedTeletextHeaderObserver,
        source_psi::SourceTeletextPsiObserver,
    },
    DesiredVideoState, FfmpegCommand, InputFile, PipelineRequest, SourceTeletextInputPlan,
};
use ersatzrs_infra::{
    ffprobe::{
        teletext_probe::{TeletextProbePid, TeletextProbePrograms, TeletextProbeSnapshot},
        FfprobeMediaVersion,
    },
    remote_stream::{
        prepare_native_caption_producer, spawn_ffmpeg_with_prepared_remote_input,
        PreparedRemoteStreamInput, RemoteStreamCommand, RemoteStreamSpawnContext,
    },
    transcode_capacity::{try_reserve_active_transcode, ActiveTranscodeSlot},
};
use tokio::io::{AsyncRead, AsyncReadExt as _};
use tokio::process::Command;
use tokio::time::Instant;

use super::*;

fn write_json(root: &Path, name: &str, value: &serde_json::Value) {
    std::fs::write(root.join(name), serde_json::to_vec_pretty(value).unwrap()).unwrap();
}

fn producer(source: &Path, root: &Path) -> RemoteStreamCommand {
    RemoteStreamCommand {
        tool_paths: Vec::new(),
        program: "sh".into(),
        args: vec![
            "-c".into(),
            "printf 'started\\n' >> \"$2\"; printf '%s\\n' \"$$\" > \"$3\"; cat -- \"$1\" || exit 7; printf 'sent\\n' > \"$4\"; while :; do sleep 1; done".into(),
            "synthetic-shared-caption".into(),
            source.to_str().unwrap().into(),
            root.join("invocations").to_str().unwrap().into(),
            root.join("producer.pid").to_str().unwrap().into(),
            root.join("source-sent").to_str().unwrap().into(),
        ],
        working_directory: root.into(),
        environment: BTreeMap::new(),
    }
}

async fn producer_alive(root: &Path) -> bool {
    let pid = tokio::fs::read_to_string(root.join("producer.pid"))
        .await
        .unwrap();
    assert!(pid.trim().parse::<u32>().unwrap() > 0);
    let mut command = Command::new("sh");
    command.args([
        "-c",
        "kill -0 \"$1\" 2>/dev/null",
        "synthetic-pid-check",
        pid.trim(),
    ]);
    command.kill_on_drop(true);
    tokio::time::timeout(Duration::from_secs(2), command.status())
        .await
        .unwrap()
        .unwrap()
        .success()
}

fn observe_prefix(prefix: &[u8], pid: u16, program: u16, root: &Path) {
    let page = ersatzrs_domain::TeletextPageNumber::new(888).unwrap();
    let mut psi = SourceTeletextPsiObserver::new_source_epoch(pid).unwrap();
    let mut assembler = SuppliedTeletextAssembler::new_source_epoch(pid).unwrap();
    let mut complete_end = None;
    for (index, packet) in prefix.chunks_exact(188).enumerate() {
        let packet = packet.try_into().unwrap();
        psi.push(packet).unwrap();
        if matches!(
            assembler.push(packet).unwrap(),
            TeletextAssemblyEvent::Complete(_)
        ) {
            complete_end = Some((index + 1) * 188);
        }
    }
    let psi = psi
        .observation()
        .expect("fixed64KiB prefix must contain complete current PSI");
    assert!(!psi.current_tables_changed);
    let memberships = psi.memberships().collect::<Vec<_>>();
    assert_eq!(memberships.len(), 1);
    let (selected, elementary) = memberships[0];
    assert_eq!(selected.program_number, program);
    assert_eq!(elementary.stream_type, 6);
    let descriptors = parse_source_teletext_descriptors(&elementary.descriptors).unwrap();
    assert!(descriptors.iter().any(|entry| entry.descriptor_tag == 0x56
        && entry.descriptor_type == 2
        && entry.display_page() == Some(page)));
    let end = complete_end.expect("fixed prefix lacks a complete selected teletext PES");
    let mut headers = SuppliedTeletextHeaderObserver::new_source_epoch_with_pages(pid).unwrap();
    for packet in prefix[..end].chunks_exact(188) {
        headers.push(packet.try_into().unwrap()).unwrap();
    }
    // Finish only at an actually completed PES, never the arbitrary prefix tail.
    let headers = headers.finish().unwrap();
    assert!(!headers.has_unobserved_prefix());
    assert!(
        headers
            .headers()
            .iter()
            .any(|entry| entry.header.display_page() == Some(page)),
        "fixed prefix advertises888 but contains no actual protected888 header"
    );
    write_json(
        root,
        "prefix-observation.json",
        &serde_json::json!({
            "prefix_bytes":prefix.len(), "complete_transport_packets":prefix.len()/188,
            "partial_transport_tail_bytes":prefix.len()%188, "complete_pes_end_bytes":end,
            "complete_pes":headers.complete_pes(), "observed_headers":headers.headers().len(),
            "pid":pid, "program_number":program, "observed_page":888,
            "source_eof":false, "whole_source_or_page_readiness":false,
        }),
    );
}

fn software_desired() -> DesiredVideoState {
    let profile = FFmpegProfile {
        thread_count: 1,
        transcode_video: true,
        transcode_audio: true,
        video_format: FFmpegProfileVideoFormat::H264,
        audio_format: FFmpegProfileAudioFormat::Aac,
        video_preset: Some("ultrafast".into()),
        video_bitrate: VIDEO_KBPS,
        video_buffer_size: 2000,
        audio_bitrate: AUDIO_KBPS,
        audio_buffer_size: 256,
        audio_channels: 1,
        audio_sample_rate: 48,
        normalize_colors: false,
        normalize_framerate: false,
        deinterlace_video: Some(false),
        ..Default::default()
    };
    let resolution = ersatzrs_domain::Resolution {
        width: 492,
        height: 250,
        ..Default::default()
    };
    DesiredVideoState::from_profile(
        &profile,
        &resolution,
        None,
        None,
        StreamingMode::TransportStream,
    )
}

fn caption_command(
    ffmpeg: &str,
    media: &FfprobeMediaVersion,
    index: i32,
) -> (FfmpegCommand, SourceTeletextInputPlan) {
    let video = ersatzrs_infra::playback::source_video_stream_from_probe(media)
        .unwrap()
        .unwrap();
    let audio = ersatzrs_infra::playback::source_audio_streams_from_probe(media).unwrap();
    assert_eq!(video.index, 0);
    assert_eq!(audio.len(), 2);
    let dutch = media
        .streams
        .iter()
        .find(|stream| stream.language.as_deref() == Some("nld"))
        .expect("retained prefix must identify actual Dutch alternate audio");
    assert_eq!(dutch.index, 3);
    let mut request = PipelineRequest::new(InputFile::video("pipe:0"), software_desired())
        .with_source_video_stream(video)
        .with_source_audio_streams(audio);
    request.inputs[1] = request.inputs[1].clone().with_stream_index(3);
    request.inputs.push(InputFile::teletext_subtitle(
        "pipe:0",
        usize::try_from(index).unwrap(),
        ersatzrs_domain::TeletextPageNumber::new(888).unwrap(),
    ));
    request.state.report_progress = false;
    request.state.thread_count = Some(1);
    request.state.metadata.audio_language = Some("nld".into());
    request.ffmpeg_executable = Some(ffmpeg.into());
    let (command, plan) = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
        ersatzrs_domain::HardwareAccelerationKind::None,
        request,
    )
    .unwrap();
    let plan = plan.expect("actual selected exact-page bitmap plan");
    assert!(plan.matches_command(&command));
    assert_eq!(plan.caption_physical_input_index(), 0);
    assert_eq!(plan.video_stream(), Some((0, 0)));
    assert_eq!(plan.audio_stream(), Some((0, 3)));
    assert_eq!(plan.timing().effective_input_seek_microseconds(), Some(0));
    assert!(plan.timing().duration().is_none());
    assert_eq!(
        command
            .arguments
            .windows(2)
            .filter(|pair| pair[0] == "-i")
            .map(|pair| pair[1].as_str())
            .collect::<Vec<_>>(),
        ["pipe:0"]
    );
    (command, plan)
}

async fn capture_open(reader: &mut (impl AsyncRead + Unpin)) -> std::io::Result<Vec<u8>> {
    let mut bytes = Vec::new();
    let mut clock = SourceTeletextClockObserver::new_source_epoch(0x100, 0).unwrap();
    loop {
        let mut packet = [0; 188];
        reader.read_exact(&mut packet).await?;
        bytes.extend_from_slice(&packet);
        if bytes.len() > MAX_CAPTURE {
            return Err(std::io::Error::other(
                "shared-pipe output exceeds existing capture bound",
            ));
        }
        let report = clock.push(&packet).map_err(std::io::Error::other)?;
        if report
            .last
            .is_some_and(|last| last.elapsed_ticks >= 3 * 27_000_000)
        {
            return Ok(bytes);
        }
    }
}

async fn capture_consumer(
    prepared: PreparedRemoteStreamInput,
    command: &FfmpegCommand,
    root: &Path,
) -> Vec<u8> {
    write_json(
        root,
        "command.json",
        &serde_json::json!({
            "executable":command.executable,"arguments":command.arguments,
            "environment_empty":command.environment.is_empty(),
        }),
    );
    let reservation = try_reserve_active_transcode(
        DEFAULT_ACTIVE_TRANSCODE_LIMIT,
        ActiveTranscodeSlot::new_mpeg_ts("synthetic-shared-caption"),
    )
    .unwrap();
    let mut child = spawn_ffmpeg_with_prepared_remote_input(
        command,
        Some(root),
        prepared,
        RemoteStreamSpawnContext::mpeg_ts("synthetic-shared-caption"),
    )
    .await
    .unwrap();
    let mut stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();
    let stderr_task = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr
            .take(65_537)
            .read_to_end(&mut bytes)
            .await
            .map(|_| bytes)
    });
    let captured = tokio::time::timeout(Duration::from_secs(15), capture_open(&mut stdout)).await;
    let producer_was_open = producer_alive(root).await;
    let consumer_was_open = child.try_wait().unwrap().is_none();
    // Hold admission until the actual encoder and managed producer both join.
    let cleanup = tokio::time::timeout(Duration::from_secs(10), child.kill()).await;
    drop(stdout);
    if !cleanup.as_ref().is_ok_and(std::result::Result::is_ok) {
        reservation.retain_until_restart();
    }
    cleanup
        .expect("shared consumer cleanup deadline")
        .expect("shared consumer/producer reaped");
    assert!(
        !producer_alive(root).await,
        "managed original producer must be reaped"
    );
    let stderr = tokio::time::timeout(Duration::from_secs(5), stderr_task)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    tokio::fs::write(root.join("consumer.stderr"), &stderr)
        .await
        .unwrap();
    assert!(
        stderr.len() <= 65_536,
        "stderr exceeded existing diagnostic bound"
    );
    drop(child);
    drop(reservation);
    capacity_zero().await;
    let output = captured
        .expect("actual shared-pipe output before15s, without producer EOF")
        .expect("actual complete TS prefix before producer EOF");
    tokio::fs::write(root.join("captured.ts"), &output)
        .await
        .unwrap();
    assert!(
        producer_was_open && consumer_was_open,
        "output must precede either process EOF"
    );
    output
}

async fn observed_media(
    prepared: &PreparedRemoteStreamInput,
    ffprobe: &str,
    root: &Path,
) -> (FfprobeMediaVersion, i32) {
    let scope = LocalProbeCleanupScope::new();
    let (_cancel, cancellation) = tokio::sync::oneshot::channel();
    let observed = scope
        .scope(
            FfprobeExtractor::new(ffprobe).extract_producer_prefix_with_teletext_cancellable(
                prepared.prefix(),
                Instant::now() + Duration::from_secs(10),
                cancellation,
            ),
        )
        .await;
    scope.close();
    scope
        .wait_cleanup()
        .await
        .expect("actual prefix probe cleanup");
    let (media, snapshot) = observed.expect("bounded original prefix identity probe");
    let TeletextProbeSnapshot::Available(streams) = snapshot else {
        panic!("fixed64KiB prefix lacks usable teletext identity");
    };
    assert_eq!(streams.len(), 1);
    let selected = &streams[0];
    assert_eq!(selected.stream_index, 2);
    let TeletextProbePid::Available(pid) = selected.pid else {
        panic!("missing actual PID")
    };
    let TeletextProbePrograms::Available(programs) = &selected.programs else {
        panic!("missing actual selected-program membership")
    };
    assert_eq!(programs.len(), 1);
    observe_prefix(
        prepared.prefix(),
        pid,
        u16::try_from(programs[0].program_number).unwrap(),
        root,
    );
    (media, selected.stream_index)
}

async fn prepare_command(
    prepared: PreparedRemoteStreamInput,
    bytes: &[u8],
    tools: &[String; 3],
    root: &Path,
) -> (PreparedRemoteStreamInput, FfmpegCommand) {
    let observed = AssertUnwindSafe(async {
        assert_eq!(prepared.prefix().len(), 65_536);
        assert_eq!(
            prepared.prefix(),
            &bytes[..65_536],
            "original prefix retained unchanged"
        );
        tokio::fs::write(root.join("producer-prefix.ts"), prepared.prefix())
            .await
            .unwrap();
        let (media, index) = observed_media(&prepared, &tools[1], root).await;
        caption_command(&tools[0], &media, index).0
    })
    .catch_unwind()
    .await;
    match observed {
        Ok(command) => (prepared, command),
        Err(failure) => {
            tokio::time::timeout(Duration::from_secs(10), prepared.cancel())
                .await
                .expect("failed prefix observation cleanup deadline")
                .expect("failed prefix observation producer reaped");
            assert!(!producer_alive(root).await);
            std::panic::resume_unwind(failure);
        }
    }
}

async fn prepare_owned_command(
    source: &Path,
    bytes: &[u8],
    tools: &[String; 3],
    root: &Path,
    cancellation: tokio::sync::oneshot::Receiver<()>,
) -> (PreparedRemoteStreamInput, FfmpegCommand) {
    let mut playback = prepare_native_caption_producer(
        &producer(source, root),
        &FfprobeExtractor::new(&tools[1]),
        RemoteStreamSpawnContext::mpeg_ts("synthetic-shared-caption"),
        Instant::now() + Duration::from_secs(10),
        cancellation,
    )
    .await
    .unwrap();
    let observed = AssertUnwindSafe(async {
        let prefix = playback.prepared.prefix();
        assert!(prefix.len() <= 65_536);
        assert_eq!(prefix, &bytes[..prefix.len()]);
        tokio::fs::write(root.join("producer-prefix.ts"), prefix)
            .await
            .unwrap();
        observe_prefix(prefix, 258, 1, root);
        let tracks = playback.tracks();
        let selected = tracks
            .subtitles
            .iter()
            .find(|subtitle| subtitle.codec.as_deref() == Some("dvb_teletext"))
            .expect("same producer must supply an observed caption choice");
        let caption = playback.subtitle_input(selected).unwrap().unwrap();
        assert_eq!(
            caption,
            InputFile::teletext_subtitle(
                "pipe:0",
                2,
                ersatzrs_domain::TeletextPageNumber::new(888).unwrap()
            )
        );
        let (command, plan) = caption_command(
            &tools[0],
            playback.probe.as_ref().unwrap(),
            selected.stream_index,
        );
        playback
            .bind_caption_consumer(&command, Some(&plan))
            .unwrap();
        command
    })
    .catch_unwind()
    .await;
    match observed {
        Ok(command) => (playback.prepared, command),
        Err(failure) => {
            tokio::time::timeout(Duration::from_secs(10), playback.prepared.cancel())
                .await
                .unwrap()
                .unwrap();
            assert!(!producer_alive(root).await);
            std::panic::resume_unwind(failure);
        }
    }
}

async fn run(owned_selection: bool) {
    let tools = initialize_source_caption_runtime().await;
    let root = tempfile::tempdir().unwrap().keep();
    println!(
        "shared-pipe streaming caption viability evidence: {}",
        root.display()
    );
    let source = local_source(&tools[0], &root.join("library")).await;
    let expected = calibration(&tools, &source, &root).await;
    let bytes = tokio::fs::read(&source).await.unwrap();
    assert!(bytes.len() > 65_536);
    let (_cancel, cancellation) = tokio::sync::oneshot::channel();
    let (prepared, command) = if owned_selection {
        prepare_owned_command(&source, &bytes, &tools, &root, cancellation).await
    } else {
        let prepared = PreparedRemoteStreamInput::start_without_request(
            &producer(&source, &root),
            RemoteStreamSpawnContext::mpeg_ts("synthetic-shared-caption"),
            Instant::now() + Duration::from_secs(10),
            cancellation,
        )
        .await
        .unwrap();
        prepare_command(prepared, &bytes, &tools, &root).await
    };
    let output = capture_consumer(prepared, &command, &root).await;
    let actual = pixels(&tools[0], &output, None).await;
    compare_burn(&actual, &expected[0], &expected[1], &expected[2]);
    tone(&tools[0], &output, 660).await;
    let inventory = stream_inventory(&tools[1], &output).await;
    assert_eq!(
        inventory.len(),
        2,
        "actual output contains only selected programme AV"
    );
    assert_eq!(
        inventory
            .iter()
            .find(|stream| stream["codec_type"] == "audio")
            .unwrap()["tags"]["language"],
        "nld"
    );
    assert_eq!(
        tokio::fs::read_to_string(root.join("invocations"))
            .await
            .unwrap(),
        "started\n"
    );
    write_json(
        &root,
        "completion.json",
        &serde_json::json!({
            "case":"shared-pipe-source-caption-viability", "producer_invocations":1,
            "page":888,"audio":"nld660","output_before_producer_eof":true,
            "joined_cleanup":true,"active_transcodes":0,"ordinary_route_acceptance":false,
            "owned_page_selection_and_final_command_binding":owned_selection,
        }),
    );
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi, Unix managed producer and isolated native app data; component trial only"]
fn shared_single_producer_prefix_burns_observed_page888_before_eof() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run(false)));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi, Unix managed producer and isolated native app data; owned-selection component only"]
fn owned_live_producer_selects_observed_page_and_seals_consuming_child() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run(true)));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
