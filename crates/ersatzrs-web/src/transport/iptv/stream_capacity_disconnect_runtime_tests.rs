//! Staged native disconnect proof, before explicit fixture Stop. This child of
//! the existing capacity fixture reuses its source, application and HTTP hosts.

use std::{future::Future, pin::Pin, sync::Arc};

use ersatzrs_core::hls::{HlsAdmissionStatus, HlsSessionHandle};
use tokio::time::Instant;

use super::*;

const OWNED_CLEANUP: Duration = Duration::from_secs(20);
const SHARED_IDLE_CLEANUP: Duration = Duration::from_secs(80);
type HttpBody =
    Pin<Box<dyn futures_util::Stream<Item = reqwest::Result<axum::body::Bytes>> + Send>>;

struct Fixture {
    root: PathBuf,
    state: AppState,
    public: Server,
    internal: Server,
    unused_provider: Server,
    generation: i64,
    calls: Vec<u8>,
}

impl Fixture {
    async fn start(root: PathBuf, mode: StreamingMode) -> Self {
        tokio::fs::create_dir(&root).await.unwrap();
        // A lifecycle-only proof reuses the same managed four-stream source.
        // Unchanged caption/audio/transport oracles are not replayed here.
        let tools = [
            exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG"),
            exact_tool("FFPROBE_PATH"),
        ];
        assert_eq!(ersatzrs_ffmpeg::ffmpeg_path(), tools[0]);
        assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), tools[1]);
        let library = root.join("library");
        local_source_duration(&tools[0], &library, "60").await;
        let unused_provider = Server::start(Router::new()).await;
        let state = application(&root, &unused_provider).await;
        let (item, _) = scan_source(&state, &library, &tools[1]).await;
        software_profile(&state).await;
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
        let generation = install_and_accept(&state, &root).await;
        let internal = Server::start(build_internal_app(state.clone())).await;
        state.internal_http_addr.set(internal.address).unwrap();
        state.next_engine.media.configure(internal.address).unwrap();
        let public = Server::start(build_app(state.clone())).await;
        state.streaming_http_addr.set(public.address).unwrap();
        let calls = tokio::fs::read(root.join("addon-calls")).await.unwrap();
        start_item(&state, item, ChannelSubtitleMode::Any, "nld", mode).await;
        sqlx::query(
            r#"UPDATE "PlayoutItem" SET "Finish"=?,"OutPoint"='00:01:00' WHERE "PlayoutId"=1"#,
        )
        .bind(super::super::super::super::encode_sql_datetime(
            Utc::now() + chrono::Duration::seconds(60),
        ))
        .execute(&state.pool)
        .await
        .unwrap();
        assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
        assert_eq!(
            load_active_transcode_limit(&state.pool)
                .await
                .unwrap()
                .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
            2
        );
        assert!(state.hls_sessions.active_channels().await.is_empty());
        Self {
            root,
            state,
            public,
            internal,
            unused_provider,
            generation,
            calls,
        }
    }

    async fn finish(self, mut observation: Value, result: Result<(), String>) -> Value {
        observation["error"] = result.err().map_or(Value::Null, Value::String);
        observation["before_fixture_stop"] = snapshot(&self.state).await;
        // Failure must not leave an HTTP host or native producer behind. All
        // acceptance observations above precede this explicit fallback cleanup.
        observation["fallback_cleanup"] = cleanup_wait(&self.state).await;
        observation["addon_unchanged"] = json!(
            tokio::fs::read(self.root.join("addon-calls"))
                .await
                .unwrap()
                == self.calls
        );
        observation["generation_unchanged"] = json!(
            self.state
                .application
                .accepted_teletext_generation(1)
                .await
                .unwrap()
                .unwrap()
                .id
                == self.generation
        );
        self.public.finish().await;
        self.internal.finish().await;
        self.unused_provider.finish().await;
        self.state.pool.close().await;
        tokio::fs::write(
            self.root.join("disconnect-observation.json"),
            serde_json::to_vec_pretty(&observation).unwrap(),
        )
        .await
        .unwrap();
        observation
    }
}

