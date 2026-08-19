use async_trait::async_trait;
use ersatzrs_core::repositories::{
    AddonMediaListAdminForm, AddonMediaListAdminRow, AddonMediaListItemAdminRow,
    AddonMediaListItemCounts, AddonMediaListItemDetail, AddonMediaListNfoForm,
    AddonMediaListNfoMetadata, AddonMediaListReplacementCandidate, AddonMediaListRepository,
    RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListPresentationKind,
    EntityId,
};
use sqlx::{Pool, Row};
use std::path::{Path, PathBuf};

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
                      "list"."PresentationKind", "list"."RequestedPresentationKind",
                      "list"."PresentationError",
                      COUNT("matched"."Id") AS "MatchCount"
               FROM "AddonMediaList" AS "list"
               LEFT JOIN "AddonMediaListItem" AS "matched"
                 ON "matched"."AddonMediaListId" = "list"."Id"
                AND "matched"."MediaItemId" IS NOT NULL
               GROUP BY "list"."Id", "list"."AddonId", "list"."ProviderId",
                        "list"."SourceUrl", "list"."Name", "list"."ItemCount",
                        "list"."AutoRefresh", "list"."GeneratePlaylist",
                        "list"."LastUpdate", "list"."LastMatch", "list"."LastError",
                        "list"."PresentationKind", "list"."RequestedPresentationKind",
                        "list"."PresentationError"
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
                    presentation_kind: crate::repositories::common::decode_enum_column(
                        "AddonMediaListPresentationKind",
                        row,
                        "PresentationKind",
                    )?,
                    requested_presentation_kind: crate::repositories::common::decode_enum_column(
                        "AddonMediaListPresentationKind",
                        row,
                        "RequestedPresentationKind",
                    )?,
                    presentation_error: row.try_get("PresentationError").repository()?,
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
                      "ContentKindOverride", "PresentationKind",
                      "RequestedPresentationKind", "PresentationError", "ChapterInput"
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
                presentation_kind: crate::repositories::common::decode_enum_column(
                    "AddonMediaListPresentationKind",
                    &row,
                    "PresentationKind",
                )?,
                requested_presentation_kind: crate::repositories::common::decode_enum_column(
                    "AddonMediaListPresentationKind",
                    &row,
                    "RequestedPresentationKind",
                )?,
                presentation_error: row.try_get("PresentationError").repository()?,
                legacy_chapter_input: row
                    .try_get::<Option<String>, _>("ChapterInput")
                    .repository()?
                    .unwrap_or_default(),
            })
        })
        .transpose()
    }

    async fn list_items(&self, id: EntityId) -> Result<Vec<AddonMediaListItemAdminRow>> {
        self.query_items(id, None, 0, None).await
    }

    async fn list_items_page(
        &self,
        id: EntityId,
        availability: AddonMediaListItemAvailability,
        page: u32,
        page_size: Option<u32>,
    ) -> Result<Vec<AddonMediaListItemAdminRow>> {
        self.query_items(id, Some(availability), page, page_size)
            .await
    }

    async fn item_counts(&self, id: EntityId) -> Result<AddonMediaListItemCounts> {
        let rows = sqlx::query(
            r#"SELECT "Availability", COUNT(*) AS "Total"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ?
               GROUP BY "Availability""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let mut counts = AddonMediaListItemCounts::default();
        for row in rows {
            let total = u64::try_from(row.try_get::<i64, _>("Total").repository()?).unwrap_or(0);
            match crate::repositories::common::decode_enum_column(
                "AddonMediaListItemAvailability",
                &row,
                "Availability",
            )? {
                AddonMediaListItemAvailability::Available => counts.available = total,
                AddonMediaListItemAvailability::Unavailable => counts.unavailable = total,
                AddonMediaListItemAvailability::Unknown => counts.unknown = total,
            }
        }
        Ok(counts)
    }

    async fn update_item_content_kind(
        &self,
        item_id: EntityId,
        content_kind: AddonMediaListContentKind,
    ) -> Result<()> {
        let updated =
            sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ContentKind" = ? WHERE "Id" = ?"#)
                .bind(crate::repositories::common::encode_enum(content_kind))
                .bind(item_id)
                .execute(&self.0)
                .await
                .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list item",
                id: item_id,
            });
        }
        Ok(())
    }

    async fn apply_list_content_kind(
        &self,
        id: EntityId,
        content_kind: AddonMediaListContentKind,
    ) -> Result<u64> {
        let encoded = crate::repositories::common::encode_enum(content_kind);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        // The list value is the bulk default, so it is stored as well: entries
        // enumerated later inherit the same classification.
        sqlx::query(r#"UPDATE "AddonMediaList" SET "ContentKindOverride" = ? WHERE "Id" = ?"#)
            .bind(encoded)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "ContentKind" = ? WHERE "AddonMediaListId" = ?"#,
        )
        .bind(encoded)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(updated.rows_affected())
    }

    async fn get_item_detail(
        &self,
        list_id: EntityId,
        item_id: EntityId,
    ) -> Result<Option<AddonMediaListItemDetail>> {
        let items = self.list_items(list_id).await?;
        let Some(item) = items.iter().find(|item| item.id == item_id).cloned() else {
            return Ok(None);
        };
        let path = item_nfo_path_for(&self.0, list_id, item_id).await?;
        let nfo = crate::addon_media_nfo::read_form(path.as_deref()).map_err(map_nfo_error)?;
        let child_ids = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "FragmentItemId" FROM "AddonMediaListChapter"
               WHERE "SourceItemId" = ? ORDER BY "ChapterIndex", "Id""#,
        )
        .bind(item_id)
        .fetch_all(&self.0)
        .await
        .repository()?;
        let chapters = child_ids
            .into_iter()
            .filter_map(|id| items.iter().find(|item| item.id == id).cloned())
            .collect();
        Ok(Some(AddonMediaListItemDetail {
            item,
            nfo,
            chapters,
        }))
    }

    async fn get_show_nfo(&self, id: EntityId) -> Result<AddonMediaListNfoForm> {
        let path = show_nfo_path_for(&self.0, id).await?;
        crate::addon_media_nfo::read_form(path.as_deref()).map_err(map_nfo_error)
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

    async fn change_addon(&self, id: EntityId, addon_id: &str) -> Result<()> {
        let provider = InstalledAddonMediaListProvider::new(self.0.clone());
        AddonMediaListRemoteSynchronizer::new(self.0.clone(), provider)
            .change_addon(id, addon_id)
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
        if existing.legacy_chapter_input != chapter_input
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

    async fn request_presentation(
        &self,
        id: EntityId,
        presentation_kind: AddonMediaListPresentationKind,
        show_title: Option<&str>,
    ) -> Result<()> {
        let _form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        // A presentation without a projection would be stored and then quietly
        // do nothing, leaving the list claiming a library kind it never entered.
        if !presentation_kind.is_projected() {
            return Err(RepositoryError::Conflict(format!(
                "presentation {presentation_kind:?} has no projection yet"
            )));
        }
        if presentation_kind == AddonMediaListPresentationKind::TelevisionShow {
            let path = show_nfo_path_for(&self.0, id).await?.ok_or_else(|| {
                RepositoryError::Conflict("managed list storage is unavailable".to_owned())
            })?;
            let mut nfo = crate::addon_media_nfo::read_form(Some(&path)).map_err(map_nfo_error)?;
            if nfo.metadata.title.trim().is_empty() {
                show_title
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        RepositoryError::Conflict("a television show title is required".to_owned())
                    })?
                    .clone_into(&mut nfo.metadata.title);
                crate::addon_media_nfo::save_user(&path, "tvshow", &nfo.metadata, &nfo.etag)
                    .map_err(map_nfo_error)?;
            }
        }
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "RequestedPresentationKind" = ?, "PresentationError" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(crate::repositories::common::encode_enum(presentation_kind))
        .bind(id)
        .execute(&self.0)
        .await
        .repository()?;

        let result = reconcile_tv_projection(&self.0, id, presentation_kind).await;
        if let Err(error) = &result {
            let _ = sqlx::query(
                r#"UPDATE "AddonMediaList" SET "PresentationError" = ? WHERE "Id" = ?"#,
            )
            .bind(error.to_string())
            .bind(id)
            .execute(&self.0)
            .await;
        }
        result?;
        AddonMediaListSynchronizer::new(self.0.clone())
            .match_list_items(id)
            .await
    }

    async fn save_show_nfo(
        &self,
        id: EntityId,
        metadata: &AddonMediaListNfoMetadata,
        expected_etag: &str,
    ) -> Result<()> {
        let path = show_nfo_path_for(&self.0, id).await?.ok_or_else(|| {
            RepositoryError::Conflict("managed list storage is unavailable".to_owned())
        })?;
        crate::addon_media_nfo::save_user(&path, "tvshow", metadata, expected_etag)
            .map_err(map_nfo_error)?;
        let form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        if form.requested_presentation_kind == AddonMediaListPresentationKind::TelevisionShow {
            reconcile_tv_projection(&self.0, id, form.requested_presentation_kind).await?;
            AddonMediaListSynchronizer::new(self.0.clone())
                .match_list_items(id)
                .await?;
        }
        Ok(())
    }

    async fn save_item_nfo(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        metadata: &AddonMediaListNfoMetadata,
        expected_etag: &str,
    ) -> Result<()> {
        let path = item_nfo_path_for(&self.0, list_id, item_id)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict(
                    "item must be scanned before its NFO can be edited".to_owned(),
                )
            })?;
        let presentation =
            self.get_admin_form(list_id)
                .await?
                .ok_or(RepositoryError::NotFound {
                    kind: "add-on media list",
                    id: list_id,
                })?;
        let root = if presentation.requested_presentation_kind
            == AddonMediaListPresentationKind::TelevisionShow
        {
            "episodedetails"
        } else {
            // The entry classifies itself. The list-wide value is only the
            // default new entries inherit, so a mixed list would otherwise
            // write every item under one wrong NFO root.
            let item_kind = sqlx::query_scalar::<_, i32>(
                r#"SELECT "ContentKind" FROM "AddonMediaListItem"
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(item_id)
            .bind(list_id)
            .fetch_optional(&self.0)
            .await
            .repository()?
            .and_then(
                <AddonMediaListContentKind as crate::repositories::common::DbEnum>::from_db_int,
            )
            .unwrap_or(presentation.content_kind_override);
            let item_kind = match item_kind {
                AddonMediaListContentKind::Auto => presentation.content_kind_override,
                kind => kind,
            };
            nfo_root_for_content_kind(item_kind)
        };
        crate::addon_media_nfo::save_user(&path, root, metadata, expected_etag)
            .map_err(map_nfo_error)?;
        if presentation.requested_presentation_kind
            == AddonMediaListPresentationKind::TelevisionShow
        {
            reconcile_tv_projection(&self.0, list_id, presentation.requested_presentation_kind)
                .await?;
            AddonMediaListSynchronizer::new(self.0.clone())
                .match_list_items(list_id)
                .await?;
        }
        Ok(())
    }

    async fn reset_nfo(&self, list_id: EntityId, item_id: Option<EntityId>) -> Result<()> {
        let path = if let Some(item_id) = item_id {
            item_nfo_path_for(&self.0, list_id, item_id).await?
        } else {
            show_nfo_path_for(&self.0, list_id).await?
        }
        .ok_or_else(|| RepositoryError::Conflict("managed NFO path is unavailable".to_owned()))?;
        crate::addon_media_nfo::reset(&path).map_err(map_nfo_error)
    }

    async fn update_item_chapters(&self, item_id: EntityId, chapter_input: &str) -> Result<()> {
        let row = sqlx::query(
            r#"SELECT "item"."AddonMediaListId", "item"."ParentProviderId",
                      "list"."AddonId", "list"."SourceUrl", "list"."ContentKindOverride"
               FROM "AddonMediaListItem" AS "item"
               JOIN "AddonMediaList" AS "list" ON "list"."Id" = "item"."AddonMediaListId"
               WHERE "item"."Id" = ?"#,
        )
        .bind(item_id)
        .fetch_optional(&self.0)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "add-on media list item",
            id: item_id,
        })?;
        if row
            .try_get::<Option<String>, _>("ParentProviderId")
            .repository()?
            .is_some()
        {
            return Err(RepositoryError::Conflict(
                "chapters can only be edited on a full source item".to_owned(),
            ));
        }
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ChapterInput" = ? WHERE "Id" = ?"#)
            .bind((!chapter_input.trim().is_empty()).then_some(chapter_input.trim()))
            .bind(item_id)
            .execute(&self.0)
            .await
            .repository()?;
        let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
        let provider = InstalledAddonMediaListProvider::new(self.0.clone());
        AddonMediaListRemoteSynchronizer::new(self.0.clone(), provider)
            .add_or_refresh(
                row.try_get::<String, _>("AddonId").repository()?.as_str(),
                row.try_get::<String, _>("SourceUrl").repository()?.as_str(),
                None,
                crate::repositories::common::decode_enum_column(
                    "AddonMediaListContentKind",
                    &row,
                    "ContentKindOverride",
                )?,
                "",
            )
            .await?;
        AddonMediaListSynchronizer::new(self.0.clone())
            .match_list_items(list_id)
            .await
    }

    async fn delete(&self, id: EntityId) -> Result<()> {
        reconcile_tv_projection(&self.0, id, AddonMediaListPresentationKind::RemoteStreams).await?;
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

fn decode_optional_u64(row: &crate::db::DbRow, column: &'static str) -> Result<Option<u64>> {
    row.try_get::<Option<i64>, _>(column)
        .repository()?
        .map(|value| {
            u64::try_from(value).map_err(|_| RepositoryError::Decode {
                kind: column,
                value: value.to_string(),
            })
        })
        .transpose()
}

fn map_nfo_error(error: crate::addon_media_nfo::AddonMediaNfoError) -> RepositoryError {
    match error {
        crate::addon_media_nfo::AddonMediaNfoError::Conflict => {
            RepositoryError::Conflict(error.to_string())
        }
        crate::addon_media_nfo::AddonMediaNfoError::Invalid(message) => RepositoryError::Decode {
            kind: "managed add-on NFO",
            value: message,
        },
        _ => RepositoryError::unavailable("managed add-on NFO", error.to_string()),
    }
}

const fn nfo_root_for_content_kind(kind: AddonMediaListContentKind) -> &'static str {
    match kind {
        AddonMediaListContentKind::TelevisionEpisode => "episodedetails",
        AddonMediaListContentKind::MusicVideo => "musicvideo",
        AddonMediaListContentKind::Song => "song",
        // Kodi has no still-image NFO root, so an image keeps the generic one.
        AddonMediaListContentKind::Auto
        | AddonMediaListContentKind::Movie
        | AddonMediaListContentKind::OtherVideo
        | AddonMediaListContentKind::Image => "movie",
    }
}

