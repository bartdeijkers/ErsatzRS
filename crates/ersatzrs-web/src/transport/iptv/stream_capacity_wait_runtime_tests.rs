//! Default-capacity handoff through one ordinary HTTP response. Reuses the
//! managed Local caption fixture and retains failures before asserting them.

use ersatzrs_infra::transcode_capacity::{try_reserve_active_transcode, ActiveTranscodeSlot};

use super::*;

#[path = "stream_capacity_disconnect_runtime_tests.rs"]
mod disconnect;

#[path = "stream_capacity_dvb_transport_tests.rs"]
mod dvb_transport;

const WAIT_SECONDS: u64 = 14;
const CAPTURE_SECONDS: u64 = 36;
// Cold Hybrid includes admitted source-caption startup and three real HLS
// segments before final wrapper probing. Retain enough programme tail to cross
// the previously observed twelve-second source starvation, with no threshold
// or capacity relaxation.
const HYBRID_CAPTURE_SECONDS: u64 = 52;
const CAPTURE_LIMIT: usize = 32 * 1024 * 1024;

#[derive(Clone, Copy)]
enum CapacityAudio {
    Aac,
    Ac3Copy,
}

impl CapacityAudio {
    fn codec(self) -> &'static str {
        match self {
            Self::Aac => "aac",
            Self::Ac3Copy => "ac3",
        }
    }

    fn profile(self) -> FFmpegProfileAudioFormat {
        match self {
            Self::Aac => FFmpegProfileAudioFormat::Aac,
            Self::Ac3Copy => FFmpegProfileAudioFormat::Copy,
        }
    }
}

async fn capacity_source_audio(
    state: &AppState,
    version: i64,
    audio: CapacityAudio,
    dvb_copy: bool,
) -> Value {
    // Read the managed scan's stored inventory, without reopening the source
    // while the viewer waits. Both language alternatives must remain present.
    let streams = ersatzrs_infra::playback::load_source_audio_streams(&state.pool, version)
        .await
        .unwrap();
    assert_eq!(streams.len(), 2);
    assert_eq!(streams[0].index, 1);
    assert_eq!(streams[0].language.as_deref(), Some("eng"));
    assert_eq!(streams[1].index, 3);
    assert_eq!(streams[1].language.as_deref(), Some("nld"));
    let expected = ersatzrs_ffmpeg::SourceAudioCodec::from_ffmpeg_name(audio.codec());
    assert!(streams.iter().all(|stream| stream.codec == expected));
    let profile = ersatzrs_infra::playback::load_channel_playback_profile(&state.pool, 1)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(profile.profile.audio_format, audio.profile());
    assert_eq!(
        profile.profile.video_format,
        if dvb_copy {
            FFmpegProfileVideoFormat::Copy
        } else {
            FFmpegProfileVideoFormat::H264
        }
    );
    json!({"source_codec":audio.codec(),"profile_audio_format":format!("{:?}",profile.profile.audio_format),
        "persisted_audio":streams.iter().map(|stream| json!({"index":stream.index,
            "codec":format!("{:?}",stream.codec),"language":stream.language,
            "channels":stream.channels})).collect::<Vec<_>>()})
}

async fn capture_wait(public: &Server, root: &Path, capture_seconds: u64) -> (Vec<u8>, Value) {
    let occupied = ["capacity-fixture-first", "capacity-fixture-second"].map(|channel| {
        try_reserve_active_transcode(
            DEFAULT_ACTIVE_TRANSCODE_LIMIT,
            ActiveTranscodeSlot::new_mpeg_ts(channel),
        )
        .expect("both production-default slots must be occupied before the request")
    });
    let start = tokio::time::Instant::now();
    let release = tokio::spawn(async move {
        tokio::time::sleep_until(start + Duration::from_secs(WAIT_SECONDS)).await;
        let before = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
        drop(occupied);
        json!({"elapsed_ms":start.elapsed().as_millis(),"active_before_release":before.active,"peak":before.peak})
    });
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(25),
        client.get(public.url("/iptv/channel/1.ts")).send(),
    )
    .await;
    let mut bytes = Vec::new();
    let mut samples = Vec::new();
    let mut status = None;
    let mut error = None;
    let mut headers_ms = None;
    let mut first_bytes_ms = None;
    match response {
        Ok(Ok(response)) => {
            status = Some(response.status().as_u16());
            headers_ms = Some(start.elapsed().as_millis());
            let mut body = response.bytes_stream();
            let end = start + Duration::from_secs(capture_seconds);
            while let Ok(next) = tokio::time::timeout_at(end, body.next()).await {
                match next {
                    Some(Ok(chunk)) => {
                        if first_bytes_ms.is_none() {
                            first_bytes_ms = Some(start.elapsed().as_millis());
                        }
                        if bytes.len() + chunk.len() > CAPTURE_LIMIT {
                            error = Some("capture byte limit exceeded".to_owned());
                            break;
                        }
                        bytes.extend_from_slice(&chunk);
                        let elapsed = start.elapsed().as_secs();
                        if samples
                            .last()
                            .is_none_or(|sample: &Value| sample["second"] != elapsed)
                        {
                            let capacity =
                                active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
                            samples.push(
                                json!({"second":elapsed,"elapsed_ms":start.elapsed().as_millis(),
                                "bytes":bytes.len(),"active":capacity.active,"peak":capacity.peak}),
                            );
                        }
                    }
                    Some(Err(failure)) => {
                        error = Some(failure.to_string());
                        break;
                    }
                    None => {
                        error =
                            Some("HTTP response ended before bounded capture finished".to_owned());
                        break;
                    }
                }
            }
            drop(body);
        }
        Ok(Err(failure)) => error = Some(failure.to_string()),
        Err(_) => error = Some("HTTP response headers exceeded 25-second bound".to_owned()),
    }
    let stopped_ms = start.elapsed().as_millis();
    drop(client);
    let release = release.await.unwrap();
    tokio::fs::write(root.join("capacity-handoff.ts"), &bytes)
        .await
        .unwrap();
    let observation = json!({"request_count":1,"status":status,"headers_ms":headers_ms,
        "first_bytes_ms":first_bytes_ms,"capture_stopped_ms":stopped_ms,"capture_limit_seconds":capture_seconds,
        "release":release,"bytes":bytes.len(),"samples":samples,"error":error});
    (bytes, observation)
}

