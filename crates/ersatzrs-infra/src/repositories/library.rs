use async_trait::async_trait;
use ersatzrs_core::repositories::{LibraryRepository, Result};
use ersatzrs_domain::{EntityId, Library, MediaSourceKind};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum_column, encode_enum, load_ids, not_found, replace_ids};

#[derive(Clone)]
pub struct SqliteLibraryRepository(pub Pool<Sqlite>);

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
        .await?;
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
        let mut tx = self.0.begin().await?;
        sqlx::query(
            r#"INSERT OR REPLACE INTO "Library" (
                "Id", "Name", "MediaKind", "MediaSourceId"
             ) VALUES (?, ?, ?, ?)"#,
        )
        .bind(library.id)
        .bind(&library.name)
        .bind(encode_enum(library.media_kind))
        .bind(library.media_source_id)
        .execute(&mut *tx)
        .await?;

        sqlx::query(r#"DELETE FROM "LocalLibrary" WHERE "Id" = ?"#)
            .bind(library.id)
            .execute(&mut *tx)
            .await?;
        if matches!(library.media_source_kind, MediaSourceKind::Local) {
            sqlx::query(r#"INSERT OR REPLACE INTO "LocalLibrary" ("Id") VALUES (?)"#)
                .bind(library.id)
                .execute(&mut *tx)
                .await?;
        }

        replace_ids(
            &mut tx,
            r#"DELETE FROM "LibraryPath" WHERE "LibraryId" = ?"#,
            r#"INSERT OR REPLACE INTO "LibraryPath" ("LibraryId", "Id", "Path") VALUES (?, ?, '')"#,
            library.id,
            &library.path_ids,
        )
        .await?;

        // LibraryFolder requires a LibraryPathId; we cannot insert folders without a valid path.
        // For upsert purposes we skip folder insertion if we don't have path context.

        tx.commit().await?;
        Ok(())
    }
}

async fn resolve_media_source_kind(
    pool: &Pool<Sqlite>,
    library_id: EntityId,
) -> Result<MediaSourceKind> {
    let is_local: bool = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "LocalLibrary" WHERE "Id" = ?)"#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await?;
    if is_local {
        return Ok(MediaSourceKind::Local);
    }
    let is_plex: bool = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "PlexLibrary" WHERE "Id" = ?)"#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await?;
    if is_plex {
        return Ok(MediaSourceKind::Plex);
    }
    let is_jellyfin: bool = sqlx::query_scalar::<_, bool>(
        r#"SELECT EXISTS(SELECT 1 FROM "JellyfinLibrary" WHERE "Id" = ?)"#,
    )
    .bind(library_id)
    .fetch_one(pool)
    .await?;
    if is_jellyfin {
        return Ok(MediaSourceKind::Jellyfin);
    }
    Ok(MediaSourceKind::Emby)
}

fn map_library(row: &SqliteRow) -> Result<Library> {
    Ok(Library {
        id: row.try_get("Id")?,
        name: row.try_get("Name")?,
        media_kind: decode_enum_column("library media_kind", row, "MediaKind")?,
        media_source_id: row.try_get("MediaSourceId")?,
        // Kind is resolved separately via sub-type table lookup
        media_source_kind: MediaSourceKind::Local,
        folder_ids: Vec::new(),
        path_ids: Vec::new(),
    })
}
