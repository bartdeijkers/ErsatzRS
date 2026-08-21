use crate::addon_media_storage::{
    delete_list_library, ensure_list_library, has_media_source, validated_folder_name,
};
use crate::db::inserted_id;
use crate::repositories::SqlxResultExt as _;
use std::collections::{HashMap, HashSet};

use chrono::{Duration, Utc};
use ersatzrs_core::addon_media_list::{
    AddonMediaListProvider, EnumeratedAddonMediaList, EnumeratedAddonMediaListItem,
};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    AddonMediaList, AddonMediaListItemAvailability, AddonMediaListItemKind, EntityId,
};
use sqlx::{Pool, Row};

use crate::db::{Db, DbPool};
use crate::repositories::common::{
    decode_bool_column, decode_enum_column, decode_optional_datetime, encode_datetime, encode_enum,
};

const PLAYBACK_ORDER_CHRONOLOGICAL: i32 = 1;
const COLLECTION_TYPE_SHOW: i32 = 1;
const COLLECTION_TYPE_SEASON: i32 = 2;
const COLLECTION_TYPE_MOVIE: i32 = 10;
const COLLECTION_TYPE_EPISODE: i32 = 20;
const COLLECTION_TYPE_REMOTE_STREAM: i32 = 70;
const OMITTED_BY_PROVIDER_REASON: &str = "omitted_by_provider";

/// Maintains locally persisted Add-on media lists and generated playlists.
pub struct AddonMediaListSynchronizer {
    pool: DbPool,
}

impl AddonMediaListSynchronizer {
    #[must_use]
    pub fn new(pool: DbPool) -> Self {
        Self { pool }
    }

