use super::super::source::tests::fixture;
use super::*;

#[tokio::test]
async fn emby_and_jellyfin_address_and_item_changes_invalidate_but_credentials_do_not() {
    for kind in [RemoteMediaSourceKind::Emby, RemoteMediaSourceKind::Jellyfin] {
        let (pool, media) = fixture(kind).await;
        let original = identity(&pool, &media, &http_source()).await.unwrap();
        assert_eq!(original["item_id"], "fixture-item");
        assert!(!original.to_string().contains("api-key"));
        sqlx::query(r#"UPDATE "ConfigElement" SET "Value"='rotated-synthetic-credential'"#)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(
            identity(&pool, &media, &http_source()).await.unwrap(),
            original
        );
        let (connection_sql, item_sql) = match kind {
            RemoteMediaSourceKind::Emby => (
                r#"UPDATE "EmbyConnection" SET "Address"='https://replacement.example.test/base' WHERE "Id"=1"#,
                r#"UPDATE "EmbyMovie" SET "ItemId"='replacement-item' WHERE "Id"=10"#,
            ),
            RemoteMediaSourceKind::Jellyfin => (
                r#"UPDATE "JellyfinConnection" SET "Address"='https://replacement.example.test/base' WHERE "Id"=1"#,
                r#"UPDATE "JellyfinMovie" SET "ItemId"='replacement-item' WHERE "Id"=10"#,
            ),
            RemoteMediaSourceKind::Plex => unreachable!(),
        };
        sqlx::query(connection_sql).execute(&pool).await.unwrap();
        let moved = identity(&pool, &media, &http_source()).await.unwrap();
        assert_ne!(moved, original);
        assert_eq!(moved["address"], "https://replacement.example.test/base");
        sqlx::query(item_sql).execute(&pool).await.unwrap();
        assert_ne!(
            identity(&pool, &media, &http_source()).await.unwrap(),
            moved
        );
    }
}

