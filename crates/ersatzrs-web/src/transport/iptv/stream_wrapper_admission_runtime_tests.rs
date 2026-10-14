//! Real private routes and production legacy wrapper admission over disposable
//! `SQLite` and a synthetic provider. This selected-audio proof complements the
//! separate four-stream preservation fixture; it does not prove source DVB
//! teletext passthrough, capacity slate transitions or native platform parity.

use std::{
    collections::BTreeSet,
    path::Path,
    sync::{
        atomic::{AtomicBool, AtomicUsize},
        Mutex,
    },
};

use axum::{extract::State, middleware::Next, routing::get, Router};
use ersatzrs_core::repositories::FfmpegProfileRepository as _;
use ersatzrs_infra::transcode_capacity::{
    active_transcode_snapshot, active_transcode_test_guard_for_tests,
    reset_active_transcode_state_for_tests, try_reserve_mpeg_ts_wrapper_capacity,
};
use futures_util::StreamExt as _;
use sqlx::Executor as _;
use tower::ServiceExt as _;

use super::super::{
    legacy_wrapper_admission::LegacyWrapperAdmissionError,
    teletext_output::tests::authored_set,
    teletext_retained_output::runtime_tests::{decode_text, exact_tool, probe},
};
use super::*;

const MAX_CAPTURE: usize = 8 * 1024 * 1024;
const VIDEO_KBPS: i32 = 4000;
const AUDIO_KBPS: i32 = 128;

struct Server {
    address: std::net::SocketAddr,
    task: Option<tokio::task::JoinHandle<()>>,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
}

impl Server {
    async fn start(app: Router) -> Self {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let (stop, stopped) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            axum::serve(
                listener,
                app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
            )
            .with_graceful_shutdown(async {
                let _ = stopped.await;
            })
            .await
            .unwrap();
        });
        Self {
            address,
            task: Some(task),
            stop: Some(stop),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    async fn finish(mut self) {
        let _ = self.stop.take().unwrap().send(());
        tokio::time::timeout(Duration::from_secs(5), self.task.as_mut().unwrap())
            .await
            .expect("synthetic HTTP server failed to stop")
            .unwrap();
        self.task = None;
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(task) = &self.task {
            task.abort();
        }
    }
}

#[derive(Default)]
struct Observation {
    tokens: Mutex<Vec<uuid::Uuid>>,
    pause_concat: AtomicBool,
    concat_arrived: Notify,
    release_concat: Notify,
    items: AtomicUsize,
}

async fn observe(
    State(observation): State<Arc<Observation>>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let uri = request.uri();
    if uri.path() == "/ffmpeg/concat/1" {
        let url = reqwest::Url::parse(&format!("http://localhost{uri}")).unwrap();
        if let Some(token) = url
            .query_pairs()
            .find(|(name, _)| name == "wrapper_id")
            .and_then(|(_, value)| uuid::Uuid::parse_str(&value).ok())
        {
            observation.tokens.lock().unwrap().push(token);
            if observation.pause_concat.swap(false, Ordering::AcqRel) {
                observation.concat_arrived.notify_one();
                observation.release_concat.notified().await;
            }
        }
    } else if uri.path() == "/ffmpeg/stream/1" {
        observation.items.fetch_add(1, Ordering::AcqRel);
    }
    next.run(request).await
}

async fn synthetic_source(ffmpeg: &str, root: &Path) -> std::path::PathBuf {
    let muxrate = MpegTsMuxingOptions::from_bitrates(
        Some(u32::try_from(VIDEO_KBPS).unwrap()),
        Some(u32::try_from(AUDIO_KBPS).unwrap()),
    )
    .muxrate_bps
    .to_string();
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
            "color=c=red:size=160x90:rate=25",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000",
            "-t",
            "12",
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "mpeg2video",
            "-b:v",
            "200k",
            "-c:a",
            "aac",
            "-b:a",
            "64k",
            "-metadata:s:a:0",
            "language=eng",
            "-muxrate",
            &muxrate,
            "-pcr_period",
            "20",
            "-f",
            "mpegts",
            "pipe:1",
        ],
        &[],
        32 * 1024 * 1024,
    )
    .await
    .unwrap();
    let path = root.join("synthetic-programme.ts");
    tokio::fs::write(&path, bytes).await.unwrap();
    path
}

