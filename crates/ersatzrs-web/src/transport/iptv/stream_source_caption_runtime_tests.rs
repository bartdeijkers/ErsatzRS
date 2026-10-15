//! Opt-in ordinary Local-source bitmap Burn/Off proof, independent of authored
//! page rendering. Unix shell addon/tool ownership follows the parent fixture.
//! A passing finite routed prefix is not full source/engine/client acceptance.

use ersatzrs_core::repositories::{ManagedLibraryOperationRepository as _, ManagedLibraryScanSpec};
use ersatzrs_domain::{ChannelSubtitleMode, ManagedLibraryOperationKind, MediaSourceKind};
use ersatzrs_infra::ffprobe::LocalProbeCleanupScope;
use ersatzrs_infra::repositories::SqliteManagedLibraryOperationRepository;
use ersatzrs_scanner::{scan_local_library, LocalScanConfig};

use super::*;

const WIDTH: usize = 492;
const HEIGHT: usize = 250;
const FRAME_BYTES: usize = WIDTH * HEIGHT * 3;

async fn local_source(ffmpeg: &str, library: &Path) -> PathBuf {
    tokio::fs::create_dir(library).await.unwrap();
    let bytes = probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=c=black:size=492x250:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=660:sample_rate=48000",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-t",
            "12",
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
            "-pcr_period",
            "20",
            "-mpegts_start_pid",
            "256",
            "-mpegts_pmt_start_pid",
            "4096",
            "-f",
            "mpegts",
            "pipe:1",
        ],
        ersatzrs_ffmpeg::teletext_probe::TELETEXT_DECODE_FIXTURE,
        MAX_CAPTURE,
    )
    .await
    .unwrap();
    assert_eq!(bytes.len() % 188, 0);
    assert!(
        bytes.len() / 188 < 32768,
        "source must fit the existing complete observation window"
    );
    let path = library.join("Synthetic Caption.ts");
    tokio::fs::write(&path, bytes).await.unwrap();
    path
}

async fn scan_source(state: &AppState, library: &Path, ffprobe: &str) -> (i64, i64) {
    state.pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (20);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (20);
        INSERT INTO "Library" ("Id","MediaSourceId","MediaKind","Name") VALUES (20,20,1,'Synthetic Local');
        INSERT INTO "LocalLibrary" ("Id") VALUES (20);"#).await.unwrap();
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (20,20,?)"#)
        .bind(library.to_str().unwrap())
        .execute(&state.pool)
        .await
        .unwrap();
    let repository = SqliteManagedLibraryOperationRepository(state.pool.clone());
    let operation = repository
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            20,
            ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: true,
            },
            false,
        )
        .await
        .unwrap()
        .unwrap();
    let claim = repository
        .claim_operation(operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    let scan = LocalScanConfig {
        library_id: 20,
        deep_scan: true,
        ffprobe_path: ffprobe.to_owned(),
        managed_claim: Some(claim.clone()),
    };
    let scope = LocalProbeCleanupScope::new();
    let result = Box::pin(scope.scope(scan_local_library(&state.pool, &scan))).await;
    scope.close();
    scope.wait_cleanup().await.unwrap();
    result.unwrap();
    assert_eq!(scope.cleanup_status(), Ok(true));
    assert!(repository.complete_operation(&claim).await.unwrap());
    let rows: Vec<(i64, i64)> = sqlx::query_as(
        r#"SELECT v."MovieId",v."Id" FROM "MediaVersion" v
        JOIN "MediaItem" m ON m."Id"=v."MovieId" WHERE m."LibraryPathId"=20"#,
    )
    .fetch_all(&state.pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), 1);
    let reports: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "SourceTeletextMetadata" WHERE "MediaVersionId"=?"#,
    )
    .bind(rows[0].1)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    assert_eq!(
        reports, 1,
        "real managed scan must retain source caption metadata"
    );
    rows[0]
}

async fn software_profile(state: &AppState) {
    state
        .pool
        .execute(
            r#"UPDATE "Resolution" SET "Width"=492,"Height"=250 WHERE "Id"=1;
        UPDATE "Playout" SET "ScheduleKind"=1,"ScheduleFile"=NULL WHERE "Id"=1;"#,
        )
        .await
        .unwrap();
    state
        .repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Synthetic software bitmap caption".into(),
            resolution_id: 1,
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
        })
        .await
        .unwrap();
    config(
        state,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT,
        Some(""),
    )
    .await;
    config(state, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING, Some(POLICY)).await;
    config(
        state,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
        Some("5"),
    )
    .await;
}

