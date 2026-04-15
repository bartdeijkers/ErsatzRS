use async_trait::async_trait;
use nastv_core::repositories::{CollectionRepository, Result};
use nastv_domain::{Collection, EntityId, Playlist, SmartCollection};
use sqlx::{Pool, Sqlite};

use super::common::{fetch_data, from_json, load_ids, to_json};

#[derive(Clone)]
pub struct SqliteCollectionRepository(pub Pool<Sqlite>);

#[async_trait]
impl CollectionRepository for SqliteCollectionRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Collection> {
        let mut collection: Collection = from_json(
            "collection",
            id,
            fetch_data(&self.0, "collections", "collection", id).await?,
        )?;
        collection.item_ids = load_ids(
            &self.0,
            "SELECT id FROM collection_items WHERE collection_id = ? ORDER BY sort_order, id",
            id,
        )
        .await?;
        collection.media_item_ids = load_ids(
            &self.0,
            "SELECT media_item_id FROM collection_items WHERE collection_id = ? ORDER BY sort_order, id",
            id,
        )
        .await?;
        Ok(collection)
    }

    async fn list_for_media_item(&self, item_id: EntityId) -> Result<Vec<Collection>> {
        let rows = sqlx::query_as::<_, (i64, String)>(
            "SELECT c.id, c.data
             FROM collections c
             JOIN collection_items ci ON ci.collection_id = c.id
             WHERE ci.media_item_id = ?
             ORDER BY c.name, c.id",
        )
        .bind(item_id)
        .fetch_all(&self.0)
        .await?;
        rows.into_iter()
            .map(|(id, data)| {
                let mut collection: Collection = from_json("collection", id, data)?;
                collection.media_item_ids.clear();
                collection.item_ids.clear();
                Ok(collection)
            })
            .collect()
    }

    async fn get_smart(&self, id: EntityId) -> Result<SmartCollection> {
        from_json(
            "smart collection",
            id,
            fetch_data(&self.0, "smart_collections", "smart collection", id).await?,
        )
    }

    async fn get_playlist_with_items(&self, id: EntityId) -> Result<Playlist> {
        let mut playlist: Playlist = from_json(
            "playlist",
            id,
            fetch_data(&self.0, "playlists", "playlist", id).await?,
        )?;
        playlist.item_ids = load_ids(
            &self.0,
            "SELECT id FROM playlist_items WHERE playlist_id = ? ORDER BY sort_order, id",
            id,
        )
        .await?;
        Ok(playlist)
    }

    async fn upsert(&self, collection: &Collection) -> Result<()> {
        let mut tx = self.0.begin().await?;
        sqlx::query("INSERT OR REPLACE INTO collections (id, name, collection_type, data) VALUES (?, ?, ?, ?)")
            .bind(collection.id)
            .bind(&collection.name)
            .bind(format!("{:?}", collection.collection_type))
            .bind(to_json(collection)?)
            .execute(&mut *tx)
            .await?;

        sqlx::query("DELETE FROM collection_items WHERE collection_id = ?")
            .bind(collection.id)
            .execute(&mut *tx)
            .await?;
        for (index, media_item_id) in collection.media_item_ids.iter().enumerate() {
            let fallback_id = media_item_id
                .saturating_mul(10_000)
                .saturating_add(i64::try_from(index).unwrap_or(i64::MAX));
            sqlx::query(
                "INSERT OR REPLACE INTO collection_items (id, collection_id, media_item_id, sort_order)
                 VALUES (?, ?, ?, ?)",
            )
            .bind(collection.item_ids.get(index).copied().unwrap_or(fallback_id))
            .bind(collection.id)
            .bind(media_item_id)
            .bind(i32::try_from(index).unwrap_or(i32::MAX))
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}