    /// Match a saved Add-on media list against local metadata and update generated
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
            r#"SELECT "Id", "ProviderId", "Kind", "Title", "Year", "Season", "Episode", "Rank", "SourceUrl",
                      "Availability", "Published"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ?
               ORDER BY "Rank", "Id""#,
        )
        .bind(list_id)
        .fetch_all(&self.pool)
        .await
        .repository()?;

        if let Some(playlist_id) = playlist_id {
            // Generated playlists are a projection of the current provider
            // snapshot. Rebuild their items so removed entries and changed
            // ranks cannot leave stale rows behind.
            sqlx::query(r#"DELETE FROM "PlaylistItem" WHERE "PlaylistId" = ?"#)
                .bind(playlist_id)
                .execute(&self.pool)
                .await
                .repository()?;
        }

        for row in rows {
            let item = MatchItem {
                id: row.try_get("Id").repository()?,
                addon_id: list.addon_id.clone(),
                provider_id: row.try_get("ProviderId").repository()?,
                kind: decode_enum_column("AddonMediaListItemKind", &row, "Kind")?,
                title: row
                    .try_get::<Option<String>, _>("Title")
                    .repository()?
                    .unwrap_or_default(),
                year: row.try_get("Year").repository()?,
                season: row.try_get("Season").repository()?,
                episode: row.try_get("Episode").repository()?,
                rank: row.try_get("Rank").repository()?,
                source_url: row.try_get("SourceUrl").repository()?,
                availability: decode_enum_column(
                    "AddonMediaListItemAvailability",
                    &row,
                    "Availability",
                )?,
                guids: self
                    .load_item_guids(row.try_get("Id").repository()?)
                    .await?,
            };

            let matched_id = self.identify_item(&item).await?;
            sqlx::query(r#"UPDATE "AddonMediaListItem" SET "MediaItemId" = ? WHERE "Id" = ?"#)
                .bind(matched_id)
                .bind(item.id)
                .execute(&self.pool)
                .await
                .repository()?;

            if let Some(playlist_id) = playlist_id {
                if !decode_bool_column(&row, "Published")? {
                    continue;
                }
                if let Some(selection) = self.playlist_selection(&item, matched_id).await? {
                    self.upsert_playlist_item(playlist_id, &item, selection)
                        .await?;
                }
            }
        }

        sqlx::query(r#"UPDATE "AddonMediaList" SET "LastMatch" = ? WHERE "Id" = ?"#)
            .bind(encode_datetime(Utc::now()))
            .bind(list_id)
            .execute(&self.pool)
            .await
            .repository()?;

        Ok(())
    }

    /// Update user-editable Add-on media list settings.
    ///
    /// # Errors
    ///
    /// Returns an error when the list does not exist or persistence fails.
    pub async fn update_settings(
        &self,
        list_id: EntityId,
        auto_refresh: bool,
        generate_playlist: bool,
        content_kind_override: ersatzrs_domain::AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<()> {
        let _list = self.load_list(list_id).await?;
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "AutoRefresh" = ?, "GeneratePlaylist" = ?, "ContentKindOverride" = ?,
                   "ChapterInput" = ?,
                   "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
                   "ReviewError" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(auto_refresh)
        .bind(generate_playlist)
        .bind(encode_enum(content_kind_override))
        .bind((!chapter_input.is_empty()).then_some(chapter_input))
        .bind(list_id)
        .execute(&self.pool)
        .await
        .repository()?;

        Ok(())
    }

    /// Delete a Add-on media list and its generated playlist if present.
    ///
    /// # Errors
    ///
    /// Returns an error when persistence fails.
    pub async fn delete_list(&self, list_id: EntityId) -> Result<()> {
        delete_list_library(&self.pool, list_id).await?;
        let maybe_playlist_id = sqlx::query_scalar::<_, Option<EntityId>>(
            r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        .flatten();

        sqlx::query(r#"DELETE FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .execute(&self.pool)
            .await
            .repository()?;

        if let Some(playlist_id) = maybe_playlist_id {
            sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
                .bind(playlist_id)
                .execute(&self.pool)
                .await
                .repository()?;
        }

        Ok(())
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
               FROM "AddonMediaList"
               WHERE "LastMatch" IS NULL OR "LastMatch" <= ?
               ORDER BY "Id""#,
        )
        .bind(encode_datetime(cutoff))
        .fetch_all(&self.pool)
        .await
        .repository()?;

        let mut count = 0;
        for row in rows {
            self.match_list_items(row.try_get("Id").repository()?)
                .await?;
            count += 1;
        }

        Ok(count)
    }

    /// Re-match every persisted list after a provider or Local library scan
    /// changes candidate identities, availability, or replacement validity.
    ///
    /// # Errors
    ///
    /// Returns an error when list loading, matching, or playlist projection fails.
    pub async fn match_all_lists(&self) -> Result<usize> {
        let ids =
            sqlx::query_scalar::<_, EntityId>(r#"SELECT "Id" FROM "AddonMediaList" ORDER BY "Id""#)
                .fetch_all(&self.pool)
                .await
                .repository()?;
        for id in &ids {
            self.match_list_items(*id).await?;
        }
        Ok(ids.len())
    }

    async fn save_list(
        &self,
        list: &AddonMediaList,
        metadata: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
    ) -> Result<EntityId> {
        let provider_metadata = encode_metadata(metadata)?;
        let maybe_existing = sqlx::query(
            r#"SELECT "Id"
               FROM "AddonMediaList"
               WHERE "AddonId" = ? AND "ProviderId" = ?
               ORDER BY "Id"
               LIMIT 1"#,
        )
        .bind(&list.addon_id)
        .bind(&list.provider_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;

        if let Some(row) = maybe_existing {
            let id = row.try_get("Id").repository()?;
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "SourceUrl" = ?, "Name" = ?, "Description" = ?, "ItemCount" = ?,
                       "ContentKindOverride" = ?, "ChapterInput" = ?,
                       "ProviderMetadataJson" = ?, "LastError" = NULL,
                       "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
                       "ReviewError" = NULL
                   WHERE "Id" = ?"#,
            )
            .bind(&list.source_url)
            .bind(&list.name)
            .bind(&list.description)
            .bind(list.item_count)
            .bind(encode_enum(list.content_kind_override))
            .bind((!list.chapter_input.is_empty()).then_some(&list.chapter_input))
            .bind(&provider_metadata)
            .bind(id)
            .execute(&self.pool)
            .await
            .repository()?;
            Ok(id)
        } else {
            let mut transaction = crate::db::begin_write(&self.pool).await.repository()?;
            let result = sqlx::query(
                r#"INSERT INTO "AddonMediaList" (
                    "AddonId", "SourceUrl", "ProviderId", "Name", "Description",
                    "AutoRefresh", "GeneratePlaylist", "ItemCount", "PlaylistId",
                    "LastUpdate", "LastMatch", "LastError", "ContentKindOverride", "ChapterInput",
                    "ProviderMetadataJson", "MetadataOverrideJson"
                   ) VALUES (?, ?, ?, ?, ?, 0, 0, ?, NULL, NULL, NULL, NULL, ?, ?, ?, '{}')"#,
            )
            .bind(&list.addon_id)
            .bind(&list.source_url)
            .bind(&list.provider_id)
            .bind(&list.name)
            .bind(&list.description)
            .bind(list.item_count)
            .bind(encode_enum(list.content_kind_override))
            .bind((!list.chapter_input.is_empty()).then_some(&list.chapter_input))
            .bind(&provider_metadata)
            .execute(&mut *transaction)
            .await
            .repository()?;
            let id = inserted_id(&mut transaction, result).await.repository()?;
            transaction.commit().await.repository()?;
            Ok(id)
        }
    }

    #[allow(clippy::too_many_lines)]
    async fn save_list_items(
        &self,
        list_id: EntityId,
        incoming: &[EnumeratedAddonMediaListItem],
    ) -> Result<AddonMediaList> {
        let incoming_ids = incoming
            .iter()
            .map(|item| item.provider_id.as_str())
            .collect::<HashSet<_>>();
        let existing = self.load_existing_items(list_id).await?;
        let existing_ids = existing
            .iter()
            .map(|item| (item.provider_id.as_str(), item.id))
            .collect::<HashMap<_, _>>();

        let mut tx = crate::db::begin_write(&self.pool).await.repository()?;
        sqlx::query(r#"DELETE FROM "AddonMediaListChapter" WHERE "AddonMediaListId" = ?"#)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        let mut retained_count = 0_usize;
        let mut tombstone_rank = incoming
            .iter()
            .map(|item| item.rank)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        for item in &existing {
            if !incoming_ids.contains(item.provider_id.as_str()) {
                if item.parent_provider_id.is_some() {
                    sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "Id" = ?"#)
                        .bind(item.id)
                        .execute(&mut *tx)
                        .await
                        .repository()?;
                    continue;
                }
                // Provider snapshots can be incomplete because of pagination,
                // transient availability, or permission changes. Retain a
                // provider-neutral tombstone so an operator's pinned Local
                // replacement survives and the same provider ID can recover.
                sqlx::query(
                    r#"UPDATE "AddonMediaListItem"
                       SET "Rank" = ?, "Availability" = ?, "AvailabilityReason" = ?,
                           "AvailabilityUpdated" = ?, "MediaItemId" = NULL
                       WHERE "Id" = ?"#,
                )
                .bind(tombstone_rank)
                .bind(encode_enum(AddonMediaListItemAvailability::Unavailable))
                .bind(OMITTED_BY_PROVIDER_REASON)
                .bind(encode_datetime(Utc::now()))
                .bind(item.id)
                .execute(&mut *tx)
                .await
                .repository()?;
                tombstone_rank = tombstone_rank.saturating_add(1);
                retained_count += 1;
            }
        }

        let mut persisted_ids = HashMap::new();
        for item in incoming {
            let item_id =
                if let Some(existing_id) = existing_ids.get(item.provider_id.as_str()).copied() {
                    let image_urls = encode_image_urls(&item.additional_image_urls)?;
                    sqlx::query(
                        r#"UPDATE "AddonMediaListItem"
                       SET "Kind" = ?, "Rank" = ?, "Title" = ?, "Year" = ?,
                           "DisplayTitle" = ?, "Season" = ?, "Episode" = ?,
                           "SourceUrl" = ?, "Availability" = ?,
                           "AvailabilityReason" = ?, "AvailabilityUpdated" = ?,
                           "ContentKind" = ?, "DurationSeconds" = ?,
                           "ParentProviderId" = ?, "FragmentStartSeconds" = ?,
                           "FragmentEndSeconds" = ?, "AdditionalImageUrls" = ?,
                           "ProviderMetadataJson" = ?,
                           "MediaItemId" = NULL
                       WHERE "Id" = ?"#,
                    )
                    .bind(encode_enum(item.kind))
                    .bind(item.rank)
                    .bind(&item.title)
                    .bind(item.year)
                    .bind(&item.display_title)
                    .bind(item.season)
                    .bind(item.episode)
                    .bind(&item.source_url)
                    .bind(encode_enum(item.availability))
                    .bind(&item.availability_reason)
                    .bind(encode_datetime(Utc::now()))
                    .bind(encode_enum(item.content_kind))
                    .bind(optional_fragment_bound(item.duration_seconds, "duration")?)
                    .bind(&item.parent_provider_id)
                    .bind(optional_fragment_bound(
                        item.fragment_start_seconds,
                        "fragment start",
                    )?)
                    .bind(optional_fragment_bound(
                        item.fragment_end_seconds,
                        "fragment end",
                    )?)
                    .bind(image_urls)
                    .bind(encode_metadata(&item.metadata)?)
                    .bind(existing_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                    sqlx::query(
                        r#"DELETE FROM "AddonMediaListItemGuid" WHERE "AddonMediaListItemId" = ?"#,
                    )
                    .bind(existing_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
                    insert_item_guids(&mut tx, existing_id, &item.guids).await?;
                    existing_id
                } else {
                    let item_id = insert_item(&mut tx, list_id, item).await?;
                    insert_item_guids(&mut tx, item_id, &item.guids).await?;
                    item_id
                };
            persisted_ids.insert(item.provider_id.as_str(), item_id);
        }

        let mut chapter_indexes: HashMap<&str, i32> = HashMap::new();
        for item in incoming {
            let Some(parent_provider_id) = item.parent_provider_id.as_deref() else {
                continue;
            };
            let source_item_id =
                persisted_ids
                    .get(parent_provider_id)
                    .copied()
                    .ok_or_else(|| {
                        RepositoryError::Conflict(format!(
                            "chapter fragment {} references missing parent {parent_provider_id}",
                            item.provider_id
                        ))
                    })?;
            let fragment_item_id = persisted_ids[&item.provider_id.as_str()];
            let chapter_index = chapter_indexes.entry(parent_provider_id).or_default();
            sqlx::query(
                r#"INSERT INTO "AddonMediaListChapter" (
                    "AddonMediaListId", "SourceItemId", "FragmentItemId", "ChapterIndex",
                    "Title", "StartSeconds", "EndSeconds"
                   ) VALUES (?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(list_id)
            .bind(source_item_id)
            .bind(fragment_item_id)
            .bind(*chapter_index)
            .bind(&item.title)
            .bind(fragment_bound(
                item.fragment_start_seconds,
                "fragment start",
            )?)
            .bind(optional_fragment_bound(
                item.fragment_end_seconds,
                "fragment end",
            )?)
            .execute(&mut *tx)
            .await
            .repository()?;
            *chapter_index = chapter_index.saturating_add(1);
        }

        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "LastUpdate" = ?, "ItemCount" = ?, "LastError" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(encode_datetime(Utc::now()))
        .bind(i32::try_from(incoming.len().saturating_add(retained_count)).unwrap_or(i32::MAX))
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;

        self.load_list(list_id).await
    }

    async fn load_list(&self, list_id: EntityId) -> Result<AddonMediaList> {
        let row = sqlx::query(r#"SELECT * FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .fetch_optional(&self.pool)
            .await
            .repository()?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id: list_id,
            })?;

        Ok(AddonMediaList {
            id: row.try_get("Id").repository()?,
            addon_id: row.try_get("AddonId").repository()?,
            source_url: row.try_get("SourceUrl").repository()?,
            provider_id: row.try_get("ProviderId").repository()?,
            name: row.try_get("Name").repository()?,
            description: row.try_get("Description").repository()?,
            item_count: row.try_get("ItemCount").repository()?,
            auto_refresh: decode_bool_column(&row, "AutoRefresh")?,
            generate_playlist: decode_bool_column(&row, "GeneratePlaylist")?,
            content_kind_override: decode_enum_column(
                "AddonMediaListContentKind",
                &row,
                "ContentKindOverride",
            )?,
            presentation_kind: decode_enum_column(
                "AddonMediaListPresentationKind",
                &row,
                "PresentationKind",
            )?,
            requested_presentation_kind: decode_enum_column(
                "AddonMediaListPresentationKind",
                &row,
                "RequestedPresentationKind",
            )?,
            presentation_error: row.try_get("PresentationError").repository()?,
            chapter_input: row
                .try_get::<Option<String>, _>("ChapterInput")
                .repository()?
                .unwrap_or_default(),
            playlist_id: row.try_get("PlaylistId").repository()?,
            last_update: decode_optional_datetime(
                "AddonMediaList.LastUpdate",
                row.try_get("LastUpdate").repository()?,
            )?,
            last_match: decode_optional_datetime(
                "AddonMediaList.LastMatch",
                row.try_get("LastMatch").repository()?,
            )?,
            last_error: row.try_get("LastError").repository()?,
        })
    }

    async fn load_existing_items(&self, list_id: EntityId) -> Result<Vec<ExistingItem>> {
        let rows = sqlx::query(
            r#"SELECT "Id", "ProviderId", "ParentProviderId"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ?
               ORDER BY "Rank", "Id""#,
        )
        .bind(list_id)
        .fetch_all(&self.pool)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                Ok(ExistingItem {
                    id: row.try_get("Id").repository()?,
                    provider_id: row.try_get("ProviderId").repository()?,
                    parent_provider_id: row.try_get("ParentProviderId").repository()?,
                })
            })
            .collect()
    }

    async fn load_item_guids(&self, item_id: EntityId) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"SELECT "Guid"
               FROM "AddonMediaListItemGuid"
               WHERE "AddonMediaListItemId" = ?
               ORDER BY "Id""#,
        )
        .bind(item_id)
        .fetch_all(&self.pool)
        .await
        .repository()?;

        rows.iter()
            .map(|row| {
                Ok(row
                    .try_get::<Option<String>, _>("Guid")
                    .repository()?
                    .unwrap_or_default())
            })
            .collect()
    }

    async fn identify_item(&self, item: &MatchItem) -> Result<Option<EntityId>> {
        match item.kind {
            AddonMediaListItemKind::Movie => self.identify_movie(item).await,
            AddonMediaListItemKind::Show => self.identify_show(item).await,
            AddonMediaListItemKind::Season => self.identify_season(item).await,
            AddonMediaListItemKind::Episode => self.identify_episode(item).await,
            AddonMediaListItemKind::RemoteStream => self.identify_remote_stream(item).await,
        }
    }

    async fn identify_remote_stream(&self, item: &MatchItem) -> Result<Option<EntityId>> {
        if let Some(id) = sqlx::query_scalar(
            r#"SELECT "Id" FROM "RemoteStream"
               WHERE "AddonId" = ? AND "ProviderId" = ?
               ORDER BY "Id" LIMIT 1"#,
        )
        .bind(&item.addon_id)
        .bind(&item.provider_id)
        .fetch_optional(&self.pool)
        .await
        .repository()?
        {
            return Ok(Some(id));
        }
        let Some(source_url) = item.source_url.as_deref() else {
            return Ok(None);
        };
        sqlx::query_scalar(
            r#"SELECT "Id" FROM "RemoteStream" WHERE "Url" = ? ORDER BY "Id" LIMIT 1"#,
        )
        .bind(source_url)
        .fetch_optional(&self.pool)
        .await
        .repository()
    }

    async fn identify_movie(&self, item: &MatchItem) -> Result<Option<EntityId>> {
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
        .repository()
    }

    async fn identify_show(&self, item: &MatchItem) -> Result<Option<EntityId>> {
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
        .repository()
    }

    async fn identify_season(&self, item: &MatchItem) -> Result<Option<EntityId>> {
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
        .repository()
    }

    async fn identify_episode(&self, item: &MatchItem) -> Result<Option<EntityId>> {
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
        .repository()
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
                .await
                .repository()?
            {
                return Ok(Some(id));
            }
        }

        Ok(None)
    }

    async fn ensure_generated_playlist(&self, list: &AddonMediaList) -> Result<EntityId> {
        if let Some(playlist_id) = list.playlist_id {
            return Ok(playlist_id);
        }

        let group_id = system_playlist_group_id(&self.pool).await?;
        let name = list.name.clone();
        let mut transaction = crate::db::begin_write(&self.pool).await.repository()?;
        let result = sqlx::query(
            r#"INSERT INTO "Playlist" ("IsSystem", "Name", "PlaylistGroupId")
               VALUES (1, ?, ?)"#,
        )
        .bind(&name)
        .bind(group_id)
        .execute(&mut *transaction)
        .await
        .repository()?;
        let playlist_id = inserted_id(&mut transaction, result).await.repository()?;

        sqlx::query(r#"UPDATE "AddonMediaList" SET "PlaylistId" = ? WHERE "Id" = ?"#)
            .bind(playlist_id)
            .bind(list.id)
            .execute(&mut *transaction)
            .await
            .repository()?;
        transaction.commit().await.repository()?;

        Ok(playlist_id)
    }

    async fn upsert_playlist_item(
        &self,
        playlist_id: EntityId,
        item: &MatchItem,
        selection: PlaylistSelection,
    ) -> Result<()> {
        let collection_type = selection
            .collection_type
            .unwrap_or_else(|| collection_type_for_kind(item.kind));
        let existing_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "PlaylistItem" WHERE "PlaylistId" = ? AND "Index" = ?"#,
        )
        .bind(playlist_id)
        .bind(item.rank)
        .fetch_optional(&self.pool)
        .await
        .repository()?;

        if let Some(existing_id) = existing_id {
            sqlx::query(
                r#"UPDATE "PlaylistItem"
                   SET "CollectionType" = ?, "MediaItemId" = ?,
                       "CollectionId" = NULL, "MultiCollectionId" = NULL,
                       "SmartCollectionId" = NULL, "PlaybackOrder" = ?,
                       "IncludeInProgramGuide" = 1, "MediaVersionId" = ?,
                       "VideoStreamIndex" = ?, "AudioStreamIndex" = NULL,
                       "SubtitleStreamIndex" = NULL
                   WHERE "Id" = ?"#,
            )
            .bind(collection_type)
            .bind(selection.media_item_id)
            .bind(PLAYBACK_ORDER_CHRONOLOGICAL)
            .bind(selection.media_version_id)
            .bind(selection.video_stream_index)
            .bind(existing_id)
            .execute(&self.pool)
            .await
            .repository()?;
        } else {
            sqlx::query(
                r#"INSERT INTO "PlaylistItem" (
                    "Index", "PlaylistId", "CollectionType", "CollectionId",
                    "MediaItemId", "MultiCollectionId", "SmartCollectionId",
                    "IncludeInProgramGuide", "PlaybackOrder", "PlayAll",
                    "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
                    "SubtitleStreamIndex"
                   ) VALUES (?, ?, ?, NULL, ?, NULL, NULL, 1, ?, 0, ?, ?, NULL, NULL)"#,
            )
            .bind(item.rank)
            .bind(playlist_id)
            .bind(collection_type)
            .bind(selection.media_item_id)
            .bind(PLAYBACK_ORDER_CHRONOLOGICAL)
            .bind(selection.media_version_id)
            .bind(selection.video_stream_index)
            .execute(&self.pool)
            .await
            .repository()?;
        }

        Ok(())
    }

    async fn playlist_selection(
        &self,
        item: &MatchItem,
        provider_media_item_id: Option<EntityId>,
    ) -> Result<Option<PlaylistSelection>> {
        if item.availability != AddonMediaListItemAvailability::Unavailable {
            let projected_episode_id = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item.id)
            .fetch_optional(&self.pool)
            .await
            .repository()?;
            if let Some(episode_id) = projected_episode_id {
                return Ok(Some(PlaylistSelection {
                    media_item_id: episode_id,
                    media_version_id: None,
                    video_stream_index: None,
                    collection_type: Some(COLLECTION_TYPE_EPISODE),
                }));
            }
            return Ok(provider_media_item_id.map(PlaylistSelection::provider));
        }

        let row = sqlx::query(
            r#"SELECT "MediaItemId", "MediaVersionId"
               FROM "AddonMediaListItemReplacement"
               WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(item.id)
        .fetch_optional(&self.pool)
        .await
        .repository()?;
        let Some(row) = row else {
            return Ok(None);
        };
        let media_item_id = row.try_get("MediaItemId").repository()?;
        let media_version_id = row.try_get("MediaVersionId").repository()?;
        let candidate = crate::repositories::load_replacement_candidate(
            &self.pool,
            media_item_id,
            media_version_id,
        )
        .await?;
        Ok(candidate.map(|candidate| PlaylistSelection {
            media_item_id: candidate.media_item_id,
            media_version_id: Some(candidate.media_version_id),
            video_stream_index: Some(candidate.video_stream_index),
            collection_type: Some(candidate.collection_type),
        }))
    }
}