#[derive(Default)]
struct SourceReads {
    metadata: AtomicUsize,
    first: AtomicUsize,
    second: AtomicUsize,
}

impl SourceReads {
    fn total(&self) -> usize {
        self.metadata.load(Ordering::Acquire)
            + self.first.load(Ordering::Acquire)
            + self.second.load(Ordering::Acquire)
    }
}

async fn provider(path: &Path, reads: Arc<SourceReads>) -> Server {
    let file = tower_http::services::ServeFile::new(path);
    let metadata_reads = Arc::clone(&reads);
    let first_reads = Arc::clone(&reads);
    let second_reads = Arc::clone(&reads);
    let first_file = file.clone();
    let app = Router::new()
        .route("/library/metadata/{id}", get(move |axum::extract::Path(id): axum::extract::Path<u32>| {
            let reads = Arc::clone(&metadata_reads);
            async move {
                reads.metadata.fetch_add(1, Ordering::AcqRel);
                axum::Json(serde_json::json!({"MediaContainer":{"Metadata":[{
                    "ratingKey":id.to_string(), "title":"Synthetic programme", "Media":[{
                        "id":id, "duration":12000, "width":160, "height":90,
                        "Part":[{"id":id, "file":"missing-synthetic.ts",
                            "key":format!("/library/parts/{id}/file.ts"), "Stream":[
                                {"index":0,"streamType":1,"codec":"mpeg2video","frameRate":"25","width":160,"height":90},
                                {"index":1,"streamType":2,"codec":"aac","channels":1,"languageCode":"eng","default":true}
                            ]}]
                    }]
                }]}}))
            }
        }))
        .route("/library/parts/1/file.ts", get(move |request: axum::extract::Request| {
            let file = first_file.clone();
            let reads = Arc::clone(&first_reads);
            async move {
                reads.first.fetch_add(1, Ordering::AcqRel);
                file.oneshot(request).await.unwrap().into_response()
            }
        }))
        .route("/library/parts/2/file.ts", get(move |request: axum::extract::Request| {
            let file = file.clone();
            let reads = Arc::clone(&second_reads);
            async move {
                reads.second.fetch_add(1, Ordering::AcqRel);
                file.oneshot(request).await.unwrap().into_response()
            }
        }));
    Server::start(app).await
}

