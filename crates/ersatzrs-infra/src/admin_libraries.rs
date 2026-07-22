//! Typed admin read models for the library route family.

use ersatzrs_core::repositories::Result;
use serde::{Deserialize, Serialize};

use crate::db::DbPool;
use crate::repositories::common::map_data_access_error;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdminLibraryRow {
    #[sqlx(rename = "Id")]
    pub id: i64,
    #[sqlx(rename = "Name")]
    pub name: String,
    #[sqlx(rename = "Kind")]
    pub kind: String,
    #[sqlx(rename = "MediaKind")]
    pub media_kind: i64,
    #[sqlx(rename = "SourceName")]
    pub source_name: Option<String>,
    #[sqlx(rename = "MediaSourceId")]
    pub media_source_id: i64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, sqlx::FromRow)]
pub struct AdminExternalCollectionRow {
    #[sqlx(rename = "MediaSourceId")]
    pub media_source_id: i64,
    #[sqlx(rename = "Kind")]
    pub kind: String,
    #[sqlx(rename = "ServerName")]
    pub server_name: Option<String>,
}

/// Load every configured library without fabricating values after decode
/// failures.
///
/// # Errors
///
/// Returns a repository database or decode error when the query or typed row
/// projection fails.
pub async fn load_libraries(pool: &DbPool) -> Result<Vec<AdminLibraryRow>> {
    sqlx::query_as::<_, AdminLibraryRow>(
        r#"SELECT
               l."Id" AS "Id",
               COALESCE(l."Name", '') AS "Name",
               l."MediaKind" AS "MediaKind",
               l."MediaSourceId" AS "MediaSourceId",
               CASE
                   WHEN ll."Id" IS NOT NULL THEN 'Local'
                   WHEN pl."Id" IS NOT NULL THEN 'Plex'
                   WHEN jl."Id" IS NOT NULL THEN 'Jellyfin'
                   WHEN el."Id" IS NOT NULL THEN 'Emby'
                   ELSE 'Unknown'
               END AS "Kind",
               CASE
                   WHEN pl."Id" IS NOT NULL THEN pms."ServerName"
                   WHEN jl."Id" IS NOT NULL THEN jms."ServerName"
                   WHEN el."Id" IS NOT NULL THEN ems."ServerName"
                   ELSE NULL
               END AS "SourceName"
           FROM "Library" l
           LEFT JOIN "LocalLibrary"    ll  ON ll."Id"  = l."Id"
           LEFT JOIN "PlexLibrary"     pl  ON pl."Id"  = l."Id"
           LEFT JOIN "JellyfinLibrary" jl  ON jl."Id"  = l."Id"
           LEFT JOIN "EmbyLibrary"     el  ON el."Id"  = l."Id"
           LEFT JOIN "PlexMediaSource"     pms ON pms."Id" = l."MediaSourceId"
           LEFT JOIN "JellyfinMediaSource" jms ON jms."Id" = l."MediaSourceId"
           LEFT JOIN "EmbyMediaSource"     ems ON ems."Id" = l."MediaSourceId"
           ORDER BY "Kind", "SourceName", "Name""#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| map_data_access_error("admin library row", &error))
}

/// Load external-collection sources without dropping rows whose projection is
/// invalid.
///
/// # Errors
///
/// Returns a repository database or decode error when the query or typed row
/// projection fails.
pub async fn load_external_collections(pool: &DbPool) -> Result<Vec<AdminExternalCollectionRow>> {
    sqlx::query_as::<_, AdminExternalCollectionRow>(
        r#"SELECT DISTINCT
               l."MediaSourceId" AS "MediaSourceId",
               CASE
                   WHEN pl."Id" IS NOT NULL THEN 'Plex'
                   WHEN jl."Id" IS NOT NULL THEN 'Jellyfin'
                   WHEN el."Id" IS NOT NULL THEN 'Emby'
                   ELSE NULL
               END AS "Kind",
               COALESCE(pms."ServerName", jms."ServerName", ems."ServerName") AS "ServerName"
           FROM "Library" l
           LEFT JOIN "PlexLibrary"     pl  ON pl."Id"  = l."Id"
           LEFT JOIN "JellyfinLibrary" jl  ON jl."Id"  = l."Id"
           LEFT JOIN "EmbyLibrary"     el  ON el."Id"  = l."Id"
           LEFT JOIN "PlexMediaSource"     pms ON pms."Id" = l."MediaSourceId"
           LEFT JOIN "JellyfinMediaSource" jms ON jms."Id" = l."MediaSourceId"
           LEFT JOIN "EmbyMediaSource"     ems ON ems."Id" = l."MediaSourceId"
           WHERE (pl."ShouldSyncItems" = 1
               OR jl."ShouldSyncItems" = 1
               OR el."ShouldSyncItems" = 1)
           ORDER BY "Kind", "ServerName", "MediaSourceId""#,
    )
    .fetch_all(pool)
    .await
    .map_err(|error| map_data_access_error("admin external collection row", &error))
}

#[cfg(all(test, not(feature = "mysql")))]
mod tests {
    use super::*;
    use ersatzrs_core::repositories::RepositoryError;

    #[tokio::test]
    async fn closed_pool_is_reported_as_repository_database_error() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .connect("sqlite::memory:")
            .await
            .expect("test pool");
        pool.close().await;

        assert!(matches!(
            load_libraries(&pool).await,
            Err(RepositoryError::Unavailable { .. })
        ));
        assert!(matches!(
            load_external_collections(&pool).await,
            Err(RepositoryError::Unavailable { .. })
        ));
    }
}