async fn start_item(
    state: &AppState,
    item: i64,
    mode: ChannelSubtitleMode,
    language: &str,
    streaming_mode: StreamingMode,
) {
    let mut channel = state.repositories.channel.get_by_number("1").await.unwrap();
    channel.subtitle_mode = mode;
    channel.preferred_subtitle_language_code = Some("eng".into());
    channel.preferred_audio_language_code = Some(language.into());
    channel.streaming_mode = streaming_mode;
    state.repositories.channel.upsert(&channel).await.unwrap();
    sqlx::query(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId"=1"#)
        .execute(&state.pool)
        .await
        .unwrap();
    let now = Utc::now();
    sqlx::query(r#"INSERT INTO "PlayoutItem" ("PlayoutId","MediaItemId","Start","Finish","InPoint","OutPoint",
        "DisableWatermarks","FillerKind","GuideGroup") VALUES (1,?,?,?,'00:00:00','00:00:12',0,0,0)"#)
        .bind(item).bind(super::super::encode_sql_datetime(now))
        .bind(super::super::encode_sql_datetime(now+chrono::Duration::seconds(12)))
        .execute(&state.pool).await.unwrap();
}

async fn pixels(ffmpeg: &str, bytes: &[u8], page: Option<&str>) -> Vec<u8> {
    let mut args = vec!["-hide_banner", "-loglevel", "error", "-nostdin"];
    if let Some(page) = page {
        args.extend([
            "-txt_page:2",
            page,
            "-txt_format:2",
            "bitmap",
            "-txt_default_region:2",
            "-1",
        ]);
    }
    args.extend(["-f", "mpegts", "-i", "pipe:0"]);
    if page.is_some() {
        args.extend(["-filter_complex",
            "[0:2]scale=492:250:force_original_aspect_ratio=decrease,pad=w=492:h=250:x=-1:y=-1:color=black@0[caption];[0:0][caption]overlay=x=(W-w)/2:y=(H-h)/2:format=0,select=between(t\\,1.59\\,1.61)[frame]",
            "-map","[frame]"]);
    } else {
        args.extend(["-map", "0:v:0", "-vf", "select=between(t\\,1.59\\,1.61)"]);
    }
    args.extend([
        "-an",
        "-fps_mode",
        "passthrough",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ]);
    let pixels = probe(ffmpeg, &args, bytes, FRAME_BYTES).await.unwrap();
    assert_eq!(
        pixels.len(),
        FRAME_BYTES,
        "one genuine media-relative1.6sec frame"
    );
    pixels
}

fn compare_burn(actual: &[u8], caption: &[u8], guide: &[u8], plain: &[u8]) {
    let foreground = |pixel: &[u8]| pixel.iter().any(|value| *value > 160);
    let cells = caption
        .chunks_exact(3)
        .filter(|pixel| foreground(pixel))
        .count();
    assert!(
        cells > 500,
        "independent page888 must contain caption pixels"
    );
    let disagreements = actual
        .chunks_exact(3)
        .zip(caption.chunks_exact(3))
        .filter(|(actual, expected)| foreground(actual) != foreground(expected))
        .count();
    assert!(
        disagreements <= cells / 5 + 300,
        "routed caption positions must match independently decoded888"
    );
    let difference = |left: &[u8], right: &[u8]| -> u64 {
        left.iter()
            .zip(right)
            .map(|(l, r)| u64::from(l.abs_diff(*r)))
            .sum()
    };
    let selected = difference(actual, caption);
    assert!(
        selected < difference(actual, guide) / 2,
        "route must burn888 rather than200"
    );
    assert!(
        selected < difference(actual, plain) / 2,
        "route must not silently omit selected captions"
    );
}

async fn tone(ffmpeg: &str, bytes: &[u8], frequency: usize) {
    let pcm = probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-map",
            "0:a:0",
            "-ac",
            "1",
            "-ar",
            "48000",
            "-f",
            "s16le",
            "pipe:1",
        ],
        bytes,
        2 * 1024 * 1024,
    )
    .await
    .unwrap();
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|s| i16::from_le_bytes(s.try_into().unwrap()))
        .collect();
    let matching = samples
        .chunks_exact(4800)
        .filter(|chunk| {
            let crossings = chunk
                .windows(2)
                .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
                .count();
            (crossings * 48000 / (2 * chunk.len())).abs_diff(frequency) <= 20
                && chunk.iter().any(|s| s.unsigned_abs() > 100)
        })
        .count();
    assert!(
        matching >= 10,
        "actual selected alternate audio tone must survive"
    );
}

