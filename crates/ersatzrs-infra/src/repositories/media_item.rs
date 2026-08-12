use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{MediaItemRepository, Result};
use ersatzrs_domain::{EntityId, MediaItem};
use sqlx::{Pool, Row};

use super::common::upsert_sql;
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
                .await
                .repository()?;
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
        .await
        .repository()?;
        map_media_item(&row)
    }

    async fn upsert(&self, item: &MediaItem) -> Result<()> {
        sqlx::query(upsert_sql!(
            r#"INSERT INTO "MediaItem" (
                "Id", "LibraryPathId", "State", "ContentKind", "SubKind", "ParentMediaItemId"
            ) VALUES (?, ?, ?, ?, ?, ?)"#,
            &[&["Id"]],
        ))
        .bind(item.id)
        .bind(item.library_path_id)
        .bind(encode_enum(item.state))
        .bind(encode_enum(item.content_kind))
        .bind(item.sub_kind.as_deref())
        .bind(item.parent_media_item_id)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn delete_admin(&self, ids: &[EntityId]) -> Result<u64> {
        if ids.is_empty() {
            return Ok(0);
        }
        let placeholders = ids.iter().map(|_| "?").collect::<Vec<_>>().join(",");
        let sql = format!(r#"DELETE FROM "MediaItem" WHERE "Id" IN ({placeholders})"#);
        let mut query = sqlx::query(crate::db::assert_sql_safe(&sql));
        for id in ids {
            query = query.bind(*id);
        }
        Ok(query.execute(&self.0).await.repository()?.rows_affected())
    }

    async fn empty_trash_admin(&self) -> Result<u64> {
        Ok(sqlx::query(r#"DELETE FROM "MediaItem" WHERE "State" = 1"#)
            .execute(&self.0)
            .await
            .repository()?
            .rows_affected())
    }
}

fn map_media_item(row: &DbRow) -> Result<MediaItem> {
    Ok(MediaItem {
        id: row.try_get("Id").repository()?,
        library_path_id: row.try_get("LibraryPathId").repository()?,
        collection_ids: Vec::new(),
        trakt_list_item_ids: Vec::new(),
        state: decode_enum_column("media item state", row, "State")?,
        content_kind: decode_enum_column("content kind", row, "ContentKind")?,
        sub_kind: row.try_get("SubKind").repository()?,
        parent_media_item_id: row.try_get("ParentMediaItemId").repository()?,
    })
}
