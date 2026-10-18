//! Opt-in ordinary routed wrappers with genuine stored/accepted originals.
//!
//! The POSIX addon and exact native tools intentionally require Unix. This
//! selected-English AV fixture does not replace the separate four-stream final
//! wrapper proof, headed-client evidence or native platform acceptance.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
    time::Duration,
};

use axum::{routing::get, Router};
use chrono::Utc;
use ersatzrs_addon_contract::AddonManifest;
use ersatzrs_app::teletext_guide::SuppliedTeletextGuideContext;
use ersatzrs_core::repositories::{
    ChannelRepository as _, ChannelTeletextBindingSave, FfmpegProfileRepository as _,
    TeletextDesignMutation, TeletextGenerationMutation, TeletextProfileMutation,
    TeletextProfileSave, TeletextPublicationMutation,
};
use ersatzrs_domain::{
    Channel, FFmpegProfile, FFmpegProfileAudioFormat, FFmpegProfileVideoFormat, StreamingMode,
};
use ersatzrs_infra::transcode_capacity::{
    active_transcode_snapshot, active_transcode_test_guard_for_tests, load_active_transcode_limit,
    reset_active_transcode_state_for_tests, DEFAULT_ACTIVE_TRANSCODE_LIMIT,
};
use futures_util::StreamExt as _;
use serde_json::{json, Value};
use sqlx::Executor as _;
use tower::ServiceExt as _;

use super::super::teletext_retained_output::runtime_tests::{decode_text, exact_tool, probe};
use crate::host::{
    router::{build_app, build_internal_app},
    state::{AppState, IptvAccessTokenVerifier, SystemStartup},
    teletext_channel_documents::prepare_channel_teletext_document_candidate,
    teletext_designs::{
        request_original_teletext_design_publication, save_original_teletext_design_candidate,
    },
    teletext_generations::{accept_original_teletext_generation, TeletextGenerationTarget},
    teletext_runtime_policy::{
        parse_native_teletext_runtime_policy, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING,
    },
};

const ADDON: &str = "org.ersatzrs.teletext.routed-wrapper-fixture";
const GENERATION: &str = "genuine-routed-wrapper-original";
const POLICY: &str = include_str!("../../../tests/fixtures/teletext-output-policy-v1.json");
const MAX_CAPTURE: usize = 8 * 1024 * 1024;
const VIDEO_KBPS: i32 = 4000;
const AUDIO_KBPS: i32 = 128;

#[path = "stream_source_caption_runtime_tests.rs"]
mod source_caption_runtime_tests;

#[expect(
    clippy::disallowed_methods,
    reason = "Requires an explicit new isolated native-test app-data root, never production configuration"
)]
async fn isolated_bundled_wrapper() {
    let directory = PathBuf::from(
        std::env::var(ersatzrs_infra::file_system_layout::APP_DATA_DIR_ENV_VAR)
            .expect("runner must supply a new proof-owned ERSATZRS_APP_DATA_DIR"),
    );
    assert!(
        directory.is_absolute(),
        "proof-owned app data must be absolute"
    );
    tokio::fs::create_dir(&directory)
        .await
        .expect("proof-owned app data directory must not already exist");
    let bundled = directory.join("scripts/mpegts/default");
    assert_eq!(
        ersatzrs_infra::file_system_layout::default_mpegts_script_folder(),
        bundled,
        "native script lookup must use the explicit proof-owned directory"
    );
    tokio::fs::create_dir_all(&bundled).await.unwrap();
    for (name, contents) in [
        (
            "mpegts.yml",
            include_str!("../../../../../assets/Scripts/MpegTs/mpegts.yml"),
        ),
        (
            "run.sh",
            include_str!("../../../../../assets/Scripts/MpegTs/run.sh"),
        ),
        (
            "run.bat",
            include_str!("../../../../../assets/Scripts/MpegTs/run.bat"),
        ),
    ] {
        let mut file = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(bundled.join(name))
            .await
            .unwrap();
        tokio::io::AsyncWriteExt::write_all(&mut file, contents.as_bytes())
            .await
            .unwrap();
    }
}

async fn assert_wrapper_command_policy(state: &AppState, typed: bool) {
    let channel = state.repositories.channel.get_by_number("1").await.unwrap();
    let (command, artifacts) = super::segmenter_wrapper_command(
        state,
        &channel,
        "http://127.0.0.1:1/iptv/session/1/index.m3u8".to_owned(),
        &super::channel_stream_metadata(&channel),
        false,
        muxrate(),
        tokio::time::Instant::now() + Duration::from_secs(5),
    )
    .await
    .expect("configured wrapper preparation");
    assert_eq!(command.mpegts_output_policy().is_some(), typed);
    assert_eq!(artifacts.is_empty(), typed);
    super::cleanup_stream_artifacts(&artifacts).await;
    assert!(artifacts.iter().all(|path| !path.exists()));
}

