//! One finite authenticated provider response shared by actual Next AV/captions.

use super::*;
use crate::provider_source_storage::{Policy, ProviderSourceStore};
use ersatzrs_core::{
    locking::{EntityLocker, TeletextWorkGuard},
    repositories::RemoteMediaSourceKind,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

#[path = "provider_source_runtime_tests/callback.rs"]
mod callback;

const SOURCE_LIMIT: u64 = 32 * 1024 * 1024;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Convert,
    None,
    Off,
    Quota,
}

impl Mode {
    fn name(self) -> &'static str {
        match self {
            Self::Convert => "convert",
            Self::None => "none",
            Self::Off => "off",
            Self::Quota => "quota",
        }
    }
}

#[tokio::test]
#[ignore = "requires pinned Next0.2.2, FFmpeg9, Python3 and fresh ERSATZRS_NEXT_TELETEXT_PROOF_DIR"]
async fn real_next_remote_only_provider_teletext_shares_source_clock_audio_and_bounded_cleanup() {
    let root = fixture::proof_root();
    let ffmpeg = runtime_path("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = runtime_path("ERSATZRS_NEXT_TEST_FFPROBE");
    let clip = fixture::source(&ffmpeg, &root).await;
    fixture::independent_pages(&ffmpeg, &clip, &root).await;
    let bytes = Arc::new(tokio::fs::read(&clip).await.unwrap());
    for kind in RemoteMediaSourceKind::ALL {
        run_provider(
            &root,
            &clip,
            &bytes,
            (&ffmpeg, &ffprobe),
            kind,
            Mode::Convert,
        )
        .await;
    }
    for mode in [Mode::None, Mode::Off, Mode::Quota] {
        run_provider(
            &root,
            &clip,
            &bytes,
            (&ffmpeg, &ffprobe),
            RemoteMediaSourceKind::Jellyfin,
            mode,
        )
        .await;
    }
}

async fn prepare_provider(
    clip: &Path,
    ffprobe: &Path,
    root: &Path,
    kind: RemoteMediaSourceKind,
    mode: Mode,
) -> (crate::db::DbPool, EntityLocker) {
    let profile = FFmpegProfile {
        video_format: FFmpegProfileVideoFormat::Copy,
        audio_format: FFmpegProfileAudioFormat::Copy,
        audio_sample_rate: 48,
        audio_channels: 1,
        ..text_profile()
    };
    let (pool, locker) = prepare_schedule(clip, false, &profile).await;
    fixture::acquire_and_store_observed(&pool, ffprobe, clip, root, &[888, 889]).await;
    for sql in [
        r#"DELETE FROM "SourceTeletextMetadata""#,
        r#"DELETE FROM "LocalLibrary""#,
        r#"DELETE FROM "LocalMediaSource""#,
        r#"UPDATE "MediaFile" SET "Path"='server-only/caption-source.ts',"ProviderMediaSourceId"='selected-caption-source' WHERE "Id"=1"#,
        r#"UPDATE "Channel" SET "SubtitleMode"=3,"NextEngineTextSubtitleMode"=1 WHERE "Id"=1"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    crate::next_engine::source::tests::seed_provider(&pool, kind).await;
    if kind == RemoteMediaSourceKind::Plex {
        sqlx::query(r#"INSERT INTO "PlexMovie" ("Id") VALUES (10)"#)
            .execute(&pool)
            .await
            .unwrap();
    }
    let mut item = selection::tests::item();
    item.id = 1;
    item.playout_id = 1;
    item.start = chrono::Utc::now() - chrono::Duration::milliseconds(100);
    item.in_point = chrono::Duration::milliseconds(2000);
    item.out_point = chrono::Duration::milliseconds(9200);
    item.finish = item.start + chrono::Duration::milliseconds(7200);
    if mode == Mode::Off {
        item.pinned_stream_selection = Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 100,
            video_stream_index: 0,
            audio_stream_index: Some(3),
            subtitle_stream_index: None,
        });
    }
    crate::repositories::make_repositories(pool.clone())
        .playout
        .upsert_item(&item)
        .await
        .unwrap();
    if mode == Mode::None {
        sqlx::query(r#"UPDATE "Channel" SET "SubtitleMode"=0 WHERE "Id"=1"#)
            .execute(&pool)
            .await
            .unwrap();
    }
    for (key, value) in [
        (
            crate::provider_source_storage::TOTAL_BYTES_KEY,
            SOURCE_LIMIT,
        ),
        (
            crate::provider_source_storage::MAXIMUM_SOURCE_BYTES_KEY,
            SOURCE_LIMIT,
        ),
        (crate::provider_source_storage::MINIMUM_FREE_BYTES_KEY, 0),
    ] {
        sqlx::query(r#"INSERT INTO "ConfigElement" ("Key","Value") VALUES (?,?) ON CONFLICT("Key") DO UPDATE SET "Value"=excluded."Value""#)
            .bind(key).bind(value.to_string()).execute(&pool).await.unwrap();
    }
    (pool, locker)
}

async fn run_provider(
    root: &Path,
    clip: &Path,
    bytes: &Arc<Vec<u8>>,
    tools: (&Path, &Path),
    kind: RemoteMediaSourceKind,
    mode: Mode,
) {
    let proof = root.join(format!("{}-{}", kind.provider_name(), mode.name()));
    tokio::fs::create_dir(&proof).await.unwrap();
    let (pool, locker) = prepare_provider(clip, tools.1, &proof, kind, mode).await;
    let server = fixture_media_server().await;
    let (media, full_reads) = mock_media(&server, kind, bytes).await;
    let _wrong_plex = fixture_provider_connection(&pool, kind, &server).await;
    let storage_root = proof.join("provider-sources");
    let store = ProviderSourceStore::new(storage_root.clone());
    let quota = reserve_quota(&store, mode).await;
    let mut host = host::NextHost::new(pool.clone(), locker.clone(), proof.join("playout"));
    host.source_storage = store;
    let host = Arc::new(host);
    let events = Arc::new(tokio::sync::Mutex::new(callback::Events::default()));
    let listener = callback::listen(host.clone(), events.clone()).await;
    let registry = Arc::new(crate::hls::InMemoryHlsSessionRegistry::new());
    let output = proof.join("output");
    let task = host
        .start(
            registry.clone(),
            host::NextStart {
                channel_id: 1,
                preview: false,
                number: "1".into(),
                executable: runtime_path("ERSATZRS_NEXT_TEST_BIN"),
                output: output.clone(),
                ffmpeg: tools.0.into(),
                ffprobe: tools.1.into(),
                replacements: Vec::new(),
                idle_timeout: None,
                deadline: Instant::now() + Duration::from_secs(30),
            },
        )
        .await
        .unwrap();
    let captured = capture(&registry, &output, &proof, mode == Mode::Convert).await;
    assert!(registry.stop_channel("1").await);
    timeout(Duration::from_secs(15), task)
        .await
        .unwrap()
        .unwrap();
    listener.stop().await;
    assert!(!output.exists(), "actual Next output cleanup must complete");
    assert_eq!(locker.active_teletext_work(), 0);
    assert!(host.dynamic_contexts.lock().await.is_empty());
    if let Some(lease) = quota {
        let cleanup = lease.cleanup();
        drop(lease);
        cleanup.wait().await.unwrap();
    }
    assert_storage_clean(&storage_root);
    let events = events.lock().await;
    assert_counts(
        &pool,
        &proof,
        &events,
        (media.calls_async().await, full_reads.load(Ordering::SeqCst)),
        mode,
        captured.is_ok(),
    )
    .await;
    let cues = captured.expect("bounded remote provider playback must deliver programme");
    let item = match events.items.iter().find(|item| item.id == "1") {
        Some(item) => item.clone(),
        None if mode != Mode::Convert => static_published_item(&proof).await,
        None => panic!("actual authenticated provider item callback required"),
    };
    assert_output(tools, clip, &proof, mode, &item, &cues).await;
}

async fn mock_media<'a>(
    server: &'a httpmock::MockServer,
    kind: RemoteMediaSourceKind,
    bytes: &Arc<Vec<u8>>,
) -> (httpmock::Mock<'a>, Arc<AtomicUsize>) {
    let full_reads = Arc::new(AtomicUsize::new(0));
    let (path, header, value) = match kind {
        RemoteMediaSourceKind::Plex => ("/library/parts/11/file.mkv", "X-Plex-Token", "<token>"),
        RemoteMediaSourceKind::Jellyfin => (
            "/Videos/fixture-item/stream",
            "Authorization",
            "MediaBrowser Token=\"<api-key>\"",
        ),
        RemoteMediaSourceKind::Emby => ("/Videos/fixture-item/stream", "X-Emby-Token", "<api-key>"),
    };
    let full = full_reads.clone();
    let mock = server
        .mock_async(|when, then| {
            let when = when
                .method(httpmock::Method::GET)
                .path(path)
                .header(header, value);
            if kind != RemoteMediaSourceKind::Plex {
                when.query_param("MediaSourceId", "selected-caption-source");
            }
            let bytes = bytes.clone();
            then.respond_with(move |request| {
                if !request.headers().contains_key("range") {
                    full.fetch_add(1, Ordering::SeqCst);
                }
                fixture_range_response(request, &bytes)
            });
        })
        .await;
    (mock, full_reads)
}

async fn reserve_quota(
    store: &ProviderSourceStore,
    mode: Mode,
) -> Option<Arc<crate::provider_source_storage::ProviderSourceLease>> {
    if mode != Mode::Quota {
        return None;
    }
    let locker = EntityLocker::default();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    Some(
        store
            .reserve(
                Policy {
                    total_bytes: SOURCE_LIMIT,
                    maximum_source_bytes: SOURCE_LIMIT,
                    minimum_free_bytes: 0,
                },
                SOURCE_LIMIT,
                SOURCE_LIMIT,
                work.hold_work(),
            )
            .await
            .unwrap(),
    )
}

fn assert_storage_clean(root: &Path) {
    if !root.exists() {
        return;
    }
    for entry in std::fs::read_dir(root).unwrap() {
        let entry = entry.unwrap();
        assert_eq!(
            entry.file_name(),
            ".budget.lock",
            "no retained source, lease or reservation receipt after stop"
        );
    }
}

async fn assert_output(
    tools: (&Path, &Path),
    clip: &Path,
    proof: &Path,
    mode: Mode,
    item: &crate::next_engine::protocol::PlayoutItem,
    cues: &[String],
) {
    use crate::next_engine::protocol::Source;
    let tracks = item.tracks.as_ref().unwrap();
    let video = tracks.video.as_ref().unwrap();
    let audio = tracks.audio.as_ref().unwrap();
    assert_eq!(video.stream_index, Some(0));
    assert_eq!(audio.stream_index, Some(3));
    let Some(Source::Http {
        uri,
        in_point_ms,
        out_point_ms,
        ..
    }) = video.source.as_ref()
    else {
        panic!("actual private HTTP media source required")
    };
    let Some(Source::Http { uri: audio_uri, .. }) = audio.source.as_ref() else {
        panic!("actual private HTTP audio source required")
    };
    assert_eq!(audio_uri, uri);
    let captured = proof.join("captured.ts");
    let mut decode = Command::new(tools.0);
    decode
        .args(["-v", "error", "-nostdin", "-xerror", "-i"])
        .arg(&captured)
        .args(["-f", "null", "-"]);
    tool(decode).await;
    if mode != Mode::Convert {
        assert!(uri.contains("/ffmpeg/next-media/"));
        assert!(tracks.subtitle.is_none());
        assert!(!cues
            .iter()
            .any(|body| body.contains("889") || body.contains("ENGLISH PAGE 888")));
        clock::copied_video(tools.0, clip, &captured).await;
        clock::copied_audio(tools.1, clip, &captured).await;
        return;
    }
    assert!(uri.contains("/ffmpeg/next-source/"));
    let caption = tracks.subtitle.as_ref().unwrap();
    assert_eq!(caption.stream_index, Some(0));
    assert_eq!(clock::timing(caption), (*in_point_ms, *out_point_ms));
    assert_eq!(*out_point_ms, Some(9200));
    assert!(in_point_ms.unwrap() >= 2000);
    let Some(Source::Local { path, .. }) = caption.source.as_ref() else {
        panic!("actual complete source SRT required")
    };
    let text = tokio::fs::read_to_string(path).await.unwrap();
    assert!(!text.contains("ENGLISH PAGE 888"));
    clock::check(
        tools,
        clip,
        &captured,
        cues,
        &text,
        (in_point_ms.unwrap(), out_point_ms.unwrap(), true),
        proof,
    )
    .await;
}

async fn assert_counts(
    pool: &crate::db::DbPool,
    proof: &Path,
    events: &callback::Events,
    reads: (usize, usize),
    mode: Mode,
    delivered: bool,
) {
    let stored: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
        .fetch_one(pool)
        .await
        .unwrap();
    let (upstream_reads, full_reads) = reads;
    let acquisitions = full_reads.checked_sub(events.original_full_reads).unwrap();
    tokio::fs::write(proof.join("provider-source-proof.json"), serde_json::to_vec_pretty(&serde_json::json!({
        "mode":mode.name(),"upstream_media_reads":upstream_reads,"full_upstream_reads":full_reads,
        "complete_source_acquisitions":acquisitions,"ordinary_full_media_callbacks":events.original_full_reads,
        "ordinary_media_callbacks":events.original_reads,"retained_source_callbacks":events.retained_reads,
        "dynamic_items":events.items.len(),"stored_source_reports":stored,"delivery":delivered,
        "callback_failures":events.failures,
        "presentation_edit_keeps_existing_grant":events.presentation_edit_keeps_grant,
        "source_edit_revokes_existing_grant":events.source_edit_revokes_grant,
        "bounded_stop":true,"source_storage_clean":true,"active_work_zero":true,
    })).unwrap()).await.unwrap();
    assert_eq!(
        stored, 0,
        "ephemeral native qualification must not publish SQL readiness"
    );
    if mode == Mode::Convert {
        assert_eq!(
            events.presentation_edit_keeps_grant,
            [true],
            "presentation metadata must not revoke an existing immutable source grant"
        );
        assert_eq!(
            upstream_reads, 1,
            "AV and captions must use one complete authenticated response"
        );
        assert_eq!(events.original_reads, 0);
        assert_eq!(acquisitions, 1);
        assert!(events.retained_reads > 0);
    } else {
        assert!(upstream_reads > 0, "ordinary provider AV remains usable");
        assert_eq!(
            acquisitions, 0,
            "none/off/quota must not acquire a complete provider source"
        );
        assert_eq!(events.retained_reads, 0);
        assert!(events.original_reads > 0);
    }
}
