use async_trait::async_trait;
use nastv_core::repositories::{MediaItemRepository, Result};
use nastv_domain::{EntityId, MediaItem};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, not_found};

#[derive(Clone)]
pub struct SqliteMediaItemRepository(pub Pool<Sqlite>);

#[async_trait]
impl MediaItemRepository for SqliteMediaItemRepository {
    async fn get_with_version(&self, id: EntityId) -> Result<MediaItem> {
        let row = sqlx::query("SELECT * FROM media_items WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("media item", id, error))?;
        map_media_item(&row)
    }

    async fn list_for_library_path(&self, path_id: EntityId) -> Result<Vec<MediaItem>> {
        let rows = sqlx::query("SELECT * FROM media_items WHERE library_path_id = ? ORDER BY id")
            .bind(path_id)
            .fetch_all(&self.0)
            .await?;
        rows.iter().map(map_media_item).collect()
    }

    async fn find_by_path(&self, path: &str) -> Result<MediaItem> {
        let row = sqlx::query(
            "SELECT mi.*
             FROM media_items mi
             JOIN media_versions mv ON mv.media_item_id = mi.id
             JOIN media_files mf ON mf.media_version_id = mv.id
             WHERE mf.path = ?",
        )
        .bind(path)
        .fetch_one(&self.0)
        .await?;
        map_media_item(&row)
    }

    async fn upsert(&self, item: &MediaItem) -> Result<()> {
        sqlx::query(
            "INSERT OR REPLACE INTO media_items (id, library_path_id, state) VALUES (?, ?, ?)",
        )
        .bind(item.id)
        .bind(item.library_path_id)
        .bind(format!("{:?}", item.state))
        .execute(&self.0)
        .await?;
        Ok(())
    }
}

fn map_media_item(row: &SqliteRow) -> Result<MediaItem> {
    Ok(MediaItem {
        id: row.try_get("id")?,
        library_path_id: row.try_get("library_path_id")?,
        collection_ids: Vec::new(),
        trakt_list_item_ids: Vec::new(),
        state: decode_enum("media item state", &row.try_get::<String, _>("state")?)?,
    })
}