struct Server {
    address: std::net::SocketAddr,
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    task: Option<tokio::task::JoinHandle<()>>,
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
            stop: Some(stop),
            task: Some(task),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("http://{}{path}", self.address)
    }

    async fn finish(mut self) {
        let _ = self.stop.take().unwrap().send(());
        tokio::time::timeout(Duration::from_secs(5), self.task.as_mut().unwrap())
            .await
            .expect("owned routed fixture listener did not stop")
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

fn muxrate() -> i64 {
    ersatzrs_ffmpeg::MpegTsMuxingOptions::from_bitrates(
        Some(u32::try_from(VIDEO_KBPS).unwrap()),
        Some(u32::try_from(AUDIO_KBPS).unwrap()),
    )
    .muxrate_bps
}

async fn source(ffmpeg: &str, root: &Path) -> std::path::PathBuf {
    let rate = muxrate().to_string();
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
            &rate,
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
        &[],
        MAX_CAPTURE,
    )
    .await
    .unwrap();
    assert_eq!(bytes.len() % 188, 0);
    let path = root.join("ordinary-wrapper-source.ts");
    tokio::fs::write(&path, bytes).await.unwrap();
    path
}

async fn provider(source: &Path) -> Server {
    let file = tower_http::services::ServeFile::new(source);
    Server::start(Router::new()
        .route("/library/metadata/1",get(||async {
            axum::Json(json!({"MediaContainer":{"Metadata":[{"ratingKey":"1","title":"Synthetic source",
                "Media":[{"id":1,"duration":12000,"width":160,"height":90,"Part":[{"id":1,"file":"synthetic-missing.ts",
                    "key":"/library/parts/1/file.ts","Stream":[{"index":0,"streamType":1,"codec":"mpeg2video","frameRate":"25","width":160,"height":90},
                    {"index":1,"streamType":2,"codec":"aac","channels":1,"languageCode":"eng","default":true}]}]}]}]}}))
        }))
        .route("/library/parts/1/file.ts",get(move |request: axum::extract::Request| {
            let file=file.clone(); async move { file.oneshot(request).await.unwrap() }
        }))).await
}

