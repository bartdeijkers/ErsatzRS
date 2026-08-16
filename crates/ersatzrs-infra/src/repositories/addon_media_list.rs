use async_trait::async_trait;
use ersatzrs_core::repositories::{
    AddonMediaListAdminForm, AddonMediaListAdminRow, AddonMediaListItemAdminRow,
    AddonMediaListReplacementCandidate, AddonMediaListRepository, RepositoryError, Result,
};
use ersatzrs_domain::{AddonMediaListContentKind, EntityId};
use sqlx::{Pool, Row};

use crate::addon_media_list::InstalledAddonMediaListProvider;
use crate::addon_media_list_sync::{AddonMediaListRemoteSynchronizer, AddonMediaListSynchronizer};
use crate::repositories::common::{decode_bool_column, decode_optional_datetime};
use crate::repositories::SqlxResultExt as _;

const PAGE_SIZE_KEY: &str = "pages.addon_media_lists.page_size";

#[derive(Clone)]
pub struct SqliteAddonMediaListRepository(pub Pool<crate::db::Db>);

#[async_trait]
impl AddonMediaListRepository for SqliteAddonMediaListRepository {
    async fn list_admin(
        &self,
        page: u32,
        page_size: u32,
    ) -> Result<(Vec<AddonMediaListAdminRow>, u64)> {
        let total: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaList""#)
            .fetch_one(&self.0)
            .await
            .repository()?;
        let rows = sqlx::query(
            r#"SELECT "list"."Id", "list"."AddonId", "list"."ProviderId",
                      "list"."SourceUrl", COALESCE("list"."Name", '') AS "Name",
                      "list"."ItemCount", "list"."AutoRefresh", "list"."GeneratePlaylist",
                      "list"."LastUpdate", "list"."LastMatch", "list"."LastError",
                      COUNT("matched"."Id") AS "MatchCount"
               FROM "AddonMediaList" AS "list"
               LEFT JOIN "AddonMediaListItem" AS "matched"
                 ON "matched"."AddonMediaListId" = "list"."Id"
                AND "matched"."MediaItemId" IS NOT NULL
               GROUP BY "list"."Id", "list"."AddonId", "list"."ProviderId",
                        "list"."SourceUrl", "list"."Name", "list"."ItemCount",
                        "list"."AutoRefresh", "list"."GeneratePlaylist",
                        "list"."LastUpdate", "list"."LastMatch", "list"."LastError"
               ORDER BY "list"."Name", "list"."Id"
               LIMIT ? OFFSET ?"#,
        )
        .bind(i64::from(page_size))
        .bind(i64::from(page) * i64::from(page_size))
        .fetch_all(&self.0)
        .await
        .repository()?;

        let rows = rows
            .iter()
            .map(|row| {
                let match_count: i64 = row.try_get("MatchCount").repository()?;
                Ok(AddonMediaListAdminRow {
                    id: row.try_get("Id").repository()?,
                    addon_id: row.try_get("AddonId").repository()?,
                    provider_id: row.try_get("ProviderId").repository()?,
                    source_url: row.try_get("SourceUrl").repository()?,
                    name: row.try_get("Name").repository()?,
                    item_count: row.try_get("ItemCount").repository()?,
                    match_count: i32::try_from(match_count).map_err(|_| {
                        RepositoryError::Decode {
                            kind: "Add-on media list match count",
                            value: match_count.to_string(),
                        }
                    })?,
                    auto_refresh: decode_bool_column(row, "AutoRefresh")?,
                    generate_playlist: decode_bool_column(row, "GeneratePlaylist")?,
                    last_update: decode_optional_datetime(
                        "AddonMediaList.LastUpdate",
                        row.try_get("LastUpdate").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    last_match: decode_optional_datetime(
                        "AddonMediaList.LastMatch",
                        row.try_get("LastMatch").repository()?,
                    )?
                    .map(|value| value.to_rfc3339()),
                    last_error: row.try_get("LastError").repository()?,
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let total = u64::try_from(total).map_err(|_| RepositoryError::Decode {
            kind: "Add-on media list count",
            value: total.to_string(),
        })?;
        Ok((rows, total))
    }

    async fn get_admin_form(&self, id: EntityId) -> Result<Option<AddonMediaListAdminForm>> {
        let row = sqlx::query(
            r#"SELECT "Id", "AddonId", "SourceUrl", "AutoRefresh", "GeneratePlaylist",
                      "ContentKindOverride", "ChapterInput"
               FROM "AddonMediaList"
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_optional(&self.0)
        .await
        .repository()?;
        row.map(|row| {
            Ok(AddonMediaListAdminForm {
                id: row.try_get("Id").repository()?,
                addon_id: row.try_get("AddonId").repository()?,
                source_url: row.try_get("SourceUrl").repository()?,
                auto_refresh: decode_bool_column(&row, "AutoRefresh")?,
                generate_playlist: decode_bool_column(&row, "GeneratePlaylist")?,
                content_kind_override: crate::repositories::common::decode_enum_column(
                    "AddonMediaListContentKind",
                    &row,
                    "ContentKindOverride",
                )?,
                chapter_input: row
                    .try_get::<Option<String>, _>("ChapterInput")
                    .repository()?
                    .unwrap_or_default(),
            })
        })
        .transpose()
    }

    async fn list_items(&self, id: EntityId) -> Result<Vec<AddonMediaListItemAdminRow>> {
        let rows = sqlx::query(
            r#"SELECT "item"."Id", "item"."Rank", "item"."DisplayTitle",
                      "item"."Availability", "item"."AvailabilityReason",
                      "replacement"."MediaItemId" AS "ReplacementMediaItemId",
                      "replacement"."MediaVersionId" AS "ReplacementMediaVersionId"
               FROM "AddonMediaListItem" AS "item"
               LEFT JOIN "AddonMediaListItemReplacement" AS "replacement"
                 ON "replacement"."AddonMediaListItemId" = "item"."Id"
               WHERE "item"."AddonMediaListId" = ?
               ORDER BY "item"."Rank", "item"."Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut result = Vec::with_capacity(rows.len());
        for row in rows {
            let replacement_media_item_id = row
                .try_get::<Option<EntityId>, _>("ReplacementMediaItemId")
                .repository()?;
            let replacement_media_version_id = row
                .try_get::<Option<EntityId>, _>("ReplacementMediaVersionId")
                .repository()?;
            let candidate = match replacement_media_item_id.zip(replacement_media_version_id) {
                Some((media_item_id, media_version_id)) => {
                    load_replacement_candidate(&self.0, media_item_id, media_version_id).await?
                }
                None => None,
            };
            result.push(AddonMediaListItemAdminRow {
                id: row.try_get("Id").repository()?,
                rank: row.try_get("Rank").repository()?,
                display_title: row.try_get("DisplayTitle").repository()?,
                availability: crate::repositories::common::decode_enum_column(
                    "AddonMediaListItemAvailability",
                    &row,
                    "Availability",
                )?,
                availability_reason: row.try_get("AvailabilityReason").repository()?,
                replacement_media_item_id,
                replacement_media_version_id,
                replacement_title: candidate.as_ref().map(|item| item.title.clone()),
                replacement_path: candidate.as_ref().map(|item| item.path.clone()),
                replacement_valid: candidate.is_some(),
            });
        }
        Ok(result)
    }

    async fn search_replacement_candidates(
        &self,
        query: &str,
        limit: u32,
    ) -> Result<Vec<AddonMediaListReplacementCandidate>> {
        let pattern = format!("%{}%", query.trim());
        let rows = sqlx::query(replacement_candidate_query(true))
            .bind(&pattern)
            .bind(i64::from(limit.clamp(1, 100)))
            .fetch_all(&self.0)
            .await
            .repository()?;
        rows.iter().map(decode_replacement_candidate).collect()
    }

    async fn set_replacement(
        &self,
        item_id: EntityId,
        replacement: Option<(EntityId, EntityId)>,
    ) -> Result<()> {
        let list_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "AddonMediaListId" FROM "AddonMediaListItem" WHERE "Id" = ?"#,
        )
        .bind(item_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list item",
            id: item_id,
        })?;
        if let Some((media_item_id, media_version_id)) = replacement {
            if load_replacement_candidate(&self.0, media_item_id, media_version_id)
                .await?
                .is_none()
            {
                return Err(RepositoryError::Conflict(
                    "replacement must be an available Local video version".to_owned(),
                ));
            }
            let now = chrono::Utc::now().to_rfc3339();
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaListItemReplacement"
                   SET "MediaItemId" = ?, "MediaVersionId" = ?, "UpdatedAt" = ?
                   WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(media_item_id)
            .bind(media_version_id)
            .bind(&now)
            .bind(item_id)
            .execute(&self.0)
            .await
            .repository()?;
            if updated.rows_affected() == 0 {
                sqlx::query(
                    r#"INSERT INTO "AddonMediaListItemReplacement" (
                        "AddonMediaListItemId", "MediaItemId", "MediaVersionId", "CreatedAt", "UpdatedAt"
                       ) VALUES (?, ?, ?, ?, ?)"#,
                )
                .bind(item_id)
                .bind(media_item_id)
                .bind(media_version_id)
                .bind(&now)
                .bind(&now)
                .execute(&self.0)
                .await
                .repository()?;
            }
        } else {
            sqlx::query(
                r#"DELETE FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId" = ?"#,
            )
            .bind(item_id)
            .execute(&self.0)
            .await
            .repository()?;
        }
        AddonMediaListSynchronizer::new(self.0.clone())
            .match_list_items(list_id)
            .await
    }

    async fn add_or_refresh(
        &self,
        addon_id: &str,
        source_url: &str,
        requested_folder: Option<&str>,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<EntityId> {
        let provider = InstalledAddonMediaListProvider::new(self.0.clone());
        AddonMediaListRemoteSynchronizer::new(self.0.clone(), provider)
            .add_or_refresh(
                addon_id,
                source_url,
                requested_folder,
                content_kind_override,
                chapter_input,
            )
            .await
    }

    async fn managed_library_id(&self, id: EntityId) -> Result<Option<EntityId>> {
        crate::addon_media_storage::list_library_id(&self.0, id).await
    }

    async fn match_items(&self, id: EntityId) -> Result<()> {
        AddonMediaListSynchronizer::new(self.0.clone())
            .match_list_items(id)
            .await
    }

    async fn update_settings(
        &self,
        id: EntityId,
        auto_refresh: bool,
        generate_playlist: bool,
        content_kind_override: AddonMediaListContentKind,
        chapter_input: &str,
    ) -> Result<()> {
        let existing = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        if existing.chapter_input != chapter_input
            || existing.content_kind_override != content_kind_override
        {
            let provider = InstalledAddonMediaListProvider::new(self.0.clone());
            AddonMediaListRemoteSynchronizer::new(self.0.clone(), provider)
                .add_or_refresh(
                    &existing.addon_id,
                    &existing.source_url,
                    None,
                    content_kind_override,
                    chapter_input,
                )
                .await?;
        }
        AddonMediaListSynchronizer::new(self.0.clone())
            .update_settings(
                id,
                auto_refresh,
                generate_playlist,
                content_kind_override,
                chapter_input,
            )
            .await
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        AddonMediaListSynchronizer::new(self.0.clone())
            .delete_list(id)
            .await
    }

    async fn get_page_size(&self) -> Result<u32> {
        let value: Option<String> =
            sqlx::query_scalar(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
                .bind(PAGE_SIZE_KEY)
                .fetch_optional(&self.0)
                .await
                .repository()?;
        Ok(value
            .and_then(|value| value.parse::<u32>().ok())
            .filter(|value| *value > 0)
            .unwrap_or(10))
    }

    async fn set_page_size(&self, page_size: u32) -> Result<()> {
        let updated = sqlx::query(r#"UPDATE "ConfigElement" SET "Value" = ? WHERE "Key" = ?"#)
            .bind(page_size.to_string())
            .bind(PAGE_SIZE_KEY)
            .execute(&self.0)
            .await
            .repository()?;
        if updated.rows_affected() == 0 {
            sqlx::query(r#"INSERT INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
                .bind(PAGE_SIZE_KEY)
                .bind(page_size.to_string())
                .execute(&self.0)
                .await
                .repository()?;
        }
        Ok(())
    }
}

fn replacement_candidate_query(search: bool) -> &'static str {
    if search {
        r#"SELECT "item"."Id" AS "MediaItemId", "version"."Id" AS "MediaVersionId",
                  COALESCE(
                    (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    "file"."Path", '') AS "Title",
                  COALESCE("file"."Path", '') AS "Path",
                  CASE
                    WHEN "version"."MovieId" IS NOT NULL THEN 10
                    WHEN "version"."EpisodeId" IS NOT NULL THEN 20
                    WHEN "version"."MusicVideoId" IS NOT NULL THEN 30
                    WHEN "version"."OtherVideoId" IS NOT NULL THEN 40
                  END AS "CollectionType",
                  (SELECT MIN("stream"."Index") FROM "MediaStream" AS "stream"
                   WHERE "stream"."MediaVersionId" = "version"."Id"
                     AND "stream"."MediaStreamKind" = 0 AND "stream"."AttachedPic" = 0)
                    AS "VideoStreamIndex"
           FROM "MediaItem" AS "item"
           JOIN "LibraryPath" AS "path" ON "path"."Id" = "item"."LibraryPathId"
           JOIN "Library" AS "library" ON "library"."Id" = "path"."LibraryId"
           JOIN "LocalMediaSource" AS "local" ON "local"."Id" = "library"."MediaSourceId"
           JOIN "MediaVersion" AS "version" ON (
                "version"."MovieId" = "item"."Id"
                OR "version"."EpisodeId" = "item"."Id"
                OR "version"."MusicVideoId" = "item"."Id"
                OR "version"."OtherVideoId" = "item"."Id")
           JOIN "MediaFile" AS "file" ON "file"."MediaVersionId" = "version"."Id"
           WHERE "item"."State" = 0
             AND EXISTS (
                SELECT 1 FROM "MediaStream" AS "stream"
                WHERE "stream"."MediaVersionId" = "version"."Id"
                  AND "stream"."MediaStreamKind" = 0 AND "stream"."AttachedPic" = 0)
             AND LOWER(COALESCE(
                    (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    "file"."Path", '')) LIKE LOWER(?)
           ORDER BY "Title", "file"."Path", "version"."Id"
           LIMIT ?"#
    } else {
        r#"SELECT "item"."Id" AS "MediaItemId", "version"."Id" AS "MediaVersionId",
                  COALESCE(
                    (SELECT "Title" FROM "MovieMetadata" WHERE "MovieId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "EpisodeMetadata" WHERE "EpisodeId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "MusicVideoMetadata" WHERE "MusicVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    (SELECT "Title" FROM "OtherVideoMetadata" WHERE "OtherVideoId" = "item"."Id" ORDER BY "Id" LIMIT 1),
                    "file"."Path", '') AS "Title",
                  COALESCE("file"."Path", '') AS "Path",
                  CASE
                    WHEN "version"."MovieId" IS NOT NULL THEN 10
                    WHEN "version"."EpisodeId" IS NOT NULL THEN 20
                    WHEN "version"."MusicVideoId" IS NOT NULL THEN 30
                    WHEN "version"."OtherVideoId" IS NOT NULL THEN 40
                  END AS "CollectionType",
                  (SELECT MIN("stream"."Index") FROM "MediaStream" AS "stream"
                   WHERE "stream"."MediaVersionId" = "version"."Id"
                     AND "stream"."MediaStreamKind" = 0 AND "stream"."AttachedPic" = 0)
                    AS "VideoStreamIndex"
           FROM "MediaItem" AS "item"
           JOIN "LibraryPath" AS "path" ON "path"."Id" = "item"."LibraryPathId"
           JOIN "Library" AS "library" ON "library"."Id" = "path"."LibraryId"
           JOIN "LocalMediaSource" AS "local" ON "local"."Id" = "library"."MediaSourceId"
           JOIN "MediaVersion" AS "version" ON (
                "version"."MovieId" = "item"."Id"
                OR "version"."EpisodeId" = "item"."Id"
                OR "version"."MusicVideoId" = "item"."Id"
                OR "version"."OtherVideoId" = "item"."Id")
           JOIN "MediaFile" AS "file" ON "file"."MediaVersionId" = "version"."Id"
           WHERE "item"."Id" = ? AND "version"."Id" = ? AND "item"."State" = 0
             AND EXISTS (
                SELECT 1 FROM "MediaStream" AS "stream"
                WHERE "stream"."MediaVersionId" = "version"."Id"
                  AND "stream"."MediaStreamKind" = 0 AND "stream"."AttachedPic" = 0)
           LIMIT 1"#
    }
}

pub(crate) async fn load_replacement_candidate(
    pool: &Pool<crate::db::Db>,
    media_item_id: EntityId,
    media_version_id: EntityId,
) -> Result<Option<AddonMediaListReplacementCandidate>> {
    sqlx::query(replacement_candidate_query(false))
        .bind(media_item_id)
        .bind(media_version_id)
        .fetch_optional(pool)
        .await
        .repository()?
        .as_ref()
        .map(decode_replacement_candidate)
        .transpose()
}

fn decode_replacement_candidate(
    row: &crate::db::DbRow,
) -> Result<AddonMediaListReplacementCandidate> {
    Ok(AddonMediaListReplacementCandidate {
        media_item_id: row.try_get("MediaItemId").repository()?,
        media_version_id: row.try_get("MediaVersionId").repository()?,
        title: row.try_get("Title").repository()?,
        path: row.try_get("Path").repository()?,
        collection_type: row.try_get("CollectionType").repository()?,
        video_stream_index: row.try_get("VideoStreamIndex").repository()?,
    })
}
