use async_trait::async_trait;
use nastv_core::repositories::{LibraryRepository, Result};
use nastv_domain::{EntityId, Library};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, load_ids, not_found, replace_ids};

#[derive(Clone)]
pub struct SqliteLibraryRepository(pub Pool<Sqlite>);

#[async_trait]
impl LibraryRepository for SqliteLibraryRepository {
    async fn get_with_paths(&self, id: EntityId) -> Result<Library> {
        let row = sqlx::query("SELECT * FROM libraries WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("library", id, error))?;
        let mut library = map_library(&row)?;
        library.path_ids = load_ids(
            &self.0,
            "SELECT id FROM library_paths WHERE library_id = ? ORDER BY id",
            id,
        )
        .await?;
        library.folder_ids = load_ids(
            &self.0,
            "SELECT id FROM library_folders WHERE library_id = ? ORDER BY id",
            id,
        )
        .await?;
        Ok(library)
    }

    async fn list_for_source(&self, source_id: EntityId) -> Result<Vec<Library>> {
        let rows = sqlx::query("SELECT * FROM libraries WHERE media_source_id = ? ORDER BY name, id")
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
            "INSERT OR REPLACE INTO libraries (
                id, name, media_kind, media_source_id, media_source_kind
             ) VALUES (?, ?, ?, ?, ?)",
        )
        .bind(library.id)
        .bind(&library.name)
        .bind(format!("{:?}", library.media_kind))
        .bind(library.media_source_id)
        .bind(format!("{:?}", library.media_source_kind))
        .execute(&mut *tx)
        .await?;

        sqlx::query("DELETE FROM local_libraries WHERE id = ?")
            .bind(library.id)
            .execute(&mut *tx)
            .await?;
        if matches!(library.media_source_kind, nastv_domain::MediaSourceKind::Local) {
            sqlx::query("INSERT OR REPLACE INTO local_libraries (id) VALUES (?)")
                .bind(library.id)
                .execute(&mut *tx)
                .await?;
        }

        replace_ids(
            &mut tx,
            "DELETE FROM library_paths WHERE library_id = ?",
            "INSERT OR REPLACE INTO library_paths (library_id, id, path) VALUES (?, ?, '')",
            library.id,
            &library.path_ids,
        )
        .await?;
        replace_ids(
            &mut tx,
            "DELETE FROM library_folders WHERE library_id = ?",
            "INSERT OR REPLACE INTO library_folders (library_id, id, path) VALUES (?, ?, '')",
            library.id,
            &library.folder_ids,
        )
        .await?;

        tx.commit().await?;
        Ok(())
    }
}

fn map_library(row: &SqliteRow) -> Result<Library> {
    Ok(Library {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        media_kind: decode_enum("library media_kind", &row.try_get::<String, _>("media_kind")?)?,
        media_source_id: row.try_get("media_source_id")?,
        media_source_kind: decode_enum(
            "library media_source_kind",
            &row.try_get::<String, _>("media_source_kind")?,
        )?,
        folder_ids: Vec::new(),
        path_ids: Vec::new(),
    })
}
