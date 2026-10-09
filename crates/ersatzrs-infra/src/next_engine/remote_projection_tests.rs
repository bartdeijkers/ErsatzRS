use super::*;

#[tokio::test]
async fn prepared_artifact_changes_invalidate_identity_and_unready_artifacts_are_refused() {
    let (_root, pool) = crate::addon_media_list_playback::tests::fixture().await;
    let binding = load_binding(&pool, 100).await.unwrap();
    let original = publication_binding(&pool, &binding).await.unwrap().0;
    sqlx::query(r#"UPDATE "AddonMediaListArtifact" SET "RelativePath"='objects/replacement.media',"ByteLength"=6 WHERE "Id"=1"#)
        .execute(&pool).await.unwrap();
    let changed = publication_binding(&pool, &binding).await.unwrap().0;
    assert_ne!(digest(&original).unwrap(), digest(&changed).unwrap());
    for sql in [
        r#"UPDATE "AddonMediaListArtifact" SET "State"=0 WHERE "Id"=1"#,
        r#"UPDATE "AddonMediaListArtifact" SET "State"=2,"ByteLength"=0 WHERE "Id"=1"#,
        r#"UPDATE "AddonMediaListArtifact" SET "ByteLength"=5,"DurationMilliseconds"=NULL WHERE "Id"=1"#,
        r#"UPDATE "AddonMediaListArtifact" SET "DurationMilliseconds"=12500,"RelativePath"=NULL WHERE "Id"=1"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
        assert!(publication_binding(&pool, &binding).await.is_err());
    }
}

#[tokio::test]
async fn prepared_publication_uses_frozen_liveness_even_when_provider_is_disabled() {
    let (_root, pool) = crate::addon_media_list_playback::tests::fixture().await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled"=0"#)
        .execute(&pool)
        .await
        .unwrap();
    let binding = load_binding(&pool, 100).await.unwrap();
    assert_eq!(binding.liveness, 0);
    let (identity, liveness, prepared, _) = publication_binding(&pool, &binding).await.unwrap();
    assert_eq!(liveness, RemoteStreamLiveness::Finite);
    assert!(prepared);
    assert_eq!(identity["revision"], 1);
    assert!(identity["digest"]
        .as_str()
        .is_some_and(|value| !value.is_empty()));
}

#[tokio::test]
async fn managed_identity_refuses_missing_owner_revision_and_corrupt_snapshot() {
    for sql in [
        r#"UPDATE "RemoteStream" SET "AddonId"=NULL,"Liveness"=1,"IsLive"=0"#,
        r#"UPDATE "AddonMediaList" SET "PublishedRevision"=999"#,
        r#"UPDATE "AddonMediaListRevision" SET "SnapshotDigest"='corrupt'"#,
        r#"UPDATE "RemoteStream" SET "Url"='https://example.test/replacement'"#,
    ] {
        let (_root, pool) = crate::addon_media_list_playback::tests::fixture().await;
        sqlx::query(sql).execute(&pool).await.unwrap();
        let binding = load_binding(&pool, 100).await.unwrap();
        assert!(publication_binding(&pool, &binding).await.is_err());
    }
}

#[tokio::test]
async fn live_transport_has_no_seek_and_stream_metadata_changes_identity() {
    let (pool, locker, channel, start) = fixture().await;
    for uri in ["https://example.test/live", "rtsp://example.test/live"] {
        sqlx::query(r#"UPDATE "RemoteStream" SET "Url"=?,"Liveness"=2,"IsLive"=1 WHERE "Id"=10"#)
            .bind(uri)
            .execute(&pool)
            .await
            .unwrap();
        let before = snapshot(
            &pool,
            &locker,
            channel.id,
            1,
            start,
            start + chrono::Duration::seconds(60),
            Some(2),
        )
        .await
        .unwrap();
        let source = before
            .projection
            .item
            .tracks
            .as_ref()
            .unwrap()
            .video
            .as_ref()
            .unwrap()
            .source
            .as_ref()
            .unwrap();
        match source {
            Source::Http {
                is_live,
                in_point_ms,
                out_point_ms,
                ..
            } => {
                assert_eq!(*is_live, Some(true));
                assert_eq!((*in_point_ms, *out_point_ms), (None, None));
            }
            Source::Rtsp { .. } => {}
            _ => panic!("unexpected live source"),
        }
        sqlx::query(r#"UPDATE "MediaStream" SET "ColorTransfer"=? WHERE "Id"=1"#)
            .bind(uri)
            .execute(&pool)
            .await
            .unwrap();
        let after = snapshot(
            &pool,
            &locker,
            channel.id,
            1,
            start,
            start + chrono::Duration::seconds(60),
            Some(2),
        )
        .await
        .unwrap();
        assert_ne!(before.identity.descriptor, after.identity.descriptor);
    }
}

pub(crate) async fn fixture() -> (DbPool, EntityLocker, Channel, DateTime<Utc>) {
    let (pool, locker, mut channel, start) = schedule::tests::fixture().await;
    for sql in [
        r#"INSERT INTO "RemoteStream" ("Id","Url","Liveness","IsLive") VALUES (10,'https://example.test/finite',1,0)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL,"RemoteStreamId"=10 WHERE "Id"=100"#,
        r#"INSERT INTO "RemoteStreamMetadata" ("Id","RemoteStreamId","MetadataKind","Title","DateAdded","DateUpdated") VALUES (30,10,0,'Synthetic','2026-09-22 00:00:00','2026-09-22 00:00:00')"#,
        r#"UPDATE "Subtitle" SET "MovieMetadataId"=NULL,"RemoteStreamMetadataId"=30"#,
        r#"DELETE FROM "MovieMetadata" WHERE "MovieId"=10"#,
        r#"DELETE FROM "Movie" WHERE "Id"=10"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    channel.streaming_engine = StreamingEngine::Next;
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    (pool, locker, channel, start)
}

#[tokio::test]
async fn projected_song_uses_bound_remote_owner_and_refuses_detached_version() {
    let (pool, _locker, channel, start) = fixture().await;
    for query in [
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (11, 1, 0)"#,
        r#"INSERT INTO "Song" ("Id") VALUES (11)"#,
        r#"INSERT INTO "SongMetadata" ("Id", "SongId", "Title", "MetadataKind", "DateAdded", "DateUpdated") VALUES (31, 11, 'Projected song', 0, '2026-09-22 00:00:00', '2026-09-22 00:00:00')"#,
        r#"UPDATE "MediaVersion" SET "SongId"=11 WHERE "Id"=100"#,
        r#"DELETE FROM "MediaStream" WHERE "MediaVersionId"=100 AND "MediaStreamKind"=1"#,
        r#"UPDATE "PlayoutItem" SET "MediaItemId"=11 WHERE "Id"=1"#,
    ] {
        sqlx::query(query).execute(&pool).await.unwrap();
    }
    let media = schedule::bind_media(
        &pool,
        ersatzrs_domain::PlayoutItem {
            id: 1,
            playout_id: 1,
            media_item_id: 11,
            start,
            finish: start + chrono::Duration::seconds(60),
            out_point: chrono::Duration::seconds(60),
            ..Default::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(remote_owner(&media).unwrap(), 10);
    let projected = project_publishable(
        &pool,
        &channel,
        &media,
        &Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
        selection::AudioDirective::Published(Some(2)),
    )
    .await
    .unwrap();
    assert!(projected.projection.song.is_some());
    let audio = projected
        .projection
        .item
        .tracks
        .as_ref()
        .unwrap()
        .audio
        .as_ref()
        .unwrap();
    assert_eq!(audio.stream_index, Some(2));
    assert!(
        matches!(&audio.source, Some(Source::Http { uri, .. }) if uri == "https://example.test/finite")
    );
    sqlx::query(r#"UPDATE "MediaVersion" SET "SongId"=NULL WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(project_publishable(
        &pool,
        &channel,
        &media,
        &Resolution {
            width: 320,
            height: 180,
            ..Default::default()
        },
        selection::AudioDirective::Published(Some(2))
    )
    .await
    .is_err());
}

#[tokio::test]
async fn callback_snapshot_preserves_audio_and_releases_schedule_lock() {
    let (pool, locker, channel, start) = fixture().await;
    let selected = snapshot(
        &pool,
        &locker,
        channel.id,
        1,
        start,
        start + chrono::Duration::seconds(60),
        Some(2),
    )
    .await
    .unwrap();
    assert_eq!(selected.liveness, RemoteStreamLiveness::Finite);
    assert_eq!(selected.media.remote_stream_id, Some(10));
    assert_eq!(
        selected
            .projection
            .item
            .tracks
            .unwrap()
            .audio
            .unwrap()
            .stream_index,
        Some(2)
    );
    let again = snapshot(
        &pool,
        &locker,
        channel.id,
        1,
        start,
        start + chrono::Duration::seconds(60),
        Some(2),
    )
    .await
    .unwrap();
    assert_eq!(selected.identity.descriptor, again.identity.descriptor);
    sqlx::query(r#"UPDATE "RemoteStream" SET "Url"='https://example.test/replaced' WHERE "Id"=10"#)
        .execute(&pool)
        .await
        .unwrap();
    let changed = snapshot(
        &pool,
        &locker,
        channel.id,
        1,
        start,
        start + chrono::Duration::seconds(60),
        Some(2),
    )
    .await
    .unwrap();
    assert_ne!(selected.identity.descriptor, changed.identity.descriptor);
}

#[tokio::test]
async fn callback_refuses_unknown_source_liveness_and_raw_scripts() {
    let (pool, locker, channel, start) = fixture().await;
    for sql in [
        r#"UPDATE "RemoteStream" SET "Liveness"=0 WHERE "Id"=10"#,
        r#"UPDATE "RemoteStream" SET "Liveness"=1,"Script"='synthetic legacy command' WHERE "Id"=10"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
        assert!(snapshot(
            &pool,
            &locker,
            channel.id,
            1,
            start,
            start + chrono::Duration::seconds(60),
            Some(2)
        )
        .await
        .is_err());
    }
}

#[tokio::test]
async fn live_subtitle_guard_preserves_images_and_distinguishes_text_limitations() {
    let (pool, locker, channel, start) = fixture().await;
    let mut snapshot = snapshot(
        &pool,
        &locker,
        channel.id,
        1,
        start,
        start + chrono::Duration::seconds(60),
        Some(2),
    )
    .await
    .unwrap();
    snapshot.liveness = RemoteStreamLiveness::Live;
    snapshot
        .projection
        .item
        .tracks
        .as_mut()
        .unwrap()
        .video
        .as_mut()
        .unwrap()
        .source = Some(Source::Rtsp {
        uri: "rtsp://example.test/live".into(),
        timeout_us: None,
        probe_hint: None,
    });
    snapshot.projection.item.tracks.as_mut().unwrap().subtitle =
        Some(crate::next_engine::protocol::Track {
            source: None,
            stream_index: Some(4),
        });
    snapshot.projection.selected_subtitle = Some(playback::PlaybackSubtitle {
        id: 1,
        subtitle_kind: crate::metadata::subtitle_kind::EMBEDDED,
        title: None,
        stream_index: 4,
        codec: Some("subrip".into()),
        default: false,
        forced: false,
        sdh: false,
        language: Some("nld".into()),
        is_extracted: false,
        path: None,
    });
    assert!(validate_producer_subtitles(&snapshot, true)
        .unwrap_err()
        .to_string()
        .contains(
            "selected embedded text subtitles require an independent source/version-bound artifact"
        ));
    validate_producer_subtitles(&snapshot, false).unwrap();
    snapshot.prepared = true;
    validate_producer_subtitles(&snapshot, true).unwrap();
    snapshot.prepared = false;
    snapshot.retained = true;
    validate_producer_subtitles(&snapshot, true).unwrap();
    snapshot.retained = false;
    assert!(validate_live_subtitles(&snapshot)
        .unwrap_err()
        .to_string()
        .contains(
            "selected live embedded text subtitles cannot be streamed by the official Next engine"
        ));
    // RTSP remains live even if legacy persisted liveness says finite.
    snapshot.liveness = RemoteStreamLiveness::Finite;
    assert!(validate_live_subtitles(&snapshot).is_err());
    snapshot
        .projection
        .selected_subtitle
        .as_mut()
        .unwrap()
        .subtitle_kind = crate::metadata::subtitle_kind::SIDECAR;
    validate_live_subtitles(&snapshot).unwrap();
    let selected = snapshot.projection.selected_subtitle.as_mut().unwrap();
    selected.subtitle_kind = crate::metadata::subtitle_kind::EMBEDDED;
    selected.codec = Some("pgs".into());
    validate_live_subtitles(&snapshot).unwrap();
    snapshot.projection.selected_subtitle = None;
    snapshot.projection.item.tracks.as_mut().unwrap().subtitle = None;
    validate_live_subtitles(&snapshot).unwrap();
}
