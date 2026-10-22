//! Real internal source-file endpoint and public HLS, without a provider mapping.

use super::*;
use axum::http::{header, HeaderMap, Method, StatusCode};
use ersatzrs_infra::{file_system_layout, next_engine::media::ACCESS_HEADER};
use std::{
    collections::BTreeMap,
    sync::atomic::{AtomicUsize, Ordering},
};

#[derive(Clone, Default)]
struct Observer {
    callbacks: Callbacks,
    reads: Arc<Mutex<Vec<(String, u16, bool)>>>,
}

async fn observe(
    State(observer): State<Observer>,
    request: axum::extract::Request,
    next: Next,
) -> Response {
    let source = request.uri().path().starts_with("/ffmpeg/next-source/");
    let method = request.method().to_string();
    let range = request.headers().contains_key(header::RANGE);
    if source {
        assert!(request.headers().contains_key(ACCESS_HEADER));
    }
    let response = record_callback(State(observer.callbacks.clone()), request, next).await;
    if source {
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        observer
            .reads
            .lock()
            .await
            .push((method, response.status().as_u16(), range));
    }
    response
}

async fn source_provider(source: &Path, reads: Arc<AtomicUsize>) -> Server {
    let bytes = Arc::new(tokio::fs::read(source).await.unwrap());
    Server::start(
        Router::new().route(
            "/Videos/synthetic-movie/stream",
            get(
                move |headers: HeaderMap,
                      axum::extract::Query(query): axum::extract::Query<
                    BTreeMap<String, String>,
                >| {
                    let bytes = bytes.clone();
                    let reads = reads.clone();
                    async move {
                        assert_eq!(
                            headers[header::AUTHORIZATION],
                            "MediaBrowser Token=\"synthetic-test-only\""
                        );
                        assert_eq!(query.get("static").map(String::as_str), Some("true"));
                        assert_eq!(
                            query.get("MediaSourceId").map(String::as_str),
                            Some("synthetic-version")
                        );
                        assert!(
                            !headers.contains_key(header::RANGE),
                            "one complete provider acquisition, not AV reopens"
                        );
                        reads.fetch_add(1, Ordering::SeqCst);
                        Response::builder()
                            .status(StatusCode::OK)
                            .header(header::CONTENT_LENGTH, bytes.len())
                            .body(Body::from(bytes.as_ref().clone()))
                            .unwrap()
                    }
                },
            ),
        ),
    )
    .await
}

fn tracks(item: &PlayoutItem) -> (PathBuf, u64, u64, String, Vec<String>) {
    let tracks = item.tracks.as_ref().unwrap();
    let mut grant = None;
    let mut interval = None;
    for (track, index) in [
        (tracks.video.as_ref().unwrap(), 0),
        (tracks.audio.as_ref().unwrap(), 3),
    ] {
        assert_eq!(track.stream_index, Some(index));
        let Source::Http {
            uri,
            headers,
            in_point_ms,
            out_point_ms,
            ..
        } = track.source.as_ref().unwrap()
        else {
            panic!("AV must use the same retained internal HTTP file");
        };
        assert!(uri.contains("/ffmpeg/next-source/"));
        let value = (uri.clone(), headers.clone());
        if let Some(previous) = &grant {
            assert_eq!(previous, &value);
        }
        grant = Some(value);
        let value = (*in_point_ms, *out_point_ms);
        if let Some(previous) = &interval {
            assert_eq!(previous, &value);
        }
        interval = Some(value);
    }
    let subtitle = tracks.subtitle.as_ref().unwrap();
    assert_eq!(subtitle.stream_index, Some(0));
    let interval = interval.unwrap();
    assert_eq!(timing(subtitle), interval);
    let (seek, end) = (interval.0.unwrap(), interval.1.unwrap());
    assert!(seek >= 2000);
    assert_eq!(end, 9200);
    let Source::Local { path, .. } = subtitle.source.as_ref().unwrap() else {
        panic!("owned complete caption sidecar");
    };
    let (uri, headers) = grant.unwrap();
    (path.into(), seek, end, uri, headers)
}