async fn programme_caption_frame(
    ffmpeg: &str,
    bytes: &[u8],
    expected: &[Vec<u8>; 3],
    root: &Path,
    minimum_programme_samples: usize,
) -> Vec<u8> {
    // A wait screen deliberately occupies the opening frames. Decode the whole
    // response and compare every 25 decoded frames against the same
    // independent source-page oracle; do not reinterpret reset TS timestamps.
    let frames = probe(
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
            "0:v:0",
            "-vf",
            "select=not(mod(n\\,25)),scale=492:250",
            "-an",
            "-fps_mode",
            "passthrough",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
        bytes,
        FRAME_BYTES * 64,
    )
    .await
    .unwrap();
    assert_eq!(frames.len() % FRAME_BYTES, 0);
    let difference = |actual: &[u8], oracle: &[u8]| -> u64 {
        actual
            .iter()
            .zip(oracle)
            .map(|(left, right)| u64::from(left.abs_diff(*right)))
            .sum()
    };
    let matches: Vec<_> = frames
        .chunks_exact(FRAME_BYTES)
        .map(|frame| {
            let caption = difference(frame, &expected[0]);
            caption < difference(frame, &expected[1]) / 2
                && caption < difference(frame, &expected[2]) / 2
        })
        .collect();
    let first = matches.iter().position(|matching| *matching);
    tokio::fs::write(
        root.join("capacity-video-continuity.json"),
        serde_json::to_vec_pretty(&json!({
            "sample_every_decoded_frames":25,
            "sample_count":matches.len(),
            "first_programme_sample":first,
            "source888_matches":matches,
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let first = first.expect("programme video must decode from the same response");
    assert!(
        matches.len() - first >= minimum_programme_samples,
        "programme must remain visible through the required tail"
    );
    for frame in frames.chunks_exact(FRAME_BYTES).skip(first) {
        compare_burn(frame, &expected[0], &expected[1], &expected[2]);
    }
    frames[first * FRAME_BYTES..(first + 1) * FRAME_BYTES].to_vec()
}

async fn run_capacity_wait(mode: StreamingMode, audio: CapacityAudio) {
    run_capacity_wait_delivery(mode, audio, false).await;
}

#[expect(
    clippy::disallowed_methods,
    reason = "Requires an explicit new proof-owned directory for retained synthetic runtime evidence"
)]
async fn run_capacity_wait_delivery(mode: StreamingMode, audio: CapacityAudio, dvb_copy: bool) {
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
    let root = PathBuf::from(
        std::env::var("ERSATZRS_TELETEXT_CAPACITY_PROOF_DIR")
            .expect("runner must supply a new capacity proof directory"),
    );
    tokio::fs::create_dir(&root)
        .await
        .expect("capacity proof directory must not already exist");
    println!("default-two capacity handoff evidence: {}", root.display());
    let library = root.join("library");
    let (source, expected) = if dvb_copy {
        pinned_dvb_source(&library, &root).await
    } else {
        let source =
            local_source_duration_with_audio(&tools[0], &library, "60", audio.codec()).await;
        let expected = calibration(&tools, &source, &root).await;
        (source, expected)
    };
    let unused_provider = Server::start(Router::new()).await;
    let state = application(&root, &unused_provider).await;
    let (item, version) = scan_source(&state, &library, &tools[1]).await;
    capacity_profile(&state, audio, dvb_copy).await;
    let source_audio = capacity_source_audio(&state, version, audio, dvb_copy).await;
    capacity_segmenter_defaults(&state, mode).await;
    let generation = install_and_accept(&state, &root).await;
    let internal = Server::start(build_internal_app(state.clone())).await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let calls = tokio::fs::read(root.join("addon-calls")).await.unwrap();
    assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
    assert_eq!(
        load_active_transcode_limit(&state.pool)
            .await
            .unwrap()
            .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
        2
    );
    start_capacity_programme(&state, item, mode).await;
    let capture_seconds = if mode == StreamingMode::TransportStreamHybrid {
        HYBRID_CAPTURE_SECONDS
    } else if dvb_copy {
        46
    } else {
        CAPTURE_SECONDS
    };
    let (bytes, mut observation) = capture_wait(&public, &root, capture_seconds).await;
    observation["cleanup"] = cleanup_wait(&state).await;
    observation["mode"] = json!(format!("{mode:?}"));
    observation["audio"] = source_audio;
    observation["source_caption_delivery"] = json!(if dvb_copy { "DvbSubtitles" } else { "Burn" });
    if dvb_copy {
        record_dvb_source_clock_boundary(&mut observation, &source).await;
    }
    observation["addon_unchanged"] =
        json!(tokio::fs::read(root.join("addon-calls")).await.unwrap() == calls);
    observation["generation_unchanged"] = json!(
        state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap()
            .unwrap()
            .id
            == generation
    );
    // Close every owned listener/database before decoder or transport assertions
    // so a negative qualification does not leave a running test host behind.
    public.finish().await;
    internal.finish().await;
    unused_provider.finish().await;
    state.pool.close().await;
    if dvb_copy {
        observation["native_command_reports"] = retain_capacity_command_reports(&root).await;
    }
    tokio::fs::write(
        root.join("capacity-observation.json"),
        serde_json::to_vec_pretty(&observation).unwrap(),
    )
    .await
    .unwrap();
    println!("capacity handoff observation: {observation}");
    if dvb_copy {
        verify_dvb_wait(&tools, &root, &root, &bytes, &expected, &observation).await;
    } else {
        verify_wait(&tools, &root, &bytes, &expected, &observation).await;
    }
}

async fn record_dvb_source_clock_boundary(observation: &mut Value, source: &Path) {
    observation["actual_consumed_seek"] = Value::Null;
    observation["actual_consumed_seek_evidence_gap"] = json!(
        "Actual native FFREPORT files are retained before decoder probes. Extract the consumed final command and -ss externally; release time is not substituted for consumed seek."
    );
    observation["source_sha256_after"] =
        json!(capacity_digest(&tokio::fs::read(source).await.unwrap()));
}

async fn capacity_segmenter_defaults(state: &AppState, mode: StreamingMode) {
    if mode == StreamingMode::TransportStreamHybrid {
        // The parent fixture shortens both idle and startup to five seconds.
        // A fourteen-second occupied-capacity trial needs the real default.
        config(
            state,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
            None,
        )
        .await;
        assert_eq!(
            crate::transport::iptv::hls::load_segmenter_startup_timeout(state).await,
            Duration::from_mins(1)
        );
    }
}

async fn start_capacity_programme(state: &AppState, item: i64, mode: StreamingMode) {
    start_item(state, item, ChannelSubtitleMode::Any, "nld", mode).await;
    // Waiting must not exhaust the short baseline fixture's programme. Only
    // this synthetic item is extended; admission and worker timeouts are real.
    sqlx::query(r#"UPDATE "PlayoutItem" SET "Finish"=?,"OutPoint"='00:01:00' WHERE "PlayoutId"=1"#)
        .bind(super::super::super::encode_sql_datetime(
            Utc::now() + chrono::Duration::seconds(60),
        ))
        .execute(&state.pool)
        .await
        .unwrap();
}

async fn capacity_profile(state: &AppState, audio: CapacityAudio, dvb_copy: bool) {
    software_profile_with_audio(state, audio.profile()).await;
    if dvb_copy {
        let mut profile = state.repositories.ffmpeg_profile.get(1).await.unwrap();
        profile.video_format = FFmpegProfileVideoFormat::Copy;
        profile.transcode_video = false;
        state
            .repositories
            .ffmpeg_profile
            .upsert(&profile)
            .await
            .unwrap();
        let mut form = state
            .repositories
            .channel
            .get_editor_state(Some(1))
            .await
            .unwrap()
            .unwrap()
            .form;
        form.source_teletext_delivery = Some(1);
        form.streaming_engine = 0;
        form.streaming_mode = 1;
        let saved = state
            .repositories
            .channel
            .save_editor_form(form)
            .await
            .unwrap();
        assert!(saved.is_success(), "{:?}", saved.validation_errors);
        assert_eq!(
            ersatzrs_infra::source_teletext_delivery::load_channel_source_teletext_delivery(
                &state.pool,
                1
            )
            .await
            .unwrap(),
            ersatzrs_domain::SourceTeletextCaptionDelivery::DvbSubtitles
        );
    }
}

async fn cleanup_wait(state: &AppState) -> Value {
    let cleanup_started = tokio::time::Instant::now();
    state.hls_sessions.stop_channel("1").await;
    let cleanup = tokio::time::timeout(Duration::from_secs(20), async {
        loop {
            if active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active == 0
                && state.hls_sessions.active_channels().await.is_empty()
                && state
                    .application
                    .operations()
                    .entity_locker()
                    .active_teletext_work()
                    == 0
            {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
    })
    .await
    .is_ok();
    let capacity = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
    json!({"complete":cleanup,"elapsed_ms":cleanup_started.elapsed().as_millis(),
        "active":capacity.active,"peak":capacity.peak,"hls_channels":state.hls_sessions.active_channels().await,
        "teletext_work":state.application.operations().entity_locker().active_teletext_work()})
}

async fn verify_wait(
    tools: &[String; 3],
    root: &Path,
    bytes: &[u8],
    expected: &[Vec<u8>; 3],
    observation: &Value,
) {
    let streams = capacity_inventory(tools, root, bytes).await;

    assert_wait_lifecycle(bytes, observation);
    verify_programme_media(tools, root, bytes, expected, observation).await;
    verify_transport_for_language(&tools[2], bytes, true, "nld").await;
    assert_eq!(
        streams
            .iter()
            .filter(|stream| stream["codec_type"] == "video")
            .count(),
        1
    );
    assert_eq!(
        streams
            .iter()
            .filter(|stream| stream["codec_type"] == "audio")
            .count(),
        1
    );
    let programme_audio = streams
        .iter()
        .find(|stream| stream["codec_type"] == "audio")
        .unwrap();
    assert_eq!(
        programme_audio["codec_name"], observation["audio"]["source_codec"],
        "the programme must retain the selected source codec when Copy is requested"
    );
    assert!(streams
        .iter()
        .any(|stream| stream["codec_name"] == "dvb_teletext"));
    let authored = String::from_utf8(
        capacity_programme_probe(
            &tools[0],
            &[
                "-hide_banner",
                "-loglevel",
                "error",
                "-nostdin",
                "-txt_page",
                "200",
                "-txt_format",
                "text",
                "-txt_chop_top",
                "0",
                "-txt_duration",
                "1000",
                "-f",
                "mpegts",
                "-i",
                "pipe:0",
                "-map",
                "0:s:0",
                "-c:s",
                "srt",
                "-f",
                "srt",
                "pipe:1",
            ],
            bytes,
        )
        .await,
    )
    .unwrap();
    assert!(authored.contains("Routed stored original"));
}

fn assert_wait_lifecycle(bytes: &[u8], observation: &Value) {
    assert_eq!(observation["addon_unchanged"], true);
    assert_eq!(observation["generation_unchanged"], true);
    assert!(
        observation["cleanup"]["complete"] == true,
        "owned leases/workers must drain after caller cancellation"
    );
    assert_eq!(
        observation["cleanup"]["peak"], 2,
        "capacity cannot increase to make the handoff pass"
    );
    assert_eq!(
        observation["status"], 200,
        "capacity waiting must stay in one successful HTTP response"
    );
    assert!(
        observation["error"].is_null(),
        "response must remain live through cancellation"
    );
    assert!(
        observation["samples"]
            .as_array()
            .unwrap()
            .iter()
            .any(|sample| {
                sample["second"]
                    .as_u64()
                    .is_some_and(|second| (9..WAIT_SECONDS).contains(&second))
                    && sample["active"] == 2
                    && sample["bytes"].as_u64().unwrap_or(0) > 0
            }),
        "the same response must deliver repeated waiting output while both slots remain occupied"
    );
    assert_eq!(bytes.len() % 188, 0);
}

async fn capacity_inventory(tools: &[String; 3], root: &Path, bytes: &[u8]) -> Vec<Value> {
    let mut streams = Vec::new();
    if bytes.first() == Some(&0x47) && bytes.len().is_multiple_of(188) {
        let report = probe(&tools[2], &["--json"], bytes, 256 * 1024)
            .await
            .unwrap();
        tokio::fs::write(root.join("capacity-tsanalyze.json"), &report)
            .await
            .unwrap();
        // The programme's PMT adds teletext only after the occupied-capacity
        // prefix. Discovery must cover this bounded capture, not just its
        // opening wait film; reading packets alone does not extend probing.
        let report: Value = serde_json::from_slice(
            &capacity_programme_probe(
                &tools[1],
                &[
                    "-v",
                    "error",
                    "-f",
                    "mpegts",
                    "-i",
                    "pipe:0",
                    "-count_packets",
                    "-show_entries",
                    "stream=index,codec_name,codec_type:stream_tags=language",
                    "-of",
                    "json",
                ],
                bytes,
            )
            .await,
        )
        .unwrap();
        streams = report["streams"].as_array().unwrap().clone();
        tokio::fs::write(
            root.join("capacity-streams.json"),
            serde_json::to_vec_pretty(&streams).unwrap(),
        )
        .await
        .unwrap();
    }

    streams
}

async fn verify_programme_media(
    tools: &[String; 3],
    root: &Path,
    bytes: &[u8],
    expected: &[Vec<u8>; 3],
    observation: &Value,
) {
    // This synthetic programme is 25fps. Hybrid must pass the formerly failing
    // twelve-second tail; uniform-frame sampling is not assumed for wait films.
    let minimum_programme_seconds = if observation["mode"] == "TransportStreamHybrid" {
        13
    } else {
        3
    };
    let actual =
        programme_caption_frame(&tools[0], bytes, expected, root, minimum_programme_seconds).await;
    tokio::fs::write(root.join("capacity-programme.rgb"), &actual)
        .await
        .unwrap();
    compare_burn(&actual, &expected[0], &expected[1], &expected[2]);
    let (audio, decoder) = capacity_audio_windows(&tools[0], bytes, observation).await;
    let first = audio.iter().position(|matching| *matching);
    tokio::fs::write(
        root.join("capacity-audio-continuity.json"),
        serde_json::to_vec_pretty(&json!({
            "sample_window_ms":100,
            "decoder":decoder,
            "first_programme_window":first,
            "selected660hz_matches":audio,
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    assert_capacity_audio_tail(&audio, minimum_programme_seconds);
}

fn assert_capacity_audio_tail(audio: &[bool], minimum_programme_seconds: usize) {
    let first = audio
        .iter()
        .position(|matching| *matching)
        .expect("selected Dutch audio must replace silent waiting audio");
    assert!(
        audio.len() - first >= minimum_programme_seconds * 10,
        "selected audio must persist through the required tail"
    );
    assert!(
        audio[first..].iter().all(|matching| *matching),
        "selected alternate audio must persist through the programme tail"
    );
}

async fn capacity_audio_windows(
    ffmpeg: &str,
    bytes: &[u8],
    observation: &Value,
) -> (Vec<bool>, Value) {
    if observation["audio"]["profile_audio_format"] != "Copy" {
        return (
            tone_windows(ffmpeg, bytes, 660, 8 * 1024 * 1024).await,
            json!({"client":"FFmpeg","tool":ffmpeg,"input":"whole captured HTTP response"}),
        );
    }
    // FFmpeg's initial AAC decoder does not switch to the valid later AC3 PMT.
    // The qualified VLC client follows that change on these same whole bytes.
    let vlc = exact_tool("ERSATZRS_TELETEXT_TEST_VLC");
    let version = String::from_utf8(probe(&vlc, &["--version"], &[], 4096).await.unwrap()).unwrap();
    assert!(
        version.starts_with("VLC version 3.0.20 "),
        "the retained client receipt pins VLC3.0.20"
    );
    let arguments = [
        "--intf",
        "dummy",
        "--play-and-exit",
        "--no-sout-video",
        "--sout",
        "#transcode{acodec=s16l,channels=1,samplerate=48000}:std{access=file,mux=raw,dst=-}",
        "fd://0",
    ];
    let pcm = probe(&vlc, &arguments, bytes, 8 * 1024 * 1024)
        .await
        .unwrap();
    let windows = tone_windows_from_pcm(&pcm, 660);
    (
        windows,
        json!({"client":"VLC","tool":vlc,"version":version.trim(),
        "arguments":arguments,"input":"whole captured HTTP response",
        "pcm_format":"signed16le mono48000Hz","pcm_bytes":pcm.len()}),
    )
}

async fn capacity_programme_probe(tool: &str, args: &[&str], bytes: &[u8]) -> Vec<u8> {
    let mut bounded = vec!["-probesize", "33554432", "-analyzeduration", "40000000"];
    bounded.extend_from_slice(args);
    probe(tool, &bounded, bytes, 256 * 1024).await.unwrap()
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi/libx264, ffprobe, TSDuck, loopback and fresh app-data; default2 occupied, repeated wait, one Legacy response"]
fn legacy_capacity_wait_preserves_timeline_captions_and_selected_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_capacity_wait(
            StreamingMode::TransportStream,
            CapacityAudio::Aac,
        )));
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi/libx264, ffprobe, TSDuck, loopback and fresh app-data; default2 occupied, repeated wait, one Hybrid response"]
fn hybrid_capacity_wait_preserves_timeline_captions_and_selected_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_capacity_wait(
            StreamingMode::TransportStreamHybrid,
            CapacityAudio::Aac,
        )));
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi/libx264/AC3, ffprobe, TSDuck, VLC3.0.20, loopback and fresh app-data; default2 occupied, repeated AAC slate, one Legacy response with AC3 Copy"]
fn legacy_capacity_wait_preserves_ac3_copy_captions_and_selected_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_capacity_wait(
            StreamingMode::TransportStream,
            CapacityAudio::Ac3Copy,
        )));
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi/libx264/AC3, ffprobe, TSDuck, VLC3.0.20, loopback and fresh app-data; default2 occupied, repeated AAC slate, one Hybrid response with AC3 Copy"]
fn hybrid_capacity_wait_preserves_ac3_copy_captions_and_selected_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_capacity_wait(
            StreamingMode::TransportStreamHybrid,
            CapacityAudio::Ac3Copy,
        )));
}

#[test]
#[ignore = "requires pinned native tools/VLC3.0.20 and retained Legacy/Hybrid AC3 Copy captures; does not start hosts or recapture HTTP"]
#[expect(
    clippy::disallowed_methods,
    reason = "Reads an explicit existing synthetic proof root for bounded decoder replay"
)]
fn retained_capacity_ac3_copy_replay_preserves_media_and_transport() {
    let root = PathBuf::from(
        std::env::var("ERSATZRS_TELETEXT_CAPACITY_REPLAY_DIR")
            .expect("runner must supply the existing AC3 Copy proof root"),
    );
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let tools = [
                exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG"),
                exact_tool("FFPROBE_PATH"),
                exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE"),
            ];
            for (case, mode) in [
                ("legacy1", "TransportStream"),
                ("hybrid1", "TransportStreamHybrid"),
            ] {
                let root = root.join(case);
                let observation: Value = serde_json::from_slice(
                    &tokio::fs::read(root.join("capacity-observation.json"))
                        .await
                        .unwrap(),
                )
                .unwrap();
                assert_eq!(observation["mode"], mode);
                assert_eq!(observation["audio"]["profile_audio_format"], "Copy");
                assert_eq!(observation["audio"]["source_codec"], "ac3");
                let bytes = tokio::fs::read(root.join("capacity-handoff.ts"))
                    .await
                    .unwrap();
                let expected = [
                    tokio::fs::read(root.join("page888-media1_6.rgb"))
                        .await
                        .unwrap(),
                    tokio::fs::read(root.join("page200-media1_6.rgb"))
                        .await
                        .unwrap(),
                    tokio::fs::read(root.join("plain-media1_6.rgb"))
                        .await
                        .unwrap(),
                ];
                verify_wait(&tools, &root, &bytes, &expected, &observation).await;
            }
        });
}

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit proof-owned per-process FFREPORT directory supplied by the native runner"
)]
async fn retain_capacity_command_reports(root: &Path) -> Value {
    let directory = PathBuf::from(
        std::env::var("ERSATZRS_TELETEXT_CAPACITY_REPORT_DIR")
            .expect("owned native FFREPORT directory"),
    );
    assert!(directory.is_absolute());
    let destination = root.join("native-command-reports");
    tokio::fs::create_dir(&destination).await.unwrap();
    let mut entries = tokio::fs::read_dir(&directory).await.unwrap();
    let mut reports = Vec::new();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        if !entry.file_type().await.unwrap().is_file() {
            continue;
        }
        assert!(reports.len() < 64, "bounded native command reports");
        assert!(entry.metadata().await.unwrap().len() <= 8 * 1024 * 1024);
        let bytes = tokio::fs::read(entry.path()).await.unwrap();
        let name = entry.file_name();
        tokio::fs::write(destination.join(&name), &bytes)
            .await
            .unwrap();
        reports.push(json!({"name":name.to_string_lossy(),"bytes":bytes.len(),"sha256":capacity_digest(&bytes)}));
    }
    reports.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    json!({"cutoff":"after owned host cleanup, before decoder/client probes", "reports":reports,
        "consumed_final_source_command_requires_external_extraction":true})
}

