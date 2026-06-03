use std::collections::HashSet;

use chrono::{Duration, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_core::trakt::{TraktApiClient, TraktListItemWithGuids, TraktListLocator};
use ersatzrs_domain::{EntityId, TraktList, TraktListItemKind};
use sqlx::{Pool, Row};

use crate::db::{last_insert_id, Db, DbPool};
use crate::repositories::common::{
    decode_bool_column, decode_enum_column, decode_optional_datetime, encode_datetime, encode_enum,
};

const PLAYBACK_ORDER_CHRONOLOGICAL: i32 = 1;
const COLLECTION_TYPE_SHOW: i32 = 1;
const COLLECTION_TYPE_SEASON: i32 = 2;
const COLLECTION_TYPE_MOVIE: i32 = 10;
const COLLECTION_TYPE_EPISODE: i32 = 20;

/// Coordinates Trakt API reads with the ErsatzTV-compatible Trakt tables.
pub struct TraktSynchronizer<C> {
    pool: DbPool,
    api_client: C,
}

impl<C> TraktSynchronizer<C>
where
    C: TraktApiClient,
{
    #[must_use]
    pub fn new(pool: DbPool, api_client: C) -> Self {
        Self { pool, api_client }
    }

    /// Add or refresh a Trakt list, then immediately match its items.
    ///
    /// # Errors
    ///
    /// Returns an error when the Trakt API request fails or persistence fails.
    pub async fn add_or_refresh(&self, locator: &TraktListLocator) -> Result<EntityId> {
        let mut list = self
            .api_client
            .get_user_list(&locator.user, &locator.list)
            .await?;

        // ErsatzTV overwrites the API user slug with the requested lower-case
        // user before saving, then keeps the API list slug for future refreshes.
        list.user = locator.user.to_ascii_lowercase();
        let list_id = self.save_list(&list).await?;
        let list = self.save_list_items(list_id).await?;
        self.match_list_items(list.id).await?;
        Ok(list.id)
    }

    /// Match a saved Trakt list against local metadata and update generated
    /// playlist rows when enabled.
    ///
    /// # Errors
    ///
    /// Returns an error when the list cannot be loaded or database writes fail.
    pub async fn match_list_items(&self, list_id: EntityId) -> Result<()> {
        let list = self.load_list(list_id).await?;
        let playlist_id = if list.generate_playlist {
            Some(self.ensure_generated_playlist(&list).await?)
        } else {
            None
        };

        let rows = sqlx::query(
            r#"SELECT "Id", "Kind", "Title", "Year", "Season", "Episode", "Rank"
               FROM "TraktListItem"
               WHERE "TraktListId" = ?
               ORDER BY "Title", "Year", "Season", "Episode""#,
        )
        .bind(list_id)
        .fetch_all(&self.pool)
        .await?;

        for row in rows {
            let item = TraktMatchItem {
                id: row.try_get("Id")?,
                kind: decode_enum_column("TraktListItemKind", &row, "Kind")?,
                title: row
                    .try_get::<Option<String>, _>("Title")?
                    .unwrap_or_default(),
                year: row.try_get("Year")?,
                season: row.try_get("Season")?,
                episode: row.try_get("Episode")?,
                rank: row.try_get("Rank")?,
                guids: self.load_item_guids(row.try_get("Id")?).await?,
            };

            let matched_id = self.identify_item(&item).await?;
            sqlx::query(r#"UPDATE "TraktListItem" SET "MediaItemId" = ? WHERE "Id" = ?"#)
                .bind(matched_id)
                .bind(item.id)
                .execute(&self.pool)
                .await?;

            if let Some(playlist_id) = playlist_id {
                if let Some(media_item_id) = matched_id {
                    self.upsert_playlist_item(playlist_id, &item, media_item_id)
                        .await?;
                } else {
                    self.delete_playlist_item_at_rank(playlist_id, item.rank)
                        .await?;
                }
            }
        }

        sqlx::query(r#"UPDATE "TraktList" SET "LastMatch" = ? WHERE "Id" = ?"#)
            .bind(encode_datetime(Utc::now()))
            .bind(list_id)
            .execute(&self.pool)
            .await?;

        Ok(())
    }

    /// Update user-editable Trakt list settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the list does not exist or persistence fails.
    pub async fn update_settings(
        &self,
        list_id: EntityId,
        auto_refresh: bool,
        generate_playlist: bool,
    ) -> Result<()> {
        let list = self.load_list(list_id).await?;
        sqlx::query(
            r#"UPDATE "TraktList"
               SET "AutoRefresh" = ?, "GeneratePlaylist" = ?
               WHERE "Id" = ?"#,
        )
        .bind(auto_refresh)
        .bind(generate_playlist)
        .bind(list_id)
        .execute(&self.pool)
        .await?;

        if generate_playlist {
            self.ensure_generated_playlist(&TraktList {
                auto_refresh,
                generate_playlist,
                ..list
            })
            .await?;
            self.match_list_items(list_id).await?;
        } else if let Some(playlist_id) = list.playlist_id {
            sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
                .bind(playlist_id)
                .execute(&self.pool)
                .await?;
            sqlx::query(r#"UPDATE "TraktList" SET "PlaylistId" = NULL WHERE "Id" = ?"#)
                .bind(list_id)
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    /// Delete a Trakt list and its generated playlist if present.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    pub async fn delete_list(&self, list_id: EntityId) -> Result<()> {
        let maybe_playlist_id = sqlx::query_scalar::<_, Option<EntityId>>(
            r#"SELECT "PlaylistId" FROM "TraktList" WHERE "Id" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await?
        .flatten();

        sqlx::query(r#"DELETE FROM "TraktList" WHERE "Id" = ?"#)
            .bind(list_id)
            .execute(&self.pool)
            .await?;

        if let Some(playlist_id) = maybe_playlist_id {
            sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
                .bind(playlist_id)
                .execute(&self.pool)
                .await?;
        }

        Ok(())
    }

    /// Refresh auto-refresh lists whose last Trakt download is older than one
    /// day.
    ///
    /// # Errors
    ///
    /// Returns an error when list loading, API reads, or persistence fails.
    pub async fn refresh_due_auto_lists(&self) -> Result<usize> {
        let cutoff = Utc::now() - Duration::days(1);
        let rows = sqlx::query(
            r#"SELECT "User", "List"
               FROM "TraktList"
               WHERE "AutoRefresh" = 1 AND ("LastUpdate" IS NULL OR "LastUpdate" <= ?)
               ORDER BY "Id""#,
        )
        .bind(encode_datetime(cutoff))
        .fetch_all(&self.pool)
        .await?;

        let mut count = 0;
        for row in rows {
            let locator = TraktListLocator {
                user: row
                    .try_get::<Option<String>, _>("User")?
                    .unwrap_or_default(),
                list: row
                    .try_get::<Option<String>, _>("List")?
                    .unwrap_or_default(),
            };
            self.add_or_refresh(&locator).await?;
            count += 1;
        }

        Ok(count)
    }

    /// Re-match lists whose last local match is older than one hour.
    ///
    /// # Errors
    ///
    /// Returns an error when list loading or database writes fail.
    pub async fn match_due_lists(&self) -> Result<usize> {
        let cutoff = Utc::now() - Duration::hours(1);
        let rows = sqlx::query(
            r#"SELECT "Id"
               FROM "TraktList"
               WHERE "LastMatch" IS NULL OR "LastMatch" <= ?
               ORDER BY "Id""#,
        )
        .bind(encode_datetime(cutoff))
        .fetch_all(&self.pool)
        .await?;

        let mut count = 0;
        for row in rows {
            self.match_list_items(row.try_get("Id")?).await?;
            count += 1;
        }

        Ok(count)
    }

    async fn save_list(&self, list: &TraktList) -> Result<EntityId> {
        let maybe_existing = sqlx::query(
            r#"SELECT "Id"
               FROM "TraktList"
               WHERE "User" = ? AND "List" = ?
               ORDER BY "Id"
               LIMIT 1"#,
        )
        .bind(&list.user)
        .bind(&list.list)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(row) = maybe_existing {
            let id = row.try_get("Id")?;
            sqlx::query(
                r#"UPDATE "TraktList"
                   SET "TraktId" = ?, "Name" = ?, "Description" = ?, "ItemCount" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(list.trakt_id)
            .bind(&list.name)
            .bind(&list.description)
            .bind(list.item_count)
            .bind(id)
            .execute(&self.pool)
            .await?;
            Ok(id)
        } else {
            let result = sqlx::query(
                r#"INSERT INTO "TraktList" (
                    "AutoRefresh", "Description", "GeneratePlaylist", "ItemCount",
                    "LastMatch", "LastUpdate", "List", "Name", "PlaylistId",
                    "TraktId", "User"
                   ) VALUES (0, ?, 0, ?, NULL, NULL, ?, ?, NULL, ?, ?)"#,
            )
            .bind(&list.description)
            .bind(list.item_count)
            .bind(&list.list)
            .bind(&list.name)
            .bind(list.trakt_id)
            .bind(&list.user)
            .execute(&self.pool)
            .await?;
            Ok(last_insert_id(result))
        }
    }

    async fn save_list_items(&self, list_id: EntityId) -> Result<TraktList> {
        let list = self.load_list(list_id).await?;
        let incoming = self
            .api_client
            .get_user_list_items(&list.user, &list.list)
            .await?;
        let incoming_ids = incoming
            .iter()
            .map(|item| item.trakt_id)
            .collect::<HashSet<_>>();
        let existing = self.load_existing_items(list_id).await?;
        let existing_ids = existing
            .iter()
            .map(|item| item.trakt_id)
            .collect::<HashSet<_>>();

        let mut tx = self.pool.begin().await?;
        for item in &existing {
            if !incoming_ids.contains(&item.trakt_id) {
                sqlx::query(r#"DELETE FROM "TraktListItem" WHERE "Id" = ?"#)
                    .bind(item.id)
                    .execute(&mut *tx)
                    .await?;
            }
        }

        for item in &incoming {
            if existing_ids.contains(&item.trakt_id) {
                let existing = existing
                    .iter()
                    .find(|candidate| candidate.trakt_id == item.trakt_id)
                    .expect("existing id set and rows are aligned");
                sqlx::query(
                    r#"UPDATE "TraktListItem"
                       SET "Kind" = ?, "Rank" = ?, "Title" = ?, "Year" = ?,
                           "Season" = ?, "Episode" = ?, "MediaItemId" = NULL
                       WHERE "Id" = ?"#,
                )
                .bind(encode_enum(item.kind))
                .bind(item.rank)
                .bind(&item.title)
                .bind(item.year)
                .bind(zero_as_none(item.season))
                .bind(zero_as_none(item.episode))
                .bind(existing.id)
                .execute(&mut *tx)
                .await?;
                sqlx::query(r#"DELETE FROM "TraktListItemGuid" WHERE "TraktListItemId" = ?"#)
                    .bind(existing.id)
                    .execute(&mut *tx)
                    .await?;
                insert_item_guids(&mut tx, existing.id, &item.guids).await?;
            } else {
                let item_id = insert_item(&mut tx, list_id, item).await?;
                insert_item_guids(&mut tx, item_id, &item.guids).await?;
            }
        }

        sqlx::query(r#"UPDATE "TraktList" SET "LastUpdate" = ? WHERE "Id" = ?"#)
            .bind(encode_datetime(Utc::now()))
            .bind(list_id)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;

        self.load_list(list_id).await
    }

    async fn load_list(&self, list_id: EntityId) -> Result<TraktList> {
        let row = sqlx::query(r#"SELECT * FROM "TraktList" WHERE "Id" = ?"#)
            .bind(list_id)
            .fetch_optional(&self.pool)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "trakt list",
                id: list_id,
            })?;

        Ok(TraktList {
            id: row.try_get("Id")?,
            trakt_id: row.try_get("TraktId")?,
            name: row
                .try_get::<Option<String>, _>("Name")?
                .unwrap_or_default(),
            user: row
                .try_get::<Option<String>, _>("User")?
                .unwrap_or_default(),
            list: row
                .try_get::<Option<String>, _>("List")?
                .unwrap_or_default(),
            description: row.try_get("Description")?,
            item_count: row.try_get("ItemCount")?,
            auto_refresh: decode_bool_column(&row, "AutoRefresh")?,
            generate_playlist: decode_bool_column(&row, "GeneratePlaylist")?,
            playlist_id: row.try_get("PlaylistId")?,
            last_update: decode_optional_datetime(
                "TraktList.LastUpdate",
                row.try_get("LastUpdate")?,
            )?,
            last_match: decode_optional_datetime("TraktList.LastMatch", row.try_get("LastMatch")?)?,
            item_ids: self.load_item_ids(list_id).await?,
        })
    }

    async fn load_item_ids(&self, list_id: EntityId) -> Result<Vec<EntityId>> {
        let rows = sqlx::query(
            r#"SELECT "Id" FROM "TraktListItem" WHERE "TraktListId" = ? ORDER BY "Rank", "Id""#,
        )
        .bind(list_id)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(|row| Ok(row.try_get("Id")?)).collect()
    }

    async fn load_existing_items(&self, list_id: EntityId) -> Result<Vec<ExistingItem>> {
        let rows = sqlx::query(
            r#"SELECT "Id", "TraktId"
               FROM "TraktListItem"
               WHERE "TraktListId" = ?
               ORDER BY "Id""#,
        )
        .bind(list_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                Ok(ExistingItem {
                    id: row.try_get("Id")?,
                    trakt_id: row.try_get("TraktId")?,
                })
            })
            .collect()
    }

    async fn load_item_guids(&self, item_id: EntityId) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"SELECT "Guid"
               FROM "TraktListItemGuid"
               WHERE "TraktListItemId" = ?
               ORDER BY "Id""#,
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await?;

        rows.iter()
            .map(|row| {
                Ok(row
                    .try_get::<Option<String>, _>("Guid")?
                    .unwrap_or_default())
            })
            .collect()
    }

    async fn identify_item(&self, item: &TraktMatchItem) -> Result<Option<EntityId>> {
        match item.kind {
            TraktListItemKind::Movie => self.identify_movie(item).await,
            TraktListItemKind::Show => self.identify_show(item).await,
            TraktListItemKind::Season => self.identify_season(item).await,
            TraktListItemKind::Episode => self.identify_episode(item).await,
        }
    }

    async fn identify_movie(&self, item: &TraktMatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = self
            .identify_by_guid("MovieMetadata", "MovieMetadataId", "MovieId", &item.guids)
            .await?
        {
            return Ok(Some(id));
        }

        sqlx::query_scalar(
            r#"SELECT "MovieId"
               FROM "MovieMetadata"
               WHERE "Title" = ? AND ("Year" = ? OR ("Year" IS NULL AND ? IS NULL))
               ORDER BY "MovieId"
               LIMIT 1"#,
        )
        .bind(&item.title)
        .bind(item.year)
        .bind(item.year)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn identify_show(&self, item: &TraktMatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = self
            .identify_by_guid("ShowMetadata", "ShowMetadataId", "ShowId", &item.guids)
            .await?
        {
            return Ok(Some(id));
        }

        sqlx::query_scalar(
            r#"SELECT "ShowId"
               FROM "ShowMetadata"
               WHERE "Title" = ? AND ("Year" = ? OR ("Year" IS NULL AND ? IS NULL))
               ORDER BY "ShowId"
               LIMIT 1"#,
        )
        .bind(&item.title)
        .bind(item.year)
        .bind(item.year)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn identify_season(&self, item: &TraktMatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = self
            .identify_by_guid(
                "SeasonMetadata",
                "SeasonMetadataId",
                "SeasonId",
                &item.guids,
            )
            .await?
        {
            return Ok(Some(id));
        }

        sqlx::query_scalar(
            r#"SELECT "sem"."SeasonId"
               FROM "SeasonMetadata" AS "sem"
               JOIN "Season" AS "s" ON "s"."Id" = "sem"."SeasonId"
               JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."ShowId"
               WHERE "sm"."Title" = ?
                 AND ("sm"."Year" = ? OR ("sm"."Year" IS NULL AND ? IS NULL))
                 AND ("s"."SeasonNumber" = ? OR ("s"."SeasonNumber" IS NULL AND ? IS NULL))
               ORDER BY "sem"."SeasonId"
               LIMIT 1"#,
        )
        .bind(&item.title)
        .bind(item.year)
        .bind(item.year)
        .bind(item.season)
        .bind(item.season)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn identify_episode(&self, item: &TraktMatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = self
            .identify_by_guid(
                "EpisodeMetadata",
                "EpisodeMetadataId",
                "EpisodeId",
                &item.guids,
            )
            .await?
        {
            return Ok(Some(id));
        }

        sqlx::query_scalar(
            r#"SELECT "em"."EpisodeId"
               FROM "EpisodeMetadata" AS "em"
               JOIN "Episode" AS "e" ON "e"."Id" = "em"."EpisodeId"
               JOIN "Season" AS "s" ON "s"."Id" = "e"."SeasonId"
               JOIN "ShowMetadata" AS "sm" ON "sm"."ShowId" = "s"."ShowId"
               WHERE "sm"."Title" = ?
                 AND ("sm"."Year" = ? OR ("sm"."Year" IS NULL AND ? IS NULL))
                 AND ("s"."SeasonNumber" = ? OR ("s"."SeasonNumber" IS NULL AND ? IS NULL))
                 AND ("em"."EpisodeNumber" = ? OR ("em"."EpisodeNumber" IS NULL AND ? IS NULL))
               ORDER BY "em"."EpisodeId"
               LIMIT 1"#,
        )
        .bind(&item.title)
        .bind(item.year)
        .bind(item.year)
        .bind(item.season)
        .bind(item.season)
        .bind(item.episode)
        .bind(item.episode)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    async fn identify_by_guid(
        &self,
        table: &str,
        guid_fk: &str,
        item_fk: &str,
        guids: &[String],
    ) -> Result<Option<EntityId>> {
        for guid in guids {
            let sql = format!(
                r#"SELECT "m"."{item_fk}"
                   FROM "MetadataGuid" AS "g"
                   JOIN "{table}" AS "m" ON "m"."Id" = "g"."{guid_fk}"
                   WHERE "g"."Guid" = ?
                   ORDER BY "m"."{item_fk}"
                   LIMIT 1"#
            );
            if let Some(id) = sqlx::query_scalar::<_, EntityId>(crate::db::assert_sql_safe(&sql))
                .bind(guid)
                .fetch_optional(&self.pool)
                .await?
            {
                return Ok(Some(id));
            }
        }

        Ok(None)
    }

    async fn ensure_generated_playlist(&self, list: &TraktList) -> Result<EntityId> {
        if let Some(playlist_id) = list.playlist_id {
            return Ok(playlist_id);
        }

        let group_id = system_playlist_group_id(&self.pool).await?;
        let name = format!("{}/{}", list.user, list.list);
        let result = sqlx::query(
            r#"INSERT INTO "Playlist" ("IsSystem", "Name", "PlaylistGroupId")
               VALUES (1, ?, ?)"#,
        )
        .bind(&name)
        .bind(group_id)
        .execute(&self.pool)
        .await?;
        let playlist_id = last_insert_id(result);

        sqlx::query(r#"UPDATE "TraktList" SET "PlaylistId" = ? WHERE "Id" = ?"#)
            .bind(playlist_id)
            .bind(list.id)
            .execute(&self.pool)
            .await?;

        Ok(playlist_id)
    }

    async fn upsert_playlist_item(
        &self,
        playlist_id: EntityId,
        item: &TraktMatchItem,
        media_item_id: EntityId,
    ) -> Result<()> {
        let collection_type = match item.kind {
            TraktListItemKind::Movie => COLLECTION_TYPE_MOVIE,
            TraktListItemKind::Show => COLLECTION_TYPE_SHOW,
            TraktListItemKind::Season => COLLECTION_TYPE_SEASON,
            TraktListItemKind::Episode => COLLECTION_TYPE_EPISODE,
        };
        let existing_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "PlaylistItem" WHERE "PlaylistId" = ? AND "Index" = ?"#,
        )
        .bind(playlist_id)
        .bind(item.rank)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(existing_id) = existing_id {
            sqlx::query(
                r#"UPDATE "PlaylistItem"
                   SET "CollectionType" = ?, "MediaItemId" = ?,
                       "CollectionId" = NULL, "MultiCollectionId" = NULL,
                       "SmartCollectionId" = NULL, "PlaybackOrder" = ?,
                       "IncludeInProgramGuide" = 1
                   WHERE "Id" = ?"#,
            )
            .bind(collection_type)
            .bind(media_item_id)
            .bind(PLAYBACK_ORDER_CHRONOLOGICAL)
            .bind(existing_id)
            .execute(&self.pool)
            .await?;
        } else {
            sqlx::query(
                r#"INSERT INTO "PlaylistItem" (
                    "Index", "PlaylistId", "CollectionType", "CollectionId",
                    "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                    "IncludeInProgramGuide", "PlaybackOrder", "PlayAll"
                   ) VALUES (?, ?, ?, NULL, ?, NULL, NULL, 1, ?, 0)"#,
            )
            .bind(item.rank)
            .bind(playlist_id)
            .bind(collection_type)
            .bind(media_item_id)
            .bind(PLAYBACK_ORDER_CHRONOLOGICAL)
            .execute(&self.pool)
            .await?;
        }

        Ok(())
    }

    async fn delete_playlist_item_at_rank(&self, playlist_id: EntityId, rank: i32) -> Result<()> {
        sqlx::query(r#"DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ? AND "Index" = ?"#)
            .bind(playlist_id)
            .bind(rank)
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

async fn system_playlist_group_id(pool: &Pool<Db>) -> Result<EntityId> {
    sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "Id" FROM "PlaylistGroup" WHERE "IsSystem" = 1 ORDER BY "Id" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| {
        RepositoryError::Database(sqlx::Error::Protocol(
            "system playlist group is not initialized".into(),
        ))
    })
}

async fn insert_item(
    tx: &mut sqlx::Transaction<'_, Db>,
    list_id: EntityId,
    item: &TraktListItemWithGuids,
) -> Result<EntityId> {
    let result = sqlx::query(
        r#"INSERT INTO "TraktListItem" (
            "Episode", "Kind", "MediaItemId", "Rank", "Season", "Title",
            "TraktId", "TraktListId", "Year"
           ) VALUES (?, ?, NULL, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(zero_as_none(item.episode))
    .bind(encode_enum(item.kind))
    .bind(item.rank)
    .bind(zero_as_none(item.season))
    .bind(&item.title)
    .bind(item.trakt_id)
    .bind(list_id)
    .bind(item.year)
    .execute(&mut **tx)
    .await?;
    Ok(last_insert_id(result))
}

async fn insert_item_guids(
    tx: &mut sqlx::Transaction<'_, Db>,
    item_id: EntityId,
    guids: &[String],
) -> Result<()> {
    for guid in guids {
        sqlx::query(r#"INSERT INTO "TraktListItemGuid" ("Guid", "TraktListItemId") VALUES (?, ?)"#)
            .bind(guid)
            .bind(item_id)
            .execute(&mut **tx)
            .await?;
    }
    Ok(())
}

fn zero_as_none(value: i32) -> Option<i32> {
    (value != 0).then_some(value)
}

#[derive(Debug)]
struct ExistingItem {
    id: EntityId,
    trakt_id: i32,
}

#[derive(Debug)]
struct TraktMatchItem {
    id: EntityId,
    kind: TraktListItemKind,
    title: String,
    year: Option<i32>,
    season: Option<i32>,
    episode: Option<i32>,
    rank: i32,
    guids: Vec<String>,
}
