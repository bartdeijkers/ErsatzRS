use crate::db::DbRow;
use async_trait::async_trait;
use ersatzrs_core::repositories::{MediaItemRepository, Result};
use ersatzrs_domain::{EntityId, MediaItem};
use sqlx::{Pool, Row};

use super::common::{decode_enum_column, encode_enum, not_found};

#[derive(Clone)]
pub struct SqliteMediaItemRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl MediaItemRepository for SqliteMediaItemRepository {
    async fn get_with_version(&self, id: EntityId) -> Result<MediaItem> {
        let row = sqlx::query(r#"SELECT * FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("media item", id, error))?;
        map_media_item(&row)
    }

    async fn list_for_library_path(&self, path_id: EntityId) -> Result<Vec<MediaItem>> {
        let rows =
            sqlx::query(r#"SELECT * FROM "MediaItem" WHERE "LibraryPathId" = ? ORDER BY "Id""#)
                .bind(path_id)
                .fetch_all(&self.0)
                .await?;
        rows.iter().map(map_media_item).collect()
    }

    async fn find_by_path(&self, path: &str) -> Result<MediaItem> {
        let row = sqlx::query(
            r#"SELECT "mi".*
             FROM "MediaItem" AS "mi"
             JOIN "MediaVersion" AS "mv" ON (
                 "mv"."MovieId" = "mi"."Id"
                 OR "mv"."EpisodeId" = "mi"."Id"
                 OR "mv"."MusicVideoId" = "mi"."Id"
                 OR "mv"."OtherVideoId" = "mi"."Id"
                 OR "mv"."SongId" = "mi"."Id"
                 OR "mv"."ImageId" = "mi"."Id"
                 OR "mv"."RemoteStreamId" = "mi"."Id"
             )
             JOIN "MediaFile" AS "mf" ON "mf"."MediaVersionId" = "mv"."Id"
             WHERE "mf"."Path" = ?"#,
        )
        .bind(path)
        .fetch_one(&self.0)
        .await?;
        map_media_item(&row)
    }

    async fn upsert(&self, item: &MediaItem) -> Result<()> {
        sqlx::query(
            r#"REPLACE INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, ?, ?)"#,
        )
        .bind(item.id)
        .bind(item.library_path_id)
        .bind(encode_enum(item.state))
        .execute(&self.0)
        .await?;
        Ok(())
    }
}

fn map_media_item(row: &DbRow) -> Result<MediaItem> {
    Ok(MediaItem {
        id: row.try_get("Id")?,
        library_path_id: row.try_get("LibraryPathId")?,
        collection_ids: Vec::new(),
        trakt_list_item_ids: Vec::new(),
        state: decode_enum_column("media item state", row, "State")?,
    })
}
