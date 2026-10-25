//! Ordinary Live URL source captions, without an installed add-on producer.
//! Direct HTTP/RTSP do not reach source EOF. HLS uses closed media segments and
//! a live playlist without ENDLIST; no complete-live-source extraction is possible.

use super::*;
use axum::{body::Bytes, extract::State};
use sha2::Digest as _;
use std::{
    convert::Infallible,
    fmt::Write as _,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[derive(Clone)]
struct Source {
    bytes: Bytes,
    reads: Arc<AtomicUsize>,
    active: Arc<AtomicUsize>,
    manifests: Arc<AtomicUsize>,
    rtsp_status: Option<PathBuf>,
    segment_closes: bool,
}

#[derive(Clone, Copy)]
enum Origin {
    Http,
    Hls,
    Rtsp,
}

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit opt-in synthetic RTSP origin and counter inputs, not application configuration"
)]
fn rtsp_inputs() -> (String, PathBuf) {
    let uri = std::env::var("ERSATZRS_TELETEXT_TEST_RTSP_URL")
        .expect("requires the qualified synthetic RTSP origin");
    assert!(uri.starts_with("rtsp://127.0.0.1:"));
    let status = std::env::var_os("ERSATZRS_TELETEXT_TEST_RTSP_STATUS")
        .map(PathBuf::from)
        .expect("the qualified RTSP origin must expose its counters");
    (uri, status)
}

async fn playlist(State(source): State<Source>) -> impl axum::response::IntoResponse {
    source.manifests.fetch_add(1, Ordering::SeqCst);
    (
        [("content-type", "application/vnd.apple.mpegurl")],
        "#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:12\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:12,\nlive.ts\n",
    )
}

struct BodyOwner(Arc<AtomicUsize>);