async fn verify_case(
    tools: &[String; 3],
    output: &[u8],
    expected: &[Vec<u8>; 3],
    actual: &[u8],
    burn: bool,
    language: &str,
) {
    verify_transport_for_language(&tools[2], output, true, language).await;
    let streams = stream_inventory(&tools[1], output).await;
    assert_eq!(
        streams
            .iter()
            .filter(|s| s["codec_type"] == "audio")
            .count(),
        1
    );
    let video = streams.iter().find(|s| s["codec_type"] == "video").unwrap();
    assert_eq!(video["codec_name"], "h264");
    tone(&tools[0], output, if language == "eng" { 440 } else { 660 }).await;
    let text = String::from_utf8(decode_text(&tools[0], output, "200", "0:s:0").await).unwrap();
    assert!(text.contains("Routed stored original"));
    if burn {
        compare_burn(actual, &expected[0], &expected[1], &expected[2]);
    } else {
        assert!(
            actual.iter().all(|value| *value <= 20),
            "Off or rejected stale source must retain plain black AV without caption pixels"
        );
    }
}

async fn calibration(tools: &[String; 3], source: &Path, root: &Path) -> [Vec<u8>; 3] {
    let source_bytes = tokio::fs::read(source).await.unwrap();
    let inventory = stream_inventory(&tools[1], &source_bytes).await;
    assert_eq!(inventory.len(), 4);
    assert_eq!(inventory[2]["codec_name"], "dvb_teletext");
    assert_eq!(inventory[2]["tags"]["language"], "eng,eng");
    assert_eq!(inventory[1]["tags"]["language"], "eng");
    assert_eq!(inventory[3]["tags"]["language"], "nld");
    let text =
        String::from_utf8(decode_text(&tools[0], &source_bytes, "888", "0:s:0").await).unwrap();
    assert!(text.contains("Synthetic subtitle page 888"));
    let expected = [
        pixels(&tools[0], &source_bytes, Some("888")).await,
        pixels(&tools[0], &source_bytes, Some("200")).await,
        pixels(&tools[0], &source_bytes, None).await,
    ];
    assert!(expected[2].iter().all(|value| *value <= 20));
    for (name, bytes) in ["page888", "page200", "plain"].into_iter().zip(&expected) {
        tokio::fs::write(root.join(format!("{name}-media1_6.rgb")), bytes)
            .await
            .unwrap();
    }
    expected
}