#[tokio::test]
async fn plex_failover_and_credential_rotation_keep_identity_but_server_and_file_changes_do_not() {
    let (pool, media) = fixture(RemoteMediaSourceKind::Plex).await;
    let original = identity(&pool, &media, &http_source()).await.unwrap();
    assert_eq!(original["machine_identifier"], "fixture-server");
    assert_eq!(original["file_key"], "/library/parts/11/file.mkv");
    for sql in [
        r#"UPDATE "ConfigElement" SET "Value"='rotated-synthetic-credential'"#,
        r#"UPDATE "PlexConnection" SET "IsActive"=0 WHERE "Id"=1"#,
        r#"INSERT INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (2,1,1,'https://failover.example.test')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    assert_eq!(
        identity(&pool, &media, &http_source()).await.unwrap(),
        original
    );
    sqlx::query(
        r#"UPDATE "PlexMediaSource" SET "ClientIdentifier"='replacement-server' WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let replaced = identity(&pool, &media, &http_source()).await.unwrap();
    assert_ne!(replaced, original);
    sqlx::query(
        r#"UPDATE "PlexMediaFile" SET "Key"='/library/parts/changed/file.mkv' WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert_ne!(
        identity(&pool, &media, &http_source()).await.unwrap(),
        replaced
    );
}

#[tokio::test]
async fn local_identity_is_stable_and_ambiguous_or_missing_bindings_fail_redacted() {
    let (pool, mut media) = fixture(RemoteMediaSourceKind::Emby).await;
    sqlx::query(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .execute(&pool)
        .await
        .unwrap();
    let error = identity(&pool, &media, &local_source()).await.unwrap_err();
    assert_eq!(
        error.to_string(),
        "Next provider identity is unavailable: binding unavailable"
    );
    sqlx::query(r#"DELETE FROM "EmbyLibrary" WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        identity(&pool, &media, &local_source()).await.unwrap(),
        json!({"kind":"local", "source_id":1})
    );
    media.media_file_id = 999;
    assert!(identity(&pool, &media, &local_source()).await.is_err());
}

#[tokio::test]
async fn missing_or_ambiguous_remote_item_and_invalid_address_are_rejected() {
    let (pool, media) = fixture(RemoteMediaSourceKind::Jellyfin).await;
    sqlx::query(r#"UPDATE "JellyfinConnection" SET "Address"='file:///synthetic' WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(identity(&pool, &media, &http_source()).await.is_err());
    sqlx::query(r#"UPDATE "JellyfinConnection" SET "Address"='https://media.example.test/base' WHERE "Id"=1"#)
        .execute(&pool).await.unwrap();
    sqlx::query(r#"DELETE FROM "JellyfinMovie" WHERE "Id"=10"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(identity(&pool, &media, &http_source()).await.is_err());
    for sql in [
        r#"INSERT INTO "JellyfinMovie" ("Id","ItemId") VALUES (10,'movie-item')"#,
        r#"INSERT INTO "Show" ("Id") VALUES (10)"#,
        r#"INSERT INTO "Season" ("Id","ShowId") VALUES (10,10)"#,
        r#"INSERT INTO "Episode" ("Id","SeasonId") VALUES (10,10)"#,
        r#"INSERT INTO "JellyfinEpisode" ("Id","ItemId") VALUES (10,'episode-item')"#,
    ] {
        sqlx::query(sql).execute(&pool).await.unwrap();
    }
    assert!(identity(&pool, &media, &http_source()).await.is_err());
}

fn local_source() -> Source {
    Source::Local {
        path: "synthetic.mkv".into(),
        in_point_ms: None,
        out_point_ms: None,

        probe_hint: None,
    }
}

fn http_source() -> Source {
    Source::Http {
        probe_hint: None,
        is_live: None,
        uri: "https://media.example.test/synthetic".into(),
        headers: vec![],
        user_agent: None,
        keep_alive: None,
        reconnect: None,
        reconnect_delay_max: None,
        timeout_us: None,
        in_point_ms: None,
        out_point_ms: None,
    }
}

#[tokio::test]
async fn locally_resolved_provider_files_need_no_remote_connection_or_item_metadata() {
    for kind in RemoteMediaSourceKind::ALL {
        let (pool, mut media) = fixture(kind).await;
        let file = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(file.path(), b"synthetic source").unwrap();
        media.path = file.path().to_str().unwrap().to_owned();
        sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=1"#)
            .bind(&media.path)
            .execute(&pool)
            .await
            .unwrap();
        let source = super::super::source::resolve(&pool, &media, &[])
            .await
            .unwrap();
        assert!(matches!(source, Source::Local { .. }));
        let original = identity(&pool, &media, &source).await.unwrap();
        let removals = match kind {
            RemoteMediaSourceKind::Plex => [
                r#"DELETE FROM "PlexConnection""#,
                r#"DELETE FROM "PlexMediaFile""#,
                r#"UPDATE "PlexMediaSource" SET "ClientIdentifier"=NULL"#,
            ],
            RemoteMediaSourceKind::Emby => [
                r#"DELETE FROM "EmbyConnection""#,
                r#"DELETE FROM "EmbyMovie""#,
                r#"DELETE FROM "ConfigElement""#,
            ],
            RemoteMediaSourceKind::Jellyfin => [
                r#"DELETE FROM "JellyfinConnection""#,
                r#"DELETE FROM "JellyfinMovie""#,
                r#"DELETE FROM "ConfigElement""#,
            ],
        };
        for sql in removals {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        let resolved = super::super::source::resolve(&pool, &media, &[])
            .await
            .unwrap();
        assert_eq!(identity(&pool, &media, &resolved).await.unwrap(), original);
        assert!(identity(&pool, &media, &http_source()).await.is_err());
        media.media_file_id = 999;
        assert!(identity(&pool, &media, &resolved).await.is_err());
    }
}

#[tokio::test]
async fn generated_and_recursive_sources_have_no_provider_identity() {
    let (pool, media) = fixture(RemoteMediaSourceKind::Emby).await;
    let dynamic = Source::Dynamic {
        uri: "https://media.example.test/callback".into(),
        headers: None,
        user_agent: None,
        timeout_us: None,
        retry_budget_ms: None,
        retry_backoff_ms: None,
    };
    assert!(identity(&pool, &media, &dynamic).await.is_err());
    let generated = Source::Lavfi {
        params: "anullsrc".into(),
        probe_hint: None,
    };
    assert!(identity(&pool, &media, &generated).await.is_err());
    let mut empty = local_source();
    if let Source::Local { path, .. } = &mut empty {
        path.clear();
    }
    assert!(identity(&pool, &media, &empty).await.is_err());
}

async fn selected_variant(pool: &DbPool, file: i64, selected: Option<&str>) {
    sqlx::query(r#"UPDATE "MediaFile" SET "ProviderMediaSourceId"=? WHERE "Id"=?"#)
        .bind(selected)
        .bind(file)
        .execute(pool)
        .await
        .unwrap();
}

#[tokio::test]
async fn selected_remote_variant_is_exact_file_scoped_and_null_keeps_historical_identity() {
    for kind in [RemoteMediaSourceKind::Emby, RemoteMediaSourceKind::Jellyfin] {
        let (pool, media) = fixture(kind).await;
        let original = identity(&pool, &media, &http_source()).await.unwrap();
        assert_eq!(
            original,
            json!({"kind":kind.provider_name().to_lowercase(),"source_id":1,
            "address":"https://media.example.test/base","item_id":"fixture-item"})
        );
        selected_variant(&pool, 2, Some("unrelated-file")).await;
        assert_eq!(
            identity(&pool, &media, &http_source()).await.unwrap(),
            original
        );
        selected_variant(&pool, 1, Some("selected-source-a")).await;
        let first = identity(&pool, &media, &http_source()).await.unwrap();
        assert_eq!(first["provider_media_source_id"], "selected-source-a");
        assert_ne!(first, original);
        selected_variant(&pool, 1, Some("selected-source-b")).await;
        let second = identity(&pool, &media, &http_source()).await.unwrap();
        assert_eq!(second["provider_media_source_id"], "selected-source-b");
        assert_ne!(second, first);
        selected_variant(&pool, 1, None).await;
        assert_eq!(
            identity(&pool, &media, &http_source()).await.unwrap(),
            original
        );
        selected_variant(&pool, 1, Some(" ")).await;
        let error = identity(&pool, &media, &http_source()).await.unwrap_err();
        assert_eq!(
            error.to_string(),
            "Next provider identity is unavailable: binding unavailable"
        );
        pool.close().await;
    }
}

async fn caption_snapshot(
    pool: &DbPool,
    locker: &ersatzrs_core::locking::EntityLocker,
    relay: &super::super::media::MediaRelay,
    start: chrono::DateTime<chrono::Utc>,
) -> super::super::dynamic_projection::Snapshot {
    super::super::dynamic_projection::project_snapshot(
        pool,
        locker,
        relay,
        1,
        1,
        start,
        start + chrono::Duration::seconds(60),
        &[],
        Some(1),
    )
    .await
    .unwrap()
}

async fn cached_caption(
    cache: &super::super::burn_cache::BurnCache,
    key: &str,
    text: &str,
) -> std::path::PathBuf {
    cache
        .prepare_text(
            key,
            "srt",
            text,
            chrono::Utc::now() + chrono::Duration::minutes(5),
            tokio::time::Instant::now() + std::time::Duration::from_secs(10),
            || async { true },
        )
        .await
        .unwrap()
}

fn assert_private_source_wire(item: &super::super::protocol::PlayoutItem, selector: &str) {
    let wire = serde_json::to_value(item).unwrap().to_string();
    assert!(!wire.contains(selector));
    assert!(!wire.contains("api-key"));
}

fn assert_source_grant_changed(before: &Source, after: &Source) {
    use super::super::media::SOURCE_BINDING_HEADER;
    let (
        Source::Http {
            uri: old_uri,
            headers: old_headers,
            ..
        },
        Source::Http {
            uri: new_uri,
            headers: new_headers,
            ..
        },
    ) = (before, after)
    else {
        panic!("ordinary remote caption source must use the private HTTP relay")
    };
    assert_eq!(old_uri, new_uri, "private binding URL stays stable");
    let prefix = format!("{SOURCE_BINDING_HEADER}: ");
    let old = old_headers
        .iter()
        .find_map(|value| value.strip_prefix(prefix.as_str()))
        .unwrap();
    let new = new_headers
        .iter()
        .find_map(|value| value.strip_prefix(prefix.as_str()))
        .unwrap();
    assert_eq!(old.len(), 64);
    assert_eq!(new.len(), 64);
    assert_ne!(
        old, new,
        "the opaque source fingerprint must change with the selected variant"
    );
}

async fn embedded_caption_policy(pool: &DbPool) {
    use ersatzrs_core::repositories::ChannelRepository as _;
    use ersatzrs_domain::{ChannelSubtitleMode, StreamingEngine};

    let repository = crate::repositories::make_repositories(pool.clone());
    let mut channel = repository.channel.get_by_number("1").await.unwrap();
    channel.streaming_engine = StreamingEngine::Next;
    channel.subtitle_mode = ChannelSubtitleMode::Any;
    channel.preferred_subtitle_language_code = Some("eng".into());
    repository.channel.upsert(&channel).await.unwrap();
    for sql in [
        r#"UPDATE "Subtitle" SET "IsExtracted"=0,"Path"=NULL WHERE "Id"=1"#,
        r#"UPDATE "PlayoutItem" SET "MediaVersionId"=100,"VideoStreamIndex"=0,"AudioStreamIndex"=1,"SubtitleStreamIndex"=4"#,
    ] {
        sqlx::query(sql).execute(pool).await.unwrap();
    }
}

#[tokio::test]
async fn selected_remote_variant_invalidates_embedded_text_cache_hits_and_staged_promotion() {
    use super::super::{
        burn_cache::{BurnCache, CacheError},
        media::MediaRelay,
    };
    use tokio::{io::AsyncWriteExt as _, time::Instant};

    for kind in [RemoteMediaSourceKind::Emby, RemoteMediaSourceKind::Jellyfin] {
        let (pool, media) = fixture(kind).await;
        let locker = ersatzrs_core::locking::EntityLocker::new();
        embedded_caption_policy(&pool).await;
        let relay = MediaRelay::new();
        relay.configure("127.0.0.1:8409".parse().unwrap()).unwrap();
        selected_variant(&pool, 1, Some("source-a")).await;
        let before = caption_snapshot(&pool, &locker, &relay, media.item.start).await;
        let pending = before.projection.pending_burn.as_ref().unwrap();
        assert_eq!(pending.subtitle.stream_index, 4);
        assert!(
            pending.source_text.is_none(),
            "ordinary embedded text, not teletext readiness"
        );
        assert_private_source_wire(&before.projection.item, "source-a");
        let root = tempfile::tempdir().unwrap();
        let cache = BurnCache::new(root.path().to_owned());
        let expiry = chrono::Utc::now() + chrono::Duration::minutes(5);
        let deadline = Instant::now() + std::time::Duration::from_secs(10);
        let first = cached_caption(&cache, &before.identity.cache_key, "source-a captions").await;

        selected_variant(&pool, 1, Some("source-b")).await;
        let after = caption_snapshot(&pool, &locker, &relay, media.item.start).await;
        assert_ne!(after.identity.descriptor, before.identity.descriptor);
        assert_ne!(after.identity.cache_key, before.identity.cache_key);
        assert_source_grant_changed(
            &pending.source,
            &after.projection.pending_burn.as_ref().unwrap().source,
        );
        assert_private_source_wire(&after.projection.item, "source-b");
        let hit = cache
            .prepare_text(
                &before.identity.cache_key,
                "srt",
                "unused",
                expiry,
                deadline,
                || async {
                    let current = caption_snapshot(&pool, &locker, &relay, media.item.start).await;
                    current.identity.descriptor == before.identity.descriptor
                        && current.identity.cache_key == before.identity.cache_key
                },
            )
            .await;
        assert_eq!(hit.unwrap_err(), CacheError::Stale);
        // Model the existing real cache boundary: extraction completes, the
        // selected provider source changes, and validation runs before promotion.
        let staged = cache
            .prepare_artifact(
                &after.identity.cache_key,
                "srt",
                expiry,
                deadline,
                || async {
                    let current = caption_snapshot(&pool, &locker, &relay, media.item.start).await;
                    current.identity.descriptor == after.identity.descriptor
                        && current.identity.cache_key == after.identity.cache_key
                },
                |_path, file| async {
                    let mut file = tokio::fs::File::from_std(file);
                    file.write_all(b"source-b captions").await.unwrap();
                    file.flush().await.unwrap();
                    selected_variant(&pool, 1, Some("source-c")).await;
                    Ok(())
                },
            )
            .await;
        assert_eq!(staged.unwrap_err(), CacheError::Stale);
        assert!(!root
            .path()
            .join(format!("{}.srt", after.identity.cache_key))
            .exists());
        assert!(!root
            .path()
            .join(format!("{}.json", after.identity.cache_key))
            .exists());
        assert!(std::fs::read_dir(root.path()).unwrap().all(|entry| !entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with(".burn-")));
        let latest = caption_snapshot(&pool, &locker, &relay, media.item.start).await;
        let fresh = cached_caption(&cache, &latest.identity.cache_key, "source-c captions").await;
        assert_ne!(fresh, first);
        assert_eq!(
            tokio::fs::read_to_string(fresh).await.unwrap(),
            "source-c captions"
        );
        assert_eq!(
            tokio::fs::read_to_string(first).await.unwrap(),
            "source-a captions"
        );
        pool.close().await;
    }
}