/// Imports and refreshes provider data through an installed add-on.
pub struct AddonMediaListRemoteSynchronizer<P> {
    local: AddonMediaListSynchronizer,
    provider: P,
}

impl<P> AddonMediaListRemoteSynchronizer<P>
where
    P: AddonMediaListProvider,
{
    #[must_use]
    pub fn new(pool: DbPool, provider: P) -> Self {
        Self {
            local: AddonMediaListSynchronizer::new(pool),
            provider,
        }
    }

    /// Add or refresh an add-on media list, then match its persisted items.
    ///
    /// `requested_folder` names the storage folder for a list imported for the
    /// first time; an empty value derives the name from the list itself, and a
    /// list that already owns a folder keeps it.
    ///
    /// # Errors
    ///
    /// Returns an error when the requested folder is unusable or taken, or when
    /// provider invocation or persistence fails.
    pub async fn add_or_refresh(
        &self,
        addon_id: &str,
        source_url: &str,
        requested_folder: Option<&str>,
        content_kind_override: ersatzrs_domain::AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<EntityId> {
        // Validate before enumerating so a rejected folder name costs no
        // provider call and leaves no half-imported list behind.
        let requested_folder = match requested_folder
            .map(str::trim)
            .filter(|folder| !folder.is_empty())
        {
            Some(folder) => {
                Some(validated_folder_name(&self.local.pool, addon_id, source_url, folder).await?)
            }
            None => None,
        };
        let mut enumerated = match self
            .provider
            .enumerate(addon_id, source_url, chapter_input)
            .await
        {
            Ok(enumerated) => enumerated,
            Err(error) => {
                sqlx::query(
                    r#"UPDATE "AddonMediaList" SET "LastError" = ?
                       WHERE "AddonId" = ? AND "SourceUrl" = ?"#,
                )
                .bind(error.to_string())
                .bind(addon_id)
                .bind(source_url)
                .execute(&self.local.pool)
                .await
                .repository()?;
                return Err(error);
            }
        };
        let list = AddonMediaList {
            id: 0,
            addon_id: addon_id.to_owned(),
            source_url: source_url.to_owned(),
            provider_id: enumerated.provider_id.clone(),
            name: enumerated.name.clone(),
            description: enumerated.description.clone(),
            item_count: i32::try_from(enumerated.items.len()).unwrap_or(i32::MAX),
            auto_refresh: false,
            generate_playlist: false,
            content_kind_override,
            presentation_kind: ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            requested_presentation_kind:
                ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            presentation_error: None,
            chapter_input: chapter_input.to_owned(),
            playlist_id: None,
            last_update: None,
            last_match: None,
            last_error: None,
        };
        self.persist_enumerated(&list, &mut enumerated, requested_folder.as_deref())
            .await
    }

    /// Persist one enumerated snapshot, then project and match it.
    ///
    /// Shared by import and by an add-on change so both paths agree on chapter
    /// expansion, managed-library projection, and matching.
    async fn persist_enumerated(
        &self,
        list: &AddonMediaList,
        enumerated: &mut EnumeratedAddonMediaList,
        requested_folder: Option<&str>,
    ) -> Result<EntityId> {
        let list_id = self.local.save_list(list, &enumerated.metadata).await?;
        if list.chapter_input.trim().is_empty() {
            self.expand_item_chapters(list_id, &list.addon_id, enumerated)
                .await?;
        }
        let list = self
            .local
            .save_list_items(list_id, &enumerated.items)
            .await?;
        if has_media_source(&self.local.pool, &list.addon_id).await? {
            ensure_list_library(
                &self.local.pool,
                list.id,
                &list.addon_id,
                &list.name,
                &list.source_url,
                &list.chapter_input,
                requested_folder,
            )
            .await?;
        }
        let review_state: i32 =
            sqlx::query_scalar(r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(list.id)
                .fetch_one(&self.local.pool)
                .await
                .repository()?;
        if review_state == 2 {
            self.local.match_list_items(list.id).await?;
        }
        Ok(list.id)
    }

    /// Move one saved list to a different add-on and re-import it.
    ///
    /// List identity is `(AddonId, ProviderId)`, so the row is re-pointed at the
    /// new identity before the snapshot is stored; otherwise the refresh would
    /// insert a second list and orphan the original. Items are keyed per
    /// provider, so operator item metadata is re-derived rather than migrated.
    ///
    /// # Errors
    ///
    /// Returns an error when the list is unknown, the new add-on cannot
    /// enumerate the source, or another list already owns the resulting
    /// identity.
    pub async fn change_addon(&self, id: EntityId, addon_id: &str) -> Result<()> {
        let row = sqlx::query(
            r#"SELECT "AddonId", "SourceUrl", "ContentKindOverride", "ChapterInput"
               FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.local.pool)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list",
            id,
        })?;
        let current_addon_id: String = row.try_get("AddonId").repository()?;
        let addon_id = addon_id.trim();
        if current_addon_id == addon_id {
            return Ok(());
        }
        let source_url: String = row.try_get("SourceUrl").repository()?;
        let chapter_input = row
            .try_get::<Option<String>, _>("ChapterInput")
            .repository()?
            .unwrap_or_default();
        let content_kind_override = crate::repositories::common::decode_enum_column(
            "AddonMediaListContentKind",
            &row,
            "ContentKindOverride",
        )?;

        // Enumerating first means a failing add-on leaves the existing list
        // untouched instead of half-moved.
        let mut enumerated = self
            .provider
            .enumerate(addon_id, &source_url, &chapter_input)
            .await?;
        let conflicting: Option<EntityId> = sqlx::query_scalar(
            r#"SELECT "Id" FROM "AddonMediaList"
               WHERE "AddonId" = ? AND "ProviderId" = ? AND "Id" <> ?"#,
        )
        .bind(addon_id)
        .bind(&enumerated.provider_id)
        .bind(id)
        .fetch_optional(&self.local.pool)
        .await
        .repository()?;
        if let Some(conflicting) = conflicting {
            return Err(RepositoryError::unavailable(
                "change add-on media list add-on",
                format!("list {conflicting} already uses this add-on for the same source"),
            ));
        }
        sqlx::query(
            r#"UPDATE "AddonMediaList" SET "AddonId" = ?, "ProviderId" = ?, "LastError" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(addon_id)
        .bind(&enumerated.provider_id)
        .bind(id)
        .execute(&self.local.pool)
        .await
        .repository()?;

        let list = AddonMediaList {
            id,
            addon_id: addon_id.to_owned(),
            source_url,
            provider_id: enumerated.provider_id.clone(),
            name: enumerated.name.clone(),
            description: enumerated.description.clone(),
            item_count: i32::try_from(enumerated.items.len()).unwrap_or(i32::MAX),
            auto_refresh: false,
            generate_playlist: false,
            content_kind_override,
            presentation_kind: ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            requested_presentation_kind:
                ersatzrs_domain::AddonMediaListPresentationKind::RemoteStreams,
            presentation_error: None,
            chapter_input,
            playlist_id: None,
            last_update: None,
            last_match: None,
            last_error: None,
        };
        self.persist_enumerated(&list, &mut enumerated, None)
            .await?;
        Ok(())
    }

    async fn expand_item_chapters(
        &self,
        list_id: EntityId,
        addon_id: &str,
        enumerated: &mut EnumeratedAddonMediaList,
    ) -> Result<()> {
        let requests = sqlx::query(
            r#"SELECT "ProviderId", "SourceUrl", "ChapterInput"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
                 AND COALESCE("ChapterInput", '') <> ''
               ORDER BY "Rank", "Id""#,
        )
        .bind(list_id)
        .fetch_all(&self.local.pool)
        .await
        .repository()?;
        let mut next_rank = enumerated
            .items
            .iter()
            .map(|item| item.rank)
            .max()
            .unwrap_or(-1)
            .saturating_add(1);
        for request in requests {
            let provider_id: String = request.try_get("ProviderId").repository()?;
            let stored_url: Option<String> = request.try_get("SourceUrl").repository()?;
            let input: String = request.try_get("ChapterInput").repository()?;
            let source_url = enumerated
                .items
                .iter()
                .find(|item| item.provider_id == provider_id)
                .and_then(|item| item.source_url.as_deref())
                .or(stored_url.as_deref())
                .ok_or_else(|| {
                    RepositoryError::Conflict(format!(
                        "item '{provider_id}' has chapters but no source URL"
                    ))
                })?
                .to_owned();
            let expanded = self
                .provider
                .enumerate(addon_id, &source_url, &input)
                .await?;
            for mut fragment in expanded
                .items
                .into_iter()
                .filter(|item| item.parent_provider_id.is_some())
            {
                fragment.rank = next_rank;
                next_rank = next_rank.saturating_add(1);
                enumerated.items.push(fragment);
            }
        }
        Ok(())
    }

    /// Refresh auto-refresh lists whose provider update is older than one day.
    ///
    /// # Errors
    ///
    /// Returns an error when list loading, API reads, or persistence fails.
    pub async fn refresh_due_auto_lists(&self) -> Result<usize> {
        let cutoff = Utc::now() - Duration::days(1);
        let rows = sqlx::query(
            r#"SELECT "list"."AddonId", "list"."SourceUrl", "list"."ContentKindOverride",
                      "list"."ChapterInput"
               FROM "AddonMediaList" AS "list"
               JOIN "AddonInstallation" AS "addon"
                 ON "addon"."AddonId" = "list"."AddonId"
               WHERE "list"."AutoRefresh" = 1
                 AND "addon"."Enabled" = 1
                 AND ("list"."LastUpdate" IS NULL OR "list"."LastUpdate" <= ?)
               ORDER BY "list"."Id""#,
        )
        .bind(encode_datetime(cutoff))
        .fetch_all(&self.local.pool)
        .await
        .repository()?;

        let mut count = 0;
        for row in rows {
            let addon_id: String = row.try_get("AddonId").repository()?;
            let source_url: String = row.try_get("SourceUrl").repository()?;
            let content_kind_override =
                decode_enum_column("AddonMediaListContentKind", &row, "ContentKindOverride")?;
            let chapter_input = row
                .try_get::<Option<String>, _>("ChapterInput")
                .repository()?
                .unwrap_or_default();
            if self
                .add_or_refresh(
                    &addon_id,
                    &source_url,
                    None,
                    content_kind_override,
                    &chapter_input,
                )
                .await
                .is_ok()
            {
                count += 1;
            }
        }
        Ok(count)
    }
}

async fn system_playlist_group_id(pool: &Pool<Db>) -> Result<EntityId> {
    sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "Id" FROM "PlaylistGroup" WHERE "IsSystem" = 1 ORDER BY "Id" LIMIT 1"#,
    )
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| {
        RepositoryError::unavailable(
            "load system playlist group",
            "system playlist group is not initialized",
        )
    })
}