fn occupy_both() -> [ersatzrs_infra::transcode_capacity::ActiveTranscodeReservation; 2] {
    ["disconnect-foreign-first", "disconnect-foreign-second"].map(|number| {
        try_reserve_active_transcode(
            DEFAULT_ACTIVE_TRANSCODE_LIMIT,
            ActiveTranscodeSlot::new_mpeg_ts(number),
        )
        .unwrap()
    })
}

async fn snapshot(state: &AppState) -> Value {
    let capacity = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
    json!({"active":capacity.active,"peak":capacity.peak,
        "channels":state.hls_sessions.active_channels().await,
        "playlist_worker":state.hls_sessions.try_get("1").await.is_some(),
        "teletext_work":state.application.operations().entity_locker().active_teletext_work()})
}

async fn open(client: &reqwest::Client, public: &Server) -> Result<HttpBody, String> {
    let response = tokio::time::timeout(
        Duration::from_secs(25),
        client.get(public.url("/iptv/channel/1.ts")).send(),
    )
    .await
    .map_err(|_| "TS response headers timed out".to_owned())?
    .map_err(|error| error.to_string())?;
    if response.status() != reqwest::StatusCode::OK {
        return Err(format!("TS response status {}", response.status()));
    }
    Ok(Box::pin(response.bytes_stream()))
}

async fn append_chunk(body: &mut HttpBody, bytes: &mut Vec<u8>) -> Result<(), String> {
    let chunk = body
        .next()
        .await
        .ok_or("TS response ended before disconnect stage")?
        .map_err(|error| error.to_string())?;
    if bytes.len() + chunk.len() > CAPTURE_LIMIT {
        return Err("disconnect capture exceeded the existing bounded limit".into());
    }
    bytes.extend_from_slice(&chunk);
    Ok(())
}

async fn drain_until<T>(
    body: &mut HttpBody,
    bytes: &mut Vec<u8>,
    stage: impl Future<Output = Result<T, String>>,
) -> Result<T, String> {
    tokio::pin!(stage);
    loop {
        tokio::select! {
            biased;
            result = &mut stage => return result,
            chunk = append_chunk(body, bytes) => chunk?,
        }
    }
}

async fn repeated_slates(body: &mut HttpBody, bytes: &mut Vec<u8>) -> Result<(), String> {
    // Each existing finite slate is four seconds. More than 8.5 CBR seconds
    // necessarily crosses two complete rounds without inspecting private argv.
    let minimum = usize::try_from(muxrate()).unwrap() * 17 / 16;
    tokio::time::timeout(Duration::from_secs(25), async {
        while bytes.len() < minimum {
            append_chunk(body, bytes).await?;
        }
        Ok(())
    })
    .await
    .map_err(|_| "repeated waiting output timed out".to_owned())?
}

async fn slate_directories() -> BTreeSet<PathBuf> {
    let root = std::env::temp_dir().join("ersatzrs-mpegts-capacity-slate/channel-1");
    let Ok(mut entries) = tokio::fs::read_dir(root).await else {
        return BTreeSet::new();
    };
    let mut paths = BTreeSet::new();
    while let Ok(Some(entry)) = entries.next_entry().await {
        if entry.file_type().await.is_ok_and(|kind| kind.is_dir()) {
            paths.insert(entry.path());
        }
    }
    paths
}

