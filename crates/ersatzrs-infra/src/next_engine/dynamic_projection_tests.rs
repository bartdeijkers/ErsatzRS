use super::*;
use chrono::Duration;
use ersatzrs_core::locking::PlayoutLockGuard;
use ersatzrs_domain::{Channel, ChannelSubtitleMode};

#[test]
fn finite_source_text_cache_fences_selected_video_but_keeps_occurrence_independence() {
    let mut video = super::super::protocol::Track {
        stream_index: Some(0),
        source: Some(Source::Local {
            path: "source.ts".into(),
            probe_hint: None,
            in_point_ms: Some(2000),
            out_point_ms: Some(9200),
        }),
    };
    let initial = source_text_video_identity(Some(&video));
    if let Some(Source::Local {
        in_point_ms,
        out_point_ms,
        ..
    }) = &mut video.source
    {
        *in_point_ms = Some(10000);
        *out_point_ms = Some(17000);
    }
    assert_eq!(source_text_video_identity(Some(&video)), initial);
    video.stream_index = Some(4);
    assert_ne!(source_text_video_identity(Some(&video)), initial);
    video.stream_index = Some(0);
    if let Some(Source::Local { path, .. }) = &mut video.source {
        *path = "other.ts".into();
    }
    assert_ne!(source_text_video_identity(Some(&video)), initial);
    assert_ne!(source_text_video_identity(None), initial);
}

async fn fixture() -> (
    DbPool,
    EntityLocker,
    Channel,
    DateTime<Utc>,
    tempfile::NamedTempFile,
) {
    let (pool, locker, mut channel, start) = schedule::tests::fixture().await;
    let file = tempfile::NamedTempFile::new().unwrap();
    std::fs::write(file.path(), b"synthetic source").unwrap();
    sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(file.path().to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "Subtitle" SET "IsExtracted"=0, "Path"=NULL WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    // These identity tests exercise preparation of an explicitly selected track.
    sqlx::query(r#"UPDATE "PlayoutItem" SET "MediaVersionId"=100,"VideoStreamIndex"=0,"AudioStreamIndex"=1,"SubtitleStreamIndex"=4"#)
        .execute(&pool).await.unwrap();
    channel.streaming_engine = StreamingEngine::Next;
    channel.subtitle_mode = ChannelSubtitleMode::Default;
    channel.preferred_subtitle_language_code = Some("eng".into());
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    (pool, locker, channel, start, file)
}

async fn snapshot(pool: &DbPool, locker: &EntityLocker, start: DateTime<Utc>) -> Snapshot {
    project_snapshot(
        pool,
        locker,
        &MediaRelay::new(),
        1,
        1,
        start,
        start + Duration::seconds(60),
        &[],
        Some(1),
    )
    .await
    .unwrap()
}

#[tokio::test]
async fn dynamic_snapshot_releases_locks_and_fingerprints_repeated_occurrences() {
    let (pool, locker, channel, start, _file) = fixture().await;
    let first = snapshot(&pool, &locker, start).await;
    assert!(first.projection.pending_burn.is_some());
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_some());
    let repeated = snapshot(&pool, &locker, start).await;
    assert_eq!(first.identity.descriptor, repeated.identity.descriptor);
    assert_eq!(first.identity.cache_key, repeated.identity.cache_key);
    let schedule_snapshot = schedule::read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + Duration::seconds(60),
    )
    .await
    .unwrap()
    .unwrap();
    let media = &schedule_snapshot.items[0];
    let current_channel = make_repositories(pool.clone())
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    let identity = fingerprint(&pool, media, &repeated.projection, &current_channel)
        .await
        .unwrap();
    let mut changed_channel = current_channel;
    changed_channel.preferred_audio_language_code = Some("nld".into());
    let changed = fingerprint(&pool, media, &repeated.projection, &changed_channel)
        .await
        .unwrap();
    assert_ne!(identity.descriptor, changed.descriptor);
    assert_eq!(identity.cache_key, changed.cache_key);
    let mut projection = repeated.projection;
    projection.item.id = "another-occurrence".into();
    projection.item.start += Duration::hours(1);
    projection.item.finish += Duration::hours(1);
    if let Some(Source::Local { in_point_ms, .. }) = projection
        .item
        .tracks
        .as_mut()
        .unwrap()
        .video
        .as_mut()
        .unwrap()
        .source
        .as_mut()
    {
        *in_point_ms = Some(5000);
    }
    let other = fingerprint(&pool, media, &projection, &channel)
        .await
        .unwrap();
    assert_ne!(first.identity.descriptor, other.descriptor);
    assert_eq!(first.identity.cache_key, other.cache_key);
}