fn capacity_digest(bytes: &[u8]) -> String {
    use sha2::{Digest as _, Sha256};
    use std::fmt::Write as _;
    Sha256::digest(bytes)
        .iter()
        .fold(String::new(), |mut text, byte| {
            write!(text, "{byte:02x}").unwrap();
            text
        })
}

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit qualified synthetic source and retained independent pixel-oracle paths"
)]
async fn pinned_dvb_source(library: &Path, root: &Path) -> (PathBuf, [Vec<u8>; 3]) {
    let source = PathBuf::from(
        std::env::var("ERSATZRS_TELETEXT_CAPACITY_DVB_SOURCE")
            .expect("qualified finite-caption source"),
    );
    let oracle = PathBuf::from(
        std::env::var("ERSATZRS_TELETEXT_CAPACITY_DVB_ORACLE_DIR")
            .expect("retained independent source pixels"),
    );
    assert!(source.is_absolute() && oracle.is_absolute());
    let bytes = tokio::fs::read(&source).await.unwrap();
    assert_eq!(
        capacity_digest(&bytes),
        "b6ffa783e1cc96926a846b73fa8182755a11555296294ef9a740c09d9245c560"
    );
    tokio::fs::create_dir(library).await.unwrap();
    let source = library.join("Synthetic Caption.ts");
    tokio::fs::write(&source, bytes).await.unwrap();
    let mut expected = Vec::new();
    for (name, digest) in [
        (
            "888-A.rgb",
            "059bb32db28a0b4f8b21586bb09935342f743cc547ba89d49407220a059598f5",
        ),
        (
            "888-B.rgb",
            "ba011ee7adac246531d6c62dab7f3410ef24d9d68856d3b1116617ce08e550be",
        ),
        (
            "888-clear.rgb",
            "ff29c0aeed41d8e6b86c437391cdfe5a81d0809e652fed5c7950d562604786d1",
        ),
    ] {
        let bytes = tokio::fs::read(oracle.join(name)).await.unwrap();
        assert_eq!(bytes.len(), FRAME_BYTES);
        assert_eq!(capacity_digest(&bytes), digest);
        tokio::fs::write(root.join(name), &bytes).await.unwrap();
        expected.push(bytes);
    }
    (source, expected.try_into().unwrap())
}

