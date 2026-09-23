mod common;

use ersatzrs_core::repositories::{EmbyConnectionConfig, JellyfinConnectionConfig};
use ersatzrs_infra::{emby, jellyfin};
use httpmock::prelude::*;
use serde_json::json;
use sqlx::SqlitePool;

#[derive(Clone, Copy)]
enum Provider {
    Emby,
    Jellyfin,
}

async fn sync(provider: Provider, show: bool, pool: &SqlitePool, address: String, library: i64) {
    let library_item_id = format!("library-{library}");
    let result = match provider {
        Provider::Emby => {
            let config = EmbyConnectionConfig {
                address,
                api_key: "test".into(),
            };
            let service = emby::EmbyService::new();
            if show {
                emby::sync_emby_show_library(
                    pool,
                    &service,
                    &config,
                    1,
                    library,
                    &library_item_id,
                    true,
                )
                .await
            } else {
                emby::sync_emby_movie_library(
                    pool,
                    &service,
                    &config,
                    1,
                    library,
                    &library_item_id,
                    true,
                )
                .await
            }
        }
        Provider::Jellyfin => {
            let config = JellyfinConnectionConfig {
                address,
                authorization_header: "test".into(),
            };
            let service = jellyfin::JellyfinService::new();
            if show {
                jellyfin::sync_jellyfin_show_library(
                    pool,
                    &service,
                    &config,
                    1,
                    library,
                    &library_item_id,
                    true,
                )
                .await
            } else {
                jellyfin::sync_jellyfin_movie_library(
                    pool,
                    &service,
                    &config,
                    1,
                    library,
                    &library_item_id,
                    true,
                )
                .await
            }
        }
    };
    result.unwrap();
}

async fn sync_cast(
    provider: Provider,
    show: bool,
    pool: &SqlitePool,
    library: i64,
    names: &[&str],
) {
    let server = MockServer::start_async().await;
    let item_id = format!("item-{library}");
    let details_path = if show {
        format!("/Shows/{item_id}/Seasons")
    } else {
        format!("/Items/{item_id}/PlaybackInfo")
    };
    let people: Vec<_> = names
        .iter()
        .map(|name| {
            json!({
                "Name": name, "Type": "Actor", "Role": "Lead"
            })
        })
        .collect();
    let items = server
        .mock_async(|when, then| {
            when.method(GET)
                .path("/Items")
                .query_param("includeItemTypes", if show { "Series" } else { "Movie" });
            then.status(200).json_body(json!({
                "TotalRecordCount": 1,
                "Items": [{
                    "Id": item_id, "Name": "Synthetic title", "Etag": "unchanged-etag",
                    "Path": format!("synthetic/movie-{library}.mkv"), "LocationType": "FileSystem",
                    "DateCreated": "2026-04-17T10:00:00Z", "RunTimeTicks": 120_000_000,
                    "People": people
                }]
            }));
        })
        .await;
    let details = server
        .mock_async(|when, then| {
            if show && matches!(provider, Provider::Jellyfin) {
                when.method(GET)
                    .path("/Items")
                    .query_param("includeItemTypes", "Season")
                    .query_param("parentId", &item_id);
            } else {
                when.method(GET).path(&details_path);
            }
            then.status(200).json_body(if show {
                json!({"TotalRecordCount": 0, "Items": []})
            } else {
                json!({"MediaSources": [{"RunTimeTicks": 120_000_000, "MediaStreams": [{
                    "Type": "Video", "Codec": "h264", "Width": 320, "Height": 180,
                    "Index": 0, "IsDefault": true
                }]}]})
            });
        })
        .await;
    sync(provider, show, pool, server.base_url(), library).await;
    items.assert_async().await;
    details.assert_async().await;
}

type ActorRow = (i64, String, Option<i64>, Option<i64>, Option<i64>);