async fn application(root: &Path, provider: &Server) -> AppState {
    use crate::host::state::{IptvAccessTokenVerifier, SystemStartup};
    use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};

    let pool = ersatzrs_infra::db::init(&format!("sqlite:{}", root.join("admission.db").display()))
        .await
        .unwrap();
    pool.execute(r#"INSERT OR IGNORE INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Synthetic',160,90);
        INSERT INTO "MediaSource" ("Id") VALUES (10);
        INSERT INTO "PlexMediaSource" ("Id","ServerName","ClientIdentifier") VALUES (10,'Synthetic','synthetic');
        INSERT INTO "Library" ("Id","MediaSourceId","MediaKind") VALUES (10,10,1);
        INSERT INTO "PlexLibrary" ("Id","ShouldSyncItems") VALUES (10,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (10,10);
        INSERT INTO "ConfigElement" ("Key","Value") VALUES ('plex.server_auth_token.synthetic','synthetic-fixture');"#).await.unwrap();
    let repos = ersatzrs_infra::repositories::make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&ersatzrs_domain::FFmpegProfile {
            id: 1,
            name: "Synthetic".into(),
            resolution_id: 1,
            transcode_video: false,
            transcode_audio: false,
            video_format: FFmpegProfileVideoFormat::Copy,
            audio_format: FFmpegProfileAudioFormat::Copy,
            video_bitrate: VIDEO_KBPS,
            video_buffer_size: 2000,
            audio_bitrate: AUDIO_KBPS,
            audio_buffer_size: 256,
            audio_channels: 1,
            audio_sample_rate: 48,
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .channel
        .upsert(&Channel {
            id: 1,
            number: "1".into(),
            name: "Synthetic".into(),
            ffmpeg_profile_id: 1,
            is_enabled: true,
            streaming_mode: StreamingMode::TransportStream,
            preferred_audio_language_code: Some("eng".into()),
            subtitle_mode: ChannelSubtitleMode::None,
            ..Default::default()
        })
        .await
        .unwrap();
    repos
        .channel
        .upsert(&Channel {
            id: 2,
            number: "2".into(),
            name: "Synthetic other".into(),
            ffmpeg_profile_id: 1,
            is_enabled: true,
            streaming_mode: StreamingMode::TransportStream,
            preferred_audio_language_code: Some("eng".into()),
            subtitle_mode: ChannelSubtitleMode::None,
            ..Default::default()
        })
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","ScheduleFile","Seed") VALUES (1,1,20,'',0),(2,2,20,'',0);"#).await.unwrap();
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id","PlexMediaSourceId","IsActive","Uri") VALUES (1,10,1,?)"#)
        .bind(provider.url(""))
        .execute(&pool).await.unwrap();
    let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
    AppState::new(
        pool,
        DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        1,
        IptvAccessTokenVerifier::disabled(),
        tx,
        ersatzrs_core::locking::EntityLocker::default(),
    )
}

async fn schedule(state: &AppState, root: &Path) {
    let path = root.join("synthetic-schedule.json");
    let bytes = serde_json::to_vec(&serde_json::json!({
        "startTime":Utc::now().to_rfc3339(), "programs":[
            {"title":"Synthetic first", "serverKey":"Synthetic", "duration":60000,
             "plexFile":"/library/parts/1/file.ts", "ratingKey":"1"},
            {"title":"Synthetic second", "serverKey":"Synthetic", "duration":60000,
             "plexFile":"/library/parts/2/file.ts", "ratingKey":"2"}
        ]
    }))
    .unwrap();
    tokio::fs::write(&path, bytes).await.unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleFile"=? WHERE "Id" IN (1,2)"#)
        .bind(path.to_str().unwrap())
        .execute(&state.pool)
        .await
        .unwrap();
}

async fn set_limit(state: &AppState, limit: i32) {
    sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)"#)
        .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_ACTIVE_TRANSCODE_LIMIT)
        .bind(limit.to_string())
        .execute(&state.pool)
        .await
        .unwrap();
}

fn binding() -> Arc<TeletextOutputBinding> {
    let muxrate = u64::try_from(
        MpegTsMuxingOptions::from_bitrates(
            Some(u32::try_from(VIDEO_KBPS).unwrap()),
            Some(u32::try_from(AUDIO_KBPS).unwrap()),
        )
        .muxrate_bps,
    )
    .unwrap();
    Arc::new(
        TeletextOutputBinding::new(
            ersatzrs_ffmpeg::teletext::TeletextOutputConfig {
                program: 1,
                elementary_pid: 512,
                clock: ersatzrs_ffmpeg::teletext::TeletextClockConfig {
                    pcr_pid: 256,
                    muxrate_bps: muxrate,
                    rate_tolerance_ppm: 1000,
                    pcr_jitter_ticks: 14,
                    max_pcr_gap_packets: 300,
                    max_pcr_gap_ticks: 1_080_000,
                    min_pcr_intervals: 2,
                    max_window_packets: 1024,
                },
                max_buffer_packets: 8192,
                max_hold: Duration::from_secs(2),
                playout_delay: Duration::from_millis(100),
                max_lateness: Duration::from_millis(100),
            },
            authored_set(),
            Duration::from_secs(5),
        )
        .unwrap(),
    )
}

async fn open(state: &AppState) -> Response {
    Box::pin(serve_transport_stream_with_teletext(
        state,
        "1",
        &HeaderMap::new(),
        &hls::HlsQuery {
            subtitles: None,
            mode: Some("ts-legacy".into()),
            ..Default::default()
        },
        binding(),
    ))
    .await
}

