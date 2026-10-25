//! Ordinary direct Live HTTP source captions, without an add-on producer.
//! The existing finite bytes precede an intentionally unclosed response body.

use super::*;
use axum::{body::Bytes, extract::State};
use std::{
    convert::Infallible,
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
    let stream = futures_util::stream::unfold(
        (Some(source.bytes), BodyOwner(source.active)),
        |(bytes, owner)| async move {
            if let Some(bytes) = bytes {
                Some((Ok::<_, Infallible>(bytes), (None, owner)))
            } else {
                std::future::pending::<()>().await;
                drop(owner);
                None
            }
        },
    );
    axum::http::Response::builder()
        .header(axum::http::header::CONTENT_TYPE, "video/mp2t")
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
    let mut transaction = state.pool.begin().await.unwrap();
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
        tokio::time::timeout(Duration::from_secs(5), async {
            while source.active.load(Ordering::SeqCst) != 0 {
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("all live upstream bodies must close after stream cancellation");
        assert_eq!(
            source.reads.load(Ordering::SeqCst),
            index + 1,
            "one physical live source per request"
        );
        assert_eq!(
            state
                .application
                .operations()
                .entity_locker()
                .active_teletext_work(),
            0
        );
        println!("ordinary direct Live HTTP verified: {label}, exact888, Dutch660Hz, joined upstream/defaultcapacity cleanup");
    }
}

async fn run() {
    let tools = initialize_source_caption_runtime().await;
    let root = tempfile::tempdir().unwrap().keep();
    println!("ordinary direct live HTTP evidence: {}", root.display());
    let library = root.join("library");
    let file = local_source(&tools[0], &library).await;
    let expected = calibration(&tools, &file, &root).await;
    let source = Source {
        bytes: tokio::fs::read(&file).await.unwrap().into(),
        reads: Arc::new(AtomicUsize::new(0)),
        active: Arc::new(AtomicUsize::new(0)),
    };
    let upstream = Server::start(
        Router::new()
            .route("/live.ts", get(serve))
            .with_state(source.clone()),
    )
    .await;
    let state = application(&root, &upstream).await;
    let (item, version) = scan_source(&state, &library, &tools[1]).await;
    software_profile(&state).await;
    install_source(&state, item, version, &upstream.url("/live.ts")).await;
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
    tokio::fs::write(
        root.join("source-owner.json"),
        serde_json::to_vec_pretty(&json!({
            "upstream_reads":source.reads.load(Ordering::SeqCst),
            "upstream_active_after_capture":source.active.load(Ordering::SeqCst),
            "source_body_has_eof":false,"test_outcome_ok":outcome.is_ok()
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
        .block_on(Box::pin(run()));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