async fn application(root: &Path, provider: &Server) -> AppState {
    use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
    let pool = ersatzrs_infra::db::init(&format!(
        "sqlite:{}",
        root.join("ordinary-wrapper.db").display()
    ))
    .await
    .unwrap();
    pool.execute(r#"INSERT OR IGNORE INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Synthetic',160,90);
        INSERT INTO "MediaSource" ("Id") VALUES (10);
        INSERT INTO "PlexMediaSource" ("Id","ServerName","ClientIdentifier") VALUES (10,'Synthetic','synthetic');
        INSERT INTO "Library" ("Id","MediaSourceId","MediaKind") VALUES (10,10,1);
        INSERT INTO "PlexLibrary" ("Id","ShouldSyncItems") VALUES (10,1);
        INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (10,10);
        INSERT INTO "ConfigElement" ("Key","Value") VALUES ('plex.server_auth_token.synthetic','synthetic-fixture');"#).await.unwrap();
    let repositories = ersatzrs_infra::repositories::make_repositories(pool.clone());
    repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Synthetic copy".into(),
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
    repositories
        .channel
        .upsert(&Channel {
            id: 1,
            number: "1".into(),
            name: "Synthetic routed".into(),
            ffmpeg_profile_id: 1,
            is_enabled: true,
            streaming_mode: StreamingMode::TransportStream,
            preferred_audio_language_code: Some("eng".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Playout" ("Id","ChannelId","ScheduleKind","ScheduleFile","Seed") VALUES (1,1,20,'',0)"#).await.unwrap();
    sqlx::query(r#"INSERT INTO "PlexConnection" ("Id","PlexMediaSourceId","IsActive","Uri") VALUES (1,10,1,?)"#)
        .bind(provider.url("")).execute(&pool).await.unwrap();
    let (sender, _receiver) = tokio::sync::mpsc::unbounded_channel();
    AppState::new(
        pool,
        DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        uuid::Uuid::nil(),
        2,
        IptvAccessTokenVerifier::disabled(),
        sender,
        ersatzrs_core::locking::EntityLocker::default(),
    )
}

async fn config(state: &AppState, key: &str, value: Option<&str>) {
    match value {
        Some(value) => {
            sqlx::query(r#"INSERT OR REPLACE INTO "ConfigElement" ("Key","Value") VALUES (?,?)"#)
                .bind(key)
                .bind(value)
                .execute(&state.pool)
                .await
                .unwrap();
        }
        None => {
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
                .bind(key)
                .execute(&state.pool)
                .await
                .unwrap();
        }
    }
}

async fn programme_start(state: &AppState, root: &Path, mode: StreamingMode) {
    let mut channel = state.repositories.channel.get_by_number("1").await.unwrap();
    channel.streaming_mode = mode;
    state.repositories.channel.upsert(&channel).await.unwrap();
    let path = root.join("ordinary-wrapper-schedule.json");
    tokio::fs::write(&path,serde_json::to_vec(&json!({"startTime":Utc::now().to_rfc3339(),"programs":[
        {"title":"Synthetic twelve-second source","serverKey":"Synthetic","duration":12000,"plexFile":"/library/parts/1/file.ts","ratingKey":"1"}
    ]})).unwrap()).await.unwrap();
    sqlx::query(r#"UPDATE "Playout" SET "ScheduleFile"=? WHERE "Id"=1"#)
        .bind(path.to_str().unwrap())
        .execute(&state.pool)
        .await
        .unwrap();
}

async fn install_and_accept(state: &AppState, root: &Path) -> i64 {
    let package = root.join("package");
    tokio::fs::create_dir(&package).await.unwrap();
    let response = json!({"schema_version":1,"generation":GENERATION,"language":"nld","initial_page":200,
        "pages":[{"address":{"page":200,"subcode":0},"kind":"full_page","language":"nld","national_option":0,
            "rows":[{"row":1,"runs":[{"column":1,"style":{"foreground":"yellow","background":"black","height":"normal",
                "boxed":false,"flash":false,"conceal":false},"content":{"kind":"text","text":"Routed stored original"}}]}],
            "navigation":[{"role":"index","target":{"page":200,"subcode":null}}]}]});
    tokio::fs::write(
        root.join("addon-response.json"),
        serde_json::to_vec(&response).unwrap(),
    )
    .await
    .unwrap();
    tokio::fs::write(package.join("fixture.sh"),b"case \"$1\" in\ncheck) printf '%s' '{\"status\":\"ready\",\"code\":\"ready\",\"message\":\"ok\"}'; exit 0;;\npages) ;;\n*) exit 9;;\nesac\ntest \"$ERSATZRS_ADDON_CAPABILITY\" = teletext.pages.v1 || exit 10\ncat > ../addon-request.json\nprintf 'pages\\n' >> ../addon-calls\ncat ../addon-response.json\n").await.unwrap();
    let manifest:AddonManifest=serde_json::from_value(json!({"manifest_version":ersatzrs_addon_contract::ADDON_MANIFEST_VERSION,
        "id":ADDON,"version":"1.0.0","name":{"en-US":"Synthetic routed guide"},"summary":{"en-US":"Original route proof"},
        "license":"GPL-3.0-or-later","source_url":"https://example.test/source","host_version":"*",
        "capabilities":[{"id":"addon.check.v1","options":[]},{"id":"teletext.pages.v1","options":[]}],
        "entrypoints":{"any":{"path":"fixture.sh","kind":"posix_shell"}}})).unwrap();
    manifest.validate().unwrap();
    let content = ersatzrs_infra::addons::calculate_addon_content_sha256(&package)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt","StateRevision")
        VALUES (?,'1.0.0',0,?,?,?,?,'now','now','before-enable')"#).bind(ADDON).bind("a".repeat(64)).bind(content)
        .bind(serde_json::to_string(&manifest).unwrap()).bind(package.to_str().unwrap()).execute(&state.pool).await.unwrap();
    state.application.enable_addon(ADDON.into()).await.unwrap();
    let expected_installation = state
        .application
        .teletext_installation_snapshot(ADDON)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    let TeletextProfileMutation::Saved(profile) = state
        .application
        .save_teletext_profile(&TeletextProfileSave {
            addon_id: ADDON.into(),
            name: "Actual selected profile".into(),
            settings: Vec::new(),
            expected_installation,
            existing: None,
        })
        .await
        .unwrap()
    else {
        panic!("synthetic installed profile must save")
    };
    let profile = profile.profile;
    assert!(matches!(
        state
            .application
            .save_channel_teletext_binding(&ChannelTeletextBindingSave {
                channel_id: 1,
                addon_id: ADDON.into(),
                profile_id: profile.id,
                enabled: true,
                expected_installation: profile.validated_installation.clone(),
                expected_profile_revision: profile.revision,
                expected_binding_revision: None,
            })
            .await
            .unwrap(),
        TeletextProfileMutation::Saved(_)
    ));
    publish_original(state).await
}

async fn publish_original(state: &AppState) -> i64 {
    let policy = parse_native_teletext_runtime_policy(POLICY).unwrap();
    let guide = SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl"],
        timezone: "Europe/Amsterdam",
        captured_at: Utc::now(),
        channels: &[],
    };
    let candidate = prepare_channel_teletext_document_candidate(
        &state.application,
        1,
        Some(ADDON),
        &guide,
        policy.encoding.clone(),
    )
    .await
    .unwrap();
    let deadline = || std::time::Instant::now() + Duration::from_secs(30);
    let TeletextDesignMutation::Saved(saved) = save_original_teletext_design_candidate(
        &state.application,
        "Actual routed original".into(),
        None,
        candidate,
        policy.design,
        deadline(),
    )
    .await
    .unwrap() else {
        panic!("native original must persist")
    };
    let head = saved.head();
    let TeletextPublicationMutation::Requested(request) =
        request_original_teletext_design_publication(
            &state.application,
            head.id,
            head.revision,
            saved.digest().into(),
            head.revision,
            None,
            policy.design,
            policy.encoding.clone(),
            deadline(),
        )
        .await
        .unwrap()
    else {
        panic!("native original must create explicit pending publication")
    };
    let request = request.request().clone();
    let TeletextGenerationMutation::Accepted(accepted) = accept_original_teletext_generation(
        &state.application,
        TeletextGenerationTarget {
            expected_design_head: head.revision,
            request,
            expected_generation_id: None,
        },
        policy.design,
        policy.encoding,
        deadline(),
    )
    .await
    .unwrap() else {
        panic!("native pending original must accept")
    };
    accepted.generation().id
}

async fn capacity_zero() {
    tokio::time::timeout(Duration::from_secs(15), async {
        while active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active != 0 {
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    })
    .await
    .expect("caller drop did not release wrapper and programme capacity");
}

async fn capture(public: &Server) -> Vec<u8> {
    capture_recorded(public, None).await
}

async fn capture_recorded(public: &Server, evidence: Option<&Path>) -> Vec<u8> {
    let client = reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        .build()
        .unwrap();
    let response = tokio::time::timeout(
        Duration::from_secs(25),
        client.get(public.url("/iptv/channel/1.ts")).send(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(response.status(), axum::http::StatusCode::OK);
    assert!(response.headers()[axum::http::header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .starts_with("video/mp2t"));
    let mut body = response.bytes_stream();
    let mut bytes = Vec::new();
    tokio::time::timeout(Duration::from_secs(20), async {
        while bytes.len() < 2 * 1024 * 1024 || bytes.len() % 188 != 0 {
            let received = body.next().await;
            if received.as_ref().is_none_or(std::result::Result::is_err) {
                if let Some(path) = evidence {
                    tokio::fs::write(path, &bytes).await.unwrap();
                }
            }
            let chunk = received
                .expect("ordinary wrapper ended before source prefix")
                .unwrap();
            assert!(bytes.len() + chunk.len() <= MAX_CAPTURE);
            bytes.extend_from_slice(&chunk);
        }
    })
    .await
    .expect("ordinary wrapper did not deliver bounded source prefix");
    let active = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
    // ADR 0121 allows the finite encoder to finish/reap before the final
    // paced wrapper drains its bytes. Prove the admitted pair and its bound,
    // rather than requiring an already completed encoder to retain a slot.
    assert_eq!(active.peak, 2, "normal routed output must admit the pair");
    assert!(
        (1..=2).contains(&active.active),
        "the final wrapper must remain active within the default two slots"
    );
    drop(body);
    drop(client);
    capacity_zero().await;
    bytes
}

fn pid(packet: &[u8]) -> u16 {
    (u16::from(packet[1] & 31) << 8) | u16::from(packet[2])
}

pub(super) async fn video_hashes(ffmpeg: &str, bytes: &[u8]) -> Vec<String> {
    let output = probe(
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
    String::from_utf8(output)
        .unwrap()
        .lines()
        .filter(|line| !line.starts_with('#') && !line.is_empty())
        .map(|line| line.split(',').nth(5).unwrap().trim().into())
        .collect()
}

pub(super) async fn verify_av(ffmpeg: &str, source_hashes: &BTreeSet<String>, bytes: &[u8]) {
    verify_selected_av(ffmpeg, source_hashes, bytes, 440).await;
}

async fn verify_selected_av(
    ffmpeg: &str,
    source_hashes: &BTreeSet<String>,
    bytes: &[u8],
    frequency: usize,
) {
    let hashes = video_hashes(ffmpeg, bytes).await;
    assert!(
        hashes.len() >= 20 && hashes.iter().all(|hash| source_hashes.contains(hash)),
        "ordinary selected video must remain actual source, not a slate"
    );
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
        1024 * 1024,
    )
    .await
    .unwrap();
    let samples: Vec<_> = pcm
        .chunks_exact(2)
        .map(|sample| i16::from_le_bytes(sample.try_into().unwrap()))
        .collect();
    let tones = samples
        .chunks_exact(4800)
        .filter(|samples| {
            let crossings = samples
                .windows(2)
                .filter(|pair| (pair[0] < 0) != (pair[1] < 0))
                .count();
            (crossings * 48000 / (2 * samples.len())).abs_diff(frequency) <= 20
                && samples.iter().any(|sample| sample.unsigned_abs() > 100)
        })
        .count();
    assert!(
        tones >= 10,
        "ordinary selected audio must retain the genuine source tone"
    );
}

pub(super) async fn verify_transport(tsanalyze: &str, bytes: &[u8], selected: bool) {
    verify_transport_for_language(tsanalyze, bytes, selected, "eng").await;
}

async fn verify_transport_for_language(
    tsanalyze: &str,
    bytes: &[u8],
    selected: bool,
    language: &str,
) -> Value {
    let report: Value = serde_json::from_slice(
        &probe(tsanalyze, &["--json"], bytes, 256 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(report["ts"]["packets"]["invalid-syncs"], 0);
    assert_eq!(report["ts"]["packets"]["transport-errors"], 0);
    assert_eq!(report["ts"]["pids"]["unreferenced"], 0);
    assert!(
        report["ts"]["pcr-bitrate"]
            .as_u64()
            .unwrap()
            .abs_diff(u64::try_from(muxrate()).unwrap())
            < 2000
    );
    let pids = report["pids"].as_array().unwrap();
    for expected in [256, 257, 4096] {
        assert!(pids.iter().any(|pid| pid["id"] == expected));
    }
    assert_eq!(
        pids.iter().find(|pid| pid["id"] == 257).unwrap()["language"],
        language
    );
    assert_eq!(pids.iter().any(|pid| pid["id"] == 512), selected);
    for pid in pids {
        for field in ["discontinuities", "pts-leap", "pcr-leap"] {
            assert_eq!(pid["packets"][field], 0);
        }
    }
    report
}

async fn trial(
    state: &AppState,
    public: &Server,
    root: &Path,
    tools: &[String; 3],
    scenario: (StreamingMode, Option<&str>, bool),
    source_hashes: &BTreeSet<String>,
) {
    let (mode, policy, selected) = scenario;
    let policy_label = match policy {
        None => "absent",
        Some("{}") => "invalid",
        Some(POLICY) => "v1",
        Some(_) => "v2",
    };
    let configured_script: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"=?"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT)
            .fetch_optional(&state.pool)
            .await
            .unwrap();
    let wrapper_label = if configured_script.as_deref() == Some("") {
        "generated-native"
    } else {
        "bundled-default-opaque"
    };
    println!(
        "ordinary wrapper trial: mode={mode:?}, wrapper={wrapper_label}, policy={policy_label}, selected={selected}"
    );
    reset_active_transcode_state_for_tests();
    programme_start(state, root, mode).await;
    config(state, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING, policy).await;
    let calls = tokio::fs::read(root.join("addon-calls")).await.unwrap();
    let bytes = capture(public).await;
    assert_eq!(
        bytes.chunks_exact(188).any(|packet| pid(packet) == 512),
        selected
    );
    verify_av(&tools[0], source_hashes, &bytes).await;
    verify_transport(&tools[2], &bytes, selected).await;
    if selected {
        let decoded = decode_text(&tools[0], &bytes, "200", "0:s:0").await;
        assert!(String::from_utf8(decoded)
            .unwrap()
            .contains("Routed stored original"));
    }
    assert_eq!(
        tokio::fs::read(root.join("addon-calls")).await.unwrap(),
        calls,
        "ordinary recovery never invokes an addon"
    );
    println!(
        "ordinary wrapper verified: mode={mode:?}, wrapper={wrapper_label}, policy={policy_label}, selected={selected}, packets={}",
        bytes.len() / 188
    );
}

async fn unavailable_profile_trials(
    state: &AppState,
    public: &Server,
    root: &Path,
    tools: &[String; 3],
    live: &str,
    source_hashes: &BTreeSet<String>,
) {
    let profiles = state
        .application
        .list_teletext_profiles(ADDON)
        .await
        .unwrap();
    assert_eq!(profiles.len(), 1);
    let profile = &profiles[0].profile;
    let selected = state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .unwrap();
    let TeletextProfileMutation::Saved(deleted) = state
        .application
        .tombstone_teletext_profile(ADDON, profile.id, profile.revision)
        .await
        .unwrap()
    else {
        panic!("real selected profile must tombstone under its revision fence")
    };
    assert!(deleted.profile.deleted);
    assert_eq!(deleted.dependent_bindings_changed, 1);
    println!(
        "selected profile tombstone verified; checking both wrapper modes and policy versions"
    );
    for mode in [
        StreamingMode::TransportStream,
        StreamingMode::TransportStreamHybrid,
    ] {
        for policy in [POLICY, live] {
            trial(
                state,
                public,
                root,
                tools,
                (mode, Some(policy), false),
                source_hashes,
            )
            .await;
            assert_eq!(
                state
                    .application
                    .accepted_teletext_generation(1)
                    .await
                    .unwrap()
                    .unwrap(),
                selected,
                "unavailable profile preserves historical accepted metadata while declining authored output"
            );
        }
    }
}

async fn wrapper_precondition_trials(
    state: &AppState,
    public: &Server,
    root: &Path,
    tools: &[String; 3],
    live: &str,
    source_hashes: &BTreeSet<String>,
) {
    // The unchanged bundled default still runs as an opaque script. These
    // trials document its unresolved teletext gap while retaining actual AV;
    // they neither accept that limitation nor imply default-wrapper acceptance.
    assert_wrapper_command_policy(state, false).await;
    for policy in [POLICY, live] {
        trial(
            state,
            public,
            root,
            tools,
            (StreamingMode::TransportStreamHybrid, Some(policy), false),
            source_hashes,
        )
        .await;
    }
    // Explicit empty configuration selects the native generated wrapper. The
    // existing selected-generation oracle remains unchanged for this matrix.
    config(
        state,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT,
        Some(""),
    )
    .await;
    let native_setting: String =
        sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key"=?"#)
            .bind(ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT)
            .fetch_one(&state.pool)
            .await
            .unwrap();
    assert_eq!(native_setting, "");
    assert_wrapper_command_policy(state, true).await;
}

async fn run() {
    isolated_bundled_wrapper().await;
    let tools = [
        exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG"),
        exact_tool("FFPROBE_PATH"),
        exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE"),
    ];
    assert_eq!(ersatzrs_ffmpeg::ffmpeg_path(), tools[0]);
    assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), tools[1]);
    for tool in &tools[..2] {
        let version =
            String::from_utf8(probe(tool, &["-version"], &[], 65536).await.unwrap()).unwrap();
        assert_eq!(
            ersatzrs_ffmpeg::version::parse_version(version.lines().next().unwrap())
                .unwrap()
                .major,
            9
        );
    }
    let root = tempfile::tempdir().unwrap();
    let source = source(&tools[0], root.path()).await;
    let source_bytes = tokio::fs::read(&source).await.unwrap();
    let source_hashes = video_hashes(&tools[0], &source_bytes)
        .await
        .into_iter()
        .collect();
    let provider = provider(&source).await;
    let state = application(root.path(), &provider).await;
    assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
    assert_eq!(
        load_active_transcode_limit(&state.pool)
            .await
            .unwrap()
            .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
        2
    );
    // Shorten only this disposable worker's documented idle teardown bound;
    // capacity remains the production default and caller drop initiates stop.
    config(
        &state,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
        Some("5"),
    )
    .await;
    let internal = Server::start(build_internal_app(state.clone())).await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let generation = install_and_accept(&state, root.path()).await;
    let installed: Value = serde_json::from_slice(
        &tokio::fs::read(root.path().join("addon-request.json"))
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(installed["generation"], GENERATION);
    assert_eq!(installed["schema_version"], 1);
    let mut live: Value = serde_json::from_str(POLICY).unwrap();
    live["version"] = json!(2);
    live["live_updates"] = json!({"poll_interval_ms":100,"recovery_timeout_ms":5000});
    let live = serde_json::to_string(&live).unwrap();
    wrapper_precondition_trials(&state, &public, root.path(), &tools, &live, &source_hashes).await;
    for mode in [
        StreamingMode::TransportStream,
        StreamingMode::TransportStreamHybrid,
    ] {
        for (policy, selected) in [
            (None, false),
            (Some("{}"), false),
            (Some(POLICY), true),
            (Some(live.as_str()), true),
        ] {
            trial(
                &state,
                &public,
                root.path(),
                &tools,
                (mode, policy, selected),
                &source_hashes,
            )
            .await;
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
        }
    }
    unavailable_profile_trials(&state, &public, root.path(), &tools, &live, &source_hashes).await;
    public.finish().await;
    internal.finish().await;
    provider.finish().await;
    state.pool.close().await;
}

#[test]
#[ignore = "requires configured FFmpeg/ffprobe9, TSDuck and loopback; real normal legacy/hybrid routes, genuine accepted originals and default capacity2"]
fn normal_routed_wrappers_recover_selected_originals_and_preserve_playback_on_policy_errors() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(run());
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}

async fn multi_track_source(ffmpeg: &str, root: &Path) -> PathBuf {
    let rate = muxrate().to_string();
    // Reuse the independently generated page888 transport from the existing
    // four-stream wrapper fixture; no replacement teletext encoder is added.
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
            "-muxrate",
            &rate,
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
    assert!(!bytes.chunks_exact(188).any(|packet| pid(packet) == 512));
    let path = root.join("ordinary-multi-track-source.ts");
    tokio::fs::write(&path, bytes).await.unwrap();
    path
}

async fn multi_track_provider(source: &Path) -> Server {
    let file = tower_http::services::ServeFile::new(source);
    Server::start(Router::new()
        .route("/library/metadata/1", get(|| async {
            axum::Json(json!({"MediaContainer":{"Metadata":[{"ratingKey":"1","title":"Synthetic source",
                "Media":[{"id":1,"duration":12000,"width":160,"height":90,"Part":[{"id":1,"file":"synthetic-missing.ts",
                    "key":"/library/parts/1/file.ts","Stream":[
                    {"index":0,"streamType":1,"codec":"mpeg2video","frameRate":"25","width":160,"height":90},
                    {"index":1,"streamType":2,"codec":"aac","channels":1,"languageCode":"eng","default":true},
                    {"index":2,"streamType":3,"codec":"dvb_teletext","languageCode":"eng","default":false,"forced":false},
                    {"index":3,"streamType":2,"codec":"aac","channels":1,"languageCode":"nld","default":false}
                    ]}]}]}]}}))
        }))
        .route("/library/parts/1/file.ts", get(move |request: axum::extract::Request| {
            let file=file.clone(); async move { file.oneshot(request).await.unwrap() }
        }))).await
}

async fn stream_inventory(ffprobe: &str, bytes: &[u8]) -> Vec<Value> {
    // Consume the complete bounded pipe: metadata-only probing may close it
    // while the existing fixture helper is still writing source packets.
    let report: Value = serde_json::from_slice(
        &probe(
            ffprobe,
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
            256 * 1024,
        )
        .await
        .unwrap(),
    )
    .unwrap();
    report["streams"].as_array().unwrap().clone()
}

async fn verify_multi_track_source(tools: &[String; 3], bytes: &[u8]) -> BTreeSet<String> {
    let streams = stream_inventory(&tools[1], bytes).await;
    assert_eq!(streams.len(), 4);
    for (index, codec, kind, language) in [
        (0, "mpeg2video", "video", None),
        (1, "aac", "audio", Some("eng")),
        // The shared probe fixture advertises English main page200 and caption
        // page888 in the same stream; ffprobe retains both descriptor languages.
        (2, "dvb_teletext", "subtitle", Some("eng,eng")),
        (3, "aac", "audio", Some("nld")),
    ] {
        let stream = streams
            .iter()
            .find(|stream| stream["index"] == index)
            .unwrap();
        assert_eq!(stream["codec_name"], codec);
        assert_eq!(stream["codec_type"], kind);
        if let Some(language) = language {
            assert_eq!(stream["tags"]["language"], language);
        }
    }
    let caption = decode_text(&tools[0], bytes, "888", "0:s:0").await;
    assert!(String::from_utf8(caption)
        .unwrap()
        .contains("Synthetic subtitle page 888"));
    let report: Value = serde_json::from_slice(
        &probe(&tools[2], &["--json"], bytes, 256 * 1024)
            .await
            .unwrap(),
    )
    .unwrap();
    let pids = report["pids"].as_array().unwrap();
    for (id, language) in [(257, "eng"), (258, "eng"), (259, "nld")] {
        assert_eq!(
            pids.iter().find(|pid| pid["id"] == id).unwrap()["language"],
            language
        );
    }
    println!(
        "ordinary alternate source verified: audio=eng440,nld660, caption888=decoded, tracks=4"
    );
    video_hashes(&tools[0], bytes).await.into_iter().collect()
}

async fn alternate_audio_trial(
    state: &AppState,
    public: &Server,
    root: &Path,
    tools: &[String; 3],
    source_hashes: &BTreeSet<String>,
    scenario: (StreamingMode, &str, &str, usize),
) {
    let (mode, policy, language, frequency) = scenario;
    let label = if policy == POLICY { "v1" } else { "v2" };
    println!(
        "ordinary alternate trial: mode={mode:?}, policy={label}, language={language}, frequency={frequency}"
    );
    reset_active_transcode_state_for_tests();
    let mut channel = state.repositories.channel.get_by_number("1").await.unwrap();
    assert_eq!(
        channel.subtitle_mode,
        ersatzrs_domain::ChannelSubtitleMode::None
    );
    channel.preferred_audio_language_code = Some(language.to_owned());
    state.repositories.channel.upsert(&channel).await.unwrap();
    programme_start(state, root, mode).await;
    config(state, NATIVE_TELETEXT_RUNTIME_POLICY_SETTING, Some(policy)).await;
    let calls = tokio::fs::read(root.join("addon-calls")).await.unwrap();
    let bytes = capture(public).await;
    assert!(bytes.chunks_exact(188).any(|packet| pid(packet) == 512));
    verify_selected_av(&tools[0], source_hashes, &bytes, frequency).await;
    verify_transport_for_language(&tools[2], &bytes, true, language).await;
    let streams = stream_inventory(&tools[1], &bytes).await;
    let audio: Vec<_> = streams
        .iter()
        .filter(|stream| stream["codec_type"] == "audio")
        .collect();
    assert_eq!(
        audio.len(),
        1,
        "native item encoder emits the selected audio track"
    );
    assert_eq!(audio[0]["tags"]["language"], language);
    let subtitles: Vec<_> = streams
        .iter()
        .filter(|stream| stream["codec_type"] == "subtitle")
        .collect();
    assert_eq!(subtitles.len(), 1);
    assert_eq!(subtitles[0]["codec_name"], "dvb_teletext");
    let authored = decode_text(&tools[0], &bytes, "200", "0:s:0").await;
    assert!(String::from_utf8(authored)
        .unwrap()
        .contains("Routed stored original"));
    let captions = decode_text(&tools[0], &bytes, "888", "0:s:0").await;
    // Explicit subtitle Off intentionally omits the source caption stream.
    // This proves that existing choice survives authored teletext; selected
    // exact-page burn or verified carriage remains separate acceptance work.
    assert!(!String::from_utf8(captions)
        .unwrap()
        .contains("Synthetic subtitle page 888"));
    assert_eq!(
        tokio::fs::read(root.join("addon-calls")).await.unwrap(),
        calls
    );
    println!(
        "ordinary alternate verified: mode={mode:?}, policy={label}, language={language}, frequency={frequency}, authored200=decoded, caption888=omitted-explicit-subtitle-off, packets={}",
        bytes.len() / 188
    );
}

async fn run_alternate_audio() {
    isolated_bundled_wrapper().await;
    let tools = [
        exact_tool("ERSATZRS_TELETEXT_TEST_FFMPEG"),
        exact_tool("FFPROBE_PATH"),
        exact_tool("ERSATZRS_TELETEXT_TEST_TSANALYZE"),
    ];
    assert_eq!(ersatzrs_ffmpeg::ffmpeg_path(), tools[0]);
    assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), tools[1]);
    for tool in &tools[..2] {
        let version =
            String::from_utf8(probe(tool, &["-version"], &[], 65536).await.unwrap()).unwrap();
        assert_eq!(
            ersatzrs_ffmpeg::version::parse_version(version.lines().next().unwrap())
                .unwrap()
                .major,
            9
        );
    }
    let root = tempfile::tempdir().unwrap();
    let source = multi_track_source(&tools[0], root.path()).await;
    let source_bytes = tokio::fs::read(&source).await.unwrap();
    let hashes = verify_multi_track_source(&tools, &source_bytes).await;
    let provider = multi_track_provider(&source).await;
    let state = application(root.path(), &provider).await;
    assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
    assert_eq!(
        load_active_transcode_limit(&state.pool)
            .await
            .unwrap()
            .unwrap_or(DEFAULT_ACTIVE_TRANSCODE_LIMIT),
        2
    );
    config(
        &state,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
        Some("5"),
    )
    .await;
    config(
        &state,
        ersatzrs_domain::CONFIG_KEY_FFMPEG_DEFAULT_MPEG_TS_SCRIPT,
        Some(""),
    )
    .await;
    assert_wrapper_command_policy(&state, true).await;
    let internal = Server::start(build_internal_app(state.clone())).await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let generation = install_and_accept(&state, root.path()).await;
    let mut live: Value = serde_json::from_str(POLICY).unwrap();
    live["version"] = json!(2);
    live["live_updates"] = json!({"poll_interval_ms":100,"recovery_timeout_ms":5000});
    let live = serde_json::to_string(&live).unwrap();
    for mode in [
        StreamingMode::TransportStream,
        StreamingMode::TransportStreamHybrid,
    ] {
        for policy in [POLICY, live.as_str()] {
            for (language, frequency) in [("eng", 440), ("nld", 660)] {
                alternate_audio_trial(
                    &state,
                    &public,
                    root.path(),
                    &tools,
                    &hashes,
                    (mode, policy, language, frequency),
                )
                .await;
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
            }
        }
    }
    public.finish().await;
    internal.finish().await;
    provider.finish().await;
    state.pool.close().await;
}

#[test]
#[ignore = "requires FFmpeg/ffprobe9, TSDuck, loopback and new isolated app-data; genuine normal-route audio selection plus accepted teletext"]
fn normal_routed_wrappers_select_alternate_audio_with_accepted_teletext() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(run_alternate_audio());
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