async fn wait_for_no_capacity() {
    tokio::time::timeout(Duration::from_secs(10), async {
        while active_transcode_snapshot(2).active != 0 {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("wrapper and encoder cleanup must return both slots");
}

async fn reject_private_tokens(
    client: &reqwest::Client,
    server: &Server,
    token: uuid::Uuid,
    reads: &SourceReads,
) {
    let before = reads.total();
    for route in ["concat", "stream"] {
        for (channel, id, expected) in [
            ("1", "malformed".to_owned(), StatusCode::BAD_REQUEST),
            ("1", uuid::Uuid::new_v4().to_string(), StatusCode::NOT_FOUND),
            ("2", token.to_string(), StatusCode::NOT_FOUND),
        ] {
            let response = client
                .get(server.url(&format!(
                    "/ffmpeg/{route}/{channel}?mode=ts-legacy&wrapper_id={id}"
                )))
                .send()
                .await
                .unwrap();
            assert_eq!(response.status(), expected);
        }
    }
    assert_eq!(
        reads.total(),
        before,
        "rejected tokens discovered a provider source"
    );
}

async fn verify_transport(tsanalyze: &str, bytes: &[u8]) {
    assert_eq!(bytes.len() % 188, 0);
    let analysis = probe(tsanalyze, &["--json"], bytes, 256 * 1024)
        .await
        .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&analysis).unwrap();
    let muxrate = u64::try_from(
        MpegTsMuxingOptions::from_bitrates(
            Some(u32::try_from(VIDEO_KBPS).unwrap()),
            Some(u32::try_from(AUDIO_KBPS).unwrap()),
        )
        .muxrate_bps,
    )
    .unwrap();
    assert!(
        report["ts"]["pcr-bitrate"]
            .as_u64()
            .unwrap()
            .abs_diff(muxrate)
            < 2000
    );
    for field in ["invalid-syncs", "transport-errors"] {
        assert_eq!(report["ts"]["packets"][field], 0);
    }
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
    let pids = report["pids"].as_array().unwrap();
    for expected in [256, 257, 4096, 512] {
        assert!(pids.iter().any(|pid| pid["id"] == expected));
    }
    let selected_audio = pids.iter().find(|pid| pid["id"] == 257).unwrap();
    assert_eq!(selected_audio["language"], "eng");
    let authored_pid = pids.iter().find(|pid| pid["id"] == 512).unwrap();
    assert_eq!(authored_pid["language"], "nld");
    assert_eq!(authored_pid["pes-stream-id"], 0xbd);
    for expected in [256, 257, 512] {
        let pid = pids.iter().find(|pid| pid["id"] == expected).unwrap();
        // TSDuck reports this required field only for elementary PES PIDs.
        assert_eq!(pid["invalid-pes-prefix"], 0);
    }
    for pid in pids {
        for field in ["discontinuities", "pts-leap", "pcr-leap"] {
            assert_eq!(pid["packets"][field], 0);
        }
    }
}

async fn decoded_video_hashes(ffmpeg: &str, bytes: &[u8]) -> Vec<String> {
    let video = probe(
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
            "-an",
            "-f",
            "framemd5",
            "pipe:1",
        ],
        bytes,
        512 * 1024,
    )
    .await
    .unwrap();
    String::from_utf8(video)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.split(',').nth(5).unwrap().trim().to_owned())
        .collect()
}