async fn show_nfo_path_for(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<Option<PathBuf>> {
    let path = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "file"."Path"
           FROM "AddonMediaListItem" AS "item"
           JOIN "MediaVersion" AS "version"
             ON "version"."RemoteStreamId" = "item"."MediaItemId"
           JOIN "MediaFile" AS "file" ON "file"."MediaVersionId" = "version"."Id"
           WHERE "item"."AddonMediaListId" = ?
             AND "item"."ParentProviderId" IS NULL
           ORDER BY "item"."Rank", "item"."Id", "version"."Id", "file"."Id"
           LIMIT 1"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .flatten();
    Ok(path.and_then(|path| {
        Path::new(&path)
            .parent()
            .map(|parent| parent.join("tvshow.nfo"))
    }))
}

async fn item_nfo_path_for(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    item_id: EntityId,
) -> Result<Option<PathBuf>> {
    let path = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "file"."Path"
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "MediaVersion" AS "version"
             ON "version"."RemoteStreamId" = "item"."MediaItemId"
           LEFT JOIN "MediaFile" AS "file" ON "file"."MediaVersionId" = "version"."Id"
           WHERE "item"."AddonMediaListId" = ? AND "item"."Id" = ?
           ORDER BY "version"."Id", "file"."Id" LIMIT 1"#,
    )
    .bind(list_id)
    .bind(item_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .flatten();
    Ok(crate::addon_media_nfo::item_nfo_path(path.as_deref()))
}

struct ProjectedEpisodeInput {
    item_id: EntityId,
    metadata: AddonMediaListNfoMetadata,
    state: i32,
}

/// Season title in the spelling scanned television sources already use.
fn season_title(season_number: i32) -> String {
    if season_number == 0 {
        "Specials".to_owned()
    } else {
        format!("Season {season_number}")
    }
}

impl SqliteAddonMediaListRepository {
    /// Load list entries, optionally restricted to one availability group.
    ///
    /// One query shape serves the unfiltered internal callers and the paged
    /// editor sections so their row decoding cannot drift apart.
    async fn query_items(
        &self,
        id: EntityId,
        availability: Option<AddonMediaListItemAvailability>,
        page: u32,
        page_size: Option<u32>,
    ) -> Result<Vec<AddonMediaListItemAdminRow>> {
        // One static statement serves every combination: a NULL availability
        // matches every group, and an absent page size becomes an unbounded
        // limit both backends accept.
        let rows = sqlx::query(
            r#"SELECT "item"."Id", "item"."Rank", "item"."DisplayTitle",
                      "item"."Title", "item"."Year", "item"."Season", "item"."Episode",
                      "item"."SourceUrl", "item"."DurationSeconds", "item"."ParentProviderId",
                      "item"."ChapterInput", "item"."ProviderChapterInput",
                      "item"."ContentKind",
                      "projection"."EpisodeId" AS "ProjectedEpisodeId",
                      "item"."Availability", "item"."AvailabilityReason",
                      "replacement"."MediaItemId" AS "ReplacementMediaItemId",
                      "replacement"."MediaVersionId" AS "ReplacementMediaVersionId"
               FROM "AddonMediaListItem" AS "item"
               LEFT JOIN "AddonMediaListItemReplacement" AS "replacement"
                 ON "replacement"."AddonMediaListItemId" = "item"."Id"
               LEFT JOIN "AddonMediaListItemEpisode" AS "projection"
                 ON "projection"."AddonMediaListItemId" = "item"."Id"
               WHERE "item"."AddonMediaListId" = ?
                 AND (? IS NULL OR "item"."Availability" = ?)
               ORDER BY "item"."Rank", "item"."Id"
               LIMIT ? OFFSET ?"#,
        )
        .bind(id)
        .bind(availability.map(crate::repositories::common::encode_enum))
        .bind(availability.map(crate::repositories::common::encode_enum))
        .bind(page_size.map_or(i64::MAX, |size| i64::from(size.max(1))))
        .bind(page_size.map_or(0, |size| i64::from(page) * i64::from(size.max(1))))
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
                title: row.try_get("Title").repository()?,
                year: row.try_get("Year").repository()?,
                season: row.try_get("Season").repository()?,
                episode: row.try_get("Episode").repository()?,
                source_url: row.try_get("SourceUrl").repository()?,
                duration_seconds: decode_optional_u64(&row, "DurationSeconds")?,
                parent_provider_id: row.try_get("ParentProviderId").repository()?,
                chapter_input: row
                    .try_get::<Option<String>, _>("ChapterInput")
                    .repository()?
                    .unwrap_or_default(),
                provider_chapter_input: row
                    .try_get::<Option<String>, _>("ProviderChapterInput")
                    .repository()?
                    .unwrap_or_default(),
                projected_episode_id: row.try_get("ProjectedEpisodeId").repository()?,
                content_kind: crate::repositories::common::decode_enum_column(
                    "AddonMediaListContentKind",
                    &row,
                    "ContentKind",
                )?,
            });
        }
        Ok(result)
    }
}