async fn late_source_rejection_uses_actual_receipt(
    state: &AppState,
    source: &Path,
    item: i64,
    version: i64,
) {
    use ersatzrs_infra::playback::source_teletext::runtime::{self, RuntimeSelectionContext};
    use ersatzrs_infra::transcode_capacity::{try_reserve_active_transcode, ActiveTranscodeSlot};

    let path = source.to_str().unwrap();
    let file: i64 =
        sqlx::query_scalar(r#"SELECT "Id" FROM "MediaFile" WHERE "MediaVersionId"=? AND "Path"=?"#)
            .bind(version)
            .bind(path)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    let locker = state.application.operations().entity_locker();
    let deadline = tokio::time::Instant::now() + Duration::from_secs(15);
    let selection = runtime::select(RuntimeSelectionContext {
        pool: &state.pool,
        locker: &locker,
        media_item_id: Some(item),
        media_version_id: Some(version),
        media_file_id: Some(file),
        original_path: path,
        resolved_path: path,
        languages: &["eng".to_owned()],
        subtitle_mode: ChannelSubtitleMode::Any,
        streaming_mode: StreamingMode::TransportStream,
        playback_interval: runtime::SourceTeletextPlaybackInterval::from_start_and_duration(
            Duration::new(2, 123_456_789),
            Duration::new(9, 876_543_210),
        ),
        deadline,
    })
    .await
    .unwrap()
    .expect("actual scanned source must select an exact decoded page");
    let receipt = selection.source.expect("genuine exact-page source receipt");
    let interval = receipt
        .playback_interval()
        .expect("actual finite caller timing survives source preparation");
    assert_eq!(interval.start(), Duration::new(2, 123_456_789));
    assert_eq!(interval.duration(), Duration::new(9, 876_543_210));
    assert_eq!(locker.active_teletext_work(), 0);
    let reservation = try_reserve_active_transcode(
        DEFAULT_ACTIVE_TRANSCODE_LIMIT,
        ActiveTranscodeSlot::new_mpeg_ts("native-source-caption-preopen"),
    )
    .unwrap();
    // This marker is never executed. The actual ordinary routed AV behavior is
    // independently checked by the four existing captures below.
    let ordinary = ersatzrs_ffmpeg::FfmpegCommand::new_with_executable(
        "synthetic-ordinary-command-not-spawned",
        Vec::new(),
        vec!["-f".into(), "null".into(), "-".into()],
    );
    assert_native_inventory_fallback(state, receipt.clone(), &reservation, deadline, &ordinary)
        .await;
    let (consumer, primary) =
        assert_consumer_plan_fences(state, receipt, &reservation, deadline).await;
    let mut context = source_caption_denial_context(state, consumer, ordinary.clone());
    let modified = source.metadata().unwrap().modified().unwrap();
    let file = std::fs::File::options().write(true).open(source).unwrap();
    file.set_times(std::fs::FileTimes::new().set_modified(modified + Duration::from_secs(1)))
        .unwrap();
    let selected = super::super::source_caption_recovery::before_open(
        &primary,
        &mut context,
        Some(&reservation),
        deadline,
        None,
        None,
    )
    .await;
    // Restore the original observed mtime on the same bytes/inode before any
    // routed trial. This does not claim an immutable-file integrity guarantee.
    file.set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let selected = selected
        .unwrap()
        .expect("late source rejection must choose ordinary command");
    assert_eq!(selected, ordinary);
    assert!(context.source_caption.is_none());
    assert_eq!(context.ffmpeg_path, selected.executable);
    assert_eq!(
        context.graph_operation,
        ersatzrs_ffmpeg::selected_hardware_graph(&selected)
    );
    assert_eq!(locker.active_teletext_work(), 0);
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        1
    );
    drop(context);
    drop(reservation);
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
    println!("actual source receipt rejected changed filesystem mtime before open; exact ordinary command selected, no child spawn claimed");
}

fn source_caption_denial_context(
    state: &AppState,
    consumer: super::super::source_teletext::Consumer,
    ordinary: ersatzrs_ffmpeg::FfmpegCommand,
) -> super::super::StreamDenialRetry {
    super::super::StreamDenialRetry {
        source_caption: Some(consumer),
        ordinary_caption_fallback: Some(ordinary),
        song: None,
        pool: state.pool.clone(),
        shared_ffmpeg_runtime: std::sync::Arc::clone(&state.ffmpeg_runtime),
        runtime_capabilities: ersatzrs_ffmpeg::FfmpegCapabilityReport::default(),
        acceleration: ersatzrs_domain::HardwareAccelerationKind::None,
        source_stream: None,
        graph_operation: None,
        ffmpeg_path: "synthetic-caption-command-not-spawned".into(),
        rebuild: None,
    }
}