async fn verify_output(ffmpeg: &str, tsanalyze: &str, source: &Path, bytes: &[u8]) {
    verify_transport(tsanalyze, bytes).await;
    let authored = decode_text(ffmpeg, bytes, "200", "0:s:0").await;
    assert!(String::from_utf8(authored)
        .unwrap()
        .contains("Synthetic television guide"));
    let reference = tokio::fs::read(source).await.unwrap();
    let source_hashes: BTreeSet<_> = decoded_video_hashes(ffmpeg, &reference)
        .await
        .into_iter()
        .collect();
    let output_hashes = decoded_video_hashes(ffmpeg, bytes).await;
    assert!(output_hashes.len() >= 20);
    assert!(
        output_hashes
            .iter()
            .all(|hash| source_hashes.contains(hash)),
        "copied output must decode the synthetic source video, never a substitute slate"
    );
    let audio = probe(
        ffmpeg,
        &[
            "-hide_banner",
            "-loglevel",
            "error",
            "-nostdin",
            "-probesize",
            "32768",
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
        1024 * 1024,
    )
    .await
    .unwrap();
    let samples: Vec<_> = audio
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()))
        .collect();
    let tones = samples
        .chunks_exact(4800)
        .filter(|chunk| {
            let crossings = chunk
                .windows(2)
                .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
                .count();
            let frequency = crossings * 48_000 / (2 * chunk.len());
            frequency.abs_diff(440) <= 20 && chunk.iter().any(|sample| sample.unsigned_abs() > 100)
        })
        .count();
    assert!(
        tones >= 10,
        "selected audio must decode at least one second of the synthetic tone"
    );
}

#[test]
#[ignore = "requires exact configured FFmpeg/ffprobe 9, TSDuck and loopback HTTP"]
fn real_routed_legacy_wrapper_transfers_first_encoder_and_fences_private_requests() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(run());
    assert_eq!(active_transcode_snapshot(2).active, 0);
}

async fn check_insufficient_capacity(
    state: &AppState,
    root: &Path,
    observation: &Observation,
    reads: &SourceReads,
) {
    // Capacity one cannot admit half a wrapper. It must not discover a source.
    set_limit(state, 1).await;
    schedule(state, root).await;
    let response = open(state).await;
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    drop(response);
    assert_eq!(active_transcode_snapshot(1).active, 0);
    assert_eq!(reads.total(), 0);
    assert!(observation.tokens.lock().unwrap().is_empty());
}

async fn check_unused_encoder_cancellation(
    state: &AppState,
    observation: &Observation,
    client: &reqwest::Client,
    internal: &Server,
    reads: &SourceReads,
) -> uuid::Uuid {
    // Stop the real callback before first-item lookup: body drop must fence
    // the token and return the unused encoder without starting any source.
    set_limit(state, 2).await;
    observation.pause_concat.store(true, Ordering::Release);
    let response = open(state).await;
    assert_eq!(response.status(), StatusCode::OK);
    tokio::time::timeout(
        Duration::from_secs(5),
        observation.concat_arrived.notified(),
    )
    .await
    .unwrap();
    let token = *observation.tokens.lock().unwrap().last().unwrap();
    assert_eq!(active_transcode_snapshot(2).active, 2);
    assert!(try_reserve_mpeg_ts_wrapper_capacity(2, "competitor").is_none());
    let callbacks_before_competing_open = observation.tokens.lock().unwrap().len();
    let competing = open(state).await;
    assert_eq!(competing.status(), StatusCode::SERVICE_UNAVAILABLE);
    drop(competing);
    assert_eq!(
        observation.tokens.lock().unwrap().len(),
        callbacks_before_competing_open
    );
    assert_eq!(active_transcode_snapshot(2).active, 2);
    reject_private_tokens(client, internal, token, reads).await;
    drop(response);
    assert!(
        state.legacy_wrappers.lookup(token, "1").is_err(),
        "body drop must synchronously fence the token"
    );
    assert!(
        active_transcode_snapshot(2).active <= 1,
        "unused first encoder must already be released"
    );
    observation.release_concat.notify_one();
    wait_for_no_capacity().await;
    assert_eq!(reads.total(), 0);

    token
}