#[tokio::test]
async fn dynamic_snapshot_invalidates_source_revisions_and_local_mtime() {
    let (pool, locker, _channel, start, file) = fixture().await;
    let baseline = snapshot(&pool, &locker, start).await.identity;
    file.as_file()
        .set_times(
            std::fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_000),
            ),
        )
        .unwrap();
    let changed_file = snapshot(&pool, &locker, start).await.identity;
    assert_ne!(baseline.descriptor, changed_file.descriptor);
    assert_ne!(baseline.cache_key, changed_file.cache_key);
    sqlx::query(r#"UPDATE "MediaVersion" SET "DateUpdated"='2026-09-23 00:00:00' WHERE "Id"=100"#)
        .execute(&pool)
        .await
        .unwrap();
    let changed_version = snapshot(&pool, &locker, start).await.identity;
    assert_ne!(changed_file.descriptor, changed_version.descriptor);
    assert_ne!(changed_file.cache_key, changed_version.cache_key);
    sqlx::query(r#"UPDATE "MediaFile" SET "PathHash"='replacement' WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    let changed_binding = snapshot(&pool, &locker, start).await.identity;
    assert_ne!(changed_version.descriptor, changed_binding.descriptor);
    assert_ne!(changed_version.cache_key, changed_binding.cache_key);
}

#[tokio::test]
async fn song_background_and_pinned_silence_still_fence_original_source_changes() {
    let (pool, locker, mut channel, start, file) = fixture().await;
    for query in [
        r#"INSERT INTO "Song" ("Id") VALUES (10)"#,
        r#"INSERT INTO "SongMetadata" ("Id", "SongId", "Title", "MetadataKind", "DateAdded", "DateUpdated") VALUES (21, 10, 'Synthetic song', 0, '2026-09-22 00:00:00', '2026-09-22 00:00:00')"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL, "SongId"=10 WHERE "Id"=100"#,
        r#"DELETE FROM "MediaStream" WHERE "MediaVersionId"=100 AND "MediaStreamKind"=1"#,
        r#"UPDATE "PlayoutItem" SET "AudioStreamIndex"=NULL, "SubtitleStreamIndex"=NULL"#,
    ] {
        sqlx::query(query).execute(&pool).await.unwrap();
    }
    channel.subtitle_mode = ChannelSubtitleMode::None;
    make_repositories(pool.clone())
        .channel
        .upsert(&channel)
        .await
        .unwrap();
    let relay = MediaRelay::new();
    let project = || {
        project_snapshot(
            &pool,
            &locker,
            &relay,
            1,
            1,
            start,
            start + Duration::seconds(60),
            &[],
            None,
        )
    };
    let before = project().await.unwrap();
    assert!(before.projection.song.is_some());
    assert!(matches!(
        before.projection.original_source,
        Source::Local { .. }
    ));
    assert!(matches!(
        &before
            .projection
            .item
            .tracks
            .as_ref()
            .unwrap()
            .audio
            .as_ref()
            .unwrap()
            .source,
        Some(Source::Lavfi { .. })
    ));
    file.as_file()
        .set_times(
            std::fs::FileTimes::new().set_modified(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_700_000_002),
            ),
        )
        .unwrap();
    let after = project().await.unwrap();
    assert_ne!(before.identity.descriptor, after.identity.descriptor);
    assert_ne!(before.identity.cache_key, after.identity.cache_key);
}

#[tokio::test]
async fn dynamic_snapshot_rejects_missing_local_identity_and_stale_binding() {
    let (pool, locker, channel, start, file) = fixture().await;
    let valid = snapshot(&pool, &locker, start).await;
    let schedule_snapshot = schedule::read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + Duration::seconds(60),
    )
    .await
    .unwrap()
    .unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"='synthetic-replacement' WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(fingerprint(
        &pool,
        &schedule_snapshot.items[0],
        &valid.projection,
        &channel
    )
    .await
    .is_err());
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
        .bind(file.path().to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    drop(file);
    let error = fingerprint(
        &pool,
        &schedule_snapshot.items[0],
        &valid.projection,
        &channel,
    )
    .await
    .err()
    .unwrap();
    assert_eq!(error.to_string(), unavailable().to_string());
    drop(schedule_snapshot);
    assert!(PlayoutLockGuard::try_acquire(&locker, 1).is_some());
}

