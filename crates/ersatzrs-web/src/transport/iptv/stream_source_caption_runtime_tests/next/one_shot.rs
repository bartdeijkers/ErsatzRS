//! Ordinary installed producers refuse unqualified embedded text before opening media.
//! Independent-caption delivery remains a separate source-output acceptance obligation.
use super::*;
use crate::transport::iptv::next_media::routed_runtime_tests::fixture;

async fn install_source(state: &AppState, root: &Path, item: i64, version: i64) -> i64 {
    fixture::install_caption_source(&state.pool, root).await;
    sqlx::query(
        r#"INSERT INTO "RemoteStream" ("Id","AddonId","ProviderId","Url","Liveness","IsLive")
        VALUES (?,'org.ersatzrs.addon.fixture','item','https://example.test/one-shot',1,0)"#,
    )
    .bind(item)
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=? WHERE "Id"=?"#)
        .bind(item)
        .bind(version)
        .execute(&state.pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=?"#)
        .bind(item)
        .execute(&state.pool)
        .await
        .unwrap();
    sqlx::query(r#"DELETE FROM "Movie" WHERE "Id"=?"#)
        .bind(item)
        .execute(&state.pool)
        .await
        .unwrap();
    let metadata: i64 = sqlx::query_scalar(
        r#"INSERT INTO "RemoteStreamMetadata"
        ("RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated")
        VALUES (?,0,'Synthetic one-shot','2026-10-23','2026-10-23') RETURNING "Id""#,
    )
    .bind(item)
    .fetch_one(&state.pool)
    .await
    .unwrap();
    sqlx::query(
        r#"UPDATE "MediaFile" SET "Path"='unavailable/one-shot.yml' WHERE "MediaVersionId"=?"#,
    )
    .bind(version)
    .execute(&state.pool)
    .await
    .unwrap();
    sqlx::query_scalar(r#"INSERT INTO "Subtitle"
        ("RemoteStreamMetadataId","SubtitleKind","Title","StreamIndex","Codec","Default","Forced","SDH","Language","IsExtracted","Path","DateAdded","DateUpdated")
        SELECT ?,0,"Title","Index","Codec","Default","Forced",0,"Language",1,?,'2026-10-23','2026-10-23'
        FROM "MediaStream" WHERE "MediaVersionId"=? AND "MediaStreamKind"=3 AND "Index"=2 RETURNING "Id""#)
        .bind(metadata).bind(root.join("library/independent-889.srt").to_str().unwrap()).bind(version)
        .fetch_one(&state.pool).await.unwrap()
}

async fn refused_cases(
    state: &AppState,
    root: &Path,
    item: i64,
    version: i64,
    callbacks: &Callbacks,
    public: &str,
) {
    let client = reqwest::Client::builder()
        .no_proxy()
        .timeout(Duration::from_secs(25))
        .build()
        .unwrap();
    for (label, live, convert) in [
        ("finite-burn-refused", false, false),
        ("finite-convert-refused", false, true),
        ("live-burn-refused", true, false),
        ("live-convert-refused", true, true),
    ] {
        state
            .pool
            .execute(r#"DELETE FROM "PlayoutItem" WHERE "PlayoutId"=1"#)
            .await
            .unwrap();
        next_schedule(state, item).await;
        sqlx::query(r#"UPDATE "RemoteStream" SET "Liveness"=?,"IsLive"=? WHERE "Id"=?"#)
            .bind(if live { 2 } else { 1 })
            .bind(live)
            .bind(item)
            .execute(&state.pool)
            .await
            .unwrap();
        // A raw stream pin requests captions; it cannot qualify a teletext page.
        sqlx::query(r#"UPDATE "PlayoutItem" SET "MediaVersionId"=?,"VideoStreamIndex"=0,"AudioStreamIndex"=3,"SubtitleStreamIndex"=2 WHERE "PlayoutId"=1"#)
            .bind(version).execute(&state.pool).await.unwrap();
        let mut channel = state.repositories.channel.get_by_number("1").await.unwrap();
        channel.subtitle_mode = ChannelSubtitleMode::Any;
        channel.next_engine_text_subtitle_mode = if convert {
            NextEngineTextSubtitleMode::Convert
        } else {
            NextEngineTextSubtitleMode::Burn
        };
        state.repositories.channel.upsert(&channel).await.unwrap();
        let response = client
            .get(format!("{public}/iptv/channel/1.m3u8?mode=segmenter"))
            .send()
            .await
            .unwrap();
        // Always join session cleanup before asserting the result.
        stop_next(state).await;
        assert_eq!(
            response.status(),
            axum::http::StatusCode::SERVICE_UNAVAILABLE
        );
        assert!(
            callbacks.lock().await.is_empty(),
            "no successful item callback"
        );
        assert!(
            !root.join("producer-starts").exists(),
            "refusal must precede producer launch"
        );
        assert!(
            !root.join("producer-pid").exists(),
            "no owned producer to detach"
        );
        assert!(
            !root.join("playout/burn-subtitles").exists(),
            "never extract the consuming producer"
        );
        let case = root.join(label);
        tokio::fs::create_dir(&case).await.unwrap();
        tokio::fs::write(case.join("completion.json"), serde_json::to_vec_pretty(&json!({
            "case":label,"http_status":503,"producer_starts":0,
            "successful_callbacks":0,"raw_subtitle_index":2,"selected_audio_index":3,
            "legacy_extracted_cache_present":true,"joined_cleanup":true,
            "scope":"unsupported exact embedded-text selection refusal; no playback delivery claim"
        })).unwrap()).await.unwrap();
        println!("ordinary Next caption refusal completed: {label}");
    }
}

async fn run(tools: &[String; 3]) {
    let root = tempfile::tempdir().unwrap().keep();
    println!("ordinary Next caption refusal evidence: {}", root.display());
    let library = root.join("library");
    let source = retained_source(&tools[0], &library).await;
    let unused = Server::start(Router::new()).await;
    let mut state = application(&root, &unused).await;
    state.next_engine = Arc::new(NextHost::new(
        state.pool.clone(),
        state.application.operations().entity_locker().clone(),
        root.join("playout"),
    ));
    let (item, version, provider) = Box::pin(scan_caption_owner(
        &state,
        &library,
        &source,
        &tools[1],
        MediaSourceKind::Local,
    ))
    .await;
    assert!(provider.is_none());
    install_source(&state, &root, item, version).await;
    let callbacks = Callbacks::default();
    let internal = Server::start(build_internal_app(state.clone()).layer(
        axum::middleware::from_fn_with_state(callbacks.clone(), record_callback),
    ))
    .await;
    state.internal_http_addr.set(internal.address).unwrap();
    state.next_engine.media.configure(internal.address).unwrap();
    let public = Server::start(build_app(state.clone())).await;
    state.streaming_http_addr.set(public.address).unwrap();
    let checking_state = state.clone();
    let checking_root = root.clone();
    let checking_public = public.url("");
    let outcome = tokio::spawn(Box::pin(async move {
        refused_cases(
            &checking_state,
            &checking_root,
            item,
            version,
            &callbacks,
            &checking_public,
        )
        .await;
    }))
    .await;
    finish_caption_owner(
        &state,
        provider,
        MediaSourceKind::Local,
        public,
        internal,
        unused,
    )
    .await;
    outcome.expect("ordinary Next caption refusal group");
}

#[test]
#[ignore = "requires pinned Next/FFmpeg9, retained page889 source/SRT and isolated Unix app-data; ordinary refusal route proof"]
fn normal_next_one_shot_embedded_text_is_refused_before_source_open() {
    let _guard = active_transcode_test_guard_for_tests();
    reset_active_transcode_state_for_tests();
    assert_eq!(DEFAULT_ACTIVE_TRANSCODE_LIMIT, 2);
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .unwrap()
        .block_on(Box::pin(async {
            let tools = initialize_source_caption_runtime().await;
            run(&tools).await;
        }));
    assert_eq!(
        active_transcode_snapshot(DEFAULT_ACTIVE_TRANSCODE_LIMIT).active,
        0
    );
}