async fn capacity_dvb_frames(
    ffmpeg: &str,
    bytes: &[u8],
    index: u64,
    overlay: bool,
    root: &Path,
) -> Vec<u8> {
    let graph = if overlay {
        format!(
            "[0:{index}]scale=492:250:force_original_aspect_ratio=decrease,pad=w=492:h=250:x=-1:y=-1:color=black@0[caption];[0:v:0][caption]overlay=x=(W-w)/2:y=(H-h)/2:format=0,select=not(mod(n\\,25))[frames]"
        )
    } else {
        "[0:v:0]select=not(mod(n\\,25)),scale=492:250[frames]".into()
    };
    let args = [
        "-hide_banner",
        "-loglevel",
        "error",
        "-nostdin",
        "-probesize",
        "33554432",
        "-analyzeduration",
        "40000000",
        "-canvas_size",
        "492x250",
        "-f",
        "mpegts",
        "-i",
        "pipe:0",
        "-filter_complex",
        &graph,
        "-map",
        "[frames]",
        "-an",
        "-fps_mode",
        "passthrough",
        "-pix_fmt",
        "rgb24",
        "-f",
        "rawvideo",
        "pipe:1",
    ];
    let result = probe(ffmpeg, &args, bytes, FRAME_BYTES * 64).await;
    let name = if overlay { "dvb" } else { "plain" };
    tokio::fs::write(
        root.join(format!("capacity-{name}-decode.json")),
        serde_json::to_vec_pretty(&json!({
            "tool":ffmpeg,"args":args,"whole_http_response":true,
            "error":result.as_ref().err().map(ToString::to_string),
            "output_bytes":result.as_ref().ok().map(Vec::len),
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let frames = result.expect("whole-response decoder must preserve Copy video and DVB captions");
    assert_eq!(frames.len() % FRAME_BYTES, 0);
    tokio::fs::write(root.join(format!("capacity-{name}-frames.rgb")), &frames)
        .await
        .unwrap();
    frames
}

async fn verify_dvb_pixels(
    tools: &[String; 3],
    root: &Path,
    bytes: &[u8],
    expected: &[Vec<u8>; 3],
    index: u64,
) {
    let frames = capacity_dvb_frames(&tools[0], bytes, index, true, root).await;
    let classes: Vec<_> = frames
        .chunks_exact(FRAME_BYTES)
        .map(|frame| expected.iter().position(|candidate| frame == candidate))
        .collect();
    tokio::fs::write(
        root.join("capacity-dvb-lifecycle.json"),
        serde_json::to_vec_pretty(&json!({
            "sample_every_decoded_video_frames":25,"oracle_indices":["A","B","clear"],
            "exact_rgb_classes":classes,"unknown_class_means_wait_or_mismatch":true,
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let first_a = classes
        .iter()
        .position(|value| *value == Some(0))
        .expect("active caption A must initialize after waiting");
    let first_b = classes
        .iter()
        .enumerate()
        .skip(first_a)
        .find(|(_, value)| **value == Some(1))
        .map(|(i, _)| i)
        .expect("later source update B must remain visible");
    let first_clear = classes
        .iter()
        .enumerate()
        .skip(first_b)
        .find(|(_, value)| **value == Some(2))
        .map(|(i, _)| i)
        .expect("explicit clear must remove B");
    assert!(
        first_b - first_a >= 2 && first_clear - first_b >= 2 && classes.len() - first_clear >= 2
    );
    assert!(classes[first_a..first_b]
        .iter()
        .all(|value| *value == Some(0)));
    assert!(classes[first_b..first_clear]
        .iter()
        .all(|value| *value == Some(1)));
    assert!(classes[first_clear..].iter().all(|value| *value == Some(2)));
    let plain = capacity_dvb_frames(&tools[0], bytes, index, false, root).await;
    let plain: Vec<_> = plain.chunks_exact(FRAME_BYTES).collect();
    assert!(plain.len() > first_clear);
    assert!(
        plain[first_a..]
            .iter()
            .all(|frame| frame.iter().all(|value| *value <= 20)),
        "video Copy must keep the source video free of burned caption pixels"
    );
}

async fn verify_dvb_wait(
    tools: &[String; 3],
    root: &Path,
    evidence: &Path,
    bytes: &[u8],
    expected: &[Vec<u8>; 3],
    observation: &Value,
) {
    let streams = capacity_inventory(tools, root, bytes).await;
    assert_wait_lifecycle(bytes, observation);
    assert_eq!(
        observation["source_sha256_after"],
        "b6ffa783e1cc96926a846b73fa8182755a11555296294ef9a740c09d9245c560"
    );
    dvb_transport::verify(tools, root, evidence, bytes).await;
    assert_eq!(
        streams.len(),
        4,
        "one video, selected audio, DVB subtitles and separate authored teletext"
    );
    for codec in ["mpeg2video", "aac", "dvb_subtitle", "dvb_teletext"] {
        assert_eq!(
            streams
                .iter()
                .filter(|stream| stream["codec_name"] == codec)
                .count(),
            1,
            "{codec}"
        );
    }
    let audio = tone_windows(&tools[0], bytes, 660, 8 * 1024 * 1024).await;
    tokio::fs::write(root.join("capacity-audio-continuity.json"), serde_json::to_vec_pretty(&json!({
        "decoder":"FFmpeg","input":"whole HTTP response","sample_window_ms":100,"selected660hz_matches":audio,
    })).unwrap()).await.unwrap();
    assert_capacity_audio_tail(&audio, 25);
    let authored_index = streams
        .iter()
        .find(|stream| stream["codec_name"] == "dvb_teletext")
        .unwrap()["index"]
        .as_u64()
        .unwrap();
    let mapping = format!("0:{authored_index}");
    let authored = capacity_programme_probe(
        &tools[0],
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-txt_page",
            "200",
            "-txt_format",
            "text",
            "-txt_chop_top",
            "0",
            "-txt_duration",
            "1000",
            "-f",
            "mpegts",
            "-i",
            "pipe:0",
            "-map",
            &mapping,
            "-c:s",
            "srt",
            "-f",
            "srt",
            "pipe:1",
        ],
        bytes,
    )
    .await;
    tokio::fs::write(root.join("capacity-authored-200.srt"), &authored)
        .await
        .unwrap();
    assert!(String::from_utf8(authored)
        .unwrap()
        .contains("Routed stored original"));
    let dvb_index = streams
        .iter()
        .find(|stream| stream["codec_name"] == "dvb_subtitle")
        .unwrap()["index"]
        .as_u64()
        .unwrap();
    verify_dvb_pixels(tools, root, bytes, expected, dvb_index).await;
}

#[test]
#[ignore = "requires pinned finite-source/oracle env paths, FFmpeg9, ffprobe, TSDuck and fresh app-data; default2 repeated wait, one Legacy response with explicit DVB delivery and video Copy"]
fn legacy_capacity_wait_preserves_video_copy_dvb_cue_lifecycle_and_selected_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_capacity_wait_delivery(
            StreamingMode::TransportStream,
            CapacityAudio::Aac,
            true,
        )));
}

#[test]
#[ignore = "requires pinned finite-source/oracle env paths, FFmpeg9, ffprobe, TSDuck and fresh app-data; default2 repeated wait, one Hybrid response with explicit DVB delivery and video Copy"]
fn hybrid_capacity_wait_preserves_video_copy_dvb_cue_lifecycle_and_selected_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run_capacity_wait_delivery(
            StreamingMode::TransportStreamHybrid,
            CapacityAudio::Aac,
            true,
        )));
}

#[test]
#[ignore = "requires pinned native tools and a retained DVB Copy capture; verifies stored bytes without starting hosts, recapturing HTTP or claiming current cleanup"]
#[expect(
    clippy::disallowed_methods,
    reason = "Reads explicit synthetic evidence and writes only a separate create-new replay directory"
)]
fn retained_capacity_dvb_copy_replay_preserves_exact_cue_clock_and_media() {
    let input = PathBuf::from(
        std::env::var("ERSATZRS_TELETEXT_CAPACITY_DVB_REPLAY_INPUT_DIR")
            .expect("runner must supply the existing DVB Copy proof root"),
    );
    let output = PathBuf::from(
        std::env::var("ERSATZRS_TELETEXT_CAPACITY_DVB_REPLAY_OUTPUT_DIR")
            .expect("runner must supply a fresh replay output directory"),
    );
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let input = tokio::fs::canonicalize(input).await.unwrap();
            tokio::fs::create_dir(&output).await.unwrap();
            let tools = [
                exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG"),
                exact_tool("FFPROBE_PATH"),
                exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE"),
            ];
            let observation: Value = serde_json::from_slice(
                &dvb_transport::bounded_read(&input.join("capacity-observation.json"), 256 * 1024)
                    .await,
            )
            .unwrap();
            let bytes =
                dvb_transport::bounded_read(&input.join("capacity-handoff.ts"), CAPTURE_LIMIT)
                    .await;
            let expected = [
                dvb_transport::bounded_read(&input.join("888-A.rgb"), FRAME_BYTES).await,
                dvb_transport::bounded_read(&input.join("888-B.rgb"), FRAME_BYTES).await,
                dvb_transport::bounded_read(&input.join("888-clear.rgb"), FRAME_BYTES).await,
            ];
            verify_dvb_wait(&tools, &output, &input, &bytes, &expected, &observation).await;
        });
}
