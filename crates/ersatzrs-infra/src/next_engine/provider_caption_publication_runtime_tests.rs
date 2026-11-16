//! Row292: a scanned-away automatic provider caption must not replace programme AV.
use super::super::caption_publication_runtime_tests::{assert_programme_video, barrier};
use super::*;

struct Inputs<'a> {
    binary: &'a Path,
    ffmpeg: &'a Path,
    ffprobe: &'a Path,
    clip: &'a Path,
    root: &'a Path,
    source_intent: bool,
}

#[tokio::test]
#[ignore = "exact Next/FFmpeg inputs; synthetic authenticated providers and Unix input-open barrier"]
async fn real_next_row292_scanned_away_provider_captions_preserve_av() {
    provider_cases(false).await;
}

#[tokio::test]
#[ignore = "row292 ordinary sidecar fallback beside refused optional finite-source captions"]
async fn real_next_row292_optional_source_caption_refusal_preserves_sidecar_av() {
    provider_cases(true).await;
}

async fn provider_cases(source_intent: bool) {
    let binary = runtime_path("ERSATZRS_NEXT_TEST_BIN");
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let root = tempfile::tempdir().unwrap();
    let clip = bilingual_clip(&ffmpeg, root.path()).await;
    let inputs = Inputs {
        binary: &binary,
        ffmpeg: &ffmpeg,
        ffprobe: &ffprobe,
        clip: &clip,
        root: root.path(),
        source_intent,
    };
    for provider in RemoteMediaSourceKind::ALL {
        if source_intent && provider != RemoteMediaSourceKind::Plex {
            continue;
        }
        for convert in [false, true] {
            for case in ["present", "row-removed", "path-cleared"] {
                if source_intent && case == "path-cleared" {
                    continue;
                }
                // Jellyfin resolves sidecars by item/stream identity without a path.
                if case == "path-cleared" && provider == RemoteMediaSourceKind::Jellyfin {
                    continue;
                }
                provider_case(&inputs, provider, convert, case).await;
            }
        }
    }
}