impl Drop for BodyOwner {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn serve(State(source): State<Source>) -> axum::response::Response {
    source.reads.fetch_add(1, Ordering::SeqCst);
    source.active.fetch_add(1, Ordering::SeqCst);
    let finite = source.segment_closes;
    let length = source.bytes.len();
    let stream = futures_util::stream::unfold(
        (Some(source.bytes), BodyOwner(source.active), finite),
        |(bytes, owner, finite)| async move {
            if let Some(bytes) = bytes {
                Some((Ok::<_, Infallible>(bytes), (None, owner, finite)))
            } else {
                if !finite {
                    std::future::pending::<()>().await;
                }
                drop(owner);
                None
            }
        },
    );
    let mut response =
        axum::http::Response::builder().header(axum::http::header::CONTENT_TYPE, "video/mp2t");
    if finite {
        response = response.header(axum::http::header::CONTENT_LENGTH, length.to_string());
    }
    response
        .body(axum::body::Body::from_stream(stream))
        .unwrap()
}

async fn install_source(state: &AppState, item: i64, version: i64, url: &str) {
    let original_subtitles =
        ersatzrs_infra::playback::load_subtitles_for_media_item(&state.pool, item)
            .await
            .unwrap();
    assert!(original_subtitles
        .iter()
        .any(|subtitle| subtitle.codec.as_deref() == Some("dvb_teletext")));
    let mut transaction = ersatzrs_infra::db::begin_write(&state.pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (?,?,2,1)"#)
        .bind(item)
        .bind(url)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=? WHERE "Id"=?"#)
        .bind(item)
        .bind(version)
        .execute(&mut *transaction)
        .await
        .unwrap();
    let metadata: i64 = sqlx::query_scalar(r#"INSERT INTO "RemoteStreamMetadata" ("RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated")
        VALUES (?,0,'Synthetic direct live captions','2026-10-25','2026-10-25') RETURNING "Id""#)
        .bind(item).fetch_one(&mut *transaction).await.unwrap();
    // Move the original scanner metadata before its old owner is removed.
    sqlx::query(
        r#"UPDATE "Subtitle" SET "RemoteStreamMetadataId"=?,"MovieMetadataId"=NULL
        WHERE "MovieMetadataId" IN (SELECT "Id" FROM "MovieMetadata" WHERE "MovieId"=?)"#,
    )
    .bind(metadata)
    .bind(item)
    .execute(&mut *transaction)
    .await
    .unwrap();
    sqlx::query(r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=?"#)
        .bind(item)
        .execute(&mut *transaction)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "Movie" WHERE "Id"=?"#)
        .bind(item)
        .execute(&mut *transaction)
        .await
        .unwrap();
    // The public route must use the live URL, never reopen the scanned file.
    sqlx::query(
        r#"UPDATE "MediaFile" SET "Path"='unavailable/direct-live.ts' WHERE "MediaVersionId"=?"#,
    )
    .bind(version)
    .execute(&mut *transaction)
    .await
    .unwrap();
    transaction.commit().await.unwrap();
    assert_eq!(
        ersatzrs_infra::playback::load_subtitles_for_media_item(&state.pool, item)
            .await
            .unwrap(),
        original_subtitles,
        "the URL fixture must retain every original subtitle field and identity"
    );
}

async fn cases(
    state: &AppState,
    public: &Server,
    root: &Path,
    item: i64,
    tools: &[String; 3],
    expected: &[Vec<u8>; 3],
    source: &Source,
) {
    for (index, mode) in [ChannelSubtitleMode::None, ChannelSubtitleMode::Any]
        .into_iter()
        .enumerate()
    {
        reset_active_transcode_state_for_tests();
        start_item(state, item, mode, "nld", StreamingMode::TransportStream).await;
        let label = if mode == ChannelSubtitleMode::None {
            "off"
        } else {
            "burn"
        };
        let output =
            capture_recorded(public, Some(&root.join(format!("{label}-partial.ts")))).await;
        tokio::fs::write(root.join(format!("{label}.ts")), &output)
            .await
            .unwrap();
        let actual = pixels(&tools[0], &output, None).await;
        verify_case(
            tools,
            &output,
            expected,
            &actual,
            mode != ChannelSubtitleMode::None,
            "nld",
        )
        .await;
        source_closed(source, index + 1).await;
        assert_eq!(
            state
                .application
                .operations()
                .entity_locker()
                .active_teletext_work(),
            0
        );
        println!("ordinary Live URL verified: {label}, exact caption policy, Dutch660Hz, joined upstream/defaultcapacity cleanup");
    }
}

async fn source_closed(source: &Source, openings: usize) {
    if let Some(status) = &source.rtsp_status {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let observed = tokio::fs::read(status)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok());
                if let Some(observed) = observed {
                    if observed["active_clients"] == 0 && observed["active_media"] == 0 {
                        assert_eq!(observed["connections"], openings);
                        assert_eq!(observed["source_sends"], openings);
                        assert_eq!(observed["source_eof"], false);
                        let mut expected = String::with_capacity(64);
                        for byte in sha2::Sha256::digest(&source.bytes) {
                            write!(expected, "{byte:02x}").unwrap();
                        }
                        assert_eq!(observed["source_sha256"], expected);
                        assert_eq!(source.reads.load(Ordering::SeqCst), 0);
                        break;
                    }
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("RTSP clients and original media must close after stream cancellation");
    } else {
        tokio::time::timeout(Duration::from_secs(5), async {
            while source.active.load(Ordering::SeqCst) != 0 {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("all live upstream bodies must close after stream cancellation");
        assert_eq!(
            source.reads.load(Ordering::SeqCst),
            openings,
            "one physical live source per request"
        );
    }
}

async fn run(origin: Origin) {
    let rtsp = matches!(origin, Origin::Rtsp).then(rtsp_inputs);
    let tools = initialize_source_caption_runtime().await;
    let root = tempfile::tempdir().unwrap().keep();
    println!("ordinary Live URL evidence: {}", root.display());
    let library = root.join("library");
    let file = local_source(&tools[0], &library).await;
    let expected = calibration(&tools, &file, &root).await;
    let source = Source {
        bytes: tokio::fs::read(&file).await.unwrap().into(),
        reads: Arc::new(AtomicUsize::new(0)),
        active: Arc::new(AtomicUsize::new(0)),
        manifests: Arc::new(AtomicUsize::new(0)),
        rtsp_status: rtsp.as_ref().map(|(_, status)| status.clone()),
        segment_closes: matches!(origin, Origin::Hls),
    };
    let upstream = Server::start(
        Router::new()
            .route("/live.ts", get(serve))
            .route("/live.m3u8", get(playlist))
            .with_state(source.clone()),
    )
    .await;
    let state = application(&root, &upstream).await;
    let (item, version) = scan_source(&state, &library, &tools[1]).await;
    software_profile(&state).await;
    let source_url = match origin {
        Origin::Http => upstream.url("/live.ts"),
        Origin::Hls => upstream.url("/live.m3u8"),
        Origin::Rtsp => rtsp.unwrap().0,
    };
    install_source(&state, item, version, &source_url).await;
    let _generation = install_and_accept(&state, &root).await;
    let internal = Server::start(build_internal_app(state.clone())).await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Arc::new(Server::start(build_app(state.clone())).await);
    let outcome = tokio::spawn({
        let state = state.clone();
        let public = public.clone();
        let root = root.clone();
        let source = source.clone();
        async move {
            Box::pin(cases(
                &state, &public, &root, item, &tools, &expected, &source,
            ))
            .await;
        }
    })
    .await;
    let rtsp = if let Some(path) = &source.rtsp_status {
        tokio::fs::read(path)
            .await
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
    } else {
        None
    };
    tokio::fs::write(
        root.join("source-owner.json"),
        serde_json::to_vec_pretty(&json!({
            "upstream_reads":source.reads.load(Ordering::SeqCst),
            "upstream_active_after_capture":source.active.load(Ordering::SeqCst),
            "manifest_reads":source.manifests.load(Ordering::SeqCst),"rtsp":rtsp,
            "source_body_has_eof":source.segment_closes,
            "live_service_has_eof":false,"test_outcome_ok":outcome.is_ok()
        }))
        .unwrap(),
    )
    .await
    .unwrap();
    let public = Arc::try_unwrap(public)
        .unwrap_or_else(|_| panic!("joined trial must release its listener reference"));
    finish_caption_owner(
        &state,
        None,
        MediaSourceKind::Local,
        public,
        internal,
        upstream,
    )
    .await;
    if matches!(origin, Origin::Hls) && outcome.is_ok() {
        assert!(source.manifests.load(Ordering::SeqCst) >= 2);
    }
    outcome.expect("ordinary direct Live HTTP must preserve captions and selected audio");
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9, synthetic closed metadata/unclosed live HTTP, isolated app-data and loopback"]
fn normal_direct_live_http_preserves_source_captions_off_audio_and_cleanup() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run(Origin::Http)));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9, relative live HLS source, isolated app-data and loopback"]
fn normal_live_hls_url_preserves_source_captions_off_audio_and_cleanup() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run(Origin::Hls)));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9, qualified GStreamer1242 TS/RTP33 origin/status, isolated app-data and loopback"]
fn normal_direct_live_rtsp_preserves_source_captions_off_audio_and_cleanup() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run(Origin::Rtsp)));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
