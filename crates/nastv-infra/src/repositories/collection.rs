use async_trait::async_trait;
use nastv_core::repositories::{CollectionRepository, Result};
use nastv_domain::{Collection, EntityId, Playlist, SmartCollection};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, load_ids, not_found};

#[derive(Clone)]
pub struct SqliteCollectionRepository(pub Pool<Sqlite>);

#[async_trait]
impl CollectionRepository for SqliteCollectionRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Collection> {
        let row = sqlx::query("SELECT * FROM collections WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("collection", id, error))?;
        let mut collection = map_collection(&row)?;
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
        let rows = sqlx::query(
            "SELECT c.*
             FROM collections c
             JOIN collection_items ci ON ci.collection_id = c.id
             WHERE ci.media_item_id = ?
             ORDER BY c.name, c.id",
        )
        .bind(item_id)
        .fetch_all(&self.0)
        .await?;
        rows.iter()
            .map(|row| {
                let mut collection = map_collection(row)?;
                collection.media_item_ids.clear();
                collection.item_ids.clear();
                Ok(collection)
            })
            .collect()
    }

    async fn get_smart(&self, id: EntityId) -> Result<SmartCollection> {
        let row = sqlx::query("SELECT * FROM smart_collections WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("smart collection", id, error))?;
        Ok(SmartCollection {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            query: row.try_get("query")?,
        })
    }

    async fn get_playlist_with_items(&self, id: EntityId) -> Result<Playlist> {
        let row = sqlx::query("SELECT * FROM playlists WHERE id = ?")
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("playlist", id, error))?;
        let mut playlist = Playlist {
            id: row.try_get("id")?,
            name: row.try_get("name")?,
            item_ids: Vec::new(),
            group_ids: load_ids(
                &self.0,
                "SELECT id FROM playlist_groups WHERE playlist_id = ? ORDER BY sort_order, id",
                id,
            )
            .await?,
        };
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
        sqlx::query("INSERT OR REPLACE INTO collections (id, name, collection_type) VALUES (?, ?, ?)")
            .bind(collection.id)
            .bind(&collection.name)
            .bind(format!("{:?}", collection.collection_type))
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

fn map_collection(row: &SqliteRow) -> Result<Collection> {
    Ok(Collection {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        collection_type: decode_enum(
            "collection type",
            &row.try_get::<String, _>("collection_type")?,
        )?,
        media_item_ids: Vec::new(),
        item_ids: Vec::new(),
    })
}