async fn occupied_case(root: PathBuf, mode: StreamingMode) -> Value {
    let fixture = Fixture::start(root, mode).await;
    let occupied = occupy_both();
    let before = slate_directories().await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut bytes = Vec::new();
    let mut observation =
        json!({"stage":"occupied_disconnect","mode":format!("{mode:?}"),"request_count":1});
    let result = async {
        let mut body = open(&client, &fixture.public).await?;
        repeated_slates(&mut body, &mut bytes).await?;
        let owned: Vec<_> = slate_directories()
            .await
            .difference(&before)
            .cloned()
            .collect();
        observation["before_body_drop"] = snapshot(&fixture.state).await;
        observation["waiting_bytes"] = json!(bytes.len());
        observation["new_slate_directories"] = json!(owned.len());
        drop(body);
        if owned.is_empty() {
            return Err("no owned native slate artifact was observed".into());
        }
        let started = Instant::now();
        tokio::time::timeout(OWNED_CLEANUP, async {
            loop {
                if fixture
                    .state
                    .hls_sessions
                    .active_channels()
                    .await
                    .is_empty()
                    && owned.iter().all(|path| !path.exists())
                    && fixture
                        .state
                        .application
                        .operations()
                        .entity_locker()
                        .active_teletext_work()
                        == 0
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| {
            "occupied response did not clean up before foreign lease release".to_owned()
        })?;
        observation["pre_release_cleanup_ms"] = json!(started.elapsed().as_millis());
        observation["before_foreign_release"] = snapshot(&fixture.state).await;
        let capacity = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
        if capacity.active != 2 || capacity.peak != 2 {
            return Err("disconnect changed a foreign lease or exceeded default capacity".into());
        }
        if fixture.state.hls_sessions.try_get("1").await.is_some() {
            return Err("waiting opened an HLS source worker without capacity".into());
        }
        Ok(())
    }
    .await;
    drop(client);
    // These foreign owners remain alive through the preceding acceptance wait.
    drop(occupied);
    tokio::fs::write(fixture.root.join("waiting-prefix.ts"), &bytes)
        .await
        .unwrap();
    fixture.finish(observation, result).await
}

async fn same_worker(state: &AppState, worker: &Arc<dyn HlsSessionHandle>) -> bool {
    state
        .hls_sessions
        .try_get("1")
        .await
        .is_some_and(|current| Arc::ptr_eq(&current, worker))
}

async fn first_worker(state: &AppState) -> Result<Arc<dyn HlsSessionHandle>, String> {
    tokio::time::timeout(OWNED_CLEANUP, async {
        loop {
            if let Some(worker) = state.hls_sessions.try_get("1").await {
                return worker;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .map_err(|_| "no actual Hybrid producer worker appeared after release".into())
}

async fn actual_warmup(
    state: &AppState,
    worker: &Arc<dyn HlsSessionHandle>,
) -> Result<Value, String> {
    let directory = crate::transport::iptv::hls::hls_working_dir("1");
    let path = directory.join("live000000.ts");
    let minimum = crate::transport::iptv::hls::load_initial_segment_count(state).await;
    tokio::time::timeout(OWNED_CLEANUP, async {
        loop {
            if !same_worker(state, worker).await { return Err("Hybrid worker disappeared or changed during startup".into()); }
            let readiness = state.hls_sessions.initial_segments_ready(worker).await;
            let bytes = tokio::fs::metadata(&path).await.map_or(0, |metadata| metadata.len());
            let completed_segments = tokio::fs::read_to_string(directory.join("live.m3u8"))
                .await
                .map_or(0, |playlist| playlist.lines().filter(|line| line.starts_with("#EXTINF:")).count());
            let active = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active;
            if readiness == Some(false)
                && completed_segments < minimum && worker.admission_status() == HlsAdmissionStatus::Transcoding && bytes >= 188 && active == 2 {
                return Ok(json!({"readiness":format!("{:?}",worker.startup_readiness()),"cached_initial_segments_ready":false,
                    "first_native_segment_bytes":bytes,"completed_segments":completed_segments,
                    "required_segments":minimum,"active":active,"same_worker":true}));
            }
            if readiness == Some(true) { return Err("native warmup completed before the existing observation points exposed it".into()); }
            tokio::task::yield_now().await;
        }
    }).await.map_err(|_| "actual incomplete Hybrid warmup was not observable".to_owned())?
}

async fn wrapper_released(
    state: &AppState,
    worker: &Arc<dyn HlsSessionHandle>,
    artifacts: &[PathBuf],
) -> Result<Value, String> {
    let started = Instant::now();
    tokio::time::timeout(OWNED_CLEANUP, async {
        loop {
            if !same_worker(state, worker).await { return Err("viewer cancellation removed or replaced the shared HLS worker".into()); }
            let capacity = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
            if worker.admission_status() == HlsAdmissionStatus::Transcoding && capacity.active == 1
                && artifacts.iter().all(|path| !path.exists()) {
                return Ok(json!({"elapsed_ms":started.elapsed().as_millis(),"active":1,"peak":capacity.peak,
                    "same_worker":true,"producer_admission":"Transcoding","owned_artifacts_removed":true}));
            }
            if capacity.active == 0 { return Err("actual producer lease disappeared with the wrapper".into()); }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }).await.map_err(|_| "wrapper lease did not drain separately within twenty seconds".to_owned())?
}

async fn get_text(client: &reqwest::Client, url: String) -> Result<String, String> {
    tokio::time::timeout(OWNED_CLEANUP, async {
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_success() {
            return Err(format!(
                "ordinary HLS consumer status {}",
                response.status()
            ));
        }
        response.text().await.map_err(|error| error.to_string())
    })
    .await
    .map_err(|_| "ordinary HLS consumer request timed out".to_owned())?
}

async fn hls_segment(
    client: &reqwest::Client,
    public: &Server,
    previous: Option<&str>,
) -> Result<Value, String> {
    tokio::time::timeout(OWNED_CLEANUP, async {
        loop {
            let playlist = get_text(client, public.url("/iptv/session/1/hls.m3u8")).await?;
            let uri = playlist
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .rfind(|uri| previous.is_none_or(|previous| *uri > previous));
            if let Some(uri) = uri {
                let url = if uri.starts_with("http://") {
                    uri.to_owned()
                } else {
                    public.url(&format!("/iptv/session/1/{uri}"))
                };
                let response = client
                    .get(url)
                    .send()
                    .await
                    .map_err(|error| error.to_string())?;
                if !response.status().is_success() {
                    return Err(format!("ordinary HLS segment status {}", response.status()));
                }
                let bytes = response.bytes().await.map_err(|error| error.to_string())?;
                if bytes.is_empty()
                    || !bytes.len().is_multiple_of(188)
                    || bytes.chunks_exact(188).any(|packet| packet[0] != 0x47)
                {
                    return Err(
                        "ordinary HLS consumer did not receive complete native TS packets".into(),
                    );
                }
                return Ok(json!({"uri":uri,"bytes":bytes.len(),"complete_ts_packets":true}));
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    })
    .await
    .map_err(|_| "ordinary HLS consumer did not receive a distinct segment".to_owned())?
}

// Stop touching the healthy worker. Observe the real default60s idle
// lifetime, allowing20s teardown grace; no Stop is sent before this returns.
async fn idle_cleanup(state: &AppState) -> Result<Value, String> {
    let idle = Instant::now();
    tokio::time::timeout(SHARED_IDLE_CLEANUP, async {
        loop {
            if state.hls_sessions.active_channels().await.is_empty()
                && !crate::transport::iptv::hls::hls_working_dir("1").exists()
                && active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active == 0
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
    .map_err(|_| {
        "shared HLS session did not perform default-idle cleanup before fixture Stop".to_owned()
    })?;
    Ok(json!({"elapsed_ms":idle.elapsed().as_millis(),"after_cleanup":snapshot(state).await}))
}

async fn shared_case(root: PathBuf) -> Value {
    let fixture = Fixture::start(root, StreamingMode::TransportStreamHybrid).await;
    let occupied = occupy_both();
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut bytes = Vec::new();
    let mut programme = Vec::new();
    let mut observation = json!({"stage":"hybrid_warmup_and_established_disconnect","default_startup_idle_seconds":60,
        "warmup_request_count":1,"established_independent_viewer_request_count":0});
    let mut occupied = Some(occupied);
    let before = slate_directories().await;
    let result = async {
        let mut body = open(&client, &fixture.public).await?;
        repeated_slates(&mut body, &mut bytes).await?;
        let owned: Vec<_> = slate_directories()
            .await
            .difference(&before)
            .cloned()
            .collect();
        observation["warmup_slate_directories"] = json!(owned.len());
        if owned.is_empty() {
            return Err("no warmup slate artifact was observed".into());
        }
        observation["before_foreign_release"] = snapshot(&fixture.state).await;
        drop(occupied.take());
        let worker = drain_until(&mut body, &mut bytes, first_worker(&fixture.state)).await?;
        // The ordinary HLS consumer waits for the same startup reserve. It is
        // an independent client, not a replacement TS connection.
        let master = get_text(
            &client,
            fixture.public.url("/iptv/channel/1.m3u8?mode=segmenter"),
        );
        let cancel_warmup = async {
            observation["warmup"] = drain_until(
                &mut body,
                &mut bytes,
                actual_warmup(&fixture.state, &worker),
            )
            .await?;
            drop(body);
            observation["warmup_wrapper_released"] =
                wrapper_released(&fixture.state, &worker, &owned).await?;
            Ok::<(), String>(())
        };
        let (cancelled, master) = tokio::join!(cancel_warmup, master);
        cancelled?;
        let master = master?;
        observation["ordinary_master_bytes"] = json!(master.len());
        if !same_worker(&fixture.state, &worker).await {
            return Err("HLS consumer restarted instead of retaining the warmup worker".into());
        }
        let first = hls_segment(&client, &fixture.public, None).await?;
        observation["hls_after_warmup_disconnect"] = first.clone();
        observation["same_worker_after_warmup"] = json!(same_worker(&fixture.state, &worker).await);
        let mut second_body = open(&client, &fixture.public).await?;
        observation["established_independent_viewer_request_count"] = json!(1);
        tokio::time::timeout(OWNED_CLEANUP, async {
            while programme.len() < 1024 * 1024 {
                append_chunk(&mut second_body, &mut programme).await?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| "established wrapper did not deliver programme bytes".to_owned())??;
        let capacity = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
        observation["before_established_body_drop"] = snapshot(&fixture.state).await;
        if capacity.active != 2 || capacity.peak != 2 {
            return Err(
                "established shared wrapper did not own exactly its producer/wrapper pair".into(),
            );
        }
        drop(second_body);
        observation["established_wrapper_released"] =
            wrapper_released(&fixture.state, &worker, &[]).await?;
        observation["hls_after_established_disconnect"] =
            hls_segment(&client, &fixture.public, first["uri"].as_str()).await?;
        if !same_worker(&fixture.state, &worker).await {
            return Err(
                "established wrapper disconnect restarted the ordinary HLS consumer".into(),
            );
        }
        observation["same_worker_after_established"] = json!(true);
        observation["default_idle_cleanup"] = idle_cleanup(&fixture.state).await?;
        Ok(())
    }
    .await;
    drop(occupied);
    drop(client);
    tokio::fs::write(fixture.root.join("warmup-wait.ts"), &bytes)
        .await
        .unwrap();
    tokio::fs::write(fixture.root.join("established-programme.ts"), &programme)
        .await
        .unwrap();
    fixture.finish(observation, result).await
}

#[expect(
    clippy::disallowed_methods,
    reason = "Requires an explicit fresh isolated directory for retained native disconnect evidence"
)]
async fn proof_root() -> PathBuf {
    let _ = tracing_subscriber::fmt()
        .with_env_filter("ersatzrs_web=debug,ersatzrs_infra=debug")
        .with_test_writer()
        .try_init();
    isolated_bundled_wrapper().await;
    let root = PathBuf::from(
        std::env::var("ERSATZRS_TELETEXT_DISCONNECT_PROOF_DIR")
            .expect("runner must supply a fresh disconnect proof directory"),
    );
    tokio::fs::create_dir(&root)
        .await
        .expect("disconnect proof root must not already exist");
    root
}

fn verify(observation: &Value) {
    assert!(
        observation["error"].is_null(),
        "native disconnect stage failed: {observation}"
    );
    assert_eq!(observation["addon_unchanged"], true);
    assert_eq!(observation["generation_unchanged"], true);
    assert_eq!(observation["fallback_cleanup"]["complete"], true);
    assert_eq!(observation["fallback_cleanup"]["peak"], 2);
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi/libx264, ffprobe, loopback and fresh app-data/proof roots; real default2 occupied Legacy and Hybrid disconnects"]
fn native_occupied_capacity_disconnect_preserves_foreign_leases() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let root = proof_root().await;
            let mut observations = Vec::new();
            for (case, mode) in [
                ("legacy", StreamingMode::TransportStream),
                ("hybrid", StreamingMode::TransportStreamHybrid),
            ] {
                observations.push(occupied_case(root.join(case), mode).await);
            }
            for observation in &observations {
                verify(observation);
            }
        });
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi/libx264, ffprobe, loopback and fresh app-data/proof roots; actual Hybrid warmup, shared ordinary HLS and default60s idle cleanup"]
fn native_hybrid_disconnect_preserves_shared_worker_and_default_idle_cleanup() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let root = proof_root().await;
            let observation = shared_case(root.join("hybrid-shared")).await;
            verify(&observation);
        });
}

// This proves native response registration and actual lease cleanup. The
// private UUID fence remains covered by the existing routed legacy admission
// fixture; ordinary responses deliberately do not expose that token.
async fn admitted_legacy_case(root: PathBuf) -> Value {
    let fixture = Fixture::start(root, StreamingMode::TransportStream).await;
    let client = reqwest::Client::builder().no_proxy().build().unwrap();
    let mut bytes = Vec::new();
    let mut observation = json!({
        "stage":"admitted_legacy_disconnect",
        "request_count":1,
        "new_evidence":"actual native response registration and pair leases drain before fixture Stop",
        "uuid_observed":false,
        "reused_token_fence_test":"real_routed_legacy_wrapper_transfers_first_encoder_and_fences_private_requests"
    });
    let result = async {
        let mut body = open(&client, &fixture.public).await?;
        tokio::time::timeout(OWNED_CLEANUP, async {
            while bytes.len() < 1024 * 1024 {
                append_chunk(&mut body, &mut bytes).await?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| "admitted Legacy response did not deliver its bounded prefix".to_owned())??;
        observation["before_body_drop"] = snapshot(&fixture.state).await;
        observation["prefix_bytes"] = json!(bytes.len());
        let capacity = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
        if !fixture.state.hls_sessions.is_active("1").await
            || capacity.active != 2
            || capacity.peak != 2
        {
            return Err(
                "admitted Legacy prefix did not retain its native response and default pair".into(),
            );
        }
        drop(body);
        let dropped = Instant::now();
        tokio::time::timeout(OWNED_CLEANUP, async {
            loop {
                if !fixture.state.hls_sessions.is_active("1").await
                    && fixture
                        .state
                        .hls_sessions
                        .active_channels()
                        .await
                        .is_empty()
                    && active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active == 0
                    && fixture
                        .state
                        .application
                        .operations()
                        .entity_locker()
                        .active_teletext_work()
                        == 0
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .map_err(|_| "admitted Legacy response did not drain before fixture Stop".to_owned())?;
        observation["pre_stop_cleanup_ms"] = json!(dropped.elapsed().as_millis());
        observation["after_body_drop_before_stop"] = snapshot(&fixture.state).await;
        Ok(())
    }
    .await;
    drop(client);
    tokio::fs::write(fixture.root.join("admitted-programme-prefix.ts"), &bytes)
        .await
        .unwrap();
    fixture.finish(observation, result).await
}

#[test]
#[ignore = "requires FFmpeg9/libzvbi/libx264, ffprobe, loopback and fresh app-data/proof roots; admitted native Legacy response drop drains actual default2 leases before Stop"]
fn native_admitted_legacy_disconnect_drains_response_and_leases_before_stop() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            let root = proof_root().await;
            let observation = admitted_legacy_case(root.join("legacy-admitted")).await;
            verify(&observation);
        });
}