async fn check_retained_delivery(
    callbacks: Callbacks,
    client: reqwest::Client,
    public: String,
    source: PathBuf,
    root: PathBuf,
    tools: [String; 3],
) -> (String, Vec<String>) {
    let cues = timeout(Duration::from_secs(35), delivered(&client, &public, &root))
        .await
        .expect("bounded public retained-source AV and caption delivery");
    let callbacks = callbacks.lock().await;
    let item = callbacks
        .iter()
        .find(|item| {
            item.tracks
                .as_ref()
                .is_some_and(|tracks| tracks.subtitle.is_some())
        })
        .expect("actual authenticated caption callback");
    let (sidecar, seek, end, uri, headers) = tracks(item);
    drop(callbacks);
    let text = tokio::fs::read_to_string(&sidecar).await.unwrap();
    let mut decode = Command::new(&tools[0]);
    decode
        .args(["-v", "error", "-nostdin", "-xerror", "-i"])
        .arg(root.join("captured.ts"))
        .args(["-f", "null", "-"]);
    tool(decode).await;
    check(
        (Path::new(&tools[0]), Path::new(&tools[1])),
        &source,
        &root.join("captured.ts"),
        &cues,
        &text,
        (seek, end, true),
        &root,
    )
    .await;
    (uri, headers)
}

async fn check_retained_burn_delivery(
    callbacks: Callbacks,
    client: reqwest::Client,
    public: String,
    source: PathBuf,
    root: PathBuf,
    tools: [String; 3],
) -> (String, Vec<String>) {
    let bytes = timeout(
        Duration::from_secs(35),
        policies::capture(&client, &public, &root),
    )
    .await
    .expect("complete retained Burn programme before the following fallback discontinuity");
    let callbacks = callbacks.lock().await;
    let item = callbacks
        .iter()
        .find(|item| {
            item.tracks
                .as_ref()
                .is_some_and(|tracks| tracks.subtitle.is_some())
        })
        .expect("actual authenticated retained Burn caption callback");
    let (sidecar, seek, end, uri, headers) = tracks(item);
    drop(callbacks);
    let text = tokio::fs::read_to_string(&sidecar).await.unwrap();
    let independent =
        tokio::fs::read_to_string(source.parent().unwrap().join("independent-889.srt"))
            .await
            .unwrap();
    assert_eq!(cue_blocks(&text), cue_blocks(&independent));
    assert!(text.contains("889 FINAL") && !text.contains("ENGLISH PAGE 888"));
    Box::pin(policies::output(
        &tools,
        &root,
        &bytes,
        policies::Policy::Burn,
    ))
    .await;
    let captured = root.join("captured.ts");
    let mut probe = Command::new(&tools[1]);
    probe
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height",
            "-of",
            "json",
        ])
        .arg(&captured);
    let dimensions: Value = serde_json::from_slice(&tool(probe).await).unwrap();
    assert_eq!(dimensions["streams"][0]["width"], 320);
    assert_eq!(dimensions["streams"][0]["height"], 180);
    Box::pin(retained_clock::check(
        (&tools[0], &tools[1]),
        &source,
        &sidecar,
        &root,
        (seek, end),
    ))
    .await;
    (uri, headers)
}

async fn configure_remote(
    state: &AppState,
    source: &Path,
    ffprobe: &str,
    address: &str,
) -> Option<mapped_provider_fixture::Provider> {
    let (item, _, provider) = Box::pin(scan_caption_owner(
        state,
        source.parent().unwrap(),
        source,
        ffprobe,
        MediaSourceKind::Jellyfin,
    ))
    .await;
    // Retain the real scanned provider/file/track identity, but no physical
    // mapping or stored teletext report can authorize playback preparation.
    for sql in [
        r#"DELETE FROM "JellyfinPathReplacement""#,
        r#"DELETE FROM "SourceTeletextMetadata""#,
    ] {
        state.pool.execute(sql).await.unwrap();
    }
    mapped_provider_fixture::update_connection(&state.pool, address, MediaSourceKind::Jellyfin)
        .await;
    for (key, value) in [
        (
            ersatzrs_infra::provider_source_storage::TOTAL_BYTES_KEY,
            "33554432",
        ),
        (
            ersatzrs_infra::provider_source_storage::MAXIMUM_SOURCE_BYTES_KEY,
            "33554432",
        ),
        (
            ersatzrs_infra::provider_source_storage::MINIMUM_FREE_BYTES_KEY,
            "0",
        ),
    ] {
        config(state, key, Some(value)).await;
    }
    next_schedule(state, item).await;
    provider
}