async fn assert_native_inventory_fallback(
    state: &AppState,
    receipt: std::sync::Arc<ersatzrs_infra::playback::source_teletext::runtime::PlaybackSource>,
    reservation: &ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation,
    deadline: tokio::time::Instant,
    ordinary: &ersatzrs_ffmpeg::FfmpegCommand,
) {
    let profile = ersatzrs_infra::playback::load_playback_profile(&state.pool, 1)
        .await
        .unwrap()
        .unwrap();
    let caption = receipt.input();
    let desired = ersatzrs_ffmpeg::DesiredVideoState::from_profile(
        &profile.profile,
        &profile.resolution,
        None,
        None,
        StreamingMode::TransportStream,
    );
    let mut request = ersatzrs_ffmpeg::PipelineRequest::new(
        ersatzrs_ffmpeg::InputFile::video(&caption.path),
        desired,
    );
    // A valid command seal and matching caption cannot turn a real subtitle
    // stream into the video track asserted by stale selection metadata.
    request.source_video_stream = Some(
        ersatzrs_ffmpeg::SourceVideoStream::new(ersatzrs_ffmpeg::SourceVideoCodec::H264)
            .with_index(caption.streams[0].index),
    );
    request.inputs.push(caption);
    let interval = receipt.playback_interval().unwrap();
    request.state.start = Some(interval.start());
    request.state.duration = Some(interval.duration());
    request.ffmpeg_executable = Some("synthetic-caption-command-not-spawned".into());
    let (command, plan) = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
        ersatzrs_domain::HardwareAccelerationKind::None,
        request,
    )
    .unwrap();
    receipt
        .check_consumer_command(&command, plan.as_ref())
        .expect("command and caption identity match before actual inventory checking");
    let consumer = super::super::source_teletext::Consumer::new(receipt, plan);
    let mut context = source_caption_denial_context(state, consumer, ordinary.clone());
    let selected = super::super::source_caption_recovery::before_open(
        &command,
        &mut context,
        Some(reservation),
        deadline,
        None,
        None,
    )
    .await
    .unwrap()
    .expect("actual native subtitle cannot qualify as selected video");
    assert_eq!(&selected, ordinary);
    assert!(context.source_caption.is_none());
    assert_eq!(
        state
            .application
            .operations()
            .entity_locker()
            .active_teletext_work(),
        0
    );
    eprintln!("actual native wrong-role selection rejected despite valid plan/caption seal; exact ordinary command retained and probe work drained");
}

async fn assert_consumer_plan_fences(
    state: &AppState,
    receipt: std::sync::Arc<ersatzrs_infra::playback::source_teletext::runtime::PlaybackSource>,
    reservation: &ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation,
    deadline: tokio::time::Instant,
) -> (
    super::super::source_teletext::Consumer,
    ersatzrs_ffmpeg::FfmpegCommand,
) {
    let profile = ersatzrs_infra::playback::load_playback_profile(&state.pool, 1)
        .await
        .unwrap()
        .unwrap();
    let caption = receipt.input();
    let desired = ersatzrs_ffmpeg::DesiredVideoState::from_profile(
        &profile.profile,
        &profile.resolution,
        None,
        None,
        StreamingMode::TransportStream,
    );
    let mut request = ersatzrs_ffmpeg::PipelineRequest::new(
        ersatzrs_ffmpeg::InputFile::video(&caption.path),
        desired,
    );
    request.inputs.push(caption.clone());
    let interval = receipt.playback_interval().unwrap();
    request.state.start = Some(interval.start());
    request.state.duration = Some(interval.duration());
    request.ffmpeg_executable = Some("synthetic-caption-command-not-spawned".into());
    let (primary, plan) = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
        ersatzrs_domain::HardwareAccelerationKind::None,
        request.clone(),
    )
    .unwrap();
    let plan = plan.expect("actual generated source-caption plan");
    assert!(plan.matches_command(&primary));
    let mut consumer =
        super::super::source_teletext::Consumer::new(receipt.clone(), Some(plan.clone()));
    consumer
        .before_open(&primary, Some(reservation), deadline)
        .await
        .unwrap();
    let mut changed = primary.clone();
    changed.executable.push_str("-changed");
    assert!(consumer
        .clone()
        .before_open(&changed, Some(reservation), deadline)
        .await
        .is_err());
    let mut changed = primary.clone();
    changed
        .environment
        .push(ersatzrs_ffmpeg::EnvironmentVariable::new(
            "SYNTHETIC_PLAN",
            "changed",
        ));
    assert!(consumer
        .clone()
        .before_open(&changed, Some(reservation), deadline)
        .await
        .is_err());
    let mut changed = primary.clone();
    changed.arguments.push("-nostats".into());
    assert!(consumer
        .clone()
        .before_open(&changed, Some(reservation), deadline)
        .await
        .is_err());
    let mut missing = super::super::source_teletext::Consumer::new(receipt.clone(), None);
    assert!(missing
        .before_open(&primary, Some(reservation), deadline)
        .await
        .is_err());
    assert!(consumer.clone().before_retry(&primary, None).await.is_err());
    let mut retried = consumer.clone();
    retried.before_retry(&primary, Some(plan)).await.unwrap();
    assert!(
        retried.before_retry(&primary, None).await.is_err(),
        "retry cannot borrow its previous plan"
    );
    assert_consumer_interval_fences(&request, &receipt, &consumer, reservation, deadline).await;
    let stream = request
        .inputs
        .last_mut()
        .unwrap()
        .streams
        .first_mut()
        .unwrap();
    stream.source_teletext_page = Some(ersatzrs_domain::TeletextPageNumber::new(200).unwrap());
    let (wrong_page, wrong_plan) =
        ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
            ersatzrs_domain::HardwareAccelerationKind::None,
            request,
        )
        .unwrap();
    assert!(
        super::super::source_teletext::Consumer::new(receipt, wrong_plan)
            .before_open(&wrong_page, Some(reservation), deadline)
            .await
            .is_err()
    );
    println!("generated consumer fences: matching nonzero initial/fresh retry and native-equivalent timing accepted; changed seek/duration, executable/argv, missing or reused retry plan and independently sealed wrong page rejected before spawn; unknown duration/independent caption clock not inferred");
    (consumer, primary)
}

