use super::*;
use crate::next_engine::schedule;
use chrono::Duration;
use ersatzrs_core::repositories::PathReplacementAdmin;
use serde_json::Value;

// Dynamic fixture table names come only from the closed provider enum.
async fn fixture(kind: RemoteMediaSourceKind) -> (DbPool, ScheduledMedia) {
    let (pool, locker, channel, start) = schedule::tests::fixture().await;
    seed_provider(&pool, kind).await;
    let mut read = schedule::read_locked(
        &pool,
        &locker,
        &channel,
        start,
        start + Duration::minutes(1),
    )
    .await
    .unwrap()
    .unwrap();
    (pool, read.items.remove(0))
}

pub(crate) async fn seed_provider(pool: &DbPool, kind: RemoteMediaSourceKind) {
    let provider = kind.provider_name();
    for sql in [
        format!(r#"INSERT INTO "{provider}MediaSource" ("Id") VALUES (1)"#),
        format!(r#"INSERT INTO "{provider}Library" ("Id", "ShouldSyncItems") VALUES (1, 1)"#),
    ] {
        sqlx::query(crate::db::assert_sql_safe(&sql))
            .execute(pool)
            .await
            .unwrap();
    }
    if kind == RemoteMediaSourceKind::Plex {
        for sql in [
            r#"UPDATE "PlexMediaSource" SET "ClientIdentifier" = 'fixture-server' WHERE "Id" = 1"#,
            r#"INSERT INTO "PlexConnection" ("Id", "PlexMediaSourceId", "IsActive", "Uri") VALUES (1, 1, 1, 'https://media.example.test')"#,
            r#"INSERT INTO "PlexMediaFile" ("Id", "PlexId", "Key") VALUES (1, 11, '/library/parts/11/file.mkv'), (2, 12, '/library/parts/12/file.mkv')"#,
            r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('plex.server_auth_token.fixture-server', '<token>')"#,
        ] {
            sqlx::query(sql).execute(pool).await.unwrap();
        }
    } else {
        let name = provider.to_lowercase();
        for sql in [
            format!(
                r#"INSERT INTO "{provider}Connection" ("Id", "{provider}MediaSourceId", "Address") VALUES (1, 1, 'https://media.example.test/base')"#
            ),
            format!(
                r#"INSERT INTO "{provider}Movie" ("Id", "ItemId") VALUES (10, 'fixture-item')"#
            ),
            format!(
                r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES ('{name}.api_key.https://media.example.test/base', '<api-key>')"#
            ),
        ] {
            sqlx::query(crate::db::assert_sql_safe(&sql))
                .execute(pool)
                .await
                .unwrap();
        }
    }
}

#[tokio::test]
async fn all_providers_fall_back_to_their_authenticated_http_source() {
    for kind in RemoteMediaSourceKind::ALL {
        let (pool, media) = fixture(kind).await;
        let value = serde_json::to_value(resolve(&pool, &media, &[]).await.unwrap()).unwrap();
        assert_eq!(value["source_type"], "http");
        let (uri, header) = match kind {
            RemoteMediaSourceKind::Plex => (
                "https://media.example.test/library/parts/11/file.mkv",
                "X-Plex-Token: <token>",
            ),
            RemoteMediaSourceKind::Jellyfin => (
                "https://media.example.test/base/Videos/fixture-item/stream?static=true",
                "Authorization: MediaBrowser Token=\"<api-key>\"",
            ),
            RemoteMediaSourceKind::Emby => (
                "https://media.example.test/base/Videos/fixture-item/stream?static=true",
                "X-Emby-Token: <api-key>",
            ),
        };
        assert_eq!(value["uri"], uri);
        assert_eq!(value["headers"], serde_json::json!([header]));
        assert_eq!(value["timeout_us"], 10_000_000);
    }
}

#[tokio::test]
async fn local_preference_uses_only_the_owning_servers_replacements() {
    for kind in RemoteMediaSourceKind::ALL {
        let (pool, media) = fixture(kind).await;
        let own = tempfile::tempdir().unwrap();
        let unrelated = tempfile::tempdir().unwrap();
        for folder in [&own, &unrelated] {
            tokio::fs::write(folder.path().join("first.mkv"), b"synthetic")
                .await
                .unwrap();
        }
        let provider = kind.provider_name();
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (2)"#)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query(crate::db::assert_sql_safe(format!(
            r#"INSERT INTO "{provider}MediaSource" ("Id") VALUES (2)"#
        )))
        .execute(&pool)
        .await
        .unwrap();
        let repo = SqliteMediaSourceRepository(pool.clone());
        for (id, folder) in [(2, &unrelated), (1, &own)] {
            repo.save_path_replacements_admin(
                kind,
                id,
                &[PathReplacementAdmin {
                    id: 0,
                    remote_path: "media".to_owned(),
                    local_path: folder.path().to_string_lossy().into_owned(),
                }],
            )
            .await
            .unwrap();
        }
        // Local access must not depend on server connectivity or credentials.
        sqlx::query(r#"DELETE FROM "ConfigElement""#)
            .execute(&pool)
            .await
            .unwrap();
        let value = serde_json::to_value(resolve(&pool, &media, &[]).await.unwrap()).unwrap();
        assert_eq!(value["source_type"], "local");
        assert_eq!(
            value["path"],
            own.path().join("first.mkv").to_str().unwrap()
        );
        tokio::fs::remove_file(own.path().join("first.mkv"))
            .await
            .unwrap();
        assert!(resolve(&pool, &media, &[]).await.is_err());
    }
}

