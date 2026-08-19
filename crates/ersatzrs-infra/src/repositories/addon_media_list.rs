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
        let path = self.parent_nfo_path(id).await?;
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
        // A presentation with a parent needs that parent named before anything
        // is projected, and the name is durable in its NFO rather than the row.
        if let Some((file_name, root)) = parent_nfo_for(presentation_kind) {
            let path = parent_nfo_path_for(&self.0, id, file_name)
                .await?
                .ok_or_else(|| {
                    RepositoryError::Conflict("managed list storage is unavailable".to_owned())
                })?;
            let mut nfo = crate::addon_media_nfo::read_form(Some(&path)).map_err(map_nfo_error)?;
            if nfo.metadata.title.trim().is_empty() {
                show_title
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .ok_or_else(|| {
                        RepositoryError::Conflict(format!("{file_name} must contain a title"))
                    })?
                    .clone_into(&mut nfo.metadata.title);
                crate::addon_media_nfo::save_user(&path, root, &nfo.metadata, &nfo.etag)
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

        let result = reconcile_projection(&self.0, id, presentation_kind).await;
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
        let path = self.parent_nfo_path(id).await?.ok_or_else(|| {
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
            reconcile_projection(&self.0, id, form.requested_presentation_kind).await?;
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
            reconcile_projection(&self.0, list_id, presentation.requested_presentation_kind)
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
            self.parent_nfo_path(list_id).await?
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
        reconcile_projection(&self.0, id, AddonMediaListPresentationKind::RemoteStreams).await?;
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
        // Kodi defines no song or still-image NFO document, so those reuse the
        // nearest root it does define. Writing an invented root would produce a
        // sidecar no Kodi-compatible reader accepts, and `serialize` refuses it.
        AddonMediaListContentKind::MusicVideo | AddonMediaListContentKind::Song => "musicvideo",
        AddonMediaListContentKind::Auto
        | AddonMediaListContentKind::Movie
        | AddonMediaListContentKind::OtherVideo
        | AddonMediaListContentKind::Image => "movie",
    }
}

/// Path of the list-level NFO document for a presentation with a parent.
///
/// Kodi names the parent document after the parent kind, so a television list
/// owns `tvshow.nfo` and a music-video list owns `artist.nfo`.
async fn parent_nfo_path_for(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    file_name: &str,
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
            .map(|parent| parent.join(file_name))
    }))
}

/// The list-level NFO document and root a presentation writes, if it has one.
const fn parent_nfo_for(
    presentation: AddonMediaListPresentationKind,
) -> Option<(&'static str, &'static str)> {
    match presentation {
        AddonMediaListPresentationKind::TelevisionShow => Some(("tvshow.nfo", "tvshow")),
        // Kodi has no artist NFO root in this serializer, so the managed artist
        // document reuses the show root; only its file name differs.
        AddonMediaListPresentationKind::MusicVideos => Some(("artist.nfo", "tvshow")),
        _ => None,
    }
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
    /// Path of the list-level NFO for whichever parent the list requested.
    async fn parent_nfo_path(&self, id: EntityId) -> Result<Option<PathBuf>> {
        let requested = self
            .get_admin_form(id)
            .await?
            .map(|form| form.requested_presentation_kind)
            .unwrap_or_default();
        let (file_name, _) = parent_nfo_for(requested).unwrap_or(("tvshow.nfo", "tvshow"));
        parent_nfo_path_for(&self.0, id, file_name).await
    }

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

/// Apply the requested presentation, reverting whatever the list had before.
///
/// `Auto` is resolved from the entries themselves; every other kind is applied
/// as asked. Projections other than the effective one are always reverted
/// first, so switching between them cannot leave two live hierarchies over the
/// same provider items.
async fn reconcile_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    requested: AddonMediaListPresentationKind,
) -> Result<()> {
    let effective = if requested == AddonMediaListPresentationKind::Auto {
        resolve_auto_presentation(pool, list_id).await?
    } else {
        requested
    };
    match effective {
        AddonMediaListPresentationKind::RemoteStreams | AddonMediaListPresentationKind::Auto => {
            revert_projections(pool, list_id).await
        }
        AddonMediaListPresentationKind::TelevisionShow => {
            revert_music_video_projection(pool, list_id).await?;
            revert_flat_projections(pool, list_id).await?;
            apply_tv_projection(pool, list_id).await
        }
        AddonMediaListPresentationKind::Movies => {
            apply_flat(pool, list_id, FlatProjection::MOVIES).await
        }
        AddonMediaListPresentationKind::OtherVideos => {
            apply_flat(pool, list_id, FlatProjection::OTHER_VIDEOS).await
        }
        AddonMediaListPresentationKind::Songs => {
            apply_flat(pool, list_id, FlatProjection::SONGS).await
        }
        AddonMediaListPresentationKind::Images => {
            apply_flat(pool, list_id, FlatProjection::IMAGES).await
        }
        AddonMediaListPresentationKind::MusicVideos => {
            revert_tv_projection(pool, list_id).await?;
            revert_flat_projections(pool, list_id).await?;
            apply_music_video_projection(pool, list_id).await
        }
    }
}