async fn capture_active_item(
    state: &AppState,
    root: &Path,
    observation: &Observation,
    reads: &SourceReads,
    token: uuid::Uuid,
) -> (Vec<u8>, uuid::Uuid) {
    // The first real item can start at limit two only by taking the reserved
    // encoder, rather than attempting an additional capacity admission.
    schedule(state, root).await;
    let response = open(state).await;
    assert_eq!(response.status(), StatusCode::OK);
    let mut body = response.into_body().into_data_stream();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), async {
        while bytes.len() < 4 * 1024 * 1024 || bytes.len() % 188 != 0 {
            let chunk = body
                .next()
                .await
                .expect("wrapper ended before synthetic content")
                .unwrap();
            assert!(bytes.len() + chunk.len() <= MAX_CAPTURE);
            bytes.extend_from_slice(&chunk);
        }
    })
    .await
    .unwrap();
    let active_token = *observation.tokens.lock().unwrap().last().unwrap();
    assert_ne!(active_token, token);
    let request = state.legacy_wrappers.lookup(active_token, "1").unwrap();
    assert_ne!(request.wrapper_slot_id().unwrap(), 0);
    assert!(matches!(
        request.begin_item(),
        Err(LegacyWrapperAdmissionError::ItemInFlight)
    ));
    assert_eq!(active_transcode_snapshot(2).active, 2);
    assert!(active_transcode_snapshot(2).peak <= 2);
    assert!(try_reserve_mpeg_ts_wrapper_capacity(2, "competitor").is_none());
    let callbacks_before_competing_open = observation.tokens.lock().unwrap().len();
    let competing = open(state).await;
    assert_eq!(competing.status(), StatusCode::SERVICE_UNAVAILABLE);
    drop(competing);
    assert_eq!(
        observation.tokens.lock().unwrap().len(),
        callbacks_before_competing_open
    );
    assert_eq!(active_transcode_snapshot(2).active, 2);
    assert!(reads.first.load(Ordering::Acquire) > 0);
    assert_eq!(reads.second.load(Ordering::Acquire), 0);
    drop(body);
    assert!(state.legacy_wrappers.lookup(active_token, "1").is_err());
    assert!(matches!(
        request.begin_item(),
        Err(LegacyWrapperAdmissionError::Closed)
    ));
    wait_for_no_capacity().await;
    assert_eq!(
        reads.second.load(Ordering::Acquire),
        0,
        "cancelled wrapper opened another programme"
    );
    (bytes, active_token)
}

async fn reject_closed_tokens(
    client: &reqwest::Client,
    internal: &Server,
    reads: &SourceReads,
    active_token: uuid::Uuid,
) {
    let before = reads.total();
    for route in ["concat", "stream"] {
        let response = client
            .get(internal.url(&format!("/ffmpeg/{route}/1?wrapper_id={active_token}")))
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
    assert_eq!(reads.total(), before);
}

async fn run() {
    let ffmpeg = exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG");
    let tsanalyze = exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE");
    assert_eq!(
        ersatzrs_ffmpeg::ffmpeg_path(),
        ffmpeg,
        "configure FFMPEG_PATH for the actual host builder"
    );
    let ffprobe = exact_tool("FFPROBE_PATH");
    assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), ffprobe);
    for tool in [&ffmpeg, &ffprobe] {
        let version =
            String::from_utf8(probe(tool, &["-version"], &[], 64 * 1024).await.unwrap()).unwrap();
        assert_eq!(
            ersatzrs_ffmpeg::version::parse_version(version.lines().next().unwrap())
                .unwrap()
                .major,
            9
        );
    }
    let root = tempfile::tempdir().unwrap();
    let source = synthetic_source(&ffmpeg, root.path()).await;
    let reads = Arc::new(SourceReads::default());
    let provider = provider(&source, Arc::clone(&reads)).await;
    let state = application(root.path(), &provider).await;
    let observation = Arc::new(Observation::default());
    let app = crate::host::router::build_internal_app(state.clone()).layer(
        axum::middleware::from_fn_with_state(Arc::clone(&observation), observe),
    );
    let internal = Server::start(app).await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();

    check_insufficient_capacity(&state, root.path(), &observation, &reads).await;
    let previous_token =
        check_unused_encoder_cancellation(&state, &observation, &client, &internal, &reads).await;
    let (bytes, active_token) =
        capture_active_item(&state, root.path(), &observation, &reads, previous_token).await;
    verify_output(&ffmpeg, &tsanalyze, &source, &bytes).await;
    reject_closed_tokens(&client, &internal, &reads, active_token).await;
    internal.finish().await;
    provider.finish().await;
    state.pool.close().await;
}