async fn insert_item(
    tx: &mut sqlx::Transaction<'_, Db>,
    list_id: EntityId,
    item: &EnumeratedAddonMediaListItem,
) -> Result<EntityId> {
    let result = sqlx::query(
        r#"INSERT INTO "AddonMediaListItem" (
            "ProviderId", "DisplayTitle", "Episode", "Kind", "MediaItemId",
            "Rank", "Season", "Title", "AddonMediaListId", "Year", "SourceUrl",
            "Availability", "AvailabilityReason", "AvailabilityUpdated", "ContentKind",
            "CollectionRole", "DurationSeconds", "ParentProviderId", "FragmentStartSeconds",
            "FragmentEndSeconds", "AdditionalImageUrls", "ProviderMetadataJson",
            "MetadataOverrideJson"
           ) VALUES (?, ?, ?, ?, NULL, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, '{}')"#,
    )
    .bind(&item.provider_id)
    .bind(&item.display_title)
    .bind(item.episode)
    .bind(encode_enum(item.kind))
    .bind(item.rank)
    .bind(item.season)
    .bind(&item.title)
    .bind(list_id)
    .bind(item.year)
    .bind(&item.source_url)
    .bind(encode_enum(item.availability))
    .bind(&item.availability_reason)
    .bind(encode_datetime(Utc::now()))
    .bind(encode_enum(item.content_kind))
    .bind(encode_enum(item.collection_role))
    .bind(optional_fragment_bound(item.duration_seconds, "duration")?)
    .bind(&item.parent_provider_id)
    .bind(optional_fragment_bound(
        item.fragment_start_seconds,
        "fragment start",
    )?)
    .bind(optional_fragment_bound(
        item.fragment_end_seconds,
        "fragment end",
    )?)
    .bind(encode_image_urls(&item.additional_image_urls)?)
    .bind(encode_metadata(&item.metadata)?)
    .execute(&mut **tx)
    .await
    .repository()?;
    inserted_id(tx, result).await.repository()
}