/// Detach the Music Video projection, its managed Artist, and their media items.
async fn revert_music_video_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<()> {
    let references: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*)
           FROM "AddonMediaListItemMusicVideo" AS "projection"
           WHERE "projection"."AddonMediaListItemId" IN (
               SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
           ) AND (
               EXISTS (
                   SELECT 1 FROM "CollectionItem"
                   WHERE "MediaItemId" = "projection"."MusicVideoId"
               )
               OR EXISTS (
                   SELECT 1 FROM "PlaylistItem"
                   WHERE "MediaItemId" = "projection"."MusicVideoId"
                     AND "PlaylistId" <> COALESCE(
                         (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?), -1
                     )
               )
               OR EXISTS (
                   SELECT 1 FROM "PlayoutItem"
                   WHERE "MediaItemId" = "projection"."MusicVideoId"
               )
           )"#,
    )
    .bind(list_id)
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if references > 0 {
        return Err(RepositoryError::Conflict(
            "projected music videos are still referenced by a collection, playlist, or playout"
                .to_owned(),
        ));
    }

    let music_video_ids = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "projection"."MusicVideoId"
           FROM "AddonMediaListItemMusicVideo" AS "projection"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."Id" = "projection"."AddonMediaListItemId"
           WHERE "item"."AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let artist_id = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "ArtistId" FROM "AddonMediaListArtist" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    if music_video_ids.is_empty() && artist_id.is_none() {
        return Ok(());
    }

    let mut tx = crate::db::begin_write(pool).await.repository()?;
    for music_video_id in music_video_ids {
        // The Remote Stream version is shared, not owned.
        sqlx::query(r#"UPDATE "MediaVersion" SET "MusicVideoId" = NULL WHERE "MusicVideoId" = ?"#)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    if let Some(artist_id) = artist_id {
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(artist_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = 0, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

/// Project one list as an Artist with Music Videos over its Remote Streams.
///
/// `MusicVideo.ArtistId` is NOT NULL, so this is the only remaining kind with a
/// parent: one managed Artist named by `artist.nfo`, the way a television list
/// is named by `tvshow.nfo`.
#[allow(clippy::too_many_lines)]
async fn apply_music_video_projection(pool: &Pool<crate::db::Db>, list_id: EntityId) -> Result<()> {
    let artist_path = parent_nfo_path_for(pool, list_id, "artist.nfo")
        .await?
        .ok_or_else(|| {
            RepositoryError::Conflict("managed list storage is unavailable".to_owned())
        })?;
    let artist_form =
        crate::addon_media_nfo::read_form(Some(&artist_path)).map_err(map_nfo_error)?;
    if artist_form.metadata.title.trim().is_empty() {
        return Err(RepositoryError::Conflict(
            "artist.nfo must contain an artist name".to_owned(),
        ));
    }

    let rows = sqlx::query(
        r#"SELECT "Id", "Title", "Year", "Availability"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
           ORDER BY "Rank", "Id""#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let mut projected = Vec::with_capacity(rows.len());
    for row in rows {
        let item_id: EntityId = row.try_get("Id").repository()?;
        let path = item_nfo_path_for(pool, list_id, item_id)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict(format!(
                    "item {item_id} must be scanned before music video conversion"
                ))
            })?;
        let form = crate::addon_media_nfo::read_form(Some(&path)).map_err(map_nfo_error)?;
        let mut metadata = form.metadata;
        if metadata.title.trim().is_empty() {
            metadata.title = row.try_get("Title").repository()?;
        }
        metadata.year = metadata.year.or(row.try_get("Year").repository()?);
        crate::addon_media_nfo::save_user(&path, "musicvideo", &metadata, &form.etag)
            .map_err(map_nfo_error)?;
        let availability: i32 = row.try_get("Availability").repository()?;
        projected.push(ProjectedEpisodeInput {
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
    let now_text = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;

    let artist_id = if let Some(artist_id) = sqlx::query_scalar::<_, EntityId>(
        r#"SELECT "ArtistId" FROM "AddonMediaListArtist" WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut *tx)
    .await
    .repository()?
    {
        artist_id
    } else {
        let result =
            sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, 0)"#)
                .bind(library_path_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        let artist_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
        sqlx::query(r#"INSERT INTO "Artist" ("Id") VALUES (?)"#)
            .bind(artist_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtist" ("AddonMediaListId", "ArtistId")
               VALUES (?, ?)"#,
        )
        .bind(list_id)
        .bind(artist_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        artist_id
    };
    // Consumers join artist metadata to resolve the name and artwork, so an
    // artist without one is invisible to them.
    sqlx::query(r#"DELETE FROM "ArtistMetadata" WHERE "ArtistId" = ?"#)
        .bind(artist_id)
        .execute(&mut *tx)
        .await
        .repository()?;
    sqlx::query(
        r#"INSERT INTO "ArtistMetadata" (
              "ArtistId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
              "Biography", "Year", "ReleaseDate", "DateAdded", "DateUpdated"
           ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?)"#,
    )
    .bind(artist_id)
    .bind(&artist_form.metadata.title)
    .bind(&artist_form.metadata.original_title)
    .bind(&artist_form.metadata.sort_title)
    .bind(&artist_form.metadata.plot)
    .bind(artist_form.metadata.year)
    .bind(&artist_form.metadata.release_date)
    .bind(&now_text)
    .bind(&now_text)
    .execute(&mut *tx)
    .await
    .repository()?;

    for input in projected {
        let music_video_id = if let Some(music_video_id) = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "MusicVideoId" FROM "AddonMediaListItemMusicVideo"
               WHERE "AddonMediaListItemId" = ?"#,
        )
        .bind(input.item_id)
        .fetch_optional(&mut *tx)
        .await
        .repository()?
        {
            sqlx::query(r#"UPDATE "MusicVideo" SET "ArtistId" = ? WHERE "Id" = ?"#)
                .bind(artist_id)
                .bind(music_video_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            music_video_id
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, ?)"#)
                    .bind(library_path_id)
                    .bind(input.state)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let music_video_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
            sqlx::query(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES (?, ?)"#)
                .bind(music_video_id)
                .bind(artist_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItemMusicVideo"
                       ("AddonMediaListItemId", "MusicVideoId")
                   VALUES (?, ?)"#,
            )
            .bind(input.item_id)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            music_video_id
        };
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = ? WHERE "Id" = ?"#)
            .bind(input.state)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"UPDATE "MediaVersion" SET "MusicVideoId" = ?
               WHERE "RemoteStreamId" = (
                   SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id" = ?
               )"#,
        )
        .bind(music_video_id)
        .bind(input.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(r#"DELETE FROM "MusicVideoMetadata" WHERE "MusicVideoId" = ?"#)
            .bind(music_video_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(
            r#"INSERT INTO "MusicVideoMetadata" (
                  "MusicVideoId", "MetadataKind", "Title", "OriginalTitle", "SortTitle",
                  "Plot", "Year", "ReleaseDate", "DateAdded", "DateUpdated"
               ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(music_video_id)
        .bind(&input.metadata.title)
        .bind(&input.metadata.original_title)
        .bind(&input.metadata.sort_title)
        .bind(&input.metadata.plot)
        .bind(input.metadata.year)
        .bind(&input.metadata.release_date)
        .bind(&now_text)
        .bind(&now_text)
        .execute(&mut *tx)
        .await
        .repository()?;
    }

    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = ?, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListPresentationKind::MusicVideos,
    ))
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

/// Apply one flat projection after detaching whatever the list had before.
async fn apply_flat(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    projection: FlatProjection,
) -> Result<()> {
    revert_tv_projection(pool, list_id).await?;
    revert_music_video_projection(pool, list_id).await?;
    for other in FlatProjection::ALL {
        if other.link_table != projection.link_table {
            revert_flat_projection(pool, list_id, other).await?;
        }
    }
    apply_flat_projection(pool, list_id, projection).await
}

/// Derive the presentation from how the entries classify themselves.
///
/// A list only auto-projects when every entry agrees and none is still
/// unclassified; anything else stays plain Remote Streams, because guessing a
/// hierarchy for a mixed list would be worse than leaving it alone.
async fn resolve_auto_presentation(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<AddonMediaListPresentationKind> {
    let kinds = sqlx::query_scalar::<_, i32>(
        r#"SELECT DISTINCT "ContentKind" FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL"#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    let [only] = kinds.as_slice() else {
        return Ok(AddonMediaListPresentationKind::RemoteStreams);
    };
    let Some(kind) =
        <AddonMediaListContentKind as crate::repositories::common::DbEnum>::from_db_int(*only)
    else {
        return Ok(AddonMediaListPresentationKind::RemoteStreams);
    };
    let derived = AddonMediaListPresentationKind::from_content_kind(kind)
        .unwrap_or(AddonMediaListPresentationKind::RemoteStreams);
    // A kind whose projection does not exist yet must not be entered silently.
    Ok(if derived.is_projected() {
        derived
    } else {
        AddonMediaListPresentationKind::RemoteStreams
    })
}

/// Revert one list to plain Remote Streams, detaching every projection.
///
/// Reverting is refused while a projected media item is referenced from
/// outside the list, so a schedule or collection cannot lose its target.
async fn revert_projections(pool: &Pool<crate::db::Db>, list_id: EntityId) -> Result<()> {
    revert_music_video_projection(pool, list_id).await?;
    revert_flat_projections(pool, list_id).await?;
    revert_tv_projection(pool, list_id).await
}

/// One parentless media kind an add-on list can be projected into.
///
/// Movies, Other Videos, Songs, and Images all become one media item per entry
/// with no container above them, so they differ only in which tables they write.
#[derive(Clone, Copy)]
struct FlatProjection {
    /// Link table joining one list entry to its projected media item.
    link_table: &'static str,
    /// Column naming the projected media item inside the link table.
    link_column: &'static str,
    /// Concrete media table, which shares its primary key with `MediaItem`.
    media_table: &'static str,
    /// Column naming the projected media item inside the metadata table.
    metadata_owner_column: &'static str,
    /// Metadata table written from each entry's NFO.
    metadata_table: &'static str,
    /// NFO document root this kind is stored under.
    nfo_root: &'static str,
    /// Whether the metadata table has the video description and rating columns.
    ///
    /// `SongMetadata` and `ImageMetadata` do not, so writing them would name a
    /// column that does not exist.
    descriptive_metadata: bool,
    presentation: AddonMediaListPresentationKind,
}

impl FlatProjection {
    const MOVIES: Self = Self {
        link_table: "AddonMediaListItemMovie",
        link_column: "MovieId",
        media_table: "Movie",
        metadata_owner_column: "MovieId",
        metadata_table: "MovieMetadata",
        nfo_root: "movie",
        descriptive_metadata: true,
        presentation: AddonMediaListPresentationKind::Movies,
    };
    const OTHER_VIDEOS: Self = Self {
        link_table: "AddonMediaListItemOtherVideo",
        link_column: "OtherVideoId",
        media_table: "OtherVideo",
        metadata_owner_column: "OtherVideoId",
        metadata_table: "OtherVideoMetadata",
        nfo_root: "movie",
        descriptive_metadata: true,
        presentation: AddonMediaListPresentationKind::OtherVideos,
    };

    const SONGS: Self = Self {
        link_table: "AddonMediaListItemSong",
        link_column: "SongId",
        media_table: "Song",
        metadata_owner_column: "SongId",
        metadata_table: "SongMetadata",
        // Kodi defines no song NFO document; `musicvideo` is the nearest root it
        // does define, and the only music-shaped one `serialize` accepts.
        nfo_root: "musicvideo",
        descriptive_metadata: false,
        presentation: AddonMediaListPresentationKind::Songs,
    };

    const IMAGES: Self = Self {
        link_table: "AddonMediaListItemImage",
        link_column: "ImageId",
        media_table: "Image",
        metadata_owner_column: "ImageId",
        metadata_table: "ImageMetadata",
        // Kodi defines no still-image NFO document either; the generic root is
        // the only one `serialize` accepts for a non-music item.
        nfo_root: "movie",
        descriptive_metadata: false,
        presentation: AddonMediaListPresentationKind::Images,
    };

    /// Every flat projection, so a revert can detach whichever one is live.
    const ALL: [Self; 4] = [Self::MOVIES, Self::OTHER_VIDEOS, Self::SONGS, Self::IMAGES];
}

/// Detach every flat projection a list may have, in turn.
async fn revert_flat_projections(pool: &Pool<crate::db::Db>, list_id: EntityId) -> Result<()> {
    for projection in FlatProjection::ALL {
        revert_flat_projection(pool, list_id, projection).await?;
    }
    Ok(())
}

/// Detach one flat projection and delete the media items it created.
///
/// Mirrors the television revert: a projected item still referenced from a
/// collection, another playlist, or a playout blocks the change instead of
/// silently removing a schedule's target.
///
/// The identifiers come from a closed set of compile-time constants, never from
/// operator input, so the statements are assembled rather than parameterized.
async fn revert_flat_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    projection: FlatProjection,
) -> Result<()> {
    let FlatProjection {
        link_table,
        link_column,
        ..
    } = projection;
    let references: i64 = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        r#"SELECT COUNT(*)
           FROM "{link_table}" AS "projection"
           WHERE "projection"."AddonMediaListItemId" IN (
               SELECT "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId" = ?
           ) AND (
               EXISTS (SELECT 1 FROM "CollectionItem" WHERE "MediaItemId" = "projection"."{link_column}")
               OR EXISTS (
                   SELECT 1 FROM "PlaylistItem"
                   WHERE "MediaItemId" = "projection"."{link_column}"
                     AND "PlaylistId" <> COALESCE(
                         (SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?), -1
                     )
               )
               OR EXISTS (SELECT 1 FROM "PlayoutItem" WHERE "MediaItemId" = "projection"."{link_column}")
           )"#
    )))
    .bind(list_id)
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    if references > 0 {
        return Err(RepositoryError::Conflict(format!(
            "projected {} items are still referenced by a collection, playlist, or playout",
            projection.media_table
        )));
    }

    let media_ids = sqlx::query_scalar::<_, EntityId>(sqlx::AssertSqlSafe(format!(
        r#"SELECT "projection"."{link_column}"
           FROM "{link_table}" AS "projection"
           JOIN "AddonMediaListItem" AS "item"
             ON "item"."Id" = "projection"."AddonMediaListItemId"
           WHERE "item"."AddonMediaListId" = ?"#
    )))
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;
    if media_ids.is_empty() {
        return Ok(());
    }

    let mut tx = crate::db::begin_write(pool).await.repository()?;
    for media_id in media_ids {
        // The Remote Stream version is shared, not owned, so it is detached
        // rather than deleted: the provider item remains playable.
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"UPDATE "MediaVersion" SET "{link_column}" = NULL WHERE "{link_column}" = ?"#
        )))
        .bind(media_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(r#"DELETE FROM "MediaItem" WHERE "Id" = ?"#)
            .bind(media_id)
            .execute(&mut *tx)
            .await
            .repository()?;
    }
    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = 0, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

/// Project every entry of one list as a Movie over its Remote Stream version.
///
/// A movie list has no parent container, so this is the television projection
/// without the Show and Season levels: each entry's NFO owns its metadata and
/// the scanned Remote Stream `MediaVersion` is attached to the projected movie.
/// Project every entry of one list into a parentless media kind.
///
/// This is the television projection without the Show and Season levels: each
/// entry's NFO owns its metadata and the scanned Remote Stream `MediaVersion` is
/// attached to the projected item. The identifiers come from a closed set of
/// compile-time constants, never from operator input.
#[allow(clippy::too_many_lines)]
async fn apply_flat_projection(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    projection: FlatProjection,
) -> Result<()> {
    let FlatProjection {
        link_table,
        link_column,
        media_table,
        metadata_owner_column,
        metadata_table,
        nfo_root,
        descriptive_metadata,
        presentation,
    } = projection;
    let rows = sqlx::query(
        r#"SELECT "Id", "Title", "Year", "Availability"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
           ORDER BY "Rank", "Id""#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?;

    let mut projected = Vec::with_capacity(rows.len());
    for row in rows {
        let item_id: EntityId = row.try_get("Id").repository()?;
        let path = item_nfo_path_for(pool, list_id, item_id)
            .await?
            .ok_or_else(|| {
                RepositoryError::Conflict(format!(
                    "item {item_id} must be scanned before {media_table} conversion"
                ))
            })?;
        let form = crate::addon_media_nfo::read_form(Some(&path)).map_err(map_nfo_error)?;
        let mut metadata = form.metadata;
        if metadata.title.trim().is_empty() {
            metadata.title = row.try_get("Title").repository()?;
        }
        metadata.year = metadata.year.or(row.try_get("Year").repository()?);
        crate::addon_media_nfo::save_user(&path, nfo_root, &metadata, &form.etag)
            .map_err(map_nfo_error)?;
        let availability: i32 = row.try_get("Availability").repository()?;
        projected.push(ProjectedEpisodeInput {
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
    let now_text = crate::repositories::common::encode_datetime(chrono::Utc::now());
    let mut tx = crate::db::begin_write(pool).await.repository()?;

    for input in projected {
        let media_id = if let Some(media_id) =
            sqlx::query_scalar::<_, EntityId>(sqlx::AssertSqlSafe(format!(
                r#"SELECT "{link_column}" FROM "{link_table}"
                   WHERE "AddonMediaListItemId" = ?"#
            )))
            .bind(input.item_id)
            .fetch_optional(&mut *tx)
            .await
            .repository()?
        {
            media_id
        } else {
            let result =
                sqlx::query(r#"INSERT INTO "MediaItem" ("LibraryPathId", "State") VALUES (?, ?)"#)
                    .bind(library_path_id)
                    .bind(input.state)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            let media_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"INSERT INTO "{media_table}" ("Id") VALUES (?)"#
            )))
            .bind(media_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"INSERT INTO "{link_table}" ("AddonMediaListItemId", "{link_column}")
                   VALUES (?, ?)"#
            )))
            .bind(input.item_id)
            .bind(media_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            media_id
        };
        sqlx::query(r#"UPDATE "MediaItem" SET "State" = ? WHERE "Id" = ?"#)
            .bind(input.state)
            .bind(media_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"UPDATE "MediaVersion" SET "{link_column}" = ?
               WHERE "RemoteStreamId" = (
                   SELECT "MediaItemId" FROM "AddonMediaListItem" WHERE "Id" = ?
               )"#
        )))
        .bind(media_id)
        .bind(input.item_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        sqlx::query(sqlx::AssertSqlSafe(format!(
            r#"DELETE FROM "{metadata_table}" WHERE "{metadata_owner_column}" = ?"#
        )))
        .bind(media_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        // Only the video kinds carry a rating and the long-form description
        // columns; a song or an image metadata table simply has no such columns.
        if descriptive_metadata {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"INSERT INTO "{metadata_table}" (
                      "{metadata_owner_column}", "MetadataKind", "Title", "OriginalTitle",
                      "SortTitle", "Outline", "Plot", "Tagline", "Year", "ReleaseDate",
                      "ContentRating", "DateAdded", "DateUpdated"
                   ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#
            )))
            .bind(media_id)
            .bind(&input.metadata.title)
            .bind(&input.metadata.original_title)
            .bind(&input.metadata.sort_title)
            .bind(&input.metadata.outline)
            .bind(&input.metadata.plot)
            .bind(&input.metadata.tagline)
            .bind(input.metadata.year)
            .bind(&input.metadata.release_date)
            .bind(input.metadata.content_ratings.first())
            .bind(&now_text)
            .bind(&now_text)
            .execute(&mut *tx)
            .await
            .repository()?;
        } else {
            sqlx::query(sqlx::AssertSqlSafe(format!(
                r#"INSERT INTO "{metadata_table}" (
                      "{metadata_owner_column}", "MetadataKind", "Title", "OriginalTitle",
                      "SortTitle", "Year", "ReleaseDate", "DateAdded", "DateUpdated"
                   ) VALUES (?, 1, ?, ?, ?, ?, ?, ?, ?)"#
            )))
            .bind(media_id)
            .bind(&input.metadata.title)
            .bind(&input.metadata.original_title)
            .bind(&input.metadata.sort_title)
            .bind(input.metadata.year)
            .bind(&input.metadata.release_date)
            .bind(&now_text)
            .bind(&now_text)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
    }

    sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "PresentationKind" = ?, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(crate::repositories::common::encode_enum(presentation))
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    tx.commit().await.repository()?;
    Ok(())
}

#[allow(clippy::too_many_lines)]
async fn revert_tv_projection(pool: &Pool<crate::db::Db>, list_id: EntityId) -> Result<()> {
    {
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
        Ok(())
    }
}

#[allow(clippy::too_many_lines)]
async fn apply_tv_projection(pool: &Pool<crate::db::Db>, list_id: EntityId) -> Result<()> {
    let show_path = parent_nfo_path_for(pool, list_id, "tvshow.nfo")
        .await?
        .ok_or_else(|| {
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

#[cfg(test)]
mod tests {
    use super::nfo_root_for_content_kind;
    use ersatzrs_domain::AddonMediaListContentKind;

    /// Every classification must map to a root the NFO serializer accepts.
    /// `Song` once mapped to an invented `song` root, so saving the NFO of any
    /// song-classified entry failed with "unsupported NFO root".
    #[test]
    fn every_content_kind_maps_to_a_serializable_nfo_root() {
        for kind in [
            AddonMediaListContentKind::Auto,
            AddonMediaListContentKind::TelevisionEpisode,
            AddonMediaListContentKind::Movie,
            AddonMediaListContentKind::MusicVideo,
            AddonMediaListContentKind::OtherVideo,
            AddonMediaListContentKind::Song,
            AddonMediaListContentKind::Image,
        ] {
            let root = nfo_root_for_content_kind(kind);
            crate::addon_media_nfo::serialize(
                root,
                &ersatzrs_core::repositories::AddonMediaListNfoMetadata {
                    title: "Fixture".to_owned(),
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("{kind:?} maps to unusable root {root}: {error}"));
        }
    }
}