async fn actors(pool: &SqlitePool) -> Vec<ActorRow> {
    sqlx::query_as(
        r#"SELECT "Id", "Name", "MovieMetadataId", "ShowMetadataId", "ArtworkId"
           FROM "Actor" ORDER BY "Name""#,
    )
    .fetch_all(pool)
    .await
    .unwrap()
}

async fn attach_portrait(pool: &SqlitePool, actor_id: i64) -> i64 {
    let artwork_id = sqlx::query(
        r#"INSERT INTO "Artwork" ("ArtworkKind", "Path", "DateAdded", "DateUpdated")
           VALUES (0, 'synthetic/actor.png', '2026-04-17', '2026-04-17')"#,
    )
    .execute(pool)
    .await
    .unwrap()
    .last_insert_rowid();
    sqlx::query(r#"UPDATE "Actor" SET "ArtworkId" = ? WHERE "Id" = ?"#)
        .bind(artwork_id)
        .bind(actor_id)
        .execute(pool)
        .await
        .unwrap();
    artwork_id
}

async fn verify_refresh(provider: Provider, show: bool) {
    let pool = common::test_db().await;
    // A different library supplies an independent owner whose actor and portrait
    // must survive every refresh of the subject library.
    sync_cast(provider, show, &pool, 2, &["Unrelated"]).await;
    let unrelated_id = actors(&pool).await[0].0;
    let artwork_id = attach_portrait(&pool, unrelated_id).await;
    let unrelated = actors(&pool).await.remove(0);

    sync_cast(provider, show, &pool, 1, &["Old", "Retained"]).await;
    let initial = actors(&pool).await;
    assert_eq!(initial.len(), 3);
    for row in &initial {
        assert!(if show {
            row.3.is_some()
        } else {
            row.2.is_some()
        });
    }
    let old_id = initial.iter().find(|row| row.1 == "Old").unwrap().0;
    // Each actor owns a distinct portrait (IX_Actor_ArtworkId is unique).
    let old_artwork_id = attach_portrait(&pool, old_id).await;

    for names in [&["Retained"][..], &["Retained"][..], &[][..], &[][..]] {
        sync_cast(provider, show, &pool, 1, names).await;
        let rows = actors(&pool).await;
        assert_eq!(
            rows.len(),
            names.len() + 1,
            "no detached or repeated actor rows"
        );
        assert!(!rows.iter().any(|row| row.0 == old_id));
        assert!(
            rows.contains(&unrelated),
            "unrelated owner and artwork reference survive"
        );
        for row in rows.iter().filter(|row| row.0 != unrelated_id) {
            assert_eq!(row.1, "Retained");
            assert!(if show {
                row.3.is_some()
            } else {
                row.2.is_some()
            });
        }
        let portraits: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Artwork" WHERE "Id" = ?"#)
            .bind(artwork_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(portraits, 1);
    }
    let deleted = ersatzrs_infra::artwork_cleanup::clean_orphaned_artwork(&pool, 100_000)
        .await
        .unwrap();
    assert_eq!(
        deleted.actors_deleted, 0,
        "refresh already removed stale actors"
    );
    assert_eq!(
        deleted.artwork_deleted, 1,
        "only the removed actor's portrait is orphaned"
    );
    let remaining: Vec<i64> = sqlx::query_scalar(r#"SELECT "Id" FROM "Artwork" ORDER BY "Id""#)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, vec![artwork_id]);
    assert!(!remaining.contains(&old_artwork_id));
    assert_eq!(actors(&pool).await, vec![unrelated]);
}

#[tokio::test]
async fn emby_movie_refresh_physically_removes_old_actors() {
    verify_refresh(Provider::Emby, false).await;
}

#[tokio::test]
async fn emby_show_refresh_physically_removes_old_actors() {
    verify_refresh(Provider::Emby, true).await;
}

#[tokio::test]
async fn jellyfin_movie_refresh_physically_removes_old_actors() {
    verify_refresh(Provider::Jellyfin, false).await;
}

#[tokio::test]
async fn jellyfin_show_refresh_physically_removes_old_actors() {
    verify_refresh(Provider::Jellyfin, true).await;
}