async fn provider_case(
    inputs: &Inputs<'_>,
    kind: RemoteMediaSourceKind,
    convert: bool,
    case: &str,
) {
    let label = format!("{}-{convert}-{case}", kind.provider_name());
    let root = inputs.root.join(&label);
    std::fs::create_dir(&root).unwrap();
    let (pool, locker) = provider_text_pool(inputs.clip, &text_profile(), kind).await;
    configure_automatic(&pool, kind, convert).await;
    if inputs.source_intent {
        // A stale optional descriptor must retain ordinary sidecar/AV fallback.
        // The complete source deliberately has no DVB page to qualify.
        sqlx::query(r#"INSERT INTO "MediaStream" ("Index","Codec","MediaStreamKind","Language","Default","Forced","Channels","MediaVersionId") VALUES (5,'dvb_teletext',3,'eng',0,0,0,100)"#)
            .execute(&pool).await.unwrap();
    }
    let server = fixture_media_server().await;
    configure_server(&server, inputs.clip, kind).await;
    let _connection = fixture_provider_connection(&pool, kind, &server).await;
    let host = Arc::new(host::NextHost::new(
        pool.clone(),
        locker,
        root.join("playout"),
    ));
    let mut callbacks = callback_listener(host.clone()).await;
    if inputs.source_intent {
        assert_source_intent(&host).await;
    }
    let barrier = barrier(&root, inputs.binary);
    let output = root.join("output");
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: barrier.executable,
                output: output.clone(),
                ffmpeg: inputs.ffmpeg.to_owned(),
                ffprobe: inputs.ffprobe.to_owned(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_mins(1),
            },
        )
        .await
        .unwrap();
    timeout(Duration::from_secs(10), async {
        while !barrier.waiting.exists() {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("provider worker did not reach publication barrier");
    mutate(&pool, case).await;
    if case != "present" {
        assert!(registry.playout_updated("1").await);
    }
    std::fs::write(&barrier.release, b"open").unwrap();
    let handle = registry.try_get("1").await.unwrap();
    timeout(Duration::from_secs(20), async {
        loop {
            match handle.startup_readiness() {
                HlsStartupReadiness::Ready => break,
                HlsStartupReadiness::Failed => {
                    panic!("provider caption failed before readiness: {label}")
                }
                _ => tokio::time::sleep(Duration::from_millis(5)).await,
            }
        }
    })
    .await
    .unwrap();
    let playlist = handle.trim_playlist().await.unwrap();
    let segment =
        assert_text_playlist(&output, &playlist, convert && case == "present", false).await;
    let sample = root.join("sample.ts");
    std::fs::copy(segment, &sample).unwrap();
    assert!(registry.stop_channel("1").await);
    timeout(Duration::from_secs(5), task)
        .await
        .unwrap()
        .unwrap();
    callbacks.stop().await;
    assert!(!output.exists());
    assert_programme_video(inputs.ffmpeg, &sample, &label).await;
    assert_text_video_and_audio(inputs.ffmpeg, &sample, !convert && case == "present").await;
    println!(
        "row292 provider={label} programme=blue audio=nld880 captions={} cleanup=joined",
        case == "present"
    );
}

async fn assert_source_intent(host: &host::NextHost) {
    use ersatzrs_core::repositories::ChannelRepository as _;
    let channel = crate::repositories::make_repositories(host.pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    let media = schedule::bind_media(&host.pool, selection::tests::item())
        .await
        .unwrap();
    let source =
        crate::next_engine::source::resolve_published(&host.pool, &media, &[], Some(&host.media))
            .await
            .unwrap();
    assert!(
        crate::next_engine::retained_source_origin::RetainedSourceIntent::load(
            &host.pool, &channel, &media, &source
        )
        .await
        .unwrap()
        .is_some(),
        "fixture must reach the optional finite-source publication branch"
    );
}

async fn configure_automatic(pool: &crate::db::DbPool, kind: RemoteMediaSourceKind, convert: bool) {
    sqlx::query(r#"UPDATE "MediaStream" SET "MediaStreamKind"=5 WHERE "Language"='nld' AND "MediaStreamKind"=3"#).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "PlayoutItem" SET "MediaVersionId"=NULL,"VideoStreamIndex"=NULL,"AudioStreamIndex"=NULL,"SubtitleStreamIndex"=NULL"#).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "Channel" SET "SubtitleMode"=2,"PreferredAudioLanguageCode"='nld',"PreferredSubtitleLanguageCode"='nld',"NextEngineTextSubtitleMode"=? WHERE "Id"=1"#).bind(i32::from(convert)).execute(pool).await.unwrap();
    sqlx::query(r#"UPDATE "Subtitle" SET "SubtitleKind"=1,"Path"=? WHERE "Language"='nld'"#)
        .bind(match kind {
            RemoteMediaSourceKind::Plex => Some("/library/streams/123"),
            RemoteMediaSourceKind::Jellyfin => None,
            RemoteMediaSourceKind::Emby => Some("source-id"),
        })
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaStream" SET "MediaStreamKind"=5 WHERE "Language"='nld' AND "MediaStreamKind"=3"#).execute(pool).await.unwrap();
}

async fn configure_server(server: &httpmock::MockServer, clip: &Path, kind: RemoteMediaSourceKind) {
    let (header, value, media_path, subtitle_path) = provider_endpoints(kind);
    let bytes = Arc::new(tokio::fs::read(clip).await.unwrap());
    let _ = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path(media_path)
                .header(header, value);
            then.respond_with(move |request| fixture_range_response(request, &bytes));
        })
        .await;
    let _ = server
        .mock_async(|when, then| {
            when.method(httpmock::Method::GET)
                .path(subtitle_path)
                .header(header, value);
            then.status(200).body(CUES);
        })
        .await;
}

async fn mutate(pool: &crate::db::DbPool, case: &str) {
    match case {
        "row-removed" => {
            sqlx::query(r#"DELETE FROM "Subtitle" WHERE "Language"='nld'"#)
                .execute(pool)
                .await
                .unwrap();
        }
        "path-cleared" => {
            sqlx::query(r#"UPDATE "Subtitle" SET "Path"=NULL WHERE "Language"='nld'"#)
                .execute(pool)
                .await
                .unwrap();
        }
        "present" => {}
        _ => panic!("unknown provider caption case"),
    }
}
