use async_trait::async_trait;
use ersatzrs_core::repositories::{CollectionRepository, Result};
use ersatzrs_domain::{Collection, EntityId, Playlist, SmartCollection};
use sqlx::sqlite::SqliteRow;
use sqlx::{Pool, Row, Sqlite};

use super::common::{decode_enum, load_ids, not_found};

#[derive(Clone)]
pub struct SqliteCollectionRepository(pub Pool<Sqlite>);

#[async_trait]
impl CollectionRepository for SqliteCollectionRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Collection> {
        let row = sqlx::query(r#"SELECT * FROM "Collection" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("collection", id, error))?;
        let mut collection = map_collection(&row)?;
        // ErsatzTV CollectionItem has no Id or sort_order - use MediaItemId as the item id
        collection.item_ids = load_ids(
            &self.0,
            r#"SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ? ORDER BY "MediaItemId""#,
            id,
        )
        .await?;
        collection.media_item_ids = load_ids(
            &self.0,
            r#"SELECT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ? ORDER BY "MediaItemId""#,
            id,
        )
        .await?;
        Ok(collection)
    }

    async fn list_for_media_item(&self, item_id: EntityId) -> Result<Vec<Collection>> {
        let rows = sqlx::query(
            r#"SELECT c.*
             FROM "Collection" c
             JOIN "CollectionItem" ci ON ci."CollectionId" = c."Id"
             WHERE ci."MediaItemId" = ?
             ORDER BY c."Name", c."Id""#,
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
        let row = sqlx::query(r#"SELECT * FROM "SmartCollection" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("smart collection", id, error))?;
        Ok(SmartCollection {
            id: row.try_get("Id")?,
            name: row.try_get("Name")?,
            query: row.try_get("Query")?,
        })
    }

    async fn get_playlist_with_items(&self, id: EntityId) -> Result<Playlist> {
        let row = sqlx::query(r#"SELECT * FROM "Playlist" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("playlist", id, error))?;
        let mut playlist = Playlist {
            id: row.try_get("Id")?,
            name: row.try_get("Name")?,
            item_ids: Vec::new(),
            group_ids: load_ids(
                &self.0,
                r#"SELECT "Id" FROM "PlaylistGroup" WHERE "Id" = (SELECT "PlaylistGroupId" FROM "Playlist" WHERE "Id" = ?) ORDER BY "Id""#,
                id,
            )
            .await?,
        };
        playlist.item_ids = load_ids(
            &self.0,
            r#"SELECT "Id" FROM "PlaylistItem" WHERE "PlaylistId" = ? ORDER BY "Index", "Id""#,
            id,
        )
        .await?;
        Ok(playlist)
    }

    async fn upsert(&self, collection: &Collection) -> Result<()> {
        let mut tx = self.0.begin().await?;
        // ErsatzTV Collection has no collection_type column; UseCustomPlaybackOrder is always false
        sqlx::query(
            r#"INSERT OR REPLACE INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (?, ?, 0)"#,
        )
        .bind(collection.id)
        .bind(&collection.name)
        .execute(&mut *tx)
        .await?;

        sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ?"#)
            .bind(collection.id)
            .execute(&mut *tx)
            .await?;
        for media_item_id in &collection.media_item_ids {
            sqlx::query(
                r#"INSERT OR REPLACE INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (?, ?)"#,
            )
            .bind(collection.id)
            .bind(media_item_id)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        Ok(())
    }
}

fn map_collection(row: &SqliteRow) -> Result<Collection> {
    Ok(Collection {
        id: row.try_get("Id")?,
        name: row.try_get("Name")?,
        // ErsatzTV Collection has no collection_type; default to Collection
        collection_type: ersatzrs_domain::CollectionType::Collection,
        media_item_ids: Vec::new(),
        item_ids: Vec::new(),
    })
}