async fn insert_item_guids(
    tx: &mut sqlx::Transaction<'_, Db>,
    item_id: EntityId,
    guids: &[String],
) -> Result<()> {
    for guid in guids {
        sqlx::query(r#"INSERT INTO "AddonMediaListItemGuid" ("Guid", "AddonMediaListItemId") VALUES (?, ?)"#)
            .bind(guid)
            .bind(item_id)
            .execute(&mut **tx)
            .await
            .repository()?;
    }
    Ok(())
}

#[derive(Debug)]
struct ExistingItem {
    id: EntityId,
    provider_id: String,
    parent_provider_id: Option<String>,
}

fn encode_image_urls(urls: &[String]) -> Result<Option<String>> {
    if urls.is_empty() {
        return Ok(None);
    }
    serde_json::to_string(urls)
        .map(Some)
        .map_err(|error| RepositoryError::Decode {
            kind: "add-on media list image URLs",
            value: error.to_string(),
        })
}

fn encode_metadata(
    metadata: &ersatzrs_core::repositories::AddonMediaListNfoMetadata,
) -> Result<String> {
    serde_json::to_string(metadata).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata document",
        value: error.to_string(),
    })
}

fn fragment_bound(value: Option<u64>, kind: &'static str) -> Result<i64> {
    optional_fragment_bound(value, kind)?.ok_or_else(|| {
        RepositoryError::Conflict(format!("{kind} is required for a chapter fragment"))
    })
}

