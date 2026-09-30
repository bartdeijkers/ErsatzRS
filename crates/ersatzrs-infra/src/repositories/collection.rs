use crate::db::DbRow;
use crate::repositories::SqlxResultExt as _;
use async_trait::async_trait;
use ersatzrs_core::repositories::{
    CollectionAdminRow, CollectionDetailAdmin, CollectionItemCardAdmin, CollectionMemberAdmin,
    CollectionRepository, MediaItemSuggestionAdmin, MultiCollectionAdmin, MultiCollectionAdminRow,
    MultiCollectionEditorOptions, MultiCollectionItem, MultiCollectionSelectOption,
    PlaybackOverrides, PlaylistAdminRow, PlaylistEditorAdmin, PlaylistEditorOptions,
    PlaylistGroupAdminRow, PlaylistItemAdmin, PlaylistItemInsert, PlaylistItemOption,
    RerunCollectionAdmin, RerunCollectionAdminRow, RerunCollectionEditorOptions,
    RerunCollectionReferenceKind, RerunCollectionSelectOption, Result, SmartCollectionAdminRow,
};
use ersatzrs_domain::{Collection, EntityId, PinnedStreamSelection, Playlist, SmartCollection};
use sqlx::{Pool, Row};

use super::common::{decode_bool_column, load_ids, not_found};

use super::common::upsert_sql;
const COLLECTION_TYPE_COLLECTION: i32 = 0;
const COLLECTION_TYPE_SMART_COLLECTION: i32 = 5;
const COLLECTION_TYPE_SHOW: i32 = 1;
const COLLECTION_TYPE_SEASON: i32 = 2;
const COLLECTION_TYPE_ARTIST: i32 = 3;
const COLLECTION_TYPE_MULTI_COLLECTION: i32 = 4;
const COLLECTION_TYPE_MOVIE: i32 = 10;
const COLLECTION_TYPE_EPISODE: i32 = 20;
const COLLECTION_TYPE_MUSIC_VIDEO: i32 = 30;
const COLLECTION_TYPE_OTHER_VIDEO: i32 = 40;
const COLLECTION_TYPE_SONG: i32 = 50;
const COLLECTION_TYPE_IMAGE: i32 = 60;
const COLLECTION_TYPE_REMOTE_STREAM: i32 = 70;

#[derive(Clone)]
pub struct SqliteCollectionRepository(pub Pool<crate::db::Db>);

#[async_trait]
#[allow(clippy::too_many_lines)]
impl CollectionRepository for SqliteCollectionRepository {
    async fn get_with_items(&self, id: EntityId) -> Result<Collection> {
        let row = sqlx::query(r#"SELECT * FROM "Collection" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("collection", id, error))?;
        let mut collection = map_collection(&row)?;
        // ADR 0048 allows duplicate rows per media item (distinct playback
        // overrides); the domain Collection tracks distinct media item ids.
        collection.item_ids = load_ids(
            &self.0,
            r#"SELECT DISTINCT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ? ORDER BY "MediaItemId""#,
            id,
        )
        .await?;
        collection.media_item_ids = load_ids(
            &self.0,
            r#"SELECT DISTINCT "MediaItemId" FROM "CollectionItem" WHERE "CollectionId" = ? ORDER BY "MediaItemId""#,
            id,
        )
        .await?;
        Ok(collection)
    }

    async fn list_for_media_item(&self, item_id: EntityId) -> Result<Vec<Collection>> {
        let rows = sqlx::query(
            r#"SELECT "c".*
             FROM "Collection" AS "c"
             JOIN "CollectionItem" AS "ci" ON "ci"."CollectionId" = "c"."Id"
             WHERE "ci"."MediaItemId" = ?
             ORDER BY "c"."Name", "c"."Id""#,
        )
        .bind(item_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
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
            id: row.try_get("Id").repository()?,
            name: row.try_get("Name").repository()?,
            query: row.try_get("Query").repository()?,
        })
    }