async fn assert_consumer_interval_fences(
    request: &ersatzrs_ffmpeg::PipelineRequest,
    receipt: &std::sync::Arc<ersatzrs_infra::playback::source_teletext::runtime::PlaybackSource>,
    consumer: &super::super::source_teletext::Consumer,
    reservation: &ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation,
    deadline: tokio::time::Instant,
) {
    let interval = receipt.playback_interval().unwrap();
    for (start, duration) in [
        (
            interval.start() + Duration::from_secs(1),
            interval.duration(),
        ),
        (
            interval.start(),
            interval.duration() + Duration::from_secs(1),
        ),
    ] {
        let mut changed = request.clone();
        changed.state.start = Some(start);
        changed.state.duration = Some(duration);
        let (changed, changed_plan) =
            ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
                ersatzrs_domain::HardwareAccelerationKind::None,
                changed,
            )
            .unwrap();
        assert!(changed_plan.as_ref().unwrap().matches_command(&changed));
        assert!(receipt
            .check_consumer_command(&changed, changed_plan.as_ref())
            .is_err());
        assert!(super::super::source_teletext::Consumer::new(
            receipt.clone(),
            changed_plan.clone(),
        )
        .before_open(&changed, Some(reservation), deadline)
        .await
        .is_err());
        assert!(consumer
            .clone()
            .before_retry(&changed, changed_plan)
            .await
            .is_err());
    }
    // Native independently drops submicrosecond fractions from each argument.
    // Missing -t and an independently opened caption remain noncomparable;
    // these pure checks do not grant native bitmap or seek eligibility.
    let mut rounded = request.clone();
    rounded.state.start = Some(Duration::from_micros(
        u64::try_from(interval.start().as_micros()).unwrap(),
    ));
    rounded.state.duration = Some(Duration::from_micros(
        u64::try_from(interval.duration().as_micros()).unwrap(),
    ));
    let mut unbounded = request.clone();
    unbounded.state.duration = None;
    let mut independent = request.clone();
    independent.inputs[0].path = "synthetic-other-source.ts".into();
    for compatible in [rounded, unbounded, independent] {
        let (command, plan) = ersatzrs_ffmpeg::build_pipeline_command_with_source_teletext_plan(
            ersatzrs_domain::HardwareAccelerationKind::None,
            compatible,
        )
        .unwrap();
        assert!(plan.as_ref().unwrap().matches_command(&command));
        receipt
            .check_consumer_command(&command, plan.as_ref())
            .expect("compatible or unknown timing must not create a false mismatch");
    }
}

