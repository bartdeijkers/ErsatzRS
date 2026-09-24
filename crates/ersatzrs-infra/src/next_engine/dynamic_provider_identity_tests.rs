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