#[tokio::test]
async fn plex_file_binding_and_transport_errors_do_not_leak_values() {
    let (pool, mut media) = fixture(RemoteMediaSourceKind::Plex).await;
    media.media_file_id = 2;
    // A stale file/path binding must not silently stream a different file.
    assert!(resolve(&pool, &media, &[]).await.is_err());
    media.path = "media/second.mkv".to_owned();
    let value: Value = serde_json::to_value(resolve(&pool, &media, &[]).await.unwrap()).unwrap();
    assert_eq!(
        value["uri"],
        "https://media.example.test/library/parts/12/file.mkv"
    );
    sqlx::query(
        r#"UPDATE "PlexMediaFile" SET "Key" = 'https://other.example.test/private' WHERE "Id" = 2"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let Err(error) = resolve(&pool, &media, &[]).await else {
        panic!("must reject a different origin");
    };
    assert!(!error.to_string().contains("other.example.test"));
    assert!(!format!("{error:?}").contains("<token>"));
}

#[test]
fn provider_urls_preserve_base_paths_and_encode_item_segments() {
    assert_eq!(
        video_url("https://media.example.test/base/", "item/with ? space")
            .unwrap()
            .as_str(),
        "https://media.example.test/base/Videos/item%2Fwith%20%3F%20space/stream?static=true"
    );
    for address in [
        "file:///private",
        "https://name:password@media.example.test",
        "https://media.example.test?key=value",
    ] {
        assert!(server_url(address).is_err());
    }
}

#[tokio::test]
async fn global_credentials_are_only_a_single_server_compatibility_fallback() {
    for kind in RemoteMediaSourceKind::ALL {
        let (pool, media) = fixture(kind).await;
        let global_key = match kind {
            RemoteMediaSourceKind::Plex => "plex.current_server_auth_token",
            RemoteMediaSourceKind::Jellyfin => "jellyfin.current_authorization_header",
            RemoteMediaSourceKind::Emby => "emby.current_server_api_key",
        };
        sqlx::query(
            r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, '<global-placeholder>')"#,
        )
        .bind(global_key)
        .execute(&pool)
        .await
        .unwrap();
        let value = serde_json::to_value(resolve(&pool, &media, &[]).await.unwrap()).unwrap();
        assert!(!value.to_string().contains("global-placeholder"));
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" <> ?"#)
            .bind(global_key)
            .execute(&pool)
            .await
            .unwrap();
        assert!(resolve(&pool, &media, &[]).await.is_ok());
        sqlx::query(r#"INSERT INTO "MediaSource" ("Id") VALUES (2)"#)
            .execute(&pool)
            .await
            .unwrap();
        // Table identifier comes only from the closed provider enum.
        let sql = format!(
            r#"INSERT INTO "{}MediaSource" ("Id") VALUES (2)"#,
            kind.provider_name()
        );
        sqlx::query(crate::db::assert_sql_safe(&sql))
            .execute(&pool)
            .await
            .unwrap();
        assert!(resolve(&pool, &media, &[]).await.is_err());
    }
}

#[tokio::test]
async fn episode_identities_are_resolved_without_using_a_movie_identity() {
    for kind in [RemoteMediaSourceKind::Jellyfin, RemoteMediaSourceKind::Emby] {
        let (pool, _) = fixture(kind).await;
        for sql in [
            r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (11, 1, 0), (12, 1, 0), (13, 1, 0)"#,
            r#"INSERT INTO "Show" ("Id") VALUES (11)"#,
            r#"INSERT INTO "Season" ("Id", "ShowId") VALUES (12, 11)"#,
            r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (13, 12)"#,
        ] {
            sqlx::query(sql).execute(&pool).await.unwrap();
        }
        let query = match kind {
            RemoteMediaSourceKind::Jellyfin => {
                r#"INSERT INTO "JellyfinEpisode" ("Id", "ItemId") VALUES (13, 'episode-item')"#
            }
            RemoteMediaSourceKind::Emby => {
                r#"INSERT INTO "EmbyEpisode" ("Id", "ItemId") VALUES (13, 'episode-item')"#
            }
            RemoteMediaSourceKind::Plex => unreachable!(),
        };
        sqlx::query(query).execute(&pool).await.unwrap();
        assert_eq!(
            provider_item_id(&pool, 13, kind).await.unwrap(),
            "episode-item"
        );
        assert_eq!(
            provider_item_id(&pool, 10, kind).await.unwrap(),
            "fixture-item"
        );
        assert!(provider_item_id(&pool, 12, kind).await.is_err());
    }
}

#[test]
fn transport_values_cannot_inject_headers_or_expand_environment_templates() {
    let url = server_url("https://media.example.test/video").unwrap();
    for header in [
        "X-Fixture: <token>\r\nX-Injected: true",
        "X-Fixture: {{VARIABLE}}",
    ] {
        assert!(http_source(&url, header.to_owned()).is_err());
    }
}