#[allow(clippy::too_many_lines)]
async fn reconcile_tv_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    requested: AddonMediaListPresentationKind,
) -> Result<()> {
    if requested == AddonMediaListPresentationKind::RemoteStreams {
        let references: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*)
               FROM "AddonMediaListItemEpisode" AS "projection"
               WHERE "projection"."AddonMediaListItemId" IN (
                   SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
               ) AND (
                   EXISTS (SELECT 1 FROM "CollectionItem" WHERE "MediaItemId" = "projection"."EpisodeId")
                   OR EXISTS (
                       SELECT 1 FROM "PlaylistItem"
                       WHERE "MediaItemId" = "projection"."EpisodeId"
                         AND "PlaylistId" <> COALESCE(
                             (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?), -1
                         )
                   )
                   OR EXISTS (SELECT 1 FROM "PlayoutItem" WHERE "MediaItemId" = "projection"."EpisodeId")
               )"#,
        )
        .bind(list_id)
        .bind(list_id)
        .fetch_one(pool)
        .await
        .repository()?;
        if references > 0 {
            return Err(RepositoryError::Conflict(
                "projected episodes are still referenced by a collection, playlist, or playout"
                    .to_owned(),
            ));
        }
        let show_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "ShowId" FROM "AddonMediaListTelevisionShow"
               WHERE "AddonMediaListId" = ?"#,
        )
        .bind(list_id)
        .fetch_optional(pool)
        .await
        .repository()?;
        let mut tx = crate::db::begin_write(pool).await.repository()?;
        if let Some(show_id) = show_id {
            let episode_ids = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "projection"."EpisodeId"
                   FROM "AddonMediaListItemEpisode" AS "projection"
                   JOIN "AddonMediaListItem" AS "item"
                     ON "item"."Id" = "projection"."AddonMediaListItemId"
                   WHERE "item"."AddonMediaListId" = ?"#,
            )
            .bind(list_id)
            .fetch_all(&mut *tx)
            .await
            .repository()?;
            for episode_id in episode_ids {
                sqlx::query(
                    r#"UPDATE "MediaVersion" SET "EpisodeId" = NULL WHERE "EpisodeId" = ?"#,
                )
                .bind(episode_id)
                .execute(&mut *tx)
                .await
                .repository()?;
                sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
                    .bind(episode_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            let season_ids = sqlx::query_scalar::<_, EntityId>(
                r#"SELECT "Id" FROM "Season" WHERE "ShowId" = ?"#,
            )
            .bind(show_id)
            .fetch_all(&mut *tx)
            .await
            .repository()?;
            for season_id in season_ids {
                sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
                    .bind(season_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
                .bind(show_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "PresentationKind" = 0, "RequestedPresentationKind" = 0,
                   "PresentationError" = NULL WHERE "Id" = ?"#,
        )
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        return Ok(());
    }

    let show_path = show_nfo_path_for(pool, list_id).await?.ok_or_else(|| {
        RepositoryError::Conflict("managed list storage is unavailable".to_owned())
    })?;
    let show_form = crate::addon_media_nfo::read_form(Some(&show_path)).map_err(map_nfo_error)?;
    if show_form.metadata.title.trim().is_empty() {
        return Err(RepositoryError::Conflict(
            "tvshow.nfo must contain a show title".to_owned(),
        ));
    }
    let show_title = show_form.metadata.title.trim().to_owned();
    let rows = sqlx::query(
        r#"SELECT "Id", "Title", "Year", "Season", "Episode", "Availability"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
           ORDER BY "Rank", "Id""#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut episodes = Vec::with_capacity(rows.len());
    for row in rows {
        let item_id: EntityId = row.try_get("Id").repository()?;
        let path = item_nfo_path_for(pool, list_id, item_id)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict(format!(
                    "item {item_id} must be scanned before television conversion"
                ))
            })?;
        let form = crate::addon_media_nfo::read_form(Some(&path)).map_err(map_nfo_error)?;
        let mut metadata = form.metadata;
        if metadata.title.trim().is_empty() {
            metadata.title = row.try_get("Title").repository()?;
        }
        metadata.show_title = Some(show_title.clone());
        metadata.season = Some(
            metadata
                .season
                .or(row.try_get("Season").repository()?)
                .unwrap_or(0),
        );
        metadata.episode = Some(
            metadata
                .episode
                .or(row.try_get("Episode").repository()?)
                .unwrap_or(0),
        );
        metadata.year = metadata.year.or(row.try_get("Year").repository()?);
        crate::addon_media_nfo::save_user(&path, "episodedetails", &metadata, &form.etag)
            .map_err(map_nfo_error)?;
        let availability: i32 = row.try_get("Availability").repository()?;
        episodes.push(ProjectedEpisodeInput {
            item_id,
            metadata,
            state: i32::from(availability == 1) * 2,
        });
    }

    let library_path_id = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "path"."Id"
           FROM "AddonMediaListLibrary" AS "managed"
           JOIN "LibraryPath" AS "path" ON "path"."LibraryId" = "managed"."LibraryId"
           WHERE "managed"."AddonMediaListId" = ? ORDER BY "path"."Id" LIMIT 1"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or_else(|| RepositoryError::Conflict("managed library path is unavailable".to_owned()))?;
    let now = chrono::Utc::now();
    let now_text = crate::repositories::common::encode_datetime(now);
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    let show_id = if let Some(show_id) = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "ShowId" FROM "AddonMediaListTelevisionShow"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut *tx)
    .await
    .repository()?
    {
        show_id
    } else {
        let result =
            sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
                .bind(library_path_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        let show_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
        sqlx::query(r#"INSERT INTO "Show" ("Id") VALUES (?)"#)
            .bind(show_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListTelevisionShow" ("AddonMediaListId", "ShowId")
               VALUES (?, ?)"#,
        )
        .bind(list_id)
        .bind(show_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        show_id
    };
    sqlx::query(r#"DELETE FROM "ShowMetadata" WHERE "ShowId" = ?"#)
        .bind(show_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    sqlx::query(
        r#"INSERT INTO "ShowMetadata" (
              "ShowId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
              "Outline", "Plot", "Tagline", "Year", "ReleaseDate",
              "ContentRating", "DateAdded", "DateUpdated"
           ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(show_id)
    .bind(&show_form.metadata.title)
    .bind(&show_form.metadata.original_title)
    .bind(&show_form.metadata.sort_title)
    .bind(&show_form.metadata.outline)
    .bind(&show_form.metadata.plot)
    .bind(&show_form.metadata.tagline)
    .bind(show_form.metadata.year)
    .bind(&show_form.metadata.release_date)
    .bind(show_form.metadata.content_ratings.first())
    .bind(&now_text)
    .bind(&now_text)
    .execute(&mut *tx)
    .await
    .repository()?;

    for input in episodes {
        let season_number = input.metadata.season.unwrap_or(0);
        let season_id = if let Some(season_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "Season" WHERE "ShowId" = ? AND "SeasonNumber" = ?
               ORDER BY "Id" LIMIT 1"#,
        )
        .bind(show_id)
        .bind(season_number)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        {
            season_id
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
                    .bind(library_path_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let season_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
            sqlx::query(
                r#"INSERT INTO "Season" ("Id", "ShowId", "SeasonNumber") VALUES (?, ?, ?)"#,
            )
            .bind(season_id)
            .bind(show_id)
            .bind(season_number)
            .execute(&mut *tx)
            .await
            .repository()?;
            season_id
        };
        // Every other season source carries metadata, and consumers join it to
        // resolve the season title and artwork, so a projected season without
        // one is invisible to them — including the show detail page. This also
        // repairs seasons projected before the metadata row was written.
        sqlx::query(
            r#"INSERT INTO "SeasonMetadata"
                   ("SeasonId", "MetadataKind", "Title", "SortTitle",
                    "DateAdded", "DateUpdated")
               SELECT ?, 0, ?, ?, ?, ?
               WHERE NOT EXISTS (
                   SELECT 1 FROM "SeasonMetadata" WHERE "SeasonId" = ?
               )"#,
        )
        .bind(season_id)
        .bind(season_title(season_number))
        .bind(format!("{season_number:02}"))
        .bind(&now_text)
        .bind(&now_text)
        .bind(season_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        let episode_id = if let Some(episode_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "EpisodeId" FROM "AddonMediaListItemEpisode"
               WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(input.item_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        {
            sqlx::query(r#"UPDATE "Episode" SET "SeasonId" = ? WHERE "Id" = ?"#)
                .bind(season_id)
                .bind(episode_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            episode_id
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, ?)"#)
                    .bind(library_path_id)
                    .bind(input.state)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let episode_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
            sqlx::query(r#"INSERT INTO "Episode" ("Id", "SeasonId") VALUES (?, ?)"#)
                .bind(episode_id)
                .bind(season_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItemEpisode" ("AddonMediaListItemId", "EpisodeId")
                   VALUES (?, ?)"#,
            )
            .bind(input.item_id)
            .bind(episode_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            episode_id
        };
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = ? WHERE "Id" = ?"#)
            .bind(input.state)
            .bind(episode_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"UPDATE "MediaVersion" SET "EpisodeId" = ?
               WHERE "RemoteStreamId" = (
                   SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id" = ?
               )"#,
        )
        .bind(episode_id)
        .bind(input.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(r#"DELETE FROM "EpisodeMetadata" WHERE "EpisodeId" = ?"#)
            .bind(episode_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"INSERT INTO "EpisodeMetadata" (
                  "EpisodeId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
                  "Outline", "Plot", "Tagline", "Year", "ReleaseDate", "EpisodeNumber",
                  "DateAdded", "DateUpdated"
               ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(episode_id)
        .bind(&input.metadata.title)
        .bind(&input.metadata.original_title)
        .bind(&input.metadata.sort_title)
        .bind(&input.metadata.outline)
        .bind(&input.metadata.plot)
        .bind(&input.metadata.tagline)
        .bind(input.metadata.year)
        .bind(&input.metadata.release_date)
        .bind(input.metadata.episode.unwrap_or(0))
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
    }
    let empty_seasons = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "season"."Id" FROM "Season" AS "season"
           WHERE "season"."ShowId" = ?
             AND NOT EXISTS (SELECT 1 FROM "Episode" WHERE "SeasonId" = "season"."Id")"#,
    )
    .bind(show_id)
    .fetch_all(&mut *tx)
    .await
    .repository()?;
    for season_id in empty_seasons {
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(season_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = 1, "RequestedPresentationKind" = 1,
               "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
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
                     AND "stream"."MediaStreamKind" = 1 AND "stream"."AttachedPic" = 0)
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
                  AND "stream"."MediaStreamKind" = 1 AND "stream"."AttachedPic" = 0)
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
                     AND "stream"."MediaStreamKind" = 1 AND "stream"."AttachedPic" = 0)
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
                  AND "stream"."MediaStreamKind" = 1 AND "stream"."AttachedPic" = 0)
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
