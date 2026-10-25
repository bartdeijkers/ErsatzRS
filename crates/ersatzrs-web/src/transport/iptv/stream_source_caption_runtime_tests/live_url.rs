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
    requires_reentry: bool,
    native_hls: bool,
}

#[derive(Clone, Copy)]
enum Origin {
    Http,
    HttpEof,
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
    let modes: &[ChannelSubtitleMode] = if source.native_hls {
        &[ChannelSubtitleMode::None]
    } else {
        &[ChannelSubtitleMode::None, ChannelSubtitleMode::Any]
    };
    for (index, &mode) in modes.iter().enumerate() {
        reset_active_transcode_state_for_tests();
        start_item(
            state,
            item,
            mode,
            "nld",
            if source.native_hls {
                StreamingMode::HttpLiveStreamingSegmenter
            } else {
                StreamingMode::TransportStream
            },
        )
        .await;
        let label = if mode == ChannelSubtitleMode::None {
            "off"
        } else {
            "burn"
        };
        let openings = if source.requires_reentry {
            source.reads.load(Ordering::SeqCst) + 2
        } else {
            index + 1
        };
        if source.native_hls {
            let address = public.address;
            let capture_root = root.to_path_buf();
            let response = tokio::spawn(async move {
                super::hls::capture_segments(address, &capture_root, 2).await
            })
            .await;
            stop_native_hls(state).await;
            let captured = response.expect("native HLS EOF capture assertion");
            // Verify both sides of the actual source EOF, not only the first
            // buffered segment or an attempt counter without delivered AV.
            for segment in captured {
                super::hls::verify(tools, &segment, expected, false).await;
            }
            source_closed(source, openings).await;
            assert_eq!(
                state
                    .application
                    .operations()
                    .entity_locker()
                    .active_teletext_work(),
                0
            );
            let capacity = active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT);
            assert!(capacity.peak > 0 && capacity.peak <= DEFAULT_ACTIVE_TRANSCODE_LIMIT);
            assert_eq!(capacity.active, 0);
            continue;
        }
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
        if source.requires_reentry {
            post_eof_media(tools, &output, expected, mode != ChannelSubtitleMode::None).await;
        }
        source_closed(source, openings).await;
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

async fn post_eof_media(tools: &[String; 3], output: &[u8], expected: &[Vec<u8>; 3], burn: bool) {
    // The 2.4s/25fps source contains60 video frames. A real frame75 and
    // selected audio at2.8..3.2s must therefore come after its actual EOF.
    let frame = probe(
        &tools[0],
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
            "select=eq(n\\,75)",
            "-an",
            "-fps_mode",
            "passthrough",
            "-pix_fmt",
            "rgb24",
            "-f",
            "rawvideo",
            "pipe:1",
        ],
        output,
        FRAME_BYTES,
    )
    .await
    .unwrap();
    assert_eq!(
        frame.len(),
        FRAME_BYTES,
        "real video must continue after source EOF"
    );
    if burn {
        compare_burn(&frame, &expected[0], &expected[1], &expected[2]);
    } else {
        assert!(frame.iter().all(|value| *value <= 20));
    }
    let windows = tone_windows(&tools[0], output, 660, 2 * 1024 * 1024).await;
    assert!(
        windows.len() >= 32,
        "selected audio must continue beyond original source duration"
    );
    assert!(
        windows[28..32].iter().all(|matching| *matching),
        "every post-EOF audio window must retain Dutch660Hz"
    );
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
        let actual = source.reads.load(Ordering::SeqCst);
        if source.requires_reentry {
            assert!(
                actual >= openings,
                "same viewer response must survive actual raw source EOF and reenter the source"
            );
        } else {
            assert_eq!(actual, openings, "one physical live source per request");
        }
    }
}

async fn configure_native_hls_startup(state: &AppState, native_hls: bool) {
    if native_hls {
        // The shared finite/local fixture forces five-second startup. Restore
        // the product default for three paced Live fragments; delivery still
        // has the independent unchanged twenty-second client bound.
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

async fn run(origin: Origin, native_hls: bool) {
    let rtsp = matches!(origin, Origin::Rtsp).then(rtsp_inputs);
    let tools = initialize_source_caption_runtime().await;
    let root = tempfile::tempdir().unwrap().keep();
    println!("ordinary Live URL evidence: {}", root.display());
    let library = root.join("library");
    let file = if matches!(origin, Origin::HttpEof) {
        local_source_duration(&tools[0], &library, "2.4").await
    } else {
        local_source(&tools[0], &library).await
    };
    let expected = calibration(&tools, &file, &root).await;
    let source = Source {
        bytes: tokio::fs::read(&file).await.unwrap().into(),
        reads: Arc::new(AtomicUsize::new(0)),
        active: Arc::new(AtomicUsize::new(0)),
        manifests: Arc::new(AtomicUsize::new(0)),
        rtsp_status: rtsp.as_ref().map(|(_, status)| status.clone()),
        segment_closes: matches!(origin, Origin::Hls | Origin::HttpEof),
        requires_reentry: matches!(origin, Origin::HttpEof),
        native_hls,
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
    configure_native_hls_startup(&state, native_hls).await;
    let source_url = match origin {
        Origin::Http | Origin::HttpEof => upstream.url("/live.ts"),
        Origin::Hls => upstream.url("/live.m3u8"),
        Origin::Rtsp => rtsp.unwrap().0,
    };
    install_source(&state, item, version, &source_url).await;
    let _generation = if native_hls {
        None
    } else {
        Some(install_and_accept(&state, &root).await)
    };
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
            "live_service_has_eof":false,"requires_reentry":source.requires_reentry,"test_outcome_ok":outcome.is_ok()
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
        .block_on(Box::pin(run(Origin::Http, false)));
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
        .block_on(Box::pin(run(Origin::Hls, false)));
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
        .block_on(Box::pin(run(Origin::Rtsp, false)));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9, short raw HTTP source EOF, isolated app-data and loopback"]
fn normal_live_raw_http_eof_retains_same_response_captions_audio_and_cleanup() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run(Origin::HttpEof, false)));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}

#[test]
#[ignore = "requires pinned FFmpeg/ffprobe9, native HLS raw source EOF, isolated app-data and loopback"]
fn normal_native_hls_raw_eof_retains_delivered_programme_and_alternate_audio() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(run(Origin::HttpEof, true)));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