    async fn get_playlist_with_items(&self, id: EntityId) -> Result<Playlist> {
        let row = sqlx::query(r#"SELECT * FROM "Playlist" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .map_err(|error| not_found("playlist", id, error))?;
        let mut playlist = Playlist {
            id: row.try_get("Id").repository()?,
            name: row.try_get("Name").repository()?,
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
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        // ErsatzTV Collection has no collection_type column; UseCustomPlaybackOrder is always false
        sqlx::query(upsert_sql!(
            r#"INSERT INTO "Collection" ("Id", "Name", "UseCustomPlaybackOrder") VALUES (?, ?, 0)"#,
            &[&["Id"], &["Name"]],
        ))
        .bind(collection.id)
        .bind(&collection.name)
        .execute(&mut *tx)
        .await
        .repository()?;

        sqlx::query(r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ?"#)
            .bind(collection.id)
            .execute(&mut *tx)
            .await
            .repository()?;
        // CollectionItem is keyed by a surrogate Id (ADR 0048), so dedupe the
        // media item ids here to keep upsert idempotent for default rows.
        let media_item_ids: std::collections::BTreeSet<_> =
            collection.media_item_ids.iter().copied().collect();
        for media_item_id in media_item_ids {
            sqlx::query(
                r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId") VALUES (?, ?)"#,
            )
            .bind(collection.id)
            .bind(media_item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }

        tx.commit().await.repository()?;
        Ok(())
    }

    async fn list_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<CollectionAdminRow>, u64)> {
        let search = search.trim();
        let like = format!("%{search}%");
        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "Collection"
               WHERE (? = '' OR "Name" LIKE ?)"#,
        )
        .bind(search)
        .bind(&like)
        .fetch_one(&self.0)
        .await
        .repository()?;
        let rows = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "Collection"
               WHERE (? = '' OR "Name" LIKE ?)
               ORDER BY "Name"
               LIMIT ? OFFSET ?"#,
        )
        .bind(search)
        .bind(&like)
        .bind(i64::from(page_size))
        .bind(i64::from(page) * i64::from(page_size))
        .fetch_all(&self.0)
        .await
        .repository()?;
        let rows = rows
            .iter()
            .map(|row| {
                Ok(CollectionAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((rows, u64::try_from(total.max(0)).unwrap_or_default()))
    }

    async fn get_admin(&self, id: EntityId) -> Result<Option<CollectionAdminRow>> {
        let row = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "Collection"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| {
            Ok(CollectionAdminRow {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .transpose()
    }

    async fn admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "Collection"
               WHERE LOWER("Name") = LOWER(?) AND "Id" != ?"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn save_admin(&self, id: EntityId, name: &str) -> Result<EntityId> {
        if id == 0 {
            let mut tx = crate::db::begin_write(&self.0).await.repository()?;
            let result = sqlx::query(
                r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder")
                   VALUES (?, 0)"#,
            )
            .bind(name)
            .execute(&mut *tx)
            .await
            .repository()?;
            let id = inserted_id(&mut tx, result).await?;
            tx.commit().await.repository()?;
            Ok(id)
        } else {
            let result = sqlx::query(r#"UPDATE "Collection" SET "Name" = ? WHERE "Id" = ?"#)
                .bind(name)
                .bind(id)
                .execute(&self.0)
                .await
                .repository()?;
            if result.rows_affected() == 0 {
                Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                    kind: "collection",
                    id,
                })
            } else {
                Ok(id)
            }
        }
    }

    async fn create_with_default_item_admin(
        &self,
        name: &str,
        media_item_id: EntityId,
    ) -> Result<EntityId> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let media_item_exists: Option<EntityId> =
            sqlx::query_scalar(r#"SELECT "Id" FROM "MediaItem" WHERE "Id" = ?"#)
                .bind(media_item_id)
                .fetch_optional(&mut *tx)
                .await
                .repository()?;
        if media_item_exists.is_none() {
            return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "media item",
                id: media_item_id,
            });
        }

        let result = sqlx::query(
            r#"INSERT INTO "Collection" ("Name", "UseCustomPlaybackOrder")
               VALUES (?, 0)"#,
        )
        .bind(name)
        .execute(&mut *tx)
        .await
        .repository()?;
        let collection_id = inserted_id(&mut tx, result).await?;
        sqlx::query(
            r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId")
               VALUES (?, ?)"#,
        )
        .bind(collection_id)
        .bind(media_item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(collection_id)
    }

    async fn delete_admin(&self, id: EntityId) -> Result<()> {
        crate::provider_collections::delete_admin(&self.0, id).await
    }

    async fn add_default_item(
        &self,
        collection_id: EntityId,
        media_item_id: EntityId,
    ) -> Result<()> {
        sqlx::query(
            r#"INSERT INTO "CollectionItem" ("CollectionId", "MediaItemId")
               SELECT ?, ?
               WHERE NOT EXISTS (
                   SELECT 1 FROM "CollectionItem"
                   WHERE "CollectionId" = ? AND "MediaItemId" = ?
                     AND "PreferredAudioLanguageCode" IS NULL
                     AND "PreferredAudioTitle" IS NULL
                     AND "PreferredSubtitleLanguageCode" IS NULL
                     AND "SubtitleMode" IS NULL
               )"#,
        )
        .bind(collection_id)
        .bind(media_item_id)
        .bind(collection_id)
        .bind(media_item_id)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn remove_item(&self, collection_id: EntityId, media_item_id: EntityId) -> Result<()> {
        sqlx::query(
            r#"DELETE FROM "CollectionItem"
               WHERE "CollectionId" = ? AND "MediaItemId" = ?"#,
        )
        .bind(collection_id)
        .bind(media_item_id)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn playout_ids_using(&self, collection_id: EntityId) -> Result<Vec<EntityId>> {
        sqlx::query_scalar(
            r#"SELECT DISTINCT "PlayoutId"
               FROM "PlayoutProgramScheduleAnchor"
               WHERE "CollectionId" = ?
               ORDER BY "PlayoutId""#,
        )
        .bind(collection_id)
        .fetch_all(&self.0)
        .await
        .repository()
    }

    async fn list_smart_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<SmartCollectionAdminRow>, u64)> {
        let search = search.trim();
        let like = format!("%{search}%");
        // ErsatzTV counts all rows before applying the search query.
        let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SmartCollection""#)
            .fetch_one(&self.0)
            .await
            .repository()?;
        let rows = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                      COALESCE("Query", '') AS "Query"
               FROM "SmartCollection"
               WHERE (? = '' OR "Name" LIKE ?)
               ORDER BY "Name"
               LIMIT ? OFFSET ?"#,
        )
        .bind(search)
        .bind(&like)
        .bind(i64::from(page_size))
        .bind(i64::from(page) * i64::from(page_size))
        .fetch_all(&self.0)
        .await
        .repository()?;
        let rows = rows
            .iter()
            .map(|row| {
                Ok(SmartCollectionAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                    query: row.try_get("Query").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((rows, u64::try_from(total.max(0)).unwrap_or_default()))
    }

    async fn get_smart_admin(&self, id: EntityId) -> Result<Option<SmartCollectionAdminRow>> {
        let row = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                      COALESCE("Query", '') AS "Query"
               FROM "SmartCollection"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| {
            Ok(SmartCollectionAdminRow {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
                query: row.try_get("Query").repository()?,
            })
        })
        .transpose()
    }

    async fn smart_admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "SmartCollection"
               WHERE LOWER("Name") = LOWER(?) AND "Id" != ?"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn save_smart_admin(&self, id: EntityId, name: &str, query: &str) -> Result<EntityId> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let duplicate: bool = sqlx::query_scalar(
            r#"SELECT EXISTS(SELECT 1 FROM "SmartCollection" WHERE LOWER("Name") = LOWER(?) AND "Id" <> ?)"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&mut *tx)
        .await
        .repository()?;
        if duplicate {
            return Err(ersatzrs_core::repositories::RepositoryError::Conflict(
                "SmartCollection name must be unique".into(),
            ));
        }
        if id == 0 {
            let result =
                sqlx::query(r#"INSERT INTO "SmartCollection" ("Name", "Query") VALUES (?, ?)"#)
                    .bind(name)
                    .bind(query)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let id = inserted_id(&mut tx, result).await?;
            tx.commit().await.repository()?;
            Ok(id)
        } else {
            let result = sqlx::query(
                r#"UPDATE "SmartCollection"
                   SET "Name" = ?, "Query" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(name)
            .bind(query)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
            if result.rows_affected() == 0 {
                Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                    kind: "smart collection",
                    id,
                })
            } else {
                tx.commit().await.repository()?;
                Ok(id)
            }
        }
    }

    async fn delete_smart_admin(&self, id: EntityId) -> Result<()> {
        let deleted = sqlx::query(r#"DELETE FROM "SmartCollection" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if deleted.rows_affected() == 0 {
            return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "smart collection",
                id,
            });
        }
        Ok(())
    }

    async fn playout_ids_using_smart(&self, id: EntityId) -> Result<Vec<EntityId>> {
        sqlx::query_scalar(
            r#"SELECT DISTINCT "PlayoutId"
               FROM "PlayoutProgramScheduleAnchor"
               WHERE "SmartCollectionId" = ?
               ORDER BY "PlayoutId""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()
    }

    async fn list_multi_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<MultiCollectionAdminRow>, u64)> {
        let search = search.trim();
        let like = format!("%{search}%");
        let total: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "MultiCollection"
               WHERE (? = '' OR "Name" LIKE ?)"#,
        )
        .bind(search)
        .bind(&like)
        .fetch_one(&self.0)
        .await
        .repository()?;
        let rows = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "MultiCollection"
               WHERE (? = '' OR "Name" LIKE ?)
               ORDER BY "Name"
               LIMIT ? OFFSET ?"#,
        )
        .bind(search)
        .bind(&like)
        .bind(i64::from(page_size))
        .bind(i64::from(page) * i64::from(page_size))
        .fetch_all(&self.0)
        .await
        .repository()?;
        let rows = rows
            .iter()
            .map(|row| {
                Ok(MultiCollectionAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((rows, u64::try_from(total.max(0)).unwrap_or_default()))
    }

    async fn get_multi_admin(&self, id: EntityId) -> Result<Option<MultiCollectionAdmin>> {
        let row = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "MultiCollection"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };

        let mut items = load_multi_collection_items(&self.0, id).await?;
        items.extend(load_multi_smart_collection_items(&self.0, id).await?);
        items.sort_by_key(|item| item.name.to_lowercase());
        Ok(Some(MultiCollectionAdmin {
            id: row.try_get("Id").repository()?,
            name: row.try_get("Name").repository()?,
            items,
        }))
    }

    async fn get_multi_editor_options(&self) -> Result<MultiCollectionEditorOptions> {
        let collection_rows = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                      COALESCE("UseCustomPlaybackOrder", 0) AS "UseCustomPlaybackOrder"
               FROM "Collection"
               ORDER BY "Name""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        let smart_rows = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "SmartCollection"
               ORDER BY "Name""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        Ok(MultiCollectionEditorOptions {
            collections: collection_rows
                .iter()
                .map(|row| {
                    Ok(MultiCollectionSelectOption {
                        id: row.try_get("Id").repository()?,
                        name: row.try_get("Name").repository()?,
                        collection_type: COLLECTION_TYPE_COLLECTION,
                        use_custom_playback_order: row
                            .try_get::<i64, _>("UseCustomPlaybackOrder")
                            .repository()?
                            != 0,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
            smart_collections: smart_rows
                .iter()
                .map(|row| {
                    Ok(MultiCollectionSelectOption {
                        id: row.try_get("Id").repository()?,
                        name: row.try_get("Name").repository()?,
                        collection_type: COLLECTION_TYPE_SMART_COLLECTION,
                        use_custom_playback_order: false,
                    })
                })
                .collect::<Result<Vec<_>>>()?,
        })
    }

    async fn multi_admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "MultiCollection"
               WHERE "Name" = ? AND "Id" <> ?"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn save_multi_admin(
        &self,
        id: EntityId,
        name: &str,
        items: &[MultiCollectionItem],
        replace_items: bool,
    ) -> Result<EntityId> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let id = if id == 0 {
            let result = sqlx::query(r#"INSERT INTO "MultiCollection" ("Name") VALUES (?)"#)
                .bind(name)
                .execute(&mut *tx)
                .await
                .repository()?;
            inserted_id(&mut tx, result).await?
        } else {
            let result = sqlx::query(r#"UPDATE "MultiCollection" SET "Name" = ? WHERE "Id" = ?"#)
                .bind(name)
                .bind(id)
                .execute(&mut *tx)
                .await
                .repository()?;
            if result.rows_affected() == 0 {
                return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                    kind: "multi collection",
                    id,
                });
            }
            id
        };

        if replace_items {
            sqlx::query(r#"DELETE FROM "MultiCollectionItem" WHERE "MultiCollectionId" = ?"#)
                .bind(id)
                .execute(&mut *tx)
                .await
                .repository()?;
            sqlx::query(r#"DELETE FROM "MultiCollectionSmartItem" WHERE "MultiCollectionId" = ?"#)
                .bind(id)
                .execute(&mut *tx)
                .await
                .repository()?;
            for item in items {
                if let Some(collection_id) = item.collection_id {
                    sqlx::query(
                        r#"INSERT INTO "MultiCollectionItem" (
                               "MultiCollectionId", "CollectionId",
                               "ScheduleAsGroup", "PlaybackOrder"
                           ) VALUES (?, ?, ?, ?)"#,
                    )
                    .bind(id)
                    .bind(collection_id)
                    .bind(item.schedule_as_group)
                    .bind(item.playback_order)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                } else if let Some(smart_collection_id) = item.smart_collection_id {
                    sqlx::query(
                        r#"INSERT INTO "MultiCollectionSmartItem" (
                               "MultiCollectionId", "SmartCollectionId",
                               "ScheduleAsGroup", "PlaybackOrder"
                           ) VALUES (?, ?, ?, ?)"#,
                    )
                    .bind(id)
                    .bind(smart_collection_id)
                    .bind(item.schedule_as_group)
                    .bind(item.playback_order)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                }
            }
        }
        tx.commit().await.repository()?;
        Ok(id)
    }

    async fn delete_multi_admin(&self, id: EntityId) -> Result<()> {
        let result = sqlx::query(r#"DELETE FROM "MultiCollection" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "multi collection",
                id,
            })
        } else {
            Ok(())
        }
    }

    async fn playout_ids_using_multi(&self, id: EntityId) -> Result<Vec<EntityId>> {
        sqlx::query_scalar(
            r#"SELECT DISTINCT "PlayoutId"
               FROM "PlayoutProgramScheduleAnchor"
               WHERE "MultiCollectionId" = ?
               ORDER BY "PlayoutId""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()
    }

    async fn list_rerun_admin(
        &self,
        search: &str,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<RerunCollectionAdminRow>, u64)> {
        let search = search.trim();
        let like = format!("%{search}%");
        // ErsatzTV counts all rows before applying the search query.
        let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "RerunCollection""#)
            .fetch_one(&self.0)
            .await
            .repository()?;
        let rows = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name"
               FROM "RerunCollection"
               WHERE (? = '' OR "Name" LIKE ?)
               ORDER BY "Name"
               LIMIT ? OFFSET ?"#,
        )
        .bind(search)
        .bind(&like)
        .bind(i64::from(page_size))
        .bind(i64::from(page) * i64::from(page_size))
        .fetch_all(&self.0)
        .await
        .repository()?;
        let rows = rows
            .iter()
            .map(|row| {
                Ok(RerunCollectionAdminRow {
                    id: row.try_get("Id").repository()?,
                    name: row.try_get("Name").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Ok((rows, u64::try_from(total.max(0)).unwrap_or_default()))
    }

    async fn get_rerun_admin(&self, id: EntityId) -> Result<Option<RerunCollectionAdmin>> {
        let row = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name", "CollectionType",
                      "CollectionId", "MediaItemId", "MultiCollectionId",
                      "SmartCollectionId", "FirstRunPlaybackOrder", "RerunPlaybackOrder"
               FROM "RerunCollection"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| map_rerun_collection(&row)).transpose()
    }

    async fn get_rerun_editor_options(&self) -> Result<RerunCollectionEditorOptions> {
        Ok(RerunCollectionEditorOptions {
            collections: load_rerun_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "Collection" ORDER BY "Name""#,
            )
            .await?,
            multi_collections: load_rerun_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "MultiCollection" ORDER BY "Name""#,
            )
            .await?,
            smart_collections: load_rerun_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "SmartCollection" ORDER BY "Name""#,
            )
            .await?,
            shows: load_rerun_options(
                &self.0,
                r#"SELECT "s"."Id" AS "Id",
                          COALESCE("sm"."Title", '???') || ' (' ||
                          COALESCE(CAST("sm"."Year" AS TEXT), '???') || ')' AS "Name"
                   FROM "Show" AS "s"
                   LEFT JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
                   ORDER BY "Name""#,
            )
            .await?,
            seasons: load_rerun_options(
                &self.0,
                r#"SELECT "se"."Id" AS "Id",
                          COALESCE("sm"."Title", '???') || ' (' ||
                          COALESCE(CAST("sm"."Year" AS TEXT), '???') || ') - ' ||
                          CASE WHEN "se"."SeasonNumber" = 0 THEN 'Specials'
                               ELSE 'Season ' || CAST("se"."SeasonNumber" AS TEXT) END AS "Name"
                   FROM "Season" AS "se"
                   LEFT JOIN "Show" AS "s" ON "s"."Id" = "se"."ShowId"
                   LEFT JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
                   ORDER BY "Name""#,
            )
            .await?,
            artists: load_rerun_options(
                &self.0,
                r#"SELECT "a"."Id" AS "Id", COALESCE("am"."Title", '???') AS "Name"
                   FROM "Artist" AS "a"
                   LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "a"."Id"
                   ORDER BY "Name""#,
            )
            .await?,
        })
    }

    async fn rerun_admin_name_exists(&self, id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "RerunCollection"
               WHERE "Name" = ? AND "Id" <> ?"#,
        )
        .bind(name)
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn rerun_reference_exists(
        &self,
        kind: RerunCollectionReferenceKind,
        id: EntityId,
    ) -> Result<bool> {
        let sql = match kind {
            RerunCollectionReferenceKind::Collection => {
                r#"SELECT COUNT(*) FROM "Collection" WHERE "Id" = ?"#
            }
            RerunCollectionReferenceKind::Show => r#"SELECT COUNT(*) FROM "Show" WHERE "Id" = ?"#,
            RerunCollectionReferenceKind::Season => {
                r#"SELECT COUNT(*) FROM "Season" WHERE "Id" = ?"#
            }
            RerunCollectionReferenceKind::Artist => {
                r#"SELECT COUNT(*) FROM "Artist" WHERE "Id" = ?"#
            }
            RerunCollectionReferenceKind::MultiCollection => {
                r#"SELECT COUNT(*) FROM "MultiCollection" WHERE "Id" = ?"#
            }
            RerunCollectionReferenceKind::SmartCollection => {
                r#"SELECT COUNT(*) FROM "SmartCollection" WHERE "Id" = ?"#
            }
        };
        let count: i64 = sqlx::query_scalar(sql)
            .bind(id)
            .fetch_one(&self.0)
            .await
            .repository()?;
        Ok(count > 0)
    }

    async fn save_rerun_admin(&self, form: &RerunCollectionAdmin) -> Result<EntityId> {
        if form.id == 0 {
            let mut tx = crate::db::begin_write(&self.0).await.repository()?;
            let result = sqlx::query(
                r#"INSERT INTO "RerunCollection" (
                       "Name", "CollectionType", "CollectionId", "MediaItemId",
                       "MultiCollectionId", "SmartCollectionId",
                       "FirstRunPlaybackOrder", "RerunPlaybackOrder"
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(&form.name)
            .bind(form.collection_type)
            .bind(form.collection_id)
            .bind(form.media_item_id)
            .bind(form.multi_collection_id)
            .bind(form.smart_collection_id)
            .bind(form.first_run_playback_order)
            .bind(form.rerun_playback_order)
            .execute(&mut *tx)
            .await
            .repository()?;
            let id = inserted_id(&mut tx, result).await?;
            tx.commit().await.repository()?;
            Ok(id)
        } else {
            let result = sqlx::query(
                r#"UPDATE "RerunCollection"
                      SET "Name" = ?, "CollectionType" = ?, "CollectionId" = ?,
                          "MediaItemId" = ?, "MultiCollectionId" = ?,
                          "SmartCollectionId" = ?, "FirstRunPlaybackOrder" = ?,
                          "RerunPlaybackOrder" = ?
                    WHERE "Id" = ?"#,
            )
            .bind(&form.name)
            .bind(form.collection_type)
            .bind(form.collection_id)
            .bind(form.media_item_id)
            .bind(form.multi_collection_id)
            .bind(form.smart_collection_id)
            .bind(form.first_run_playback_order)
            .bind(form.rerun_playback_order)
            .bind(form.id)
            .execute(&self.0)
            .await
            .repository()?;
            if result.rows_affected() == 0 {
                Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                    kind: "rerun collection",
                    id: form.id,
                })
            } else {
                Ok(form.id)
            }
        }
    }