fn optional_fragment_bound(value: Option<u64>, kind: &'static str) -> Result<Option<i64>> {
    value
        .map(|value| {
            i64::try_from(value).map_err(|_| RepositoryError::Decode {
                kind,
                value: value.to_string(),
            })
        })
        .transpose()
}

#[derive(Debug)]
struct MatchItem {
    id: EntityId,
    addon_id: String,
    provider_id: String,
    kind: AddonMediaListItemKind,
    title: String,
    year: Option<i32>,
    season: Option<i32>,
    episode: Option<i32>,
    rank: i32,
    guids: Vec<String>,
    source_url: Option<String>,
    availability: AddonMediaListItemAvailability,
}

#[derive(Clone, Copy, Debug)]
struct PlaylistSelection {
    media_item_id: EntityId,
    media_version_id: Option<EntityId>,
    video_stream_index: Option<i32>,
    collection_type: Option<i32>,
}

impl PlaylistSelection {
    const fn provider(media_item_id: EntityId) -> Self {
        Self {
            media_item_id,
            media_version_id: None,
            video_stream_index: None,
            collection_type: None,
        }
    }
}

const fn collection_type_for_kind(kind: AddonMediaListItemKind) -> i32 {
    match kind {
        AddonMediaListItemKind::Movie => COLLECTION_TYPE_MOVIE,
        AddonMediaListItemKind::Show => COLLECTION_TYPE_SHOW,
        AddonMediaListItemKind::Season => COLLECTION_TYPE_SEASON,
        AddonMediaListItemKind::Episode => COLLECTION_TYPE_EPISODE,
        AddonMediaListItemKind::RemoteStream => COLLECTION_TYPE_REMOTE_STREAM,
    }
}