async fn finish_hybrid_case(state: &AppState, mode: StreamingMode) {
    if mode != StreamingMode::TransportStreamHybrid {
        return;
    }
    // The actual native HLS item worker and final wrapper each use one
    // default slot. Retire the owned session before the next preference/source
    // change; no prior worker may serve that case.
    state.hls_sessions.stop_channel("1").await;
    capacity_zero().await;
    // Stop requests cancellation; the owning worker unregisters after its
    // asynchronous cleanup. Do not manually remove a still-owned session.
    tokio::time::timeout(Duration::from_secs(5), async {
        while !state.hls_sessions.active_channels().await.is_empty() {
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .expect("retired HLS worker remained registered after cleanup");
    assert_eq!(
        state
            .application
            .operations()
            .entity_locker()
            .active_teletext_work(),
        0
    );
}

async fn run_source_captions(streaming_mode: StreamingMode) {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("ersatzrs_web=debug,ersatzrs_infra=debug")
        .with_test_writer()
        .try_init();
    isolated_bundled_wrapper().await;
    let tools = [
        exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG"),
        exact_tool("FFPROBE_PATH"),
        exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE"),
    ];
    assert_eq!(ersatzrs_ffmpeg::ffmpeg_path(), tools[0]);
    assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), tools[1]);
    let root = tempfile::tempdir().unwrap().keep();
    println!("bounded routed source-caption evidence: {}", root.display());
    let library = root.join("library");
    let source = local_source(&tools[0], &library).await;
    let expected = calibration(&tools, &source, &root).await;
    // Reuse the parent application constructor. This empty server supplies its
    // unrelated Plex fixture address; the actual item below is managed Local.
    let unused_provider = Server::start(Router::new()).await;
    let state = application(&root, &unused_provider).await;
    let (item, version) = scan_source(&state, &library, &tools[1]).await;
    software_profile(&state).await;
    let generation = install_and_accept(&state, &root).await;
    let internal = Server::start(build_internal_app(state.clone())).await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let calls = tokio::fs::read(root.join("addon-calls")).await.unwrap();
    Box::pin(late_source_rejection_uses_actual_receipt(
        &state, &source, item, version,
    ))
    .await;
    // Establish ordinary AV first, before attributing any routed deadline
    // failure to the optional source-caption decoder. Every case still runs.
    for (label, mode, language) in [
        ("off-nld", ChannelSubtitleMode::None, "nld"),
        ("burn-eng", ChannelSubtitleMode::Any, "eng"),
        ("burn-nld", ChannelSubtitleMode::Any, "nld"),
        ("stale-eng", ChannelSubtitleMode::Any, "eng"),
    ] {
        if label == "stale-eng" {
            // Change only the exact source revision. The current report remains
            // stale until a real rescan; unrelated video/audio remain intact.
            let modified = source.metadata().unwrap().modified().unwrap();
            std::fs::File::options()
                .write(true)
                .open(&source)
                .unwrap()
                .set_times(
                    std::fs::FileTimes::new().set_modified(modified + Duration::from_secs(1)),
                )
                .unwrap();
        }
        reset_active_transcode_state_for_tests();
        start_item(&state, item, mode, language, streaming_mode).await;
        let output =
            capture_recorded(&public, Some(&root.join(format!("{label}-partial.ts")))).await;
        finish_hybrid_case(&state, streaming_mode).await;
        tokio::fs::write(root.join(format!("{label}.ts")), &output)
            .await
            .unwrap();
        let actual = pixels(&tools[0], &output, None).await;
        tokio::fs::write(root.join(format!("{label}-media1_6.rgb")), &actual)
            .await
            .unwrap();
        verify_case(
            &tools,
            &output,
            &expected,
            &actual,
            mode != ChannelSubtitleMode::None && label != "stale-eng",
            language,
        )
        .await;
        assert_eq!(
            tokio::fs::read(root.join("addon-calls")).await.unwrap(),
            calls
        );
        assert_eq!(
            state
                .application
                .accepted_teletext_generation(1)
                .await
                .unwrap()
                .unwrap()
                .id,
            generation
        );
        println!(
            "routed source caption verified: {label}, originalPID512, defaultcapacity2, packets={}",
            output.len() / 188
        );
    }
    public.finish().await;
    internal.finish().await;
    unused_provider.finish().await;
    state.pool.close().await;
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9+libzvbi/libx264, TSDuck, loopback and fresh isolated app-data; genuine Local scan and ordinary source captions"]
fn normal_local_route_burns_exact_source_caption_and_preserves_off_and_alternate_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_source_captions(
            StreamingMode::TransportStream,
        )));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9+libzvbi/libx264, TSDuck, loopback and fresh isolated app-data; actual native HLS producer plus final authored MPEG-TS wrapper"]
fn normal_hybrid_local_route_burns_exact_source_caption_and_preserves_off_and_alternate_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_source_captions(
            StreamingMode::TransportStreamHybrid,
        )));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