async fn configure_burn(state: &AppState) {
    Box::pin(software_profile(state)).await;
    state
        .pool
        .execute(r#"UPDATE "Resolution" SET "Width"=320,"Height"=180 WHERE "Id"=1"#)
        .await
        .unwrap();
    let mut channel = state.repositories.channel.get_by_number("1").await.unwrap();
    channel.next_engine_text_subtitle_mode = NextEngineTextSubtitleMode::Burn;
    state.repositories.channel.upsert(&channel).await.unwrap();
}

async fn deliver(
    callbacks: Callbacks,
    client: reqwest::Client,
    public: String,
    source: PathBuf,
    root: PathBuf,
    tools: [String; 3],
    policy: NextEngineTextSubtitleMode,
) -> (String, Vec<String>) {
    if policy == NextEngineTextSubtitleMode::Burn {
        check_retained_burn_delivery(callbacks, client, public, source, root, tools).await
    } else {
        check_retained_delivery(callbacks, client, public, source, root, tools).await
    }
}

async fn run(tools: [String; 3], policy: NextEngineTextSubtitleMode) {
    let root = tempfile::tempdir().unwrap().keep();
    println!("retained Next ordinary HTTP evidence: {}", root.display());
    let source = retained_source(&tools[0], &root.join("library")).await;
    let reads = Arc::new(AtomicUsize::new(0));
    let upstream = source_provider(&source, reads.clone()).await;
    let mut state = application(&root, &upstream).await;
    state.next_engine = Arc::new(NextHost::new(
        state.pool.clone(),
        state.application.operations().entity_locker().clone(),
        root.join("playout"),
    ));
    let provider = configure_remote(&state, &source, &tools[1], &upstream.url("")).await;
    if policy == NextEngineTextSubtitleMode::Burn {
        configure_burn(&state).await;
    }
    let observer = Observer::default();
    let internal = Server::start(build_internal_app(state.clone()).layer(
        axum::middleware::from_fn_with_state(observer.clone(), observe),
    ))
    .await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap();
    let assertion = tokio::spawn(deliver(
        observer.callbacks,
        client.clone(),
        public.url(""),
        source,
        root.clone(),
        tools,
        policy,
    ))
    .await;
    stop_next(&state).await;
    let stale_grant = if let Ok((uri, headers)) = &assertion {
        let mut request = client.request(Method::HEAD, uri);
        for header in headers {
            let (name, value) = header.split_once(':').unwrap();
            request = request.header(name, value.trim());
        }
        Some(request.send().await.unwrap().status().as_u16())
    } else {
        None
    };
    let storage = file_system_layout::provider_source_storage_folder();
    let entries = std::fs::read_dir(storage)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .collect::<Vec<_>>();
    let reports: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
        .fetch_one(&state.pool)
        .await
        .unwrap();
    let source_reads = observer.reads.lock().await.clone();
    let count = reads.load(Ordering::SeqCst);
    tokio::fs::write(
        root.join("retained-source-receipt.json"),
        serde_json::to_vec_pretty(
            &json!({"policy":format!("{policy:?}"),"upstream_full_responses":count,
            "internal_reads":source_reads,"post_stop_grant_status":stale_grant,
            "source_reports":reports,"storage_entries":entries,
            "active_work":state.application.operations().entity_locker().active_teletext_work(),
            "active_capacity":active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active}),
        )
        .unwrap(),
    )
    .await
    .unwrap();
    finish_caption_owner(
        &state,
        provider,
        MediaSourceKind::Jellyfin,
        public,
        internal,
        upstream,
    )
    .await;
    assertion
        .expect("real web retained-source endpoint must deliver the complete scheduled program");
    assert_eq!(count, 1, "AV must not download the provider a second time");
    assert_eq!(
        reports, 0,
        "ephemeral native proof never manufactures SQL readiness"
    );
    assert_eq!(stale_grant, Some(410));
    assert_eq!(entries, [std::ffi::OsString::from(".budget.lock")]);
    assert!(source_reads
        .iter()
        .any(|(method, status, range)| method == "GET" && *status == 206 && *range));
}

#[test]
#[ignore = "requires candidate4 via ETV_NEXT_FOLDER, paired retained source/889 SRT, FFmpeg9 and fresh isolated app-data/state/TMPDIR"]
fn normal_next_remote_only_provider_uses_retained_http_source_for_public_av_and_captions() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    let storage = file_system_layout::provider_source_storage_folder();
    assert!(
        !storage.exists(),
        "runner must isolate the persistent source-storage root"
    );
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            Box::pin(run(
                initialize_source_caption_runtime().await,
                NextEngineTextSubtitleMode::Convert,
            ))
            .await;
        });
}

#[test]
#[ignore = "requires qualified shared-clock Next/FFmpeg candidate via ETV_NEXT_FOLDER, paired source/889 SRT and fresh isolated app-data/state/TMPDIR"]
fn normal_next_remote_only_provider_burns_page889_from_retained_http_source() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    let storage = file_system_layout::provider_source_storage_folder();
    assert!(
        !storage.exists(),
        "runner must isolate the persistent source-storage root"
    );
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(async {
            Box::pin(run(
                initialize_source_caption_runtime().await,
                NextEngineTextSubtitleMode::Burn,
            ))
            .await;
        });
}
