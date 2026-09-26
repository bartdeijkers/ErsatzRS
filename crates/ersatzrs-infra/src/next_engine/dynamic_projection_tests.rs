use super::*;
use chrono::Duration;
use ersatzrs_core::locking::PlayoutLockGuard;
use ersatzrs_domain::{Channel, ChannelSubtitleMode};

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
async fn image_templates_use_bound_source_geometry_and_item_variables() {
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
    let graphics = serde_json::to_value(&first.projection.item.graphics).unwrap();
    assert_eq!(graphics[0]["source"]["path"], "synthetic/320-240-180.png");
    assert!(first.projection.pending_burn.is_some());
    sqlx::query(r#"UPDATE "PlayoutItemGraphicsElement" SET "Variables"='{"FixtureRoot":"changed"}' WHERE "PlayoutItemId"=1"#)
        .execute(&pool).await.unwrap();
    let changed = snapshot(&pool, &locker, start).await;
    assert_ne!(first.identity.descriptor, changed.identity.descriptor);
    assert_eq!(first.identity.cache_key, changed.identity.cache_key);
}
