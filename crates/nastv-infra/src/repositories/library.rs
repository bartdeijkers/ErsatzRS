use async_trait::async_trait;
use nastv_core::repositories::{LibraryRepository, Result};
use nastv_domain::{EntityId, Library};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, load_ids, replace_ids, to_json};

#[derive(Clone)]
pub struct SqliteLibraryRepository(pub Pool<Sqlite>);

#[async_trait]
impl LibraryRepository for SqliteLibraryRepository {
    async fn get_with_paths(&self, id: EntityId) -> Result<Library> {
        let mut library: Library = from_json(
            "library",
            id,
            fetch_data(&self.0, "libraries", "library", id).await?,
        )?;
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
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM libraries WHERE media_source_id = ? ORDER BY name, id",
        )
        .bind(source_id)
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| {
                let mut library: Library = from_json("library", id, data)?;
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
                id, name, media_kind, media_source_id, media_source_kind, data
             ) VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(library.id)
        .bind(&library.name)
        .bind(format!("{:?}", library.media_kind))
        .bind(library.media_source_id)
        .bind(format!("{:?}", library.media_source_kind))
        .bind(to_json(library)?)
        .execute(&mut *tx)
        .await?;

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