    async fn delete_rerun_admin(&self, id: EntityId) -> Result<()> {
        let result = sqlx::query(r#"DELETE FROM "RerunCollection" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "rerun collection",
                id,
            })
        } else {
            Ok(())
        }
    }

    async fn playout_ids_using_rerun(&self, id: EntityId) -> Result<Vec<EntityId>> {
        sqlx::query_scalar(
            r#"SELECT DISTINCT "PlayoutId"
               FROM "PlayoutProgramScheduleAnchor"
               WHERE "RerunCollectionId" = ?
               ORDER BY "PlayoutId""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()
    }

    async fn list_playlist_groups_admin(&self) -> Result<Vec<PlaylistGroupAdminRow>> {
        let rows = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                      COALESCE("IsSystem", 0) AS "IsSystem"
               FROM "PlaylistGroup"
               ORDER BY COALESCE("IsSystem", 0) DESC, "Name""#,
        )
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut groups = Vec::with_capacity(rows.len());
        for row in rows {
            let id: EntityId = row.try_get("Id").repository()?;
            groups.push(PlaylistGroupAdminRow {
                id,
                name: row.try_get("Name").repository()?,
                is_system: decode_bool_column(&row, "IsSystem")?,
                playlists: load_playlists_for_group(&self.0, id).await?,
            });
        }
        Ok(groups)
    }

    async fn get_playlist_group_admin(
        &self,
        id: EntityId,
    ) -> Result<Option<PlaylistGroupAdminRow>> {
        let row = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                      COALESCE("IsSystem", 0) AS "IsSystem"
               FROM "PlaylistGroup"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(PlaylistGroupAdminRow {
            id: row.try_get("Id").repository()?,
            name: row.try_get("Name").repository()?,
            is_system: decode_bool_column(&row, "IsSystem")?,
            playlists: load_playlists_for_group(&self.0, id).await?,
        }))
    }

    async fn create_playlist_group_admin(&self, name: &str) -> Result<PlaylistGroupAdminRow> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result =
            sqlx::query(r#"INSERT INTO "PlaylistGroup" ("Name", "IsSystem") VALUES (?, 0)"#)
                .bind(name)
                .execute(&mut *tx)
                .await
                .repository()?;
        let id = inserted_id(&mut tx, result).await?;
        tx.commit().await.repository()?;
        Ok(PlaylistGroupAdminRow {
            id,
            name: name.to_owned(),
            is_system: false,
            playlists: Vec::new(),
        })
    }

    async fn playlist_name_exists(&self, group_id: EntityId, name: &str) -> Result<bool> {
        let count: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "Playlist"
               WHERE "PlaylistGroupId" = ? AND LOWER("Name") = LOWER(?)"#,
        )
        .bind(group_id)
        .bind(name)
        .fetch_one(&self.0)
        .await
        .repository()?;
        Ok(count > 0)
    }

    async fn create_playlist_admin(
        &self,
        group_id: EntityId,
        name: &str,
    ) -> Result<PlaylistAdminRow> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "Playlist" ("Name", "PlaylistGroupId", "IsSystem")
               VALUES (?, ?, 0)"#,
        )
        .bind(name)
        .bind(group_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        let id = inserted_id(&mut tx, result).await?;
        tx.commit().await.repository()?;
        Ok(PlaylistAdminRow {
            id,
            playlist_group_id: group_id,
            name: name.to_owned(),
            is_system: false,
        })
    }

    async fn delete_playlist_group_admin(&self, id: EntityId) -> Result<()> {
        let result = sqlx::query(r#"DELETE FROM "PlaylistGroup" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "playlist group",
                id,
            })
        } else {
            Ok(())
        }
    }

    async fn delete_playlist_admin(&self, id: EntityId) -> Result<()> {
        let result = sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
            .bind(id)
            .execute(&self.0)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "playlist",
                id,
            })
        } else {
            Ok(())
        }
    }

    async fn get_playlist_editor_admin(&self, id: EntityId) -> Result<Option<PlaylistEditorAdmin>> {
        let row = sqlx::query(
            r#"SELECT "Id", "PlaylistGroupId", COALESCE("Name", '') AS "Name",
                      COALESCE("IsSystem", 0) AS "IsSystem"
               FROM "Playlist"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        Ok(Some(PlaylistEditorAdmin {
            id: row.try_get("Id").repository()?,
            playlist_group_id: row.try_get("PlaylistGroupId").repository()?,
            name: row.try_get("Name").repository()?,
            is_system: decode_bool_column(&row, "IsSystem")?,
            items: load_playlist_items(&self.0, id).await?,
        }))
    }

    async fn get_playlist_editor_options(&self) -> Result<PlaylistEditorOptions> {
        Ok(PlaylistEditorOptions {
            collections: load_playlist_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "Collection" ORDER BY "Name" LIMIT 500"#,
            )
            .await?,
            multi_collections: load_playlist_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "MultiCollection" ORDER BY "Name" LIMIT 500"#,
            )
            .await?,
            smart_collections: load_playlist_options(
                &self.0,
                r#"SELECT "Id", COALESCE("Name", '') AS "Name"
                   FROM "SmartCollection" ORDER BY "Name" LIMIT 500"#,
            )
            .await?,
            shows: load_playlist_options(
                &self.0,
                r#"SELECT "s"."Id", COALESCE("sm"."Title", '') AS "Name"
                   FROM "Show" AS "s"
                   JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
                   ORDER BY COALESCE("sm"."SortTitle", "sm"."Title", '') LIMIT 500"#,
            )
            .await?,
            seasons: load_playlist_options(
                &self.0,
                r#"SELECT "se"."Id", COALESCE("shm"."Title", '') || ' - ' ||
                          COALESCE("sem"."Title", 'Season ' ||
                          CAST("se"."SeasonNumber" AS TEXT)) AS "Name"
                   FROM "Season" AS "se"
                   JOIN "Show" AS "sh" ON "sh"."Id" = "se"."ShowId"
                   JOIN "ShowMetadata" AS "shm" ON "shm"."ShowId" = "sh"."Id"
                   LEFT JOIN "SeasonMetadata" AS "sem" ON "sem"."SeasonId" = "se"."Id"
                   ORDER BY COALESCE("shm"."SortTitle", "shm"."Title", ''),
                            "se"."SeasonNumber" LIMIT 500"#,
            )
            .await?,
            artists: load_playlist_options(
                &self.0,
                r#"SELECT "a"."Id", COALESCE("am"."Title", '') AS "Name"
                   FROM "Artist" AS "a"
                   JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "a"."Id"
                   ORDER BY COALESCE("am"."SortTitle", "am"."Title", '') LIMIT 500"#,
            )
            .await?,
            movies: load_playlist_options(
                &self.0,
                r#"SELECT "m"."Id", COALESCE("mm"."Title", '') AS "Name"
                   FROM "Movie" AS "m"
                   JOIN "MovieMetadata" AS "mm" ON "mm"."MovieId" = "m"."Id"
                   ORDER BY COALESCE("mm"."SortTitle", "mm"."Title", '') LIMIT 500"#,
            )
            .await?,
            episodes: load_playlist_options(
                &self.0,
                r#"SELECT "ep"."Id", COALESCE("shm"."Title", '') || ' - S' ||
                          CAST(COALESCE("se"."SeasonNumber", 0) AS TEXT) || ' E' ||
                          CAST(COALESCE("em"."EpisodeNumber", 0) AS TEXT) || ' - ' ||
                          COALESCE("em"."Title", '') AS "Name"
                   FROM "Episode" AS "ep"
                   JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "ep"."Id"
                   JOIN "Season" AS "se" ON "se"."Id" = "ep"."SeasonId"
                   JOIN "Show" AS "sh" ON "sh"."Id" = "se"."ShowId"
                   JOIN "ShowMetadata" AS "shm" ON "shm"."ShowId" = "sh"."Id"
                   ORDER BY COALESCE("shm"."SortTitle", "shm"."Title", ''),
                            "se"."SeasonNumber", "em"."EpisodeNumber" LIMIT 500"#,
            )
            .await?,
            music_videos: load_playlist_options(
                &self.0,
                r#"SELECT "mv"."Id", COALESCE("mvm"."Title", '') AS "Name"
                   FROM "MusicVideo" AS "mv"
                   JOIN "MusicVideoMetadata" AS "mvm"
                     ON "mvm"."MusicVideoId" = "mv"."Id"
                   ORDER BY COALESCE("mvm"."SortTitle", "mvm"."Title", '') LIMIT 500"#,
            )
            .await?,
            other_videos: load_playlist_options(
                &self.0,
                r#"SELECT "ov"."Id", COALESCE("ovm"."Title", '') AS "Name"
                   FROM "OtherVideo" AS "ov"
                   JOIN "OtherVideoMetadata" AS "ovm"
                     ON "ovm"."OtherVideoId" = "ov"."Id"
                   ORDER BY COALESCE("ovm"."SortTitle", "ovm"."Title", '') LIMIT 500"#,
            )
            .await?,
            songs: load_playlist_options(
                &self.0,
                r#"SELECT "s"."Id", COALESCE("sm"."Title", '') AS "Name"
                   FROM "Song" AS "s"
                   JOIN "SongMetadata" AS "sm" ON "sm"."SongId" = "s"."Id"
                   ORDER BY COALESCE("sm"."SortTitle", "sm"."Title", '') LIMIT 500"#,
            )
            .await?,
            images: load_playlist_options(
                &self.0,
                r#"SELECT "i"."Id", COALESCE("im"."Title", '') AS "Name"
                   FROM "Image" AS "i"
                   JOIN "ImageMetadata" AS "im" ON "im"."ImageId" = "i"."Id"
                   ORDER BY COALESCE("im"."SortTitle", "im"."Title", '') LIMIT 500"#,
            )
            .await?,
            remote_streams: load_playlist_options(
                &self.0,
                r#"SELECT "rs"."Id", COALESCE("rsm"."Title", '') AS "Name"
                   FROM "RemoteStream" AS "rs"
                   JOIN "RemoteStreamMetadata" AS "rsm"
                     ON "rsm"."RemoteStreamId" = "rs"."Id"
                   ORDER BY COALESCE("rsm"."SortTitle", "rsm"."Title", '') LIMIT 500"#,
            )
            .await?,
        })
    }

    async fn save_playlist_admin(&self, form: &PlaylistEditorAdmin) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let result = sqlx::query(r#"UPDATE "Playlist" SET "Name" = ? WHERE "Id" = ?"#)
            .bind(form.name.trim())
            .bind(form.id)
            .execute(&mut *tx)
            .await
            .repository()?;
        if result.rows_affected() == 0 {
            return Err(ersatzrs_core::repositories::RepositoryError::NotFound {
                kind: "playlist",
                id: form.id,
            });
        }
        sqlx::query(r#"DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
            .bind(form.id)
            .execute(&mut *tx)
            .await
            .repository()?;
        for item in &form.items {
            sqlx::query(
                r#"INSERT INTO "PlaylistItem" (
                       "Index", "PlaylistId", "CollectionType", "CollectionId",
                       "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                       "PlaybackOrder", "Count", "PlayAll", "IncludeInProgramGuide",
                       "PreferredAudioLanguageCode", "PreferredAudioTitle",
                       "PreferredSubtitleLanguageCode", "SubtitleMode",
                       "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
                       "SubtitleStreamIndex"
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(item.index)
            .bind(form.id)
            .bind(item.collection_type)
            .bind(item.collection_id)
            .bind(item.media_item_id)
            .bind(item.multi_collection_id)
            .bind(item.smart_collection_id)
            .bind(item.playback_order)
            .bind(item.count)
            .bind(item.play_all)
            .bind(item.include_in_program_guide)
            .bind(&item.preferred_audio_language_code)
            .bind(&item.preferred_audio_title)
            .bind(&item.preferred_subtitle_language_code)
            .bind(item.subtitle_mode)
            .bind(
                item.pinned_stream_selection
                    .map(|selection| selection.media_version_id),
            )
            .bind(
                item.pinned_stream_selection
                    .map(|selection| selection.video_stream_index),
            )
            .bind(
                item.pinned_stream_selection
                    .and_then(|selection| selection.audio_stream_index),
            )
            .bind(
                item.pinned_stream_selection
                    .and_then(|selection| selection.subtitle_stream_index),
            )
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        tx.commit().await.repository()
    }

    async fn suggest_media_items_admin(
        &self,
        prefix: &str,
    ) -> Result<Vec<MediaItemSuggestionAdmin>> {
        let like = format!("{prefix}%");
        let rows = sqlx::query(
            r#"SELECT "Id", "Title", "Kind" FROM (
                   SELECT "m"."Id" AS "Id", COALESCE("mm"."Title", '') AS "Title",
                          'movie' AS "Kind", "mm"."SortTitle" AS "SortTitle"
                   FROM "Movie" AS "m"
                   JOIN "MovieMetadata" AS "mm" ON "mm"."MovieId" = "m"."Id"
                   WHERE "mm"."Title" LIKE ?
                   UNION ALL
                   SELECT "s"."Id" AS "Id", COALESCE("sm"."Title", '') AS "Title",
                          'show' AS "Kind", "sm"."SortTitle" AS "SortTitle"
                   FROM "Show" AS "s"
                   JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
                   WHERE "sm"."Title" LIKE ?
               ) AS "matches"
               ORDER BY "SortTitle", "Title"
               LIMIT 20"#,
        )
        .bind(&like)
        .bind(&like)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(MediaItemSuggestionAdmin {
                    id: row.try_get("Id").repository()?,
                    title: row.try_get("Title").repository()?,
                    kind: row.try_get("Kind").repository()?,
                })
            })
            .collect()
    }

    async fn media_items_exist(&self, ids: &[EntityId]) -> Result<bool> {
        for id in ids {
            let count: i64 =
                sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaItem" WHERE "Id" = ?"#)
                    .bind(id)
                    .fetch_one(&self.0)
                    .await
                    .repository()?;
            if count == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    async fn add_collection_items_admin(
        &self,
        collection_id: EntityId,
        media_item_ids: &[EntityId],
        playback: &PlaybackOverrides,
    ) -> Result<u64> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let mut added = 0;
        for media_item_id in media_item_ids {
            let sql = r#"INSERT INTO "CollectionItem" (
                             "CollectionId", "MediaItemId", "PreferredAudioLanguageCode",
                             "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "SubtitleMode"
                         )
                         SELECT ?, ?, ?, ?, ?, ?
                         WHERE NOT EXISTS (
                             SELECT 1 FROM "CollectionItem"
                             WHERE "CollectionId" = ? AND "MediaItemId" = ?
                               AND "PreferredAudioLanguageCode" IS ?
                               AND "PreferredAudioTitle" IS ?
                               AND "PreferredSubtitleLanguageCode" IS ?
                               AND "SubtitleMode" IS ?
                         )"#;
            let result = sqlx::query(sql)
                .bind(collection_id)
                .bind(media_item_id)
                .bind(&playback.preferred_audio_language_code)
                .bind(&playback.preferred_audio_title)
                .bind(&playback.preferred_subtitle_language_code)
                .bind(playback.subtitle_mode)
                .bind(collection_id)
                .bind(media_item_id)
                .bind(&playback.preferred_audio_language_code)
                .bind(&playback.preferred_audio_title)
                .bind(&playback.preferred_subtitle_language_code)
                .bind(playback.subtitle_mode)
                .execute(&mut *tx)
                .await
                .repository()?;
            added += result.rows_affected();
        }
        tx.commit().await.repository()?;
        Ok(added)
    }

    async fn add_playlist_items_admin(
        &self,
        playlist_id: EntityId,
        items: &[PlaylistItemInsert],
    ) -> Result<u64> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let max_index: Option<i64> =
            sqlx::query_scalar(r#"SELECT MAX("Index") FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
                .bind(playlist_id)
                .fetch_one(&mut *tx)
                .await
                .repository()?;
        let first_index = max_index.map_or(0, |index| index + 1);
        let mut added = 0;
        for (next_index, item) in (first_index..).zip(items) {
            let result = sqlx::query(
                r#"INSERT INTO "PlaylistItem" (
                       "Index", "PlaylistId", "CollectionType", "MediaItemId",
                       "IncludeInProgramGuide", "PlaybackOrder", "PlayAll",
                       "PreferredAudioLanguageCode", "PreferredAudioTitle",
                       "PreferredSubtitleLanguageCode", "SubtitleMode",
                       "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
                       "SubtitleStreamIndex"
                   ) VALUES (?, ?, ?, ?, 1, 1, 0, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(next_index)
            .bind(playlist_id)
            .bind(item.collection_type)
            .bind(item.media_item_id)
            .bind(&item.playback.preferred_audio_language_code)
            .bind(&item.playback.preferred_audio_title)
            .bind(&item.playback.preferred_subtitle_language_code)
            .bind(item.playback.subtitle_mode)
            .bind(
                item.pinned_stream_selection
                    .map(|selection| selection.media_version_id),
            )
            .bind(
                item.pinned_stream_selection
                    .map(|selection| selection.video_stream_index),
            )
            .bind(
                item.pinned_stream_selection
                    .and_then(|selection| selection.audio_stream_index),
            )
            .bind(
                item.pinned_stream_selection
                    .and_then(|selection| selection.subtitle_stream_index),
            )
            .execute(&mut *tx)
            .await
            .repository()?;
            added += result.rows_affected();
        }
        tx.commit().await.repository()?;
        Ok(added)
    }

    async fn get_collection_detail_admin(
        &self,
        id: EntityId,
    ) -> Result<Option<CollectionDetailAdmin>> {
        let Some(collection) = sqlx::query(
            r#"SELECT "Id", COALESCE("Name", '') AS "Name",
                      COALESCE("UseCustomPlaybackOrder", 0) AS "UseCustomPlaybackOrder"
               FROM "Collection" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        else {
            return Ok(None);
        };
        let rows = sqlx::query(
            r#"SELECT "ci"."Id" AS "CollectionItemId", "ci"."MediaItemId",
                      COALESCE("ci"."CustomIndex", 0) AS "CustomIndex",
                      "ci"."PreferredAudioLanguageCode", "ci"."PreferredAudioTitle",
                      "ci"."PreferredSubtitleLanguageCode", "ci"."SubtitleMode",
                      CASE
                        WHEN "m"."Id" IS NOT NULL THEN 'movie'
                        WHEN "s"."Id" IS NOT NULL THEN 'show'
                        WHEN "se"."Id" IS NOT NULL THEN 'season'
                        WHEN "ep"."Id" IS NOT NULL THEN 'episode'
                        WHEN "ar"."Id" IS NOT NULL THEN 'artist'
                        WHEN "mv"."Id" IS NOT NULL THEN 'music_video'
                        WHEN "ov"."Id" IS NOT NULL THEN 'other_video'
                        WHEN "sg"."Id" IS NOT NULL THEN 'song'
                        WHEN "img"."Id" IS NOT NULL THEN 'image'
                        WHEN "rs"."Id" IS NOT NULL THEN 'remote_stream'
                        ELSE ''
                      END AS "Kind",
                      COALESCE("mm"."Title", "sm"."Title", "sem"."Title", "em"."Title",
                               "am"."Title", "mvm"."Title", "ovm"."Title", "sgm"."Title",
                               "im"."Title", "rsm"."Title", '') AS "Title",
                      COALESCE("mm"."SortTitle", "sm"."SortTitle", "sem"."SortTitle",
                               "em"."SortTitle", "am"."SortTitle", "mvm"."SortTitle",
                               "ovm"."SortTitle", "sgm"."SortTitle", "im"."SortTitle",
                               "rsm"."SortTitle", '') AS "SortTitle",
                      "mm"."Year" AS "MovieYear", "sm"."Year" AS "ShowYear",
                      "se"."SeasonNumber", "sem"."Title" AS "SeasonTitle",
                      "em"."EpisodeNumber", "show_meta"."Title" AS "ShowTitle",
                      "episode_season"."SeasonNumber" AS "EpisodeSeasonNumber",
                      "episode_show_meta"."Title" AS "EpisodeShowTitle",
                      "am"."Disambiguation", "mvm"."Album" AS "MusicVideoAlbum",
                      "sgm"."Album" AS "SongAlbum",
                      COALESCE(
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "MovieMetadataId" = "mm"."Id"
                         AND "ArtworkKind" = 0 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "ShowMetadataId" = "sm"."Id"
                         AND "ArtworkKind" = 0 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "SeasonMetadataId" = "sem"."Id"
                         AND "ArtworkKind" = 0 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "EpisodeMetadataId" = "em"."Id"
                         AND "ArtworkKind" = 0 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "ArtistMetadataId" = "am"."Id"
                         AND "ArtworkKind" = 1 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "MusicVideoMetadataId" = "mvm"."Id"
                         AND "ArtworkKind" = 1 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "OtherVideoMetadataId" = "ovm"."Id"
                         AND "ArtworkKind" = 1 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "SongMetadataId" = "sgm"."Id"
                         AND "ArtworkKind" = 1 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "ImageMetadataId" = "im"."Id"
                         AND "ArtworkKind" = 1 LIMIT 1),
                        (SELECT CASE WHEN "Path" IS NOT NULL THEN "Id" END FROM "Artwork" WHERE "RemoteStreamMetadataId" = "rsm"."Id"
                         AND "ArtworkKind" = 1 LIMIT 1)
                      ) AS "Artwork"
               FROM "CollectionItem" AS "ci"
               LEFT JOIN "Movie" AS "m" ON "m"."Id" = "ci"."MediaItemId"
               LEFT JOIN "MovieMetadata" AS "mm" ON "mm"."MovieId" = "m"."Id"
               LEFT JOIN "Show" AS "s" ON "s"."Id" = "ci"."MediaItemId"
               LEFT JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
               LEFT JOIN "Season" AS "se" ON "se"."Id" = "ci"."MediaItemId"
               LEFT JOIN "SeasonMetadata" AS "sem" ON "sem"."SeasonId" = "se"."Id"
               LEFT JOIN "Show" AS "season_show" ON "season_show"."Id" = "se"."ShowId"
               LEFT JOIN "ShowMetadata" AS "show_meta"
                 ON "show_meta"."ShowId" = "season_show"."Id"
               LEFT JOIN "Episode" AS "ep" ON "ep"."Id" = "ci"."MediaItemId"
               LEFT JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "ep"."Id"
               LEFT JOIN "Season" AS "episode_season"
                 ON "episode_season"."Id" = "ep"."SeasonId"
               LEFT JOIN "Show" AS "episode_show"
                 ON "episode_show"."Id" = "episode_season"."ShowId"
               LEFT JOIN "ShowMetadata" AS "episode_show_meta"
                 ON "episode_show_meta"."ShowId" = "episode_show"."Id"
               LEFT JOIN "Artist" AS "ar" ON "ar"."Id" = "ci"."MediaItemId"
               LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "ar"."Id"
               LEFT JOIN "MusicVideo" AS "mv" ON "mv"."Id" = "ci"."MediaItemId"
               LEFT JOIN "MusicVideoMetadata" AS "mvm" ON "mvm"."MusicVideoId" = "mv"."Id"
               LEFT JOIN "OtherVideo" AS "ov" ON "ov"."Id" = "ci"."MediaItemId"
               LEFT JOIN "OtherVideoMetadata" AS "ovm" ON "ovm"."OtherVideoId" = "ov"."Id"
               LEFT JOIN "Song" AS "sg" ON "sg"."Id" = "ci"."MediaItemId"
               LEFT JOIN "SongMetadata" AS "sgm" ON "sgm"."SongId" = "sg"."Id"
               LEFT JOIN "Image" AS "img" ON "img"."Id" = "ci"."MediaItemId"
               LEFT JOIN "ImageMetadata" AS "im" ON "im"."ImageId" = "img"."Id"
               LEFT JOIN "RemoteStream" AS "rs" ON "rs"."Id" = "ci"."MediaItemId"
               LEFT JOIN "RemoteStreamMetadata" AS "rsm"
                 ON "rsm"."RemoteStreamId" = "rs"."Id"
               WHERE "ci"."CollectionId" = ?
               ORDER BY "Kind", "SortTitle", "Title", "CollectionItemId""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let artwork =
            crate::metadata_query::load_projected_artwork(&self.0, &rows, &["Artwork"]).await?;
        let items = rows
            .iter()
            .map(|row| {
                map_collection_detail_item(
                    row,
                    crate::metadata_query::projected_artwork(row, "Artwork", &artwork)?,
                )
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Some(CollectionDetailAdmin {
            id,
            name: collection.try_get("Name").repository()?,
            use_custom_playback_order: decode_bool_column(&collection, "UseCustomPlaybackOrder")?,
            items,
        }))
    }

    async fn set_collection_custom_order_enabled(
        &self,
        id: EntityId,
        enabled: bool,
    ) -> Result<bool> {
        let result =
            sqlx::query(r#"UPDATE "Collection" SET "UseCustomPlaybackOrder" = ? WHERE "Id" = ?"#)
                .bind(enabled)
                .bind(id)
                .execute(&self.0)
                .await
                .repository()?;
        Ok(result.rows_affected() > 0)
    }

    async fn remove_collection_rows(
        &self,
        collection_id: EntityId,
        row_ids: &[EntityId],
    ) -> Result<u64> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let mut removed = 0;
        for row_id in row_ids {
            removed += sqlx::query(
                r#"DELETE FROM "CollectionItem" WHERE "CollectionId" = ? AND "Id" = ?"#,
            )
            .bind(collection_id)
            .bind(row_id)
            .execute(&mut *tx)
            .await
            .repository()?
            .rows_affected();
        }
        tx.commit().await.repository()?;
        Ok(removed)
    }

    async fn update_collection_custom_order(
        &self,
        collection_id: EntityId,
        ordered_row_ids: &[EntityId],
    ) -> Result<u64> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let mut updated = 0;
        for (index, row_id) in ordered_row_ids.iter().enumerate() {
            updated += sqlx::query(
                r#"UPDATE "CollectionItem" SET "CustomIndex" = ?
                   WHERE "CollectionId" = ? AND "Id" = ?"#,
            )
            .bind(i64::try_from(index).map_err(|_| {
                ersatzrs_core::repositories::RepositoryError::Conflict(
                    "collection order exceeds i64 range".to_owned(),
                )
            })?)
            .bind(collection_id)
            .bind(row_id)
            .execute(&mut *tx)
            .await
            .repository()?
            .rows_affected();
        }
        tx.commit().await.repository()?;
        Ok(updated)
    }

    async fn list_collection_members_admin(
        &self,
        collection_id: EntityId,
    ) -> Result<Vec<CollectionMemberAdmin>> {
        let rows = sqlx::query(
            r#"SELECT DISTINCT "ci"."MediaItemId" AS "MediaItemId", "Title", "Kind"
               FROM "CollectionItem" AS "ci"
               JOIN (
                   SELECT "m"."Id" AS "Id", COALESCE("mm"."Title", '') AS "Title",
                          'movie' AS "Kind"
                   FROM "Movie" AS "m"
                   JOIN "MovieMetadata" AS "mm" ON "mm"."MovieId" = "m"."Id"
                   UNION ALL
                   SELECT "s"."Id" AS "Id", COALESCE("sm"."Title", '') AS "Title",
                          'show' AS "Kind"
                   FROM "Show" AS "s"
                   JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."Id"
               ) AS "items" ON "items"."Id" = "ci"."MediaItemId"
               WHERE "ci"."CollectionId" = ?
               ORDER BY "Title""#,
        )
        .bind(collection_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        rows.iter()
            .map(|row| {
                Ok(CollectionMemberAdmin {
                    media_item_id: row.try_get("MediaItemId").repository()?,
                    title: row.try_get("Title").repository()?,
                    kind: row.try_get("Kind").repository()?,
                })
            })
            .collect()
    }
}

fn map_collection_detail_item(
    row: &DbRow,
    artwork_path: Option<ersatzrs_core::repositories::ArtworkReference>,
) -> Result<CollectionItemCardAdmin> {
    let kind: String = row.try_get("Kind").repository()?;
    let mut title: String = row.try_get("Title").repository()?;
    let subtitle = match kind.as_str() {
        "movie" => row
            .try_get::<Option<i64>, _>("MovieYear")
            .repository()?
            .map(|year| year.to_string()),
        "show" => row
            .try_get::<Option<i64>, _>("ShowYear")
            .repository()?
            .map(|year| year.to_string()),
        "season" => {
            title = row
                .try_get::<Option<String>, _>("ShowTitle")
                .repository()?
                .unwrap_or(title);
            match row
                .try_get::<Option<String>, _>("SeasonTitle")
                .repository()?
            {
                Some(value) if !value.is_empty() => Some(value),
                _ => row
                    .try_get::<Option<i64>, _>("SeasonNumber")
                    .repository()?
                    .map(|number| format!("Season {number}")),
            }
        }
        "episode" => {
            let show = row
                .try_get::<Option<String>, _>("EpisodeShowTitle")
                .repository()?
                .unwrap_or_default();
            let season = row
                .try_get::<Option<i64>, _>("EpisodeSeasonNumber")
                .repository()?
                .unwrap_or_default();
            let episode = row
                .try_get::<Option<i64>, _>("EpisodeNumber")
                .repository()?
                .unwrap_or_default();
            Some(format!("{show} - S{season} E{episode}"))
        }
        "artist" => row.try_get("Disambiguation").repository()?,
        "music_video" => row.try_get("MusicVideoAlbum").repository()?,
        "song" => row.try_get("SongAlbum").repository()?,
        _ => None,
    };
    Ok(CollectionItemCardAdmin {
        collection_item_id: row.try_get("CollectionItemId").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        kind,
        title,
        sort_title: row.try_get("SortTitle").repository()?,
        subtitle,
        artwork_path,
        custom_index: row.try_get("CustomIndex").repository()?,
        playback: PlaybackOverrides {
            preferred_audio_language_code: row
                .try_get("PreferredAudioLanguageCode")
                .repository()?,
            preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
            preferred_subtitle_language_code: row
                .try_get("PreferredSubtitleLanguageCode")
                .repository()?,
            subtitle_mode: row
                .try_get::<Option<i64>, _>("SubtitleMode")
                .repository()?
                .map(i32::try_from)
                .transpose()
                .map_err(|_| ersatzrs_core::repositories::RepositoryError::Decode {
                    kind: "collection subtitle mode",
                    value: "outside i32 range".to_owned(),
                })?,
        },
    })
}

async fn load_playlists_for_group(
    pool: &crate::db::DbPool,
    group_id: EntityId,
) -> Result<Vec<PlaylistAdminRow>> {
    let rows = sqlx::query(
        r#"SELECT "Id", "PlaylistGroupId", COALESCE("Name", '') AS "Name",
                  COALESCE("IsSystem", 0) AS "IsSystem"
           FROM "Playlist"
           WHERE "PlaylistGroupId" = ?
           ORDER BY COALESCE("Name", '')"#,
    )
    .bind(group_id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(PlaylistAdminRow {
                id: row.try_get("Id").repository()?,
                playlist_group_id: row.try_get("PlaylistGroupId").repository()?,
                name: row.try_get("Name").repository()?,
                is_system: decode_bool_column(row, "IsSystem")?,
            })
        })
        .collect()
}

async fn load_playlist_options(
    pool: &crate::db::DbPool,
    sql: &'static str,
) -> Result<Vec<PlaylistItemOption>> {
    let rows = sqlx::query(sql).fetch_all(pool).await.repository()?;
    rows.iter()
        .map(|row| {
            Ok(PlaylistItemOption {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .collect()
}

#[allow(clippy::too_many_lines)]
async fn load_playlist_items(
    pool: &crate::db::DbPool,
    playlist_id: EntityId,
) -> Result<Vec<PlaylistItemAdmin>> {
    let rows = sqlx::query(
        r#"SELECT "pi"."Id", "pi"."Index", "pi"."CollectionType",
                  "pi"."CollectionId", "pi"."MediaItemId", "pi"."MultiCollectionId",
                  "pi"."SmartCollectionId", "pi"."PlaybackOrder", "pi"."Count",
                  "pi"."PlayAll", "pi"."IncludeInProgramGuide",
                  "pi"."PreferredAudioLanguageCode", "pi"."PreferredAudioTitle",
                  "pi"."PreferredSubtitleLanguageCode", "pi"."SubtitleMode",
                  "pi"."MediaVersionId", "pi"."VideoStreamIndex",
                  "pi"."AudioStreamIndex", "pi"."SubtitleStreamIndex",
                  "c"."Name" AS "CollectionName",
                  "mc"."Name" AS "MultiCollectionName",
                  "sc"."Name" AS "SmartCollectionName",
                  "sm"."Title" AS "ShowTitle", "sem"."Title" AS "SeasonTitle",
                  "mm"."Title" AS "MovieTitle", "am"."Title" AS "ArtistTitle",
                  "em"."Title" AS "EpisodeTitle",
                  "mvm"."Title" AS "MusicVideoTitle",
                  "ovm"."Title" AS "OtherVideoTitle",
                  "songm"."Title" AS "SongTitle", "im"."Title" AS "ImageTitle",
                  "rsm"."Title" AS "RemoteStreamTitle"
           FROM "PlaylistItem" AS "pi"
           LEFT JOIN "Collection" AS "c" ON "c"."Id" = "pi"."CollectionId"
           LEFT JOIN "MultiCollection" AS "mc" ON "mc"."Id" = "pi"."MultiCollectionId"
           LEFT JOIN "SmartCollection" AS "sc" ON "sc"."Id" = "pi"."SmartCollectionId"
           LEFT JOIN "Show" AS "show_item"
             ON "show_item"."Id" = "pi"."MediaItemId" AND "pi"."CollectionType" = 1
           LEFT JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "show_item"."Id"
           LEFT JOIN "Season" AS "season_item"
             ON "season_item"."Id" = "pi"."MediaItemId" AND "pi"."CollectionType" = 2
           LEFT JOIN "SeasonMetadata" AS "sem" ON "sem"."SeasonId" = "season_item"."Id"
           LEFT JOIN "Movie" AS "movie_item"
             ON "movie_item"."Id" = "pi"."MediaItemId" AND "pi"."CollectionType" = 10
           LEFT JOIN "MovieMetadata" AS "mm" ON "mm"."MovieId" = "movie_item"."Id"
           LEFT JOIN "Artist" AS "artist_item"
             ON "artist_item"."Id" = "pi"."MediaItemId" AND "pi"."CollectionType" = 3
           LEFT JOIN "ArtistMetadata" AS "am" ON "am"."ArtistId" = "artist_item"."Id"
           LEFT JOIN "Episode" AS "episode_item"
             ON "episode_item"."Id" = "pi"."MediaItemId" AND "pi"."CollectionType" = 20
           LEFT JOIN "EpisodeMetadata" AS "em" ON "em"."EpisodeId" = "episode_item"."Id"
           LEFT JOIN "MusicVideo" AS "music_video_item"
             ON "music_video_item"."Id" = "pi"."MediaItemId"
            AND "pi"."CollectionType" = 30
           LEFT JOIN "MusicVideoMetadata" AS "mvm"
             ON "mvm"."MusicVideoId" = "music_video_item"."Id"
           LEFT JOIN "OtherVideo" AS "other_video_item"
             ON "other_video_item"."Id" = "pi"."MediaItemId"
            AND "pi"."CollectionType" = 40
           LEFT JOIN "OtherVideoMetadata" AS "ovm"
             ON "ovm"."OtherVideoId" = "other_video_item"."Id"
           LEFT JOIN "Song" AS "song_item"
             ON "song_item"."Id" = "pi"."MediaItemId" AND "pi"."CollectionType" = 50
           LEFT JOIN "SongMetadata" AS "songm" ON "songm"."SongId" = "song_item"."Id"
           LEFT JOIN "Image" AS "image_item"
             ON "image_item"."Id" = "pi"."MediaItemId" AND "pi"."CollectionType" = 60
           LEFT JOIN "ImageMetadata" AS "im" ON "im"."ImageId" = "image_item"."Id"
           LEFT JOIN "RemoteStream" AS "remote_stream_item"
             ON "remote_stream_item"."Id" = "pi"."MediaItemId"
            AND "pi"."CollectionType" = 70
           LEFT JOIN "RemoteStreamMetadata" AS "rsm"
             ON "rsm"."RemoteStreamId" = "remote_stream_item"."Id"
           WHERE "pi"."PlaylistId" = ?
           ORDER BY "pi"."Index", "pi"."Id""#,
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            let collection_type = decode_i32(row, "CollectionType")?;
            Ok(PlaylistItemAdmin {
                id: row.try_get("Id").repository()?,
                index: decode_i32(row, "Index")?,
                collection_type,
                collection_id: row.try_get("CollectionId").repository()?,
                multi_collection_id: row.try_get("MultiCollectionId").repository()?,
                smart_collection_id: row.try_get("SmartCollectionId").repository()?,
                media_item_id: row.try_get("MediaItemId").repository()?,
                item_name: playlist_item_name(row, collection_type)?,
                playback_order: decode_i32(row, "PlaybackOrder")?,
                count: row
                    .try_get::<Option<i64>, _>("Count")
                    .repository()?
                    .map(i32::try_from)
                    .transpose()
                    .map_err(|_| ersatzrs_core::repositories::RepositoryError::Decode {
                        kind: "playlist count",
                        value: "outside i32 range".to_owned(),
                    })?,
                play_all: decode_bool_column(row, "PlayAll")?,
                include_in_program_guide: decode_bool_column(row, "IncludeInProgramGuide")?,
                preferred_audio_language_code: row
                    .try_get("PreferredAudioLanguageCode")
                    .repository()?,
                preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
                preferred_subtitle_language_code: row
                    .try_get("PreferredSubtitleLanguageCode")
                    .repository()?,
                subtitle_mode: row
                    .try_get::<Option<i64>, _>("SubtitleMode")
                    .repository()?
                    .map(i32::try_from)
                    .transpose()
                    .map_err(|_| ersatzrs_core::repositories::RepositoryError::Decode {
                        kind: "playlist subtitle mode",
                        value: "outside i32 range".to_owned(),
                    })?,
                pinned_stream_selection: decode_pinned_stream_selection(row)?,
            })
        })
        .collect()
}

fn decode_pinned_stream_selection(row: &DbRow) -> Result<Option<PinnedStreamSelection>> {
    let media_version_id: Option<EntityId> = row.try_get("MediaVersionId").repository()?;
    let video_stream_index: Option<i64> = row.try_get("VideoStreamIndex").repository()?;
    match (media_version_id, video_stream_index) {
        (None, None) => Ok(None),
        (Some(media_version_id), Some(video_stream_index)) => Ok(Some(PinnedStreamSelection {
            media_version_id,
            video_stream_index: i32::try_from(video_stream_index).map_err(|_| {
                ersatzrs_core::repositories::RepositoryError::Decode {
                    kind: "playlist video stream index",
                    value: "outside i32 range".to_owned(),
                }
            })?,
            audio_stream_index: row
                .try_get::<Option<i64>, _>("AudioStreamIndex")
                .repository()?
                .map(i32::try_from)
                .transpose()
                .map_err(|_| ersatzrs_core::repositories::RepositoryError::Decode {
                    kind: "playlist audio stream index",
                    value: "outside i32 range".to_owned(),
                })?,
            subtitle_stream_index: row
                .try_get::<Option<i64>, _>("SubtitleStreamIndex")
                .repository()?
                .map(i32::try_from)
                .transpose()
                .map_err(|_| ersatzrs_core::repositories::RepositoryError::Decode {
                    kind: "playlist subtitle stream index",
                    value: "outside i32 range".to_owned(),
                })?,
        })),
        _ => Err(ersatzrs_core::repositories::RepositoryError::Decode {
            kind: "playlist pinned stream selection",
            value: "media version and video stream must both be set".to_owned(),
        }),
    }
}

fn playlist_item_name(row: &DbRow, collection_type: i32) -> Result<String> {
    let column = match collection_type {
        COLLECTION_TYPE_COLLECTION => "CollectionName",
        COLLECTION_TYPE_MULTI_COLLECTION => "MultiCollectionName",
        COLLECTION_TYPE_SMART_COLLECTION => "SmartCollectionName",
        COLLECTION_TYPE_SHOW => "ShowTitle",
        COLLECTION_TYPE_SEASON => "SeasonTitle",
        COLLECTION_TYPE_ARTIST => "ArtistTitle",
        COLLECTION_TYPE_MOVIE => "MovieTitle",
        COLLECTION_TYPE_EPISODE => "EpisodeTitle",
        COLLECTION_TYPE_MUSIC_VIDEO => "MusicVideoTitle",
        COLLECTION_TYPE_OTHER_VIDEO => "OtherVideoTitle",
        COLLECTION_TYPE_SONG => "SongTitle",
        COLLECTION_TYPE_IMAGE => "ImageTitle",
        COLLECTION_TYPE_REMOTE_STREAM => "RemoteStreamTitle",
        _ => return Ok(String::new()),
    };
    Ok(row
        .try_get::<Option<String>, _>(column)
        .repository()?
        .unwrap_or_default())
}

fn map_rerun_collection(row: &DbRow) -> Result<RerunCollectionAdmin> {
    Ok(RerunCollectionAdmin {
        id: row.try_get("Id").repository()?,
        name: row.try_get("Name").repository()?,
        collection_type: decode_i32(row, "CollectionType")?,
        collection_id: row.try_get("CollectionId").repository()?,
        media_item_id: row.try_get("MediaItemId").repository()?,
        multi_collection_id: row.try_get("MultiCollectionId").repository()?,
        smart_collection_id: row.try_get("SmartCollectionId").repository()?,
        first_run_playback_order: decode_i32(row, "FirstRunPlaybackOrder")?,
        rerun_playback_order: decode_i32(row, "RerunPlaybackOrder")?,
    })
}

async fn load_rerun_options(
    pool: &crate::db::DbPool,
    sql: &'static str,
) -> Result<Vec<RerunCollectionSelectOption>> {
    let rows = sqlx::query(sql).fetch_all(pool).await.repository()?;
    rows.iter()
        .map(|row| {
            Ok(RerunCollectionSelectOption {
                id: row.try_get("Id").repository()?,
                name: row.try_get("Name").repository()?,
            })
        })
        .collect()
}

async fn load_multi_collection_items(
    pool: &crate::db::DbPool,
    id: EntityId,
) -> Result<Vec<MultiCollectionItem>> {
    let rows = sqlx::query(
        r#"SELECT "mci"."CollectionId", "mci"."ScheduleAsGroup",
                  "mci"."PlaybackOrder", COALESCE("c"."Name", '') AS "Name",
                  COALESCE("c"."UseCustomPlaybackOrder", 0) AS "UseCustomPlaybackOrder"
           FROM "MultiCollectionItem" AS "mci"
           JOIN "Collection" AS "c" ON "c"."Id" = "mci"."CollectionId"
           WHERE "mci"."MultiCollectionId" = ?"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(MultiCollectionItem {
                collection_id: Some(row.try_get("CollectionId").repository()?),
                smart_collection_id: None,
                name: row.try_get("Name").repository()?,
                collection_type: COLLECTION_TYPE_COLLECTION,
                use_custom_playback_order: row
                    .try_get::<i64, _>("UseCustomPlaybackOrder")
                    .repository()?
                    != 0,
                schedule_as_group: row.try_get::<i64, _>("ScheduleAsGroup").repository()? != 0,
                playback_order: decode_i32(row, "PlaybackOrder")?,
            })
        })
        .collect()
}

async fn load_multi_smart_collection_items(
    pool: &crate::db::DbPool,
    id: EntityId,
) -> Result<Vec<MultiCollectionItem>> {
    let rows = sqlx::query(
        r#"SELECT "mcsi"."SmartCollectionId", "mcsi"."ScheduleAsGroup",
                  "mcsi"."PlaybackOrder", COALESCE("sc"."Name", '') AS "Name"
           FROM "MultiCollectionSmartItem" AS "mcsi"
           JOIN "SmartCollection" AS "sc"
             ON "sc"."Id" = "mcsi"."SmartCollectionId"
           WHERE "mcsi"."MultiCollectionId" = ?"#,
    )
    .bind(id)
    .fetch_all(pool)
    .await
    .repository()?;
    rows.iter()
        .map(|row| {
            Ok(MultiCollectionItem {
                collection_id: None,
                smart_collection_id: Some(row.try_get("SmartCollectionId").repository()?),
                name: row.try_get("Name").repository()?,
                collection_type: COLLECTION_TYPE_SMART_COLLECTION,
                use_custom_playback_order: false,
                schedule_as_group: row.try_get::<i64, _>("ScheduleAsGroup").repository()? != 0,
                playback_order: decode_i32(row, "PlaybackOrder")?,
            })
        })
        .collect()
}

fn decode_i32(row: &DbRow, column: &str) -> Result<i32> {
    let value: i64 = row.try_get(column).repository()?;
    i32::try_from(value).map_err(|_| ersatzrs_core::repositories::RepositoryError::Decode {
        kind: "multi collection integer",
        value: format!("{column}={value}"),
    })
}

#[allow(clippy::unused_async)]
async fn inserted_id(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    result: crate::db::DbQueryResult,
) -> Result<EntityId> {
    let _ = tx;
    Ok(result.last_insert_rowid())
}

fn map_collection(row: &DbRow) -> Result<Collection> {
    Ok(Collection {
        id: row.try_get("Id").repository()?,
        name: row.try_get("Name").repository()?,
        // ErsatzTV Collection has no collection_type; default to Collection
        collection_type: ersatzrs_domain::CollectionType::Collection,
        media_item_ids: Vec::new(),
        item_ids: Vec::new(),
    })
}
