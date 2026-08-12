use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{
    AdminExternalCollectionRow, AdminLibraryRow, LibraryRepository, LibraryScanTarget,
    MediaStateRevision, RepositoryError, Result,
};
use ersatzrs_domain::{EntityId, Library, MediaSourceKind};
use sqlx::{Pool, Row};

use super::common::upsert_sql;
use super::common::{decode_enum_column, encode_enum, load_ids, not_found, replace_ids};

#[derive(Clone)]
pub struct SqliteLibraryRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl LibraryRepository for SqliteLibraryRepository {
    async fn get_with_paths(&self, id: EntityId) -> Result<Library> {
        let row = sqlx::query(r#"SELECT * FROM "Library" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("library", id, error))?;
        let mut library = map_library(&row)?;
        // Determine media_source_kind by checking sub-type tables
        library.media_source_kind = resolve_media_source_kind(&self.0, id).await?;
        library.path_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "LibraryPath" WHERE "LibraryId" = ? ORDER BY "Id""#,
            id,
        )
        .await?;
        library.folder_ids = load_ids(
            &self.0,
            r#"SELECT "lf"."Id"
               FROM "LibraryFolder" AS "lf"
               JOIN "LibraryPath" AS "lp" ON "lp"."Id" = "lf"."LibraryPathId"
               WHERE "lp"."LibraryId" = ?
               ORDER BY "lf"."Id""#,
            id,
        )
        .await?;
        Ok(library)
    }

