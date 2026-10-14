use super::*;

fn request(preview: bool) -> NextStart {
    NextStart {
        channel_id: 1,
        preview,
        number: "1".into(),
        executable: "fixture-next".into(),
        output: if preview { "preview" } else { "normal" }.into(),
        ffmpeg: "fixture-ffmpeg".into(),
        ffprobe: "fixture-ffprobe".into(),
        replacements: Vec::new(),
        idle_timeout: None,
        deadline: Instant::now() + Duration::from_secs(30),
    }
}

#[tokio::test]
async fn live_caption_callback_rechecks_publication_and_session_after_http_read() {
    use ersatzrs_core::repositories::PlayoutRepository as _;
    for stop_session in [false, true] {
        let (pool, locker, _, _) = super::super::remote_projection::tests::fixture().await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let uri = format!("http://{}/captions.srt", listener.local_addr().unwrap());
        sqlx::raw_sql(
            r#"UPDATE "RemoteStream" SET "Liveness"=2,"IsLive"=1;
            UPDATE "Channel" SET "SubtitleMode"=2,"PreferredSubtitleLanguageCode"='nld';
            DELETE FROM "Subtitle" WHERE "Id"<>2;"#,
        )
        .execute(&pool)
        .await
        .unwrap();
        sqlx::query(r#"UPDATE "Subtitle" SET "Path"=? WHERE "Id"=2"#)
            .bind(uri)
            .execute(&pool)
            .await
            .unwrap();
        let mut item = super::super::selection::tests::item();
        item.id = 1;
        item.playout_id = 1;
        let start = item.start;
        let finish = item.finish;
        crate::repositories::make_repositories(pool.clone())
            .playout
            .upsert_item(&item)
            .await
            .unwrap();
        let root = tempfile::tempdir().unwrap();
        let host = Arc::new(NextHost::new(pool.clone(), locker, root.path().to_owned()));
        host.media
            .configure("127.0.0.1:8409".parse().unwrap())
            .unwrap();
        let instance = retain_dynamic_context(
            &mut *host.dynamic_contexts.lock().await,
            &request(false),
            Utc::now(),
        );
        let before = super::super::dynamic_projection::project_snapshot(
            &pool,
            &host.locker,
            &host.media,
            1,
            1,
            start,
            finish,
            &[],
            Some(2),
        )
        .await
        .unwrap();
        let (started, waiting) = tokio::sync::oneshot::channel();
        let (release, released) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(serve_live_caption(listener, started, released));
        let resolving = host.clone();
        let callback = tokio::spawn(async move {
            resolving
                .resolve_dynamic(
                    1,
                    1,
                    &before.identity.descriptor,
                    Some(2),
                    "1",
                    instance,
                    start + chrono::Duration::seconds(2),
                    finish,
                )
                .await
        });
        tokio::time::timeout(Duration::from_secs(5), waiting)
            .await
            .unwrap()
            .unwrap();
        if stop_session {
            release_dynamic_context(&mut *host.dynamic_contexts.lock().await, instance);
        } else {
            sqlx::query(r#"UPDATE "RemoteStream" SET "Url"='https://example.test/replacement' WHERE "Id"=10"#)
                .execute(&pool).await.unwrap();
        }
        release.send(()).unwrap();
        server.await.unwrap();
        let result = tokio::time::timeout(Duration::from_secs(5), callback)
            .await
            .unwrap()
            .unwrap();
        let Err(error) = result else {
            panic!("stale caption preparation was returned");
        };
        // Session-bound snapshot projection rejects a stopped session before
        // reading the publication again. Both races must still reject the item.
        let expected = if stop_session {
            "Next dynamic item is unavailable: session is unavailable"
        } else {
            "Next dynamic item is unavailable: publication or session changed during caption preparation"
        };
        assert_eq!(error.to_string(), expected);
    }
}

async fn serve_live_caption(
    listener: tokio::net::TcpListener,
    started: tokio::sync::oneshot::Sender<()>,
    released: tokio::sync::oneshot::Receiver<()>,
) {
    use tokio::io::{AsyncReadExt as _, AsyncWriteExt as _};
    tokio::time::timeout(Duration::from_secs(5), async {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            assert!(header.len() < 8192);
            header.push(socket.read_u8().await.unwrap());
        }
        started.send(()).unwrap();
        released.await.unwrap();
        let cues = "1\n00:00:00,000 --> 00:00:50,000\nACTIVE\n";
        socket
            .write_all(
                format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{cues}",
                    cues.len()
                )
                .as_bytes(),
            )
            .await
            .unwrap();
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn direct_remote_callback_preserves_audio_timing_and_rejects_changed_publication() {
    let (pool, locker, channel, start) = super::super::remote_projection::tests::fixture().await;
    let root = tempfile::tempdir().unwrap();
    let host = NextHost::new(pool.clone(), locker, root.path().to_owned());
    host.media
        .configure("127.0.0.1:8409".parse().unwrap())
        .unwrap();
    let instance = retain_dynamic_context(
        &mut *host.dynamic_contexts.lock().await,
        &request(false),
        Utc::now(),
    );
    let finish = start + chrono::Duration::seconds(60);
    let before = super::super::dynamic_projection::project_snapshot(
        &pool,
        &host.locker,
        &host.media,
        channel.id,
        1,
        start,
        finish,
        &[],
        Some(2),
    )
    .await
    .unwrap();
    let now = start + chrono::Duration::seconds(7);
    let resolved = host
        .resolve_dynamic(
            channel.id,
            1,
            &before.identity.descriptor,
            Some(2),
            "1",
            instance,
            now,
            finish,
        )
        .await
        .unwrap();
    let audio = resolved.tracks.unwrap().audio.unwrap();
    assert_eq!(audio.stream_index, Some(2));
    assert!(matches!(
        audio.source,
        Some(super::super::protocol::Source::Http {
            in_point_ms: Some(7000),
            ..
        })
    ));
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"='https://example.test/new' WHERE "Id"=10"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(host
        .resolve_dynamic(
            channel.id,
            1,
            &before.identity.descriptor,
            Some(2),
            "1",
            instance,
            now,
            finish
        )
        .await
        .is_err());
    release_dynamic_context(&mut *host.dynamic_contexts.lock().await, instance);
    assert!(host
        .resolve_dynamic(
            channel.id,
            1,
            &before.identity.descriptor,
            Some(2),
            "1",
            instance,
            now,
            finish
        )
        .await
        .is_err());
}

#[tokio::test]
async fn dynamic_callbacks_bind_normal_preview_and_replacement_instances() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    pool.close().await;
    let root = tempfile::tempdir().unwrap();
    let host = NextHost::new(pool, EntityLocker::default(), root.path().to_path_buf());
    let first = request(false);
    let mut second = request(true);
    second.ffmpeg = "preview-ffmpeg".into();
    second.replacements.push(PathReplacement {
        source_path: "fixture/source".into(),
        replacement_path: "fixture/preview".into(),
        source_platform: ersatzrs_core::path_replacement::PathPlatform::Unix,
    });
    let mut contexts = host.dynamic_contexts.lock().await;
    let started_at = Utc::now();
    let normal = retain_dynamic_context(&mut contexts, &first, started_at);
    assert_eq!(contexts[&normal].started_at, started_at);
    let preview = retain_dynamic_context(&mut contexts, &second, Utc::now());
    assert_ne!(normal, preview);
    assert_eq!(contexts[&normal].request.output, first.output);
    assert_eq!(contexts[&preview].request.output, second.output);
    assert_eq!(contexts[&preview].request.ffmpeg, second.ffmpeg);
    assert_eq!(contexts[&preview].request.replacements, second.replacements);
    assert!(contexts[&normal].request.replacements.is_empty());
    let _ = release_dynamic_context(&mut contexts, normal);
    assert!(!contexts.contains_key(&normal));
    assert!(contexts.contains_key(&preview));
    let replacement = retain_dynamic_context(&mut contexts, &first, Utc::now());
    assert_ne!(replacement, normal);
    drop(contexts);

    // A closed pool proves stale/mismatched callbacks fail before any SQL read.
    for (channel, number, instance) in [
        (1, "1", normal),
        (2, "1", preview),
        (1, "other", preview),
        (1, "1", uuid::Uuid::new_v4()),
    ] {
        let error = host
            .resolve_dynamic(
                channel,
                1,
                "fixture",
                None,
                number,
                instance,
                chrono::Utc::now(),
                chrono::Utc::now() + chrono::Duration::seconds(10),
            )
            .await
            .err()
            .unwrap();
        assert!(error.to_string().contains("session is unavailable"));
    }
    let mut contexts = host.dynamic_contexts.lock().await;
    let _ = release_dynamic_context(&mut contexts, preview);
    assert!(contexts.contains_key(&replacement));
    let _ = release_dynamic_context(&mut contexts, replacement);
    assert!(contexts.is_empty());
}

#[tokio::test]
#[allow(clippy::await_holding_lock)] // Serialize shared capacity fixture state.
async fn unconfirmed_cleanup_quarantines_capacity_but_reaped_failure_does_not() {
    use crate::remote_stream::RemoteStreamError;
    let _guard = crate::transcode_capacity::active_transcode_test_guard_for_tests();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect_lazy("sqlite::memory:")
        .unwrap();
    let root = tempfile::tempdir().unwrap();
    let host = NextHost::new(pool, EntityLocker::default(), root.path().to_owned());
    let key = format!("cleanup-fixture-{}", uuid::Uuid::new_v4());
    let mut reservation = try_reserve_active_transcode(
        i32::MAX,
        ActiveTranscodeSlot::new_next_hls_with_capacity_key(&key, &key),
    );
    assert!(reservation.is_some());
    assert!(
        !host
            .quarantine_unconfirmed_cleanup(
                &RemoteStreamError::PreparedProducerFailed,
                &mut reservation
            )
            .await
    );
    assert!(reservation.is_some());
    assert!(host.cleanup_quarantine.lock().await.is_empty());
    assert!(
        host.quarantine_unconfirmed_cleanup(
            &RemoteStreamError::PreparedTerminationUnconfirmed,
            &mut reservation
        )
        .await
    );
    assert!(reservation.is_none());
    assert_eq!(host.cleanup_quarantine.lock().await.len(), 1);
    assert!(try_reserve_active_transcode(
        i32::MAX,
        ActiveTranscodeSlot::new_next_hls_with_capacity_key(&key, &key)
    )
    .is_none());
    drop(host);
    assert!(try_reserve_active_transcode(
        i32::MAX,
        ActiveTranscodeSlot::new_next_hls_with_capacity_key(&key, &key)
    )
    .is_some());
}

async fn output_frame_rate_host_fixture(
) -> (NextHost, ersatzrs_domain::Channel, chrono::DateTime<Utc>) {
    use ersatzrs_core::repositories::FfmpegProfileRepository as _;
    use ersatzrs_domain::{
        Channel, ChannelPlayoutSource, FFmpegProfile, FFmpegProfileAudioFormat,
        FFmpegProfileVideoFormat, NextEngineTextSubtitleMode, StreamingEngine,
    };
    let (pool, locker, source, start) = super::super::schedule::tests::fixture().await;
    for sql in [
        r#"UPDATE "MediaVersion" SET "RFrameRate"='30' WHERE "Id"=100"#,
        r#"INSERT INTO "MediaVersion" ("Id","DateAdded","DateUpdated","Duration","MovieId","Height","Width","VideoScanKind","RFrameRate") VALUES (101,'2026-09-22','2026-09-22','00:01:00',10,180,320,0,'25')"#,
        r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (3,101,'media/pinned-alternate.mkv','pinned-alternate')"#,
        r#"INSERT INTO "MediaStream" ("Id","Index","Codec","MediaStreamKind","Language","Channels","Title","Default","Forced","MediaVersionId") VALUES (6,0,'h264',1,NULL,0,NULL,1,0,101),(7,2,'aac',2,'nld',2,'Dutch',0,0,101),(8,4,'subrip',3,'eng',0,'English',1,0,101)"#,
        r#"UPDATE "PlayoutItem" SET "MediaVersionId"=101,"VideoStreamIndex"=0,"AudioStreamIndex"=2,"SubtitleStreamIndex"=4 WHERE "Id"=2"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    let repos = crate::repositories::make_repositories(pool.clone());
    repos
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 2,
            resolution_id: 1,
            normalize_framerate: true,
            video_format: FFmpegProfileVideoFormat::Hevc,
            audio_format: FFmpegProfileAudioFormat::Ac3,
            ..Default::default()
        })
        .await
        .unwrap();
    let mirror = Channel {
        id: 2,
        number: "2".into(),
        name: "Frame rate mirror".into(),
        ffmpeg_profile_id: 2,
        playout_source: ChannelPlayoutSource::Mirror,
        mirror_source_channel_id: Some(source.id),
        playout_offset: Some(chrono::Duration::seconds(3)),
        streaming_engine: StreamingEngine::Next,
        preferred_audio_language_code: Some("eng".into()),
        preferred_subtitle_language_code: Some("nld".into()),
        next_engine_text_subtitle_mode: NextEngineTextSubtitleMode::Convert,
        ..Default::default()
    };
    repos.channel.upsert(&mirror).await.unwrap();
    assert!(
        !load_channel_playback_profile(&pool, source.id)
            .await
            .unwrap()
            .unwrap()
            .profile
            .normalize_framerate
    );
    assert_eq!(
        super::super::schedule::frame_rates(&pool, mirror.id)
            .await
            .unwrap(),
        Vec::<String>::new()
    );
    (
        NextHost::new(pool, locker, "fixture/frame-rate-playout".into()),
        mirror,
        start,
    )
}

async fn assert_output_frame_rate_host_preserves_exact_tracks(
    host: &NextHost,
    mirror: &ersatzrs_domain::Channel,
    start: chrono::DateTime<Utc>,
) {
    let target_start = start + mirror.playout_offset.unwrap();
    let schedule = super::super::schedule::read_locked(
        &host.pool,
        &host.locker,
        mirror,
        target_start,
        target_start + chrono::Duration::minutes(2),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(schedule.items.len(), 2);
    assert_eq!(schedule.items[0].media_version_id, 100);
    let pinned = &schedule.items[1];
    assert_eq!(pinned.media_version_id, 101);
    assert_eq!(pinned.media_file_id, 3);
    assert_eq!(pinned.path, "media/pinned-alternate.mkv");
    assert_eq!(
        pinned.item.pinned_stream_selection,
        Some(ersatzrs_domain::PinnedStreamSelection {
            media_version_id: 101,
            video_stream_index: 0,
            audio_stream_index: Some(2),
            subtitle_stream_index: Some(4),
        })
    );
    let profile = load_channel_playback_profile(&host.pool, mirror.id)
        .await
        .unwrap()
        .unwrap();
    let projected = super::super::selection::project_item(
        &host.pool,
        mirror,
        &pinned.item,
        pinned.media_version_id,
        super::super::protocol::Source::Local {
            path: pinned.path.clone(),
            probe_hint: None,
            in_point_ms: None,
            out_point_ms: None,
        },
        &profile.resolution,
    )
    .await
    .unwrap();
    let wire = serde_json::to_value(projected).unwrap();
    assert_eq!(wire["tracks"]["video"]["stream_index"], 0);
    assert_eq!(
        wire["tracks"]["audio"]["stream_index"], 2,
        "exact Dutch track must survive conflicting English preference"
    );
    assert_eq!(
        wire["tracks"]["subtitle"]["stream_index"], 4,
        "exact English caption must survive conflicting Dutch preference"
    );
}

#[tokio::test]
async fn output_frame_rate_host_ordinary_and_preview_use_mirror_source_and_requested_profile() {
    let (host, mirror, start) = output_frame_rate_host_fixture().await;
    assert_output_frame_rate_host_preserves_exact_tracks(&host, &mirror, start).await;
    for normalize in [true, false] {
        sqlx::query(r#"UPDATE "FFmpegProfile" SET "NormalizeFramerate"=? WHERE "Id"=2"#)
            .bind(normalize)
            .execute(&host.pool)
            .await
            .unwrap();
        for preview in [false, true] {
            let mut start_request = request(preview);
            start_request.channel_id = mirror.id;
            start_request.number.clone_from(&mirror.number);
            let config = host.configuration(&start_request).await.unwrap();
            let wire = serde_json::to_value(config).unwrap();
            assert_eq!(
                wire["version"],
                "https://ersatztv.org/channel/version/0.1.2"
            );
            if normalize {
                assert_eq!(wire["normalization"]["video"]["frame_rate"], "25");
            } else {
                assert!(wire["normalization"]["video"].get("frame_rate").is_none());
            }
            assert_eq!(
                wire["normalization"]["video"]["format"],
                if preview { "h264" } else { "hevc" }
            );
            assert_eq!(
                wire["normalization"]["audio"]["format"],
                if preview { "aac" } else { "ac3" }
            );
            assert_eq!(wire["normalization"]["subtitle"]["mode"], "convert");
            if preview {
                assert_eq!(wire["normalization"]["video"]["bit_depth"], 8);
            }
            assert_output_frame_rate_host_preserves_exact_tracks(&host, &mirror, start).await;
        }
    }
    host.pool.close().await;
}

#[tokio::test]
async fn output_frame_rate_host_unknown_source_sentinel_preserves_startup_and_exact_pins() {
    use ersatzrs_core::repositories::PlayoutRepository as _;
    let (host, mirror, start) = output_frame_rate_host_fixture().await;
    let repos = crate::repositories::make_repositories(host.pool.clone());
    let finish = start + chrono::Duration::minutes(2);
    let items_before = repos
        .playout
        .items_in_range(1, start, finish)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaVersion" SET "RFrameRate"='0/0' WHERE "Id"=100"#)
        .execute(&host.pool)
        .await
        .unwrap();
    for alternate in ["0/0", "25"] {
        sqlx::query(r#"UPDATE "MediaVersion" SET "RFrameRate"=? WHERE "Id"=101"#)
            .bind(alternate)
            .execute(&host.pool)
            .await
            .unwrap();
        let stored_before: Vec<(i64, String)> = sqlx::query_as(
            r#"SELECT "Id","RFrameRate" FROM "MediaVersion" WHERE "Id" IN (100,101) ORDER BY "Id""#,
        )
        .fetch_all(&host.pool)
        .await
        .unwrap();
        assert_eq!(
            stored_before,
            [(100, "0/0".to_owned()), (101, alternate.to_owned())]
        );
        for preview in [false, true] {
            let mut start_request = request(preview);
            start_request.channel_id = mirror.id;
            start_request.number.clone_from(&mirror.number);
            let config = host.configuration(&start_request).await.expect(
                "known unknown-rate source metadata must not refuse Next startup configuration",
            );
            assert!(config.normalization.video.frame_rate.is_none());
            assert_eq!(
                config.version.as_deref(),
                Some("https://ersatztv.org/channel/version/0.1.2")
            );
            assert_output_frame_rate_host_preserves_exact_tracks(&host, &mirror, start).await;
            assert_eq!(repos.playout.items_in_range(1,start,finish).await.unwrap(),items_before,
                "sampling must preserve every exact version/video/audio/subtitle pin and programme clock");
            let stored_after: Vec<(i64,String)> = sqlx::query_as(
                r#"SELECT "Id","RFrameRate" FROM "MediaVersion" WHERE "Id" IN (100,101) ORDER BY "Id""#)
                .fetch_all(&host.pool).await.unwrap();
            assert_eq!(
                stored_after, stored_before,
                "unknown rate metadata must not be rewritten"
            );
        }
    }
    host.pool.close().await;
}
