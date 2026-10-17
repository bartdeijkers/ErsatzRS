//! Default-capacity handoff through one ordinary HTTP response. Reuses the
//! managed Local caption fixture and retains failures before asserting them.

use ersatzrs_infra::transcode_capacity::{try_reserve_active_transcode, ActiveTranscodeSlot};

use super::*;

const WAIT_SECONDS: u64 = 14;
const CAPTURE_SECONDS: u64 = 36;
const CAPTURE_LIMIT: usize = 32 * 1024 * 1024;

async fn capture_wait(public: &Server, root: &Path) -> (Vec<u8>, Value) {
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
            let end = start + Duration::from_secs(CAPTURE_SECONDS);
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
        "first_bytes_ms":first_bytes_ms,"capture_stopped_ms":stopped_ms,"capture_limit_seconds":CAPTURE_SECONDS,
        "release":release,"bytes":bytes.len(),"samples":samples,"error":error});
    (bytes, observation)
}

async fn programme_caption_frame(ffmpeg: &str, bytes: &[u8], caption: &[u8]) -> Vec<u8> {
    // A wait screen deliberately occupies the opening frames. Decode the whole
    // response and compare a bounded one-frame/second sample against the same
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
        FRAME_BYTES * 50,
    )
    .await
    .unwrap();
    frames
        .chunks_exact(FRAME_BYTES)
        .min_by_key(|frame| {
            frame
                .iter()
                .zip(caption)
                .map(|(left, right)| u64::from(left.abs_diff(*right)))
                .sum::<u64>()
        })
        .expect("programme video must decode from the same response")
        .to_vec()
}

#[expect(
    clippy::disallowed_methods,
    reason = "Requires an explicit new proof-owned directory for retained synthetic runtime evidence"
)]
async fn run_capacity_wait(mode: StreamingMode) {
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
    let source = local_source_duration(&tools[0], &library, "60").await;
    let expected = calibration(&tools, &source, &root).await;
    let unused_provider = Server::start(Router::new()).await;
    let state = application(&root, &unused_provider).await;
    let (item, _) = scan_source(&state, &library, &tools[1]).await;
    software_profile(&state).await;
    if mode == StreamingMode::TransportStreamHybrid {
        // The parent fixture shortens both idle and startup to five seconds.
        // A fourteen-second occupied-capacity trial needs the real default.
        config(
            &state,
            ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
            None,
        )
        .await;
        assert_eq!(
            crate::transport::iptv::hls::load_segmenter_startup_timeout(&state).await,
            Duration::from_mins(1)
        );
    }
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
    start_item(&state, item, ChannelSubtitleMode::Any, "nld", mode).await;
    // Waiting must not exhaust the short baseline fixture's programme. Only
    // this synthetic item is extended; admission and worker timeouts are real.
    sqlx::query(r#"UPDATE "PlayoutItem" SET "Finish"=?,"OutPoint"='00:01:00' WHERE "PlayoutId"=1"#)
        .bind(super::super::super::encode_sql_datetime(
            Utc::now() + chrono::Duration::seconds(60),
        ))
        .execute(&state.pool)
        .await
        .unwrap();
    let (bytes, mut observation) = capture_wait(&public, &root).await;
    observation["cleanup"] = cleanup_wait(&state).await;
    observation["mode"] = json!(format!("{mode:?}"));
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
    tokio::fs::write(
        root.join("capacity-observation.json"),
        serde_json::to_vec_pretty(&observation).unwrap(),
    )
    .await
    .unwrap();
    println!("capacity handoff observation: {observation}");
    verify_wait(&tools, &root, &bytes, &expected, &observation).await;
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
    let mut streams = Vec::new();
    if bytes.first() == Some(&0x47) && bytes.len().is_multiple_of(188) {
        let report = probe(&tools[2], &["--json"], bytes, 256 * 1024)
            .await
            .unwrap();
        tokio::fs::write(root.join("capacity-tsanalyze.json"), &report)
            .await
            .unwrap();
        streams = stream_inventory(&tools[1], bytes).await;
        tokio::fs::write(
            root.join("capacity-streams.json"),
            serde_json::to_vec_pretty(&streams).unwrap(),
        )
        .await
        .unwrap();
    }

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
    let actual = programme_caption_frame(&tools[0], bytes, &expected[0]).await;
    tokio::fs::write(root.join("capacity-programme.rgb"), &actual)
        .await
        .unwrap();
    compare_burn(&actual, &expected[0], &expected[1], &expected[2]);
    tone_with_limit(&tools[0], bytes, 660, 4 * 1024 * 1024).await;
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
    assert!(streams
        .iter()
        .any(|stream| stream["codec_name"] == "dvb_teletext"));
    let authored = String::from_utf8(decode_text(&tools[0], bytes, "200", "0:s:0").await).unwrap();
    assert!(authored.contains("Routed stored original"));
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
        .block_on(Box::pin(run_capacity_wait(StreamingMode::TransportStream)));
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
        )));
}
