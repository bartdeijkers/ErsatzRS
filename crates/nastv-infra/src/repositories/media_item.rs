use async_trait::async_trait;
use nastv_core::repositories::{MediaItemRepository, Result};
use nastv_domain::{EntityId, MediaItem};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, to_json};

#[derive(Clone)]
pub struct SqliteMediaItemRepository(pub Pool<Sqlite>);

#[async_trait]
impl MediaItemRepository for SqliteMediaItemRepository {
    async fn get_with_version(&self, id: EntityId) -> Result<MediaItem> {
        from_json(
            "media item",
            id,
            fetch_data(&self.0, "media_items", "media item", id).await?,
        )
    }

    async fn list_for_library_path(&self, path_id: EntityId) -> Result<Vec<MediaItem>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, data FROM media_items WHERE library_path_id = ? ORDER BY id",
        )
        .bind(path_id)
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| from_json("media item", id, data))
            .collect()
    }

    async fn find_by_path(&self, path: &str) -> Result<MediaItem> {
        let row = sqlx::query_as::<_, (i64, String)>(
            "SELECT mi.id, mi.data
             FROM media_items mi
             JOIN media_versions mv ON mv.media_item_id = mi.id
             JOIN media_files mf ON mf.media_version_id = mv.id
             WHERE mf.path = ?",
        )
        .bind(path)
        .fetch_one(&self.0)
        .await?;
        from_json("media item", row.0, row.1)
    }

    async fn upsert(&self, item: &MediaItem) -> Result<()> {
        sqlx::query("INSERT OR REPLACE INTO media_items (id, library_path_id, state, data) VALUES (?, ?, ?, ?)")
            .bind(item.id)
            .bind(item.library_path_id)
            .bind(format!("{:?}", item.state))
            .bind(to_json(item)?)
            .execute(&self.0)
            .await?;
        Ok(())
    }
}