#[tokio::test]
async fn watermark_changes_fence_dynamic_publication_without_discarding_subtitle_cache() {
    let (pool, locker, channel, start, _file) = fixture().await;
    let mut projected = snapshot(&pool, &locker, start).await;
    let schedule_snapshot = schedule::read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + Duration::seconds(60),
    )
    .await
    .unwrap()
    .unwrap();
    let media = &schedule_snapshot.items[0];
    projected.projection.item.watermark = Some(
        serde_json::from_value(serde_json::json!({
            "source": { "source_type": "local", "path": "synthetic/watermark.png" },
            "stream_index": 0, "location": "bottom_right", "opacity_percent": 50.0
        }))
        .unwrap(),
    );
    let changed = fingerprint(&pool, media, &projected.projection, &channel)
        .await
        .unwrap();
    assert_ne!(projected.identity.descriptor, changed.descriptor);
    assert_eq!(projected.identity.cache_key, changed.cache_key);

    projected.projection.item.watermark.as_mut().unwrap().timing =
        Some(super::super::protocol::WatermarkTiming::Periodic {
            clock: super::super::protocol::PeriodicClock::Wall,
            frequency_ms: 60_000,
            hold_ms: 10_000,
        });
    let periodic = fingerprint(&pool, media, &projected.projection, &channel)
        .await
        .unwrap();
    assert_ne!(changed.descriptor, periodic.descriptor);
    assert_eq!(changed.cache_key, periodic.cache_key);
}

#[tokio::test]
async fn canvas_projection_defers_mutable_templates_without_invalidating_subtitle_cache() {
    use ersatzrs_core::repositories::GraphicsRepository as _;
    use ersatzrs_domain::{GraphicsElement, GraphicsElementConfig, ImageGraphicsConfig};
    let (pool, locker, _channel, start, _file) = fixture().await;
    sqlx::query(r#"UPDATE "MediaVersion" SET "Width"=720,"Height"=576,"SampleAspectRatio"='16:15',"DisplayAspectRatio"='4:3' WHERE "Id"=100"#)
        .execute(&pool).await.unwrap();
    let id = crate::repositories::SqliteGraphicsRepository(pool.clone()).create_element(&GraphicsElement {
        name: "Source geometry template".into(),
        config: GraphicsElementConfig::Image(ImageGraphicsConfig {
            image: Some("{{ FixtureRoot }}/{{ Resolution.Width }}-{{ ScaledResolution.Width }}-{{ ScaledResolution.Height }}.png".into()),
        }),
        ..GraphicsElement::default()
    }).await.unwrap();
    sqlx::query(r#"INSERT INTO "PlayoutItemGraphicsElement" ("PlayoutItemId","GraphicsElementId","Variables") VALUES (1,?,'{"FixtureRoot":"synthetic"}')"#)
        .bind(id).execute(&pool).await.unwrap();
    let first = snapshot(&pool, &locker, start).await;
    let canvas = serde_json::to_value(&first.projection.canvas).unwrap();
    assert_eq!(canvas["hint"]["video"][0]["width"], 320);
    assert_eq!(canvas["hint"]["video"][0]["pix_fmt"], "bgra");
    assert!(!canvas.to_string().contains("FixtureRoot"));
    assert!(first.projection.item.graphics.is_empty());
    assert!(first.projection.pending_burn.is_some());
    sqlx::query(r#"UPDATE "PlayoutItemGraphicsElement" SET "Variables"='{"FixtureRoot":"changed"}' WHERE "PlayoutItemId"=1"#)
        .execute(&pool).await.unwrap();
    let changed = snapshot(&pool, &locker, start).await;
    assert_ne!(first.identity.descriptor, changed.identity.descriptor);
    assert_eq!(
        serde_json::to_value(&first.projection.canvas).unwrap(),
        serde_json::to_value(&changed.projection.canvas).unwrap(),
        "canvas occurrence stays stable while persisted item changes still fence track preparation"
    );
    assert_eq!(first.identity.cache_key, changed.identity.cache_key);
}