    async fn list_for_source(&self, source_id: EntityId) -> Result<Vec<Library>> {
        let rows = sqlx::query(
            r#"SELECT * FROM "Library" WHERE "MediaSourceId" = ? ORDER BY "Name", "Id""#,
        )
        .bind(source_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                let mut library = map_library(row)?;
                library.folder_ids.clear();
                library.path_ids.clear();
                Ok(library)
            })
            .collect()
    }

    async fn upsert(&self, library: &Library) -> Result<()> {
        let mut tx = self.0.begin().await.repository()?;
        sqlx::query(upsert_sql!(
            r#"INSERT INTO "Library" (
                "Id", "Name", "MediaKind", "MediaSourceId"
             ) VALUES (?, ?, ?, ?)"#,
            &[&["Id"]],
        ))
        .bind(library.id)
        .bind(&library.name)
        .bind(encode_enum(library.media_kind))
        .bind(library.media_source_id)
        .execute(&mut *tx)
        .await
        .repository()?;

        sqlx::query(r#"DELETE FROM "LocalLibrary" WHERE "Id" = ?"#)
            .bind(library.id)
            .execute(&mut *tx)
            .await
            .repository()?;
        if matches!(library.media_source_kind, MediaSourceKind::Local) {
            sqlx::query(r#"REPLACE INTO "LocalLibrary" ("Id") VALUES (?)"#)
                .bind(library.id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }

        replace_ids(
            &mut tx,
            r#"DELETE FROM "LibraryPath" WHERE "LibraryId" = ?"#,
            r#"REPLACE INTO "LibraryPath" ("LibraryId", "Id", "Path") VALUES (?, ?, '')"#,
            library.id,
            &library.path_ids,
        )
        .await?;

        // LibraryFolder requires a LibraryPathId; we cannot insert folders without a valid path.
        // For upsert purposes we skip folder insertion if we don't have path context.

        tx.commit().await.repository()?;
        Ok(())
    }

    async fn get_scan_target(&self, id: EntityId) -> Result<LibraryScanTarget> {
        let row = sqlx::query(
            r#"SELECT l."MediaKind",
                      CASE
                          WHEN ll."Id" IS NOT NULL THEN 'Local'
                          WHEN pl."Id" IS NOT NULL THEN 'Plex'
                          WHEN jl."Id" IS NOT NULL THEN 'Jellyfin'
                          WHEN el."Id" IS NOT NULL THEN 'Emby'
                          ELSE NULL
                      END AS "Provider",
                      COALESCE(
                          pl."ShouldSyncItems",
                          jl."ShouldSyncItems",
                          el."ShouldSyncItems",
                          1
                      ) AS "ShouldSyncItems"
               FROM "Library" l
               LEFT JOIN "LocalLibrary" ll ON ll."Id" = l."Id"
               LEFT JOIN "PlexLibrary" pl ON pl."Id" = l."Id"
               LEFT JOIN "JellyfinLibrary" jl ON jl."Id" = l."Id"
               LEFT JOIN "EmbyLibrary" el ON el."Id" = l."Id"
               WHERE l."Id" = ?
               LIMIT 1"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "library",
            id,
        })?;

        let provider = match row
            .try_get::<Option<String>, _>("Provider")
            .repository()?
            .as_deref()
        {
            Some("Local") => MediaSourceKind::Local,
            Some("Plex") => MediaSourceKind::Plex,
            Some("Jellyfin") => MediaSourceKind::Jellyfin,
            Some("Emby") => MediaSourceKind::Emby,
            value => {
                return Err(RepositoryError::Decode {
                    kind: "library provider",
                    value: value.unwrap_or("<missing>").to_owned(),
                });
            }
        };

        Ok(LibraryScanTarget {
            library_id: id,
            provider,
            media_kind: decode_enum_column("library media_kind", &row, "MediaKind")?,
            should_sync_items: row.try_get::<i64, _>("ShouldSyncItems").repository()? != 0,
        })
    }

    async fn show_belongs_to_library(
        &self,
        library_id: EntityId,
        show_id: EntityId,
    ) -> Result<bool> {
        sqlx::query_scalar(
            r#"SELECT EXISTS(
                   SELECT 1
                   FROM "Show" s
                   JOIN "MediaItem" mi ON mi."Id" = s."Id"
                   JOIN "LibraryPath" lp ON lp."Id" = mi."LibraryPathId"
                   WHERE s."Id" = ? AND lp."LibraryId" = ?
               )"#,
        )
        .bind(show_id)
        .bind(library_id)
        .fetch_one(&self.0)
        .await
        .repository()
    }

    async fn media_state_revision_admin(&self) -> Result<MediaStateRevision> {
        let latest_library_scan =
            sqlx::query_scalar::<_, Option<String>>(r#"SELECT MAX("LastScan") FROM "Library""#)
                .fetch_one(&self.0)
                .await
                .repository()?;
        let latest_path_scan =
            sqlx::query_scalar::<_, Option<String>>(r#"SELECT MAX("LastScan") FROM "LibraryPath""#)
                .fetch_one(&self.0)
                .await
                .repository()?;
        let unavailable_count = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "MediaItem" WHERE "State" IN (1, 2, 3)"#,
        )
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(MediaStateRevision {
            latest_library_scan,
            latest_path_scan,
            unavailable_count: u64::try_from(unavailable_count).map_err(|error| {
                RepositoryError::Decode {
                    kind: "unavailable media count",
                    value: error.to_string(),
                }
            })?,
        })
    }

    async fn list_admin(&self) -> Result<Vec<AdminLibraryRow>> {
        crate::admin_libraries::load_libraries(&self.0).await
    }

    async fn list_external_collections_admin(&self) -> Result<Vec<AdminExternalCollectionRow>> {
        crate::admin_libraries::load_external_collections(&self.0).await
    }
}

async fn resolve_media_source_kind(
    pool: &Pool<crate::db::Db>,
    library_id: EntityId,
) -> Result<MediaSourceKind> {
    let is_local: bool = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "LocalLibrary" WHERE "Id" = ?)"#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if is_local {
        return Ok(MediaSourceKind::Local);
    }
    let is_plex: bool = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "PlexLibrary" WHERE "Id" = ?)"#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if is_plex {
        return Ok(MediaSourceKind::Plex);
    }
    let is_jellyfin: bool = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "JellyfinLibrary" WHERE "Id" = ?)"#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if is_jellyfin {
        return Ok(MediaSourceKind::Jellyfin);
    }
    Ok(MediaSourceKind::Emby)
}

fn map_library(row: &DbRow) -> Result<Library> {
    Ok(Library {
        id: row.try_get("Id").repository()?,
        name: row.try_get("Name").repository()?,
        media_kind: decode_enum_column("library media_kind", row, "MediaKind")?,
        media_source_id: row.try_get("MediaSourceId").repository()?,
        // Kind is resolved separately via sub-type table lookup
        media_source_kind: MediaSourceKind::Local,
        folder_ids: Vec::new(),
        path_ids: Vec::new(),
    })
}
