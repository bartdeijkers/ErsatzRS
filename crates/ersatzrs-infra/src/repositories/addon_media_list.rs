use async_trait::async_trait;
use ersatzrs_core::repositories::{
    AddonMediaListAdminForm, AddonMediaListAdminRow, AddonMediaListArtwork,
    AddonMediaListArtworkRole, AddonMediaListArtworkUpload, AddonMediaListItemAdminRow,
    AddonMediaListItemCounts, AddonMediaListItemDetail, AddonMediaListMetadataConflict,
    AddonMediaListNfoForm, AddonMediaListNfoMetadata, AddonMediaListNfoOwnership,
    AddonMediaListPublication, AddonMediaListReplacementCandidate, AddonMediaListRepository,
    RepositoryError, Result,
};
use ersatzrs_domain::{
    AddonMediaListContentKind, AddonMediaListItemAvailability, AddonMediaListItemRole,
    AddonMediaListPresentationKind, AddonMediaListReviewState, EntityId,
};
use sha2::{Digest as _, Sha256};
use sqlx::{Pool, Row};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use crate::addon_media_list::InstalledAddonMediaListProvider;
use crate::addon_media_list_sync::{AddonMediaListRemoteSynchronizer, AddonMediaListSynchronizer};
use crate::metadata::{
    artwork_kind, replace_artwork, replace_guids, replace_metadata_languages, replace_people,
    replace_strings, ArtworkRef, PersonRef,
};
use crate::repositories::common::{decode_bool_column, decode_optional_datetime};
use crate::repositories::SqlxResultExt as _;

const PAGE_SIZE_KEY: &str = "pages.addon_media_lists.page_size";

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct PublicationJournal {
    previous_review_state: AddonMediaListReviewState,
    previous_presentation_kind: AddonMediaListPresentationKind,
    previous_items: Vec<PublishedItemSnapshot>,
    previous_playlist: Option<PlaylistSnapshot>,
    #[serde(default)]
    managed_folder: Option<String>,
    #[serde(default)]
    backup_folder: Option<String>,
    #[serde(default)]
    previous_materializations: Vec<ArtworkMaterializationSnapshot>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct ArtworkMaterializationSnapshot {
    id: EntityId,
    item_id: Option<EntityId>,
    scope_item_id: EntityId,
    role: i32,
    source: String,
    file_name: String,
    sha256: String,
    published_revision: i32,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct PublishedItemSnapshot {
    id: EntityId,
    published: bool,
    media_item_id: Option<EntityId>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct PlaylistSnapshot {
    id: EntityId,
    is_system: bool,
    name: Option<String>,
    playlist_group_id: EntityId,
    items: Vec<PlaylistItemSnapshot>,
}

#[derive(Clone, Debug, serde::Deserialize, serde::Serialize)]
struct PlaylistItemSnapshot {
    id: EntityId,
    index: i32,
    collection_type: i32,
    collection_id: Option<EntityId>,
    media_item_id: Option<EntityId>,
    multi_collection_id: Option<EntityId>,
    smart_collection_id: Option<EntityId>,
    include_in_program_guide: bool,
    playback_order: i32,
    play_all: bool,
    count: Option<i32>,
    preferred_audio_language_code: Option<String>,
    preferred_audio_title: Option<String>,
    preferred_subtitle_language_code: Option<String>,
    subtitle_mode: Option<i32>,
    media_version_id: Option<EntityId>,
    video_stream_index: Option<i32>,
    audio_stream_index: Option<i32>,
    subtitle_stream_index: Option<i32>,
}

#[derive(Clone)]
pub struct SqliteAddonMediaListRepository(pub Pool<crate::db::Db>);

#[async_trait]
#[allow(clippy::too_many_lines)]
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
                      "list"."PresentationError", "list"."ReviewState", "list"."ReviewError",
                      "list"."PublishedRevision",
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
                        "list"."PresentationError", "list"."ReviewState", "list"."ReviewError",
                        "list"."PublishedRevision"
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
                    review_state: crate::repositories::common::decode_enum_column(
                        "AddonMediaListReviewState",
                        row,
                        "ReviewState",
                    )?,
                    review_error: row.try_get("ReviewError").repository()?,
                    published_revision: row.try_get("PublishedRevision").repository()?,
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
            r#"SELECT "list"."Id", "list"."AddonId", "list"."SourceUrl",
                      "list"."AutoRefresh", "list"."GeneratePlaylist",
                      "list"."ContentKindOverride", "list"."PresentationKind",
                      "list"."RequestedPresentationKind", "list"."PresentationError",
                      "list"."ChapterInput", "list"."ReviewState", "list"."ReviewError",
                      "list"."PublishedRevision",
                      COALESCE("managed"."FolderName", '') AS "FolderName"
               FROM "AddonMediaList" AS "list"
               LEFT JOIN "AddonMediaListLibrary" AS "managed"
                 ON "managed"."AddonMediaListId" = "list"."Id"
               WHERE "list"."Id" = ?"#,
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
                folder_name: row.try_get("FolderName").repository()?,
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
                review_state: crate::repositories::common::decode_enum_column(
                    "AddonMediaListReviewState",
                    &row,
                    "ReviewState",
                )?,
                review_error: row.try_get("ReviewError").repository()?,
                published_revision: row.try_get("PublishedRevision").repository()?,
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
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let row =
            sqlx::query(r#"SELECT "AddonMediaListId" FROM "AddonMediaListItem" WHERE "Id" = ?"#)
                .bind(item_id)
                .fetch_optional(&mut *tx)
                .await
                .repository()?
                .ok_or(RepositoryError::NotFound {
                    kind: "add-on media list item",
                    id: item_id,
                })?;
        let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
        let updated =
            sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ContentKind" = ? WHERE "Id" = ?"#)
                .bind(crate::repositories::common::encode_enum(content_kind))
                .bind(item_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list item",
                id: item_id,
            });
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
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
        mark_list_ready_tx(&mut tx, id).await?;
        tx.commit().await.repository()?;
        Ok(updated.rows_affected())
    }

    async fn assign_episode_numbers(
        &self,
        id: EntityId,
        season: i32,
        starting_episode: i32,
        item_count: Option<u32>,
        missing_only: bool,
    ) -> Result<u64> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let review_state: Option<i32> =
            sqlx::query_scalar(r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id" = ?"#)
                .bind(id)
                .fetch_optional(&mut *tx)
                .await
                .repository()?;
        let Some(review_state) = review_state else {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            });
        };
        if review_state
            == crate::repositories::common::encode_enum(AddonMediaListReviewState::Publishing)
        {
            return Err(publication_in_progress());
        }

        let rows = sqlx::query(
            r#"SELECT "Id", "ProviderMetadataJson", "MetadataOverrideJson",
                      "MetadataConflictJson"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
                 AND "Included" = 1
               ORDER BY "Rank", "Id""#,
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await
        .repository()?;
        let limit = item_count.map_or(usize::MAX, |count| {
            usize::try_from(count).unwrap_or(usize::MAX)
        });
        let mut occupied_episodes = std::collections::BTreeSet::new();
        if missing_only || limit < rows.len() {
            let retained_rows = if missing_only {
                rows.as_slice()
            } else {
                &rows[limit..]
            };
            for row in retained_rows {
                let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
                let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
                let conflict_json: Option<String> =
                    row.try_get("MetadataConflictJson").repository()?;
                let metadata = metadata_form_from_json(
                    &provider_json,
                    &override_json,
                    conflict_json.as_deref(),
                )?
                .metadata;
                if metadata.season == Some(season) {
                    if let Some(episode) = metadata.episode.filter(|value| *value >= 1) {
                        occupied_episodes.insert(episode);
                    }
                }
            }
        }
        let mut assigned = 0_usize;
        let mut next_episode = starting_episode;
        for row in rows {
            if assigned == limit {
                break;
            }
            let item_id: EntityId = row.try_get("Id").repository()?;
            let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
            let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
            let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
            let mut form =
                metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())?;
            let has_valid_numbering = form.metadata.season.is_some_and(|value| value >= 0)
                && form.metadata.episode.is_some_and(|value| value >= 1);
            if missing_only && has_valid_numbering {
                continue;
            }

            while occupied_episodes.contains(&next_episode) {
                next_episode = next_episode.checked_add(1).ok_or_else(|| {
                    RepositoryError::Conflict(
                        "episode numbering exceeds the supported range".to_owned(),
                    )
                })?;
            }
            form.metadata.season = Some(season);
            form.metadata.episode = Some(next_episode);
            occupied_episodes.insert(next_episode);
            form.conflicts.remove("season");
            form.conflicts.remove("episode");
            let overrides =
                metadata_override_json(&metadata_json_value(&provider_json)?, &form.metadata)?;
            let conflicts = if form.conflicts.is_empty() {
                None
            } else {
                Some(serde_json::to_string(&form.conflicts).map_err(|error| {
                    RepositoryError::Decode {
                        kind: "add-on metadata conflicts",
                        value: error.to_string(),
                    }
                })?)
            };
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = ?
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(overrides)
            .bind(conflicts)
            .bind(item_id)
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
            assigned += 1;
            next_episode = next_episode.saturating_add(1);
        }

        if assigned > 0 {
            mark_list_ready_tx(&mut tx, id).await?;
        }
        tx.commit().await.repository()?;
        u64::try_from(assigned).map_err(|_| RepositoryError::Decode {
            kind: "assigned episode count",
            value: assigned.to_string(),
        })
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
        // The editor always reads the durable provider-plus-override draft.
        // The NFO is a published output and may intentionally still represent
        // the previous revision while this draft is being reviewed.
        let mut nfo = load_item_metadata_form(&self.0, list_id, item_id).await?;
        nfo.path_available = path.as_deref().is_some_and(Path::is_file);
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
        let mut nfo = load_list_metadata_form(&self.0, id).await?;
        nfo.path_available = path.as_deref().is_some_and(Path::is_file);
        Ok(nfo)
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
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
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
        let id = AddonMediaListRemoteSynchronizer::new(self.0.clone(), provider)
            .add_or_refresh(
                addon_id,
                source_url,
                requested_folder,
                content_kind_override,
                chapter_input,
            )
            .await?;
        if self
            .get_admin_form(id)
            .await?
            .is_some_and(|form| form.review_state == AddonMediaListReviewState::Published)
        {
            adopt_derived_presentation(&self.0, id).await?;
        }
        Ok(id)
    }

    async fn managed_library_id(&self, id: EntityId) -> Result<Option<EntityId>> {
        crate::addon_media_storage::list_library_id(&self.0, id).await
    }

    async fn match_items(&self, id: EntityId) -> Result<()> {
        let form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        if form.review_state == AddonMediaListReviewState::Publishing {
            return Err(publication_in_progress());
        }
        if form.review_state != AddonMediaListReviewState::Published {
            return Ok(());
        }
        AddonMediaListSynchronizer::new(self.0.clone())
            .match_list_items(id)
            .await
    }

    async fn stage_publication(&self, id: EntityId) -> Result<AddonMediaListPublication> {
        let form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        if form.review_state == AddonMediaListReviewState::Publishing {
            return Err(publication_in_progress());
        }
        let active_journal = sqlx::query_scalar::<_, Option<String>>(
            r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = ?"#,
        )
        .bind(id)
        .fetch_one(&self.0)
        .await
        .repository()?;
        if active_journal
            .as_deref()
            .is_some_and(|value| !value.trim().is_empty())
        {
            return Err(RepositoryError::Conflict(
                "a previous publication is still awaiting recovery".to_owned(),
            ));
        }
        let effective_presentation =
            if form.requested_presentation_kind == AddonMediaListPresentationKind::Auto {
                resolve_included_presentation(&self.0, id).await?
            } else {
                form.requested_presentation_kind
            };
        let items = self.query_items(id, None, 0, None).await?;
        if let Some(message) =
            publication_validation_error(&self.0, id, effective_presentation, &items).await?
        {
            record_publication_error(&self.0, id, &message).await?;
            return Err(RepositoryError::Conflict(message));
        }
        let library_id = self.managed_library_id(id).await?.ok_or_else(|| {
            RepositoryError::Conflict("managed list storage is unavailable".to_owned())
        })?;
        let previous_items = sqlx::query(
            r#"SELECT "Id", "Published", "MediaItemId"
               FROM "AddonMediaListItem"
               WHERE "AddonMediaListId" = ?
               ORDER BY "Id""#,
        )
        .bind(id)
        .fetch_all(&self.0)
        .await
        .repository()?
        .iter()
        .map(|row| {
            Ok(PublishedItemSnapshot {
                id: row.try_get("Id").repository()?,
                published: decode_bool_column(row, "Published")?,
                media_item_id: row.try_get("MediaItemId").repository()?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
        let previous_playlist = load_playlist_snapshot(&self.0, id).await?;
        let previous_materializations = load_artwork_materializations(&self.0, id).await?;
        let (managed_folder, backup_folder) = create_publication_backup(&self.0, id).await?;
        let journal = PublicationJournal {
            previous_review_state: form.review_state,
            previous_presentation_kind: form.presentation_kind,
            previous_items,
            previous_playlist,
            managed_folder: Some(managed_folder.to_string_lossy().into_owned()),
            backup_folder: Some(backup_folder.to_string_lossy().into_owned()),
            previous_materializations,
        };
        let journal_json = match serde_json::to_string(&journal) {
            Ok(value) => value,
            Err(error) => {
                remove_publication_backup(&managed_folder, &backup_folder);
                return Err(RepositoryError::Decode {
                    kind: "add-on publication journal",
                    value: error.to_string(),
                });
            }
        };
        let staged = async {
            let mut tx = crate::db::begin_write(&self.0).await.repository()?;
            let updated = sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "ReviewState" = 3, "ReviewError" = NULL,
                       "PublicationJournalJson" = ?, "PublicationStartedAt" = ?
                   WHERE "Id" = ? AND "ReviewState" <> 3
                     AND ("PublicationJournalJson" IS NULL OR TRIM("PublicationJournalJson") = '')"#,
            )
            .bind(journal_json)
            .bind(crate::repositories::common::encode_datetime(
                chrono::Utc::now(),
            ))
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
            if updated.rows_affected() != 1 {
                return Err(publication_in_progress());
            }
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "Published" = "Included"
                   WHERE "AddonMediaListId" = ?"#,
            )
            .bind(id)
            .execute(&mut *tx)
            .await
            .repository()?;
            tx.commit().await.repository()
        }
        .await;
        if let Err(error) = staged {
            remove_publication_backup(&managed_folder, &backup_folder);
            return Err(error);
        }
        Ok(AddonMediaListPublication {
            list_id: id,
            library_id,
        })
    }

    async fn commit_staged_publication(&self, id: EntityId) -> Result<()> {
        let journal = load_publication_journal(&self.0, id).await?;
        let form = self
            .get_admin_form(id)
            .await?
            .ok_or(RepositoryError::NotFound {
                kind: "add-on media list",
                id,
            })?;
        if form.review_state != AddonMediaListReviewState::Publishing {
            return Err(RepositoryError::Conflict(
                "the add-on media list has no staged publication".to_owned(),
            ));
        }
        let effective_presentation =
            if form.requested_presentation_kind == AddonMediaListPresentationKind::Auto {
                resolve_auto_presentation(&self.0, id).await?
            } else {
                form.requested_presentation_kind
            };
        if parent_nfo_for(effective_presentation).is_some() {
            ensure_generated_parent_nfo(&self.0, id, effective_presentation, None).await?;
        }
        reconcile_projection(&self.0, id, form.requested_presentation_kind).await?;
        AddonMediaListSynchronizer::new(self.0.clone())
            .match_list_items(id)
            .await?;

        let mut commit = crate::db::begin_write(&self.0).await.repository()?;
        if !form.generate_playlist {
            let playlist_id = sqlx::query_scalar::<_, Option<EntityId>>(
                r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(id)
            .fetch_one(&mut *commit)
            .await
            .repository()?;
            if let Some(playlist_id) = playlist_id {
                sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
                    .bind(playlist_id)
                    .execute(&mut *commit)
                    .await
                    .repository()?;
                sqlx::query(r#"UPDATE "AddonMediaList" SET "PlaylistId" = NULL WHERE "Id" = ?"#)
                    .bind(id)
                    .execute(&mut *commit)
                    .await
                    .repository()?;
            }
        }
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = 2, "PublishedRevision" = "PublishedRevision" + 1,
                   "ReviewError" = NULL, "PresentationError" = NULL,
                   "PublicationJournalJson" = NULL, "PublicationStartedAt" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .execute(&mut *commit)
        .await
        .repository()?;
        commit.commit().await.repository()?;
        cleanup_publication_backup(&journal)?;
        Ok(())
    }

    async fn restore_staged_publication(&self, id: EntityId, message: &str) -> Result<()> {
        let journal = load_publication_journal(&self.0, id).await?;
        restore_publication_backup(&journal)?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        for item in &journal.previous_items {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "Published" = ?, "MediaItemId" = ? WHERE "Id" = ?"#,
            )
            .bind(item.published)
            .bind(item.media_item_id)
            .bind(item.id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        sqlx::query(
            r#"DELETE FROM "AddonMediaListArtworkMaterialization"
               WHERE "AddonMediaListId" = ?"#,
        )
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
        for materialization in &journal.previous_materializations {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListArtworkMaterialization" (
                       "Id", "AddonMediaListId", "AddonMediaListItemId", "ScopeItemId", "Role",
                       "Source", "FileName", "Sha256", "PublishedRevision"
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(materialization.id)
            .bind(id)
            .bind(materialization.item_id)
            .bind(materialization.scope_item_id)
            .bind(materialization.role)
            .bind(&materialization.source)
            .bind(&materialization.file_name)
            .bind(&materialization.sha256)
            .bind(materialization.published_revision)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "ReviewState" = ?, "ReviewError" = ? WHERE "Id" = ?"#,
        )
        .bind(crate::repositories::common::encode_enum(
            journal.previous_review_state,
        ))
        .bind(message)
        .bind(id)
        .execute(&mut *tx)
        .await
        .repository()?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn reconcile_restored_publication(&self, id: EntityId) -> Result<()> {
        let journal = load_publication_journal(&self.0, id).await?;
        reconcile_projection(&self.0, id, journal.previous_presentation_kind).await?;
        AddonMediaListSynchronizer::new(self.0.clone())
            .match_list_items(id)
            .await?;
        restore_playlist_snapshot(&self.0, id, journal.previous_playlist.as_ref()).await?;
        sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "PublicationJournalJson" = NULL, "PublicationStartedAt" = NULL
               WHERE "Id" = ?"#,
        )
        .bind(id)
        .execute(&self.0)
        .await
        .repository()?;
        Ok(())
    }

    async fn upload_artwork(
        &self,
        list_id: EntityId,
        item_id: Option<EntityId>,
        role: AddonMediaListArtworkRole,
        content_type: &str,
        bytes: &[u8],
        expected_etag: &str,
    ) -> Result<AddonMediaListNfoForm> {
        if role == AddonMediaListArtworkRole::Other {
            return Err(RepositoryError::Conflict(
                "uploaded artwork must be assigned as poster, fanart, or thumbnail".to_owned(),
            ));
        }
        let (content_type, extension) = crate::addon_artwork::validate_image(content_type, bytes)
            .map_err(RepositoryError::Conflict)?;
        let current = match item_id {
            Some(item_id) => load_item_metadata_form(&self.0, list_id, item_id).await?,
            None => load_list_metadata_form(&self.0, list_id).await?,
        };
        if current.etag != expected_etag {
            return Err(RepositoryError::Conflict(
                "metadata changed since the editor was loaded".to_owned(),
            ));
        }
        let role_value = artwork_role_value(role);
        let scope_item_id = item_id.unwrap_or(0);
        let now = crate::repositories::common::encode_datetime(chrono::Utc::now());
        let sha256 = hex_hash(bytes);
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(item_id) = item_id {
            let owned: i64 = sqlx::query_scalar(
                r#"SELECT COUNT(*) FROM "AddonMediaListItem"
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(item_id)
            .bind(list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?;
            if owned == 0 {
                return Err(RepositoryError::NotFound {
                    kind: "add-on media list item",
                    id: item_id,
                });
            }
        }
        let existing_id = sqlx::query_scalar::<_, EntityId>(
            r#"SELECT "Id" FROM "AddonMediaListArtworkUpload"
               WHERE "AddonMediaListId" = ? AND "ScopeItemId" = ? AND "Role" = ?"#,
        )
        .bind(list_id)
        .bind(scope_item_id)
        .bind(role_value)
        .fetch_optional(&mut *tx)
        .await
        .repository()?;
        let upload_id = if let Some(upload_id) = existing_id {
            sqlx::query(
                r#"UPDATE "AddonMediaListArtworkUpload"
                   SET "ContentType" = ?, "Extension" = ?, "Bytes" = ?, "Sha256" = ?,
                       "UpdatedAt" = ?
                   WHERE "Id" = ?"#,
            )
            .bind(content_type)
            .bind(extension)
            .bind(bytes)
            .bind(&sha256)
            .bind(&now)
            .bind(upload_id)
            .execute(&mut *tx)
            .await
            .repository()?;
            upload_id
        } else {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListArtworkUpload" (
                       "AddonMediaListId", "AddonMediaListItemId", "ScopeItemId", "Role",
                       "ContentType", "Extension", "Bytes", "Sha256", "CreatedAt", "UpdatedAt"
                   ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)"#,
            )
            .bind(list_id)
            .bind(item_id)
            .bind(scope_item_id)
            .bind(role_value)
            .bind(content_type)
            .bind(extension)
            .bind(bytes)
            .bind(&sha256)
            .bind(&now)
            .bind(&now)
            .execute(&mut *tx)
            .await
            .repository()?;
            sqlx::query_scalar(
                r#"SELECT "Id" FROM "AddonMediaListArtworkUpload"
                   WHERE "AddonMediaListId" = ? AND "ScopeItemId" = ? AND "Role" = ?"#,
            )
            .bind(list_id)
            .bind(scope_item_id)
            .bind(role_value)
            .fetch_one(&mut *tx)
            .await
            .repository()?
        };
        let mut metadata = current.metadata;
        metadata.artwork.retain(|candidate| candidate.role != role);
        metadata.artwork.push(AddonMediaListArtwork {
            url: format!("{}{}", crate::addon_artwork::UPLOAD_URL_PREFIX, upload_id),
            role,
            width: None,
            height: None,
            language: None,
        });
        let provider_json: String = if let Some(item_id) = item_id {
            sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem"
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(item_id)
            .bind(list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?
        } else {
            sqlx::query_scalar(
                r#"SELECT "ProviderMetadataJson" FROM "AddonMediaList" WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .fetch_one(&mut *tx)
            .await
            .repository()?
        };
        let overrides = metadata_override_json(&metadata_json_value(&provider_json)?, &metadata)?;
        if let Some(item_id) = item_id {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(overrides)
            .bind(item_id)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL
                   WHERE "Id" = ?"#,
            )
            .bind(overrides)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        match item_id {
            Some(item_id) => load_item_metadata_form(&self.0, list_id, item_id).await,
            None => load_list_metadata_form(&self.0, list_id).await,
        }
    }

    async fn get_artwork_upload(
        &self,
        upload_id: EntityId,
    ) -> Result<Option<AddonMediaListArtworkUpload>> {
        sqlx::query_as::<_, (String, Vec<u8>)>(
            r#"SELECT "ContentType", "Bytes"
               FROM "AddonMediaListArtworkUpload" WHERE "Id" = ?"#,
        )
        .bind(upload_id)
        .fetch_optional(&self.0)
        .await
        .repository()
        .map(|row| {
            row.map(|(content_type, bytes)| AddonMediaListArtworkUpload {
                content_type,
                bytes,
            })
        })
    }

    async fn set_item_included(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        included: bool,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "Included" = ?
               WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
        )
        .bind(included)
        .bind(item_id)
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list item",
                id: item_id,
            });
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
    }

    async fn set_item_collection_role(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        role: AddonMediaListItemRole,
    ) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if role == AddonMediaListItemRole::Primary {
            // One movie collection can have only one feature. Clearing the old
            // primary is explicit and never changes the provider order.
            sqlx::query(
                r#"UPDATE "AddonMediaListItem" SET "CollectionRole" = 2
                   WHERE "AddonMediaListId" = ? AND "CollectionRole" = 1"#,
            )
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "CollectionRole" = ?
               WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
        )
        .bind(crate::repositories::common::encode_enum(role))
        .bind(item_id)
        .bind(list_id)
        .execute(&mut *tx)
        .await
        .repository()?;
        if updated.rows_affected() == 0 {
            return Err(RepositoryError::NotFound {
                kind: "add-on media list item",
                id: item_id,
            });
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
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
        let mut review_tx = crate::db::begin_write(&self.0).await.repository()?;
        mark_list_ready_tx(&mut review_tx, id).await?;
        review_tx.commit().await.repository()?;
        // A library-type edit changes only the local publication projection.
        // The imported provider snapshot is already durable, so changing its
        // list-wide content-kind override must not execute the add-on again.
        // Legacy chapter input remains provider input and therefore still
        // requires re-enumeration when that separate setting changes.
        if existing.legacy_chapter_input != chapter_input {
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
        _show_title: Option<&str>,
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
        let updated = sqlx::query(
            r#"UPDATE "AddonMediaList"
               SET "RequestedPresentationKind" = ?, "PresentationError" = NULL,
                   "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
                   "ReviewError" = NULL
               WHERE "Id" = ? AND "ReviewState" <> 3"#,
        )
        .bind(crate::repositories::common::encode_enum(presentation_kind))
        .bind(id)
        .execute(&self.0)
        .await
        .repository()?;
        if updated.rows_affected() != 1 {
            return Err(publication_in_progress());
        }
        Ok(())
    }

    async fn save_show_nfo(
        &self,
        id: EntityId,
        metadata: &AddonMediaListNfoMetadata,
        expected_etag: &str,
    ) -> Result<()> {
        let current = load_list_metadata_form(&self.0, id).await?;
        let accepted_etag = if current.etag == expected_etag {
            current.etag
        } else {
            let path = self.parent_nfo_path(id).await?;
            let disk = crate::addon_media_nfo::read_form(path.as_deref()).map_err(map_nfo_error)?;
            if disk.etag != expected_etag {
                return Err(RepositoryError::Conflict(
                    "metadata changed since the editor was loaded".to_owned(),
                ));
            }
            current.etag
        };
        save_list_metadata_override(&self.0, id, metadata, &accepted_etag).await
    }

    async fn save_item_nfo(
        &self,
        list_id: EntityId,
        item_id: EntityId,
        metadata: &AddonMediaListNfoMetadata,
        expected_etag: &str,
    ) -> Result<()> {
        let current = load_item_metadata_form(&self.0, list_id, item_id).await?;
        let accepted_etag = if current.etag == expected_etag {
            current.etag
        } else {
            let path = item_nfo_path_for(&self.0, list_id, item_id).await?;
            let disk = crate::addon_media_nfo::read_form(path.as_deref()).map_err(map_nfo_error)?;
            if disk.etag != expected_etag {
                return Err(RepositoryError::Conflict(
                    "metadata changed since the editor was loaded".to_owned(),
                ));
            }
            current.etag
        };
        save_item_metadata_override(&self.0, list_id, item_id, metadata, &accepted_etag).await
    }

    async fn reset_nfo(&self, list_id: EntityId, item_id: Option<EntityId>) -> Result<()> {
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        if let Some(item_id) = item_id {
            sqlx::query(
                r#"UPDATE "AddonMediaListItem"
                   SET "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL
                   WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
            )
            .bind(item_id)
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        } else {
            sqlx::query(
                r#"UPDATE "AddonMediaList"
                   SET "MetadataOverrideJson" = '{}', "MetadataConflictJson" = NULL
                   WHERE "Id" = ?"#,
            )
            .bind(list_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        }
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
        Ok(())
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
        let list_id: EntityId = row.try_get("AddonMediaListId").repository()?;
        let mut tx = crate::db::begin_write(&self.0).await.repository()?;
        sqlx::query(r#"UPDATE "AddonMediaListItem" SET "ChapterInput" = ? WHERE "Id" = ?"#)
            .bind((!chapter_input.trim().is_empty()).then_some(chapter_input.trim()))
            .bind(item_id)
            .execute(&mut *tx)
            .await
            .repository()?;
        mark_list_ready_tx(&mut tx, list_id).await?;
        tx.commit().await.repository()?;
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
        Ok(())
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

async fn resolve_included_presentation(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<AddonMediaListPresentationKind> {
    let kinds = sqlx::query_scalar::<_, i32>(
        r#"SELECT DISTINCT "ContentKind" FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
             AND "Included" = 1"#,
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
    Ok(AddonMediaListPresentationKind::from_content_kind(kind)
        .filter(|presentation| presentation.is_projected())
        .unwrap_or(AddonMediaListPresentationKind::RemoteStreams))
}

async fn load_publication_journal(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<PublicationJournal> {
    let value = sqlx::query_scalar::<_, Option<String>>(
        r#"SELECT "PublicationJournalJson" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?
    .filter(|value| !value.trim().is_empty())
    .ok_or_else(|| {
        RepositoryError::Conflict("the add-on media list has no publication journal".to_owned())
    })?;
    serde_json::from_str(&value).map_err(|error| RepositoryError::Decode {
        kind: "add-on publication journal",
        value: error.to_string(),
    })
}

async fn load_artwork_materializations(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<Vec<ArtworkMaterializationSnapshot>> {
    sqlx::query(
        r#"SELECT "Id", "AddonMediaListItemId", "ScopeItemId", "Role", "Source",
                  "FileName", "Sha256", "PublishedRevision"
           FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = ? ORDER BY "Id""#,
    )
    .bind(list_id)
    .fetch_all(pool)
    .await
    .repository()?
    .iter()
    .map(|row| {
        Ok(ArtworkMaterializationSnapshot {
            id: row.try_get("Id").repository()?,
            item_id: row.try_get("AddonMediaListItemId").repository()?,
            scope_item_id: row.try_get("ScopeItemId").repository()?,
            role: row.try_get("Role").repository()?,
            source: row.try_get("Source").repository()?,
            file_name: row.try_get("FileName").repository()?,
            sha256: row.try_get("Sha256").repository()?,
            published_revision: row.try_get("PublishedRevision").repository()?,
        })
    })
    .collect()
}

async fn create_publication_backup(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<(PathBuf, PathBuf)> {
    let manifest: String = sqlx::query_scalar(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let manifest_path = Path::new(&manifest);
    let managed_path = if manifest_path.is_dir() {
        manifest_path
    } else {
        manifest_path.parent().ok_or_else(|| {
            RepositoryError::unavailable(
                "snapshot add-on publication",
                "the managed manifest has no parent folder",
            )
        })?
    };
    let managed = managed_path.canonicalize().map_err(|error| {
        RepositoryError::unavailable("snapshot add-on publication", error.to_string())
    })?;
    let parent = managed.parent().ok_or_else(|| {
        RepositoryError::unavailable(
            "snapshot add-on publication",
            "the managed folder has no parent",
        )
    })?;
    let backup = parent.join(format!(
        ".ersatzrs-publication-{list_id}-{}",
        uuid::Uuid::new_v4()
    ));
    copy_publication_tree(&managed, &backup).map_err(|error| {
        RepositoryError::unavailable("snapshot add-on publication", error.to_string())
    })?;
    Ok((managed, backup))
}

fn copy_publication_tree(source: &Path, destination: &Path) -> std::io::Result<()> {
    std::fs::create_dir(destination)?;
    for entry in std::fs::read_dir(source)? {
        let entry = entry?;
        let metadata = std::fs::symlink_metadata(entry.path())?;
        if metadata.file_type().is_symlink() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "managed publication folders may not contain symbolic links",
            ));
        }
        let target = destination.join(entry.file_name());
        if metadata.is_dir() {
            copy_publication_tree(&entry.path(), &target)?;
        } else if metadata.is_file() && std::fs::hard_link(entry.path(), &target).is_err() {
            std::fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

fn publication_backup_paths(journal: &PublicationJournal) -> Result<(PathBuf, PathBuf)> {
    let managed = journal
        .managed_folder
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| {
            RepositoryError::unavailable(
                "restore add-on publication",
                "the publication journal has no managed folder",
            )
        })?;
    let backup = journal
        .backup_folder
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| {
            RepositoryError::unavailable(
                "restore add-on publication",
                "the publication journal has no backup folder",
            )
        })?;
    let safe = managed.parent().is_some()
        && managed.parent() == backup.parent()
        && backup
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".ersatzrs-publication-"));
    if !safe {
        return Err(RepositoryError::unavailable(
            "restore add-on publication",
            "the publication backup path failed its ownership check",
        ));
    }
    Ok((managed, backup))
}

fn restore_publication_backup(journal: &PublicationJournal) -> Result<()> {
    let (managed, backup) = publication_backup_paths(journal)?;
    if !backup.is_dir() || !managed.is_dir() {
        return Err(RepositoryError::unavailable(
            "restore add-on publication",
            "the managed folder or its publication backup is missing",
        ));
    }
    let failed = managed
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(format!(
            ".ersatzrs-failed-publication-{}",
            uuid::Uuid::new_v4()
        ));
    std::fs::rename(&managed, &failed).map_err(|error| {
        RepositoryError::unavailable("restore add-on publication", error.to_string())
    })?;
    if let Err(error) = std::fs::rename(&backup, &managed) {
        let rollback = std::fs::rename(&failed, &managed);
        return Err(RepositoryError::unavailable(
            "restore add-on publication",
            match rollback {
                Ok(()) => error.to_string(),
                Err(rollback_error) => {
                    format!("{error}; failed to put the staged folder back: {rollback_error}")
                }
            },
        ));
    }
    if let Err(error) = std::fs::remove_dir_all(&failed) {
        tracing::warn!(
            path = %failed.display(),
            %error,
            "unable to remove failed add-on publication folder after recovery"
        );
    }
    Ok(())
}

fn cleanup_publication_backup(journal: &PublicationJournal) -> Result<()> {
    let (managed, backup) = publication_backup_paths(journal)?;
    remove_publication_backup(&managed, &backup);
    Ok(())
}

fn remove_publication_backup(managed: &Path, backup: &Path) {
    let safe = managed.parent().is_some()
        && managed.parent() == backup.parent()
        && backup
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with(".ersatzrs-publication-"));
    if safe && backup.exists() {
        if let Err(error) = std::fs::remove_dir_all(backup) {
            tracing::warn!(
                path = %backup.display(),
                %error,
                "unable to remove completed add-on publication backup"
            );
        }
    }
}

async fn load_playlist_snapshot(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
) -> Result<Option<PlaylistSnapshot>> {
    let playlist_id = sqlx::query_scalar::<_, Option<EntityId>>(
        r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let Some(playlist_id) = playlist_id else {
        return Ok(None);
    };
    let playlist = sqlx::query(
        r#"SELECT "Id", "IsSystem", "Name", "PlaylistGroupId"
           FROM "Playlist" WHERE "Id" = ?"#,
    )
    .bind(playlist_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let items = sqlx::query(
        r#"SELECT "Id", "Index", "CollectionType", "CollectionId", "MediaItemId",
                  "MultiCollectionId", "SmartCollectionId", "IncludeInProgramGuide",
                  "PlaybackOrder", "PlayAll", "Count", "PreferredAudioLanguageCode",
                  "PreferredAudioTitle", "PreferredSubtitleLanguageCode", "SubtitleMode",
                  "MediaVersionId", "VideoStreamIndex", "AudioStreamIndex",
                  "SubtitleStreamIndex"
           FROM "PlaylistItem" WHERE "PlaylistId" = ? ORDER BY "Index", "Id""#,
    )
    .bind(playlist_id)
    .fetch_all(pool)
    .await
    .repository()?
    .iter()
    .map(|row| {
        Ok(PlaylistItemSnapshot {
            id: row.try_get("Id").repository()?,
            index: row.try_get("Index").repository()?,
            collection_type: row.try_get("CollectionType").repository()?,
            collection_id: row.try_get("CollectionId").repository()?,
            media_item_id: row.try_get("MediaItemId").repository()?,
            multi_collection_id: row.try_get("MultiCollectionId").repository()?,
            smart_collection_id: row.try_get("SmartCollectionId").repository()?,
            include_in_program_guide: decode_bool_column(row, "IncludeInProgramGuide")?,
            playback_order: row.try_get("PlaybackOrder").repository()?,
            play_all: decode_bool_column(row, "PlayAll")?,
            count: row.try_get("Count").repository()?,
            preferred_audio_language_code: row
                .try_get("PreferredAudioLanguageCode")
                .repository()?,
            preferred_audio_title: row.try_get("PreferredAudioTitle").repository()?,
            preferred_subtitle_language_code: row
                .try_get("PreferredSubtitleLanguageCode")
                .repository()?,
            subtitle_mode: row.try_get("SubtitleMode").repository()?,
            media_version_id: row.try_get("MediaVersionId").repository()?,
            video_stream_index: row.try_get("VideoStreamIndex").repository()?,
            audio_stream_index: row.try_get("AudioStreamIndex").repository()?,
            subtitle_stream_index: row.try_get("SubtitleStreamIndex").repository()?,
        })
    })
    .collect::<Result<Vec<_>>>()?;
    Ok(Some(PlaylistSnapshot {
        id: playlist.try_get("Id").repository()?,
        is_system: decode_bool_column(&playlist, "IsSystem")?,
        name: playlist.try_get("Name").repository()?,
        playlist_group_id: playlist.try_get("PlaylistGroupId").repository()?,
        items,
    }))
}

async fn restore_playlist_snapshot(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    snapshot: Option<&PlaylistSnapshot>,
) -> Result<()> {
    let current_id = sqlx::query_scalar::<_, Option<EntityId>>(
        r#"SELECT "PlaylistId" FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    match snapshot {
        None => {
            if let Some(current_id) = current_id {
                sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
                    .bind(current_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PlaylistId" = NULL WHERE "Id" = ?"#)
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
        }
        Some(snapshot) => {
            if current_id.is_some_and(|current_id| current_id != snapshot.id) {
                sqlx::query(r#"DELETE FROM "Playlist" WHERE "Id" = ?"#)
                    .bind(current_id)
                    .execute(&mut *tx)
                    .await
                    .repository()?;
            }
            let exists: i64 =
                sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playlist" WHERE "Id" = ?"#)
                    .bind(snapshot.id)
                    .fetch_one(&mut *tx)
                    .await
                    .repository()?;
            if exists == 0 {
                sqlx::query(
                    r#"INSERT INTO "Playlist" ("Id", "IsSystem", "Name", "PlaylistGroupId")
                       VALUES (?, ?, ?, ?)"#,
                )
                .bind(snapshot.id)
                .bind(snapshot.is_system)
                .bind(&snapshot.name)
                .bind(snapshot.playlist_group_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            } else {
                sqlx::query(
                    r#"UPDATE "Playlist"
                       SET "IsSystem" = ?, "Name" = ?, "PlaylistGroupId" = ?
                       WHERE "Id" = ?"#,
                )
                .bind(snapshot.is_system)
                .bind(&snapshot.name)
                .bind(snapshot.playlist_group_id)
                .bind(snapshot.id)
                .execute(&mut *tx)
                .await
                .repository()?;
            }
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PlaylistId" = ? WHERE "Id" = ?"#)
                .bind(snapshot.id)
                .bind(list_id)
                .execute(&mut *tx)
                .await
                .repository()?;
            // The matcher immediately before this function has rebuilt the
            // generated members against the restored projection identities.
            // Re-inserting the old raw media ids here would be incorrect after
            // a presentation switch recreated those identities. Keep the
            // snapshot members in the journal for audit evidence, while the
            // deterministic matcher owns their recovered rows.
            let _previous_item_count = snapshot.items.len();
        }
    }
    tx.commit().await.repository()?;
    Ok(())
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

async fn load_list_metadata_form(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
) -> Result<AddonMediaListNfoForm> {
    let row = sqlx::query(
        r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id,
    })?;
    let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
    let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
    let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
    metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())
}

async fn load_item_metadata_form(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    item_id: EntityId,
) -> Result<AddonMediaListNfoForm> {
    let row = sqlx::query(
        r#"SELECT "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson"
           FROM "AddonMediaListItem"
           WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
    )
    .bind(item_id)
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list item",
        id: item_id,
    })?;
    let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
    let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
    let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
    metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())
}

fn metadata_form_from_json(
    provider_json: &str,
    override_json: &str,
    conflict_json: Option<&str>,
) -> Result<AddonMediaListNfoForm> {
    let provider = metadata_json_value(provider_json)?;
    let overrides = metadata_json_value(override_json)?;
    let effective = merge_metadata_values(provider.clone(), &overrides);
    let metadata = serde_json::from_value(effective).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata",
        value: error.to_string(),
    })?;
    let conflicts = conflict_json
        .filter(|value| !value.trim().is_empty())
        .map(
            serde_json::from_str::<
                std::collections::BTreeMap<String, AddonMediaListMetadataConflict>,
            >,
        )
        .transpose()
        .map_err(|error| RepositoryError::Decode {
            kind: "add-on metadata conflicts",
            value: error.to_string(),
        })?
        .unwrap_or_default();
    Ok(AddonMediaListNfoForm {
        metadata,
        etag: metadata_json_etag(&provider, &overrides),
        ownership: if overrides.as_object().is_some_and(serde_json::Map::is_empty) {
            AddonMediaListNfoOwnership::Generated
        } else {
            AddonMediaListNfoOwnership::User
        },
        error: None,
        path_available: false,
        conflicts,
    })
}

fn metadata_json_value(json: &str) -> Result<serde_json::Value> {
    serde_json::from_str(json).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata JSON",
        value: error.to_string(),
    })
}

fn merge_metadata_values(
    mut provider: serde_json::Value,
    overrides: &serde_json::Value,
) -> serde_json::Value {
    if let (Some(provider), Some(overrides)) = (provider.as_object_mut(), overrides.as_object()) {
        for (field, value) in overrides {
            provider.insert(field.clone(), value.clone());
        }
    }
    provider
}

fn metadata_json_etag(provider: &serde_json::Value, overrides: &serde_json::Value) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(provider).unwrap_or_default());
    digest.update([0]);
    digest.update(serde_json::to_vec(overrides).unwrap_or_default());
    let mut etag = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut etag, "{byte:02x}").expect("writing to a String cannot fail");
    }
    etag
}

fn hex_hash(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    let mut value = String::with_capacity(64);
    for byte in digest.finalize() {
        write!(&mut value, "{byte:02x}").expect("writing to a String cannot fail");
    }
    value
}

const fn artwork_role_value(role: AddonMediaListArtworkRole) -> i32 {
    match role {
        AddonMediaListArtworkRole::Poster => 1,
        AddonMediaListArtworkRole::Fanart => 2,
        AddonMediaListArtworkRole::Thumb => 3,
        AddonMediaListArtworkRole::Other => 0,
    }
}

/// Move an already published list back to review while preserving its last
/// published item snapshot and library projection. A never-published import
/// remains a draft until its first explicit publication.
async fn mark_list_ready_tx(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    list_id: EntityId,
) -> Result<()> {
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
               "ReviewError" = NULL
           WHERE "Id" = ? AND "ReviewState" <> 3"#,
    )
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(publication_in_progress());
    }
    Ok(())
}

fn publication_in_progress() -> RepositoryError {
    RepositoryError::Conflict(
        "the add-on media list is being published; wait for publication to finish".to_owned(),
    )
}

async fn publication_validation_error(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    presentation: AddonMediaListPresentationKind,
    items: &[AddonMediaListItemAdminRow],
) -> Result<Option<String>> {
    let list_metadata = load_list_metadata_form(pool, list_id).await?.metadata;
    if list_metadata.title.trim().is_empty() {
        return Ok(Some(
            "collection title is required before publication".to_owned(),
        ));
    }
    if let Some(message) = invalid_artwork_message("collection", &list_metadata) {
        return Ok(Some(message));
    }

    let included = items
        .iter()
        .filter(|item| item.included)
        .collect::<Vec<_>>();
    if included.is_empty() {
        return Ok(Some(
            "at least one item must be included before publication".to_owned(),
        ));
    }

    if presentation == AddonMediaListPresentationKind::Movies {
        let primary_count = included
            .iter()
            .filter(|item| {
                item.parent_provider_id.is_none()
                    && item.collection_role == AddonMediaListItemRole::Primary
            })
            .count();
        if primary_count != 1 {
            return Ok(Some(format!(
                "movie publication requires exactly one included main movie; found {primary_count}"
            )));
        }
    }

    for item in included {
        if item.availability == AddonMediaListItemAvailability::Unavailable
            && !item.replacement_valid
        {
            return Ok(Some(format!(
                "{} is unavailable and has no valid local replacement",
                item.display_title
            )));
        }
        let metadata = load_item_metadata_form(pool, list_id, item.id)
            .await?
            .metadata;
        if metadata.title.trim().is_empty() {
            return Ok(Some(format!(
                "item {} requires a title before publication",
                item.rank.saturating_add(1)
            )));
        }
        if let Some(message) = invalid_artwork_message(&item.display_title, &metadata) {
            return Ok(Some(message));
        }
        if presentation == AddonMediaListPresentationKind::TelevisionShow
            && (metadata.season.is_none_or(|season| season < 0)
                || metadata.episode.is_none_or(|episode| episode < 1))
        {
            return Ok(Some(format!(
                "{} requires a valid season and episode number",
                item.display_title
            )));
        }
        if matches!(
            presentation,
            AddonMediaListPresentationKind::MusicVideos | AddonMediaListPresentationKind::Songs
        ) && metadata
            .artists
            .iter()
            .all(|artist| artist.trim().is_empty())
        {
            return Ok(Some(format!(
                "{} requires at least one artist",
                item.display_title
            )));
        }
    }
    Ok(None)
}

fn invalid_artwork_message(owner: &str, metadata: &AddonMediaListNfoMetadata) -> Option<String> {
    metadata
        .artwork
        .iter()
        .map(|candidate| candidate.url.as_str())
        .chain(metadata.artwork_urls.iter().map(String::as_str))
        .find(|url| {
            crate::addon_artwork::upload_id(url).is_none() && !is_safe_remote_artwork_url(url)
        })
        .map(|_| format!("{owner} contains an invalid artwork URL; use a bounded HTTPS URL"))
}

fn is_safe_remote_artwork_url(value: &str) -> bool {
    let value = value.trim();
    if value.is_empty() || value.len() > 2_048 {
        return false;
    }
    url::Url::parse(value).is_ok_and(|url| {
        url.scheme() == "https"
            && url.has_host()
            && url.username().is_empty()
            && url.password().is_none()
    })
}

async fn record_publication_error(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    message: &str,
) -> Result<()> {
    sqlx::query(r#"UPDATE "AddonMediaList" SET "ReviewError" = ? WHERE "Id" = ?"#)
        .bind(message)
        .bind(list_id)
        .execute(pool)
        .await
        .repository()?;
    Ok(())
}

fn metadata_override_json(
    provider: &serde_json::Value,
    metadata: &AddonMediaListNfoMetadata,
) -> Result<String> {
    let submitted = serde_json::to_value(metadata).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata",
        value: error.to_string(),
    })?;
    let mut overrides = serde_json::Map::new();
    if let Some(fields) = submitted.as_object() {
        for (field, value) in fields {
            if provider.get(field) != Some(value) {
                overrides.insert(field.clone(), value.clone());
            }
        }
    }
    serde_json::to_string(&overrides).map_err(|error| RepositoryError::Decode {
        kind: "add-on metadata override",
        value: error.to_string(),
    })
}

async fn save_list_metadata_override(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
    metadata: &AddonMediaListNfoMetadata,
    expected_etag: &str,
) -> Result<()> {
    let current = load_list_metadata_form(pool, id).await?;
    if current.etag != expected_etag {
        return Err(RepositoryError::Conflict(
            "metadata changed since the editor was loaded".to_owned(),
        ));
    }
    let provider_json: String =
        sqlx::query_scalar(r#"SELECT "ProviderMetadataJson" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(id)
            .fetch_one(pool)
            .await
            .repository()?;
    let overrides = metadata_override_json(&metadata_json_value(&provider_json)?, metadata)?;
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaList"
           SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL,
               "ReviewState" = CASE WHEN "ReviewState" = 0 THEN 0 ELSE 1 END,
               "ReviewError" = NULL
           WHERE "Id" = ? AND "ReviewState" <> 3"#,
    )
    .bind(overrides)
    .bind(id)
    .execute(pool)
    .await
    .repository()?;
    if updated.rows_affected() != 1 {
        return Err(publication_in_progress());
    }
    Ok(())
}

async fn save_item_metadata_override(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    item_id: EntityId,
    metadata: &AddonMediaListNfoMetadata,
    expected_etag: &str,
) -> Result<()> {
    let current = load_item_metadata_form(pool, list_id, item_id).await?;
    if current.etag != expected_etag {
        return Err(RepositoryError::Conflict(
            "metadata changed since the editor was loaded".to_owned(),
        ));
    }
    let provider_json: String = sqlx::query_scalar(
        r#"SELECT "ProviderMetadataJson" FROM "AddonMediaListItem"
           WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
    )
    .bind(item_id)
    .bind(list_id)
    .fetch_one(pool)
    .await
    .repository()?;
    let overrides = metadata_override_json(&metadata_json_value(&provider_json)?, metadata)?;
    let mut tx = crate::db::begin_write(pool).await.repository()?;
    sqlx::query(
        r#"UPDATE "AddonMediaListItem"
           SET "MetadataOverrideJson" = ?, "MetadataConflictJson" = NULL
           WHERE "Id" = ? AND "AddonMediaListId" = ?"#,
    )
    .bind(overrides)
    .bind(item_id)
    .bind(list_id)
    .execute(&mut *tx)
    .await
    .repository()?;
    mark_list_ready_tx(&mut tx, list_id).await?;
    tx.commit().await.repository()?;
    Ok(())
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
    let scanned_path = sqlx::query_scalar::<_, Option<String>>(
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
    if let Some(path) = scanned_path {
        return Ok(Path::new(&path)
            .parent()
            .map(|parent| parent.join(file_name)));
    }
    // A newly imported managed list has a manifest folder before its first
    // library scan has created MediaVersion/MediaFile rows. The review and
    // auto-presentation paths still need a parent NFO in that folder.
    let manifest_path = sqlx::query_scalar::<_, String>(
        r#"SELECT "ManifestPath" FROM "AddonMediaListLibrary"
           WHERE "AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(pool)
    .await
    .repository()?;
    Ok(manifest_path.and_then(|path| {
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
        AddonMediaListPresentationKind::MusicVideos => Some(("artist.nfo", "artist")),
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
    nfo_path: PathBuf,
    state: i32,
}

/// Related metadata tables supported by one projected metadata record.
///
/// The shared tables use sparse foreign keys. Keep the real column and the two
/// non-uniform credit capabilities here rather than assuming each kind has the
/// same schema: `Director` does not support songs or images, `Writer` also does
/// not support music videos, and `MusicVideoArtist` is music-video-only.
#[derive(Clone, Copy)]
struct ProjectedMetadataRelations {
    fk_column: &'static str,
    directors: bool,
    writers: bool,
    music_video_artists: bool,
}

impl ProjectedMetadataRelations {
    const ARTIST: Self = Self {
        fk_column: "ArtistMetadataId",
        directors: false,
        writers: false,
        music_video_artists: false,
    };
    const SHOW: Self = Self {
        fk_column: "ShowMetadataId",
        directors: false,
        writers: false,
        music_video_artists: false,
    };
    const EPISODE: Self = Self {
        fk_column: "EpisodeMetadataId",
        directors: true,
        writers: true,
        music_video_artists: false,
    };
    const MOVIE: Self = Self {
        fk_column: "MovieMetadataId",
        directors: true,
        writers: true,
        music_video_artists: false,
    };
    const MUSIC_VIDEO: Self = Self {
        fk_column: "MusicVideoMetadataId",
        directors: true,
        writers: false,
        music_video_artists: true,
    };
    const OTHER_VIDEO: Self = Self {
        fk_column: "OtherVideoMetadataId",
        directors: true,
        writers: true,
        music_video_artists: false,
    };
    const SONG: Self = Self {
        fk_column: "SongMetadataId",
        directors: false,
        writers: false,
        music_video_artists: false,
    };
    const IMAGE: Self = Self {
        fk_column: "ImageMetadataId",
        directors: false,
        writers: false,
        music_video_artists: false,
    };
}

/// Replace every related row represented by a managed add-on-list NFO.
///
/// Projected metadata rows are recreated on refresh, but these shared tables
/// deserve the same explicit replacement semantics: it keeps retries safe and
/// means a future metadata upsert cannot leave duplicate facets behind. Remote
/// Stream metadata remains separately scanner-owned; this writes the distinct
/// metadata row of the requested library presentation.
async fn replace_projected_metadata_relations(
    tx: &mut sqlx::Transaction<'_, crate::db::Db>,
    relations: ProjectedMetadataRelations,
    metadata_id: EntityId,
    metadata: &AddonMediaListNfoMetadata,
    nfo_path: &Path,
) -> Result<()> {
    let fk_column = relations.fk_column;
    replace_strings(tx, "Genre", fk_column, metadata_id, &metadata.genres).await?;
    replace_strings(tx, "Tag", fk_column, metadata_id, &metadata.tags).await?;
    replace_strings(tx, "Studio", fk_column, metadata_id, &metadata.studios).await?;
    replace_strings(
        tx,
        "MusicLabel",
        fk_column,
        metadata_id,
        &metadata.music_labels,
    )
    .await?;
    replace_metadata_languages(tx, fk_column, metadata_id, &metadata.languages).await?;

    let actors = metadata
        .people
        .iter()
        .filter(|person| !person.name.trim().is_empty())
        .map(|person| PersonRef::with_role(person.name.clone(), person.role.clone(), person.order))
        .collect::<Vec<_>>();
    replace_people(tx, "Actor", fk_column, metadata_id, &actors).await?;

    if relations.directors {
        let directors = metadata
            .directors
            .iter()
            .cloned()
            .map(PersonRef::new)
            .collect::<Vec<_>>();
        replace_people(tx, "Director", fk_column, metadata_id, &directors).await?;
    }
    if relations.writers {
        let writers = metadata
            .writers
            .iter()
            .cloned()
            .map(PersonRef::new)
            .collect::<Vec<_>>();
        replace_people(tx, "Writer", fk_column, metadata_id, &writers).await?;
    }
    if relations.music_video_artists {
        replace_strings(
            tx,
            "MusicVideoArtist",
            fk_column,
            metadata_id,
            &metadata.artists,
        )
        .await?;
    }
    replace_guids(tx, fk_column, metadata_id, &metadata.guids).await?;

    // Slice 1 materializes the first provider image at this scanner-known
    // basename. Until it is available, keep the NFO URL so a later scan has a
    // source rather than silently dropping provider artwork.
    let artwork_path = ["jpg", "jpeg", "png", "webp"]
        .iter()
        .map(|extension| nfo_path.with_extension(extension))
        .find(|path| path.is_file())
        .map(|path| path.display().to_string());
    let mut artwork = metadata
        .artwork
        .iter()
        .map(|candidate| ArtworkRef {
            kind: match candidate.role {
                ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster => {
                    artwork_kind::POSTER
                }
                ersatzrs_core::repositories::AddonMediaListArtworkRole::Fanart => {
                    artwork_kind::FAN_ART
                }
                ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb
                | ersatzrs_core::repositories::AddonMediaListArtworkRole::Other => {
                    artwork_kind::THUMBNAIL
                }
            },
            path: projected_artwork_path(nfo_path, &candidate.url),
            date_added: chrono::Utc::now(),
        })
        .collect::<Vec<_>>();
    if let Some(path) = artwork_path.or_else(|| metadata.artwork_urls.first().cloned()) {
        artwork.push(ArtworkRef {
            kind: artwork_kind::THUMBNAIL,
            path,
            date_added: chrono::Utc::now(),
        });
    }
    replace_artwork(
        tx,
        fk_column,
        metadata_id,
        &[
            artwork_kind::POSTER,
            artwork_kind::THUMBNAIL,
            artwork_kind::FAN_ART,
        ],
        &artwork,
    )
    .await
}

/// Convert a publication-local NFO artwork filename into the absolute path
/// consumed by the shared metadata tables. Provider HTTPS URLs are retained
/// for operator-owned NFOs whose images have not been materialized by us.
fn projected_artwork_path(nfo_path: &Path, value: &str) -> String {
    let relative = Path::new(value);
    let is_plain_file_name = relative
        .components()
        .all(|component| matches!(component, std::path::Component::Normal(_)))
        && relative.components().count() == 1;
    if is_plain_file_name {
        if let Some(parent) = nfo_path.parent() {
            let candidate = parent.join(relative);
            if candidate.is_file() {
                return candidate.display().to_string();
            }
        }
    }
    value.to_owned()
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
    #[allow(clippy::too_many_lines)]
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
                       "item"."ContentKind", "item"."CollectionRole",
                       "item"."Included", "item"."Published",
                       "item"."ProviderMetadataJson", "item"."MetadataOverrideJson",
                       "item"."MetadataConflictJson",
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
            let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
            let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
            let conflict_json: Option<String> = row.try_get("MetadataConflictJson").repository()?;
            let metadata_form =
                metadata_form_from_json(&provider_json, &override_json, conflict_json.as_deref())?;
            let metadata_conflict_count =
                u32::try_from(metadata_form.conflicts.len()).unwrap_or(u32::MAX);
            let metadata = metadata_form.metadata;
            let effective_title = metadata.title.clone();
            let effective_year = metadata.year;
            let effective_season = metadata.season;
            let effective_episode = metadata.episode;
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
                display_title: if effective_title.trim().is_empty() {
                    row.try_get("DisplayTitle").repository()?
                } else {
                    effective_title.clone()
                },
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
                title: effective_title,
                year: effective_year,
                season: effective_season,
                episode: effective_episode,
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
                collection_role: crate::repositories::common::decode_enum_column(
                    "AddonMediaListItemRole",
                    &row,
                    "CollectionRole",
                )?,
                thumbnail_url: metadata
                    .artwork
                    .iter()
                    .find(|candidate| {
                        matches!(
                            candidate.role,
                            ersatzrs_core::repositories::AddonMediaListArtworkRole::Thumb
                                | ersatzrs_core::repositories::AddonMediaListArtworkRole::Poster
                        )
                    })
                    .map(|candidate| candidate.url.clone())
                    .or_else(|| metadata.artwork_urls.first().cloned()),
                tags: metadata.tags,
                people: metadata
                    .people
                    .into_iter()
                    .map(|person| person.name)
                    .collect(),
                included: decode_bool_column(&row, "Included")?,
                published: decode_bool_column(&row, "Published")?,
                metadata_conflict_count,
            });
        }
        Ok(result)
    }
}

/// Give a freshly imported list the presentation its entries imply.
///
/// The add-ons classify each entry, so an operator who pasted a URL should not
/// have to restate what the list obviously is. A projection that needs
/// something the import cannot supply — a television list has no show title yet
/// — is recorded as the presentation error rather than failing the import: the
/// entries are already stored and playable as Remote Streams either way.
async fn adopt_derived_presentation(pool: &Pool<crate::db::Db>, id: EntityId) -> Result<()> {
    let derived = resolve_auto_presentation(pool, id).await?;
    if derived == AddonMediaListPresentationKind::RemoteStreams {
        return Ok(());
    }
    ensure_generated_parent_nfo(pool, id, derived, None).await?;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "RequestedPresentationKind" = ? WHERE "Id" = ?"#)
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListPresentationKind::Auto,
        ))
        .bind(id)
        .execute(pool)
        .await
        .repository()?;
    if let Err(error) = reconcile_projection(pool, id, AddonMediaListPresentationKind::Auto).await {
        let _ =
            sqlx::query(r#"UPDATE "AddonMediaList" SET "PresentationError" = ? WHERE "Id" = ?"#)
                .bind(error.to_string())
                .bind(id)
                .execute(pool)
                .await;
        return Ok(());
    }
    AddonMediaListSynchronizer::new(pool.clone())
        .match_list_items(id)
        .await
}

/// Materialize parent metadata from the provider list header before applying a
/// parented library presentation. Managed add-on imports must never require an
/// operator to create an empty `tvshow.nfo` or `artist.nfo` by hand.
async fn ensure_generated_parent_nfo(
    pool: &Pool<crate::db::Db>,
    id: EntityId,
    presentation: AddonMediaListPresentationKind,
    title_override: Option<&str>,
) -> Result<()> {
    let Some((file_name, root)) = parent_nfo_for(presentation) else {
        return Ok(());
    };
    let path = parent_nfo_path_for(pool, id, file_name)
        .await?
        .ok_or_else(|| {
            RepositoryError::Conflict("managed list storage is unavailable".to_owned())
        })?;
    let row = sqlx::query(
        r#"SELECT COALESCE("Name", '') AS "Name", "Description", "ProviderMetadataJson",
                  "MetadataOverrideJson"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(id)
    .fetch_optional(pool)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id,
    })?;
    let provider_title: String = row.try_get("Name").repository()?;
    let provider_json: String = row.try_get("ProviderMetadataJson").repository()?;
    let override_json: String = row.try_get("MetadataOverrideJson").repository()?;
    let effective = merge_metadata_values(
        metadata_json_value(&provider_json)?,
        &metadata_json_value(&override_json)?,
    );
    let mut metadata: AddonMediaListNfoMetadata =
        serde_json::from_value(effective).map_err(|error| RepositoryError::Decode {
            kind: "add-on list metadata",
            value: error.to_string(),
        })?;
    let title = title_override
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .or_else(|| (!metadata.title.trim().is_empty()).then_some(metadata.title.trim()))
        .unwrap_or_else(|| provider_title.trim());
    if title.is_empty() {
        return Err(RepositoryError::Conflict(format!(
            "{file_name} cannot be generated because the add-on supplied no title"
        )));
    }
    metadata.title = title.to_owned();
    if metadata.plot.is_none() {
        metadata.plot = row.try_get("Description").repository()?;
    }
    materialize_parent_artwork(pool, id, &path, &mut metadata).await?;
    let generated = crate::addon_media_nfo::serialize(root, &metadata).map_err(map_nfo_error)?;
    crate::addon_media_nfo::reconcile_generated(&path, &generated).map_err(map_nfo_error)?;
    Ok(())
}

async fn materialize_parent_artwork(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    nfo_path: &Path,
    metadata: &mut AddonMediaListNfoMetadata,
) -> Result<()> {
    let folder = nfo_path.parent().ok_or_else(|| {
        RepositoryError::unavailable(
            "publish add-on artwork",
            "the parent NFO has no containing folder",
        )
    })?;
    let selected = selected_parent_artwork(metadata);
    let client = crate::addon_artwork::client()
        .map_err(|error| RepositoryError::unavailable("publish add-on artwork", error))?;
    let revision: i32 =
        sqlx::query_scalar(r#"SELECT "PublishedRevision" FROM "AddonMediaList" WHERE "Id" = ?"#)
            .bind(list_id)
            .fetch_one(pool)
            .await
            .repository()?;
    let mut published = Vec::new();
    for role in [
        AddonMediaListArtworkRole::Poster,
        AddonMediaListArtworkRole::Fanart,
        AddonMediaListArtworkRole::Thumb,
    ] {
        let Some(mut candidate) = selected
            .iter()
            .find(|candidate| candidate.role == role)
            .cloned()
        else {
            remove_parent_materialization(pool, list_id, folder, role).await?;
            continue;
        };
        let source = candidate.url.trim();
        let (bytes, extension) =
            load_parent_artwork_bytes(pool, &client, list_id, role, source).await?;
        let existing = load_parent_materialization(pool, list_id, role).await?;
        if let Some(existing) = &existing {
            let path = folder.join(&existing.file_name);
            if existing.source == source && file_has_sha256(&path, &existing.sha256) {
                candidate.url.clone_from(&existing.file_name);
                published.push(candidate);
                continue;
            }
        }
        if let Some((file_name, sha256)) =
            existing_parent_operator_artwork(folder, role, existing.as_ref())
        {
            upsert_parent_materialization(
                pool,
                list_id,
                role,
                source,
                &file_name,
                &sha256,
                revision.saturating_add(1),
            )
            .await?;
            candidate.url = file_name;
            published.push(candidate);
            continue;
        }
        let file_name = parent_artwork_file_name(role, extension);
        let path = folder.join(&file_name);
        if path.parent() != Some(folder) {
            return Err(RepositoryError::unavailable(
                "publish add-on artwork",
                "the artwork path escaped its managed folder",
            ));
        }
        crate::addon_artwork::write_atomically(&path, &bytes)
            .map_err(|error| RepositoryError::unavailable("publish add-on artwork", error))?;
        let sha256 = hex_hash(&bytes);
        if let Some(existing) = existing.filter(|existing| existing.file_name != file_name) {
            let old_path = folder.join(existing.file_name);
            if file_has_sha256(&old_path, &existing.sha256) {
                std::fs::remove_file(old_path).map_err(|error| {
                    RepositoryError::unavailable("remove stale add-on artwork", error.to_string())
                })?;
            }
        }
        upsert_parent_materialization(
            pool,
            list_id,
            role,
            source,
            &file_name,
            &sha256,
            revision.saturating_add(1),
        )
        .await?;
        candidate.url = file_name;
        published.push(candidate);
    }
    metadata.artwork = published;
    metadata.artwork_urls.clear();
    Ok(())
}

fn selected_parent_artwork(metadata: &AddonMediaListNfoMetadata) -> Vec<AddonMediaListArtwork> {
    let mut selected = metadata.artwork.clone();
    if !selected
        .iter()
        .any(|candidate| candidate.role == AddonMediaListArtworkRole::Thumb)
    {
        if let Some(url) = metadata.artwork_urls.first() {
            selected.push(AddonMediaListArtwork {
                url: url.clone(),
                role: AddonMediaListArtworkRole::Thumb,
                width: None,
                height: None,
                language: None,
            });
        }
    }
    selected
}

async fn load_parent_artwork_bytes(
    pool: &Pool<crate::db::Db>,
    client: &reqwest::Client,
    list_id: EntityId,
    role: AddonMediaListArtworkRole,
    source: &str,
) -> Result<(Vec<u8>, &'static str)> {
    if let Some(upload_id) = crate::addon_artwork::upload_id(source) {
        let row = sqlx::query_as::<_, (Vec<u8>, String)>(
            r#"SELECT "Bytes", "ContentType" FROM "AddonMediaListArtworkUpload"
               WHERE "Id" = ? AND "AddonMediaListId" = ?
                 AND "ScopeItemId" = 0 AND "Role" = ?"#,
        )
        .bind(upload_id)
        .bind(list_id)
        .bind(artwork_role_value(role))
        .fetch_optional(pool)
        .await
        .repository()?
        .ok_or_else(|| {
            RepositoryError::Conflict(
                "selected artwork upload does not belong to this list and role".to_owned(),
            )
        })?;
        let (_, extension) = crate::addon_artwork::validate_image(&row.1, &row.0)
            .map_err(RepositoryError::Conflict)?;
        return Ok((row.0, extension));
    }
    if !is_safe_remote_artwork_url(source) {
        return Err(RepositoryError::Conflict(
            "selected artwork must use a bounded HTTPS URL".to_owned(),
        ));
    }
    crate::addon_artwork::download(client, source)
        .await
        .map_err(|error| RepositoryError::unavailable("download add-on artwork", error))
}

async fn load_parent_materialization(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    role: AddonMediaListArtworkRole,
) -> Result<Option<ArtworkMaterializationSnapshot>> {
    let row = sqlx::query(
        r#"SELECT "Id", "AddonMediaListItemId", "ScopeItemId", "Role", "Source",
                  "FileName", "Sha256", "PublishedRevision"
           FROM "AddonMediaListArtworkMaterialization"
           WHERE "AddonMediaListId" = ? AND "ScopeItemId" = 0 AND "Role" = ?"#,
    )
    .bind(list_id)
    .bind(artwork_role_value(role))
    .fetch_optional(pool)
    .await
    .repository()?;
    row.map(|row| {
        Ok(ArtworkMaterializationSnapshot {
            id: row.try_get("Id").repository()?,
            item_id: row.try_get("AddonMediaListItemId").repository()?,
            scope_item_id: row.try_get("ScopeItemId").repository()?,
            role: row.try_get("Role").repository()?,
            source: row.try_get("Source").repository()?,
            file_name: row.try_get("FileName").repository()?,
            sha256: row.try_get("Sha256").repository()?,
            published_revision: row.try_get("PublishedRevision").repository()?,
        })
    })
    .transpose()
}

fn existing_parent_operator_artwork(
    folder: &Path,
    role: AddonMediaListArtworkRole,
    previous: Option<&ArtworkMaterializationSnapshot>,
) -> Option<(String, String)> {
    ["jpg", "jpeg", "png", "webp"]
        .into_iter()
        .find_map(|extension| {
            let file_name = parent_artwork_file_name(role, extension);
            let path = folder.join(&file_name);
            if !path.is_file()
                || previous.is_some_and(|previous| {
                    previous.file_name == file_name && file_has_sha256(&path, &previous.sha256)
                })
            {
                return None;
            }
            std::fs::read(path)
                .ok()
                .map(|bytes| (file_name, hex_hash(&bytes)))
        })
}

async fn upsert_parent_materialization(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    role: AddonMediaListArtworkRole,
    source: &str,
    file_name: &str,
    sha256: &str,
    revision: i32,
) -> Result<()> {
    let role = artwork_role_value(role);
    let updated = sqlx::query(
        r#"UPDATE "AddonMediaListArtworkMaterialization"
           SET "Source" = ?, "FileName" = ?, "Sha256" = ?, "PublishedRevision" = ?
           WHERE "AddonMediaListId" = ? AND "ScopeItemId" = 0 AND "Role" = ?"#,
    )
    .bind(source)
    .bind(file_name)
    .bind(sha256)
    .bind(revision)
    .bind(list_id)
    .bind(role)
    .execute(pool)
    .await
    .repository()?;
    if updated.rows_affected() == 0 {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListArtworkMaterialization" (
                   "AddonMediaListId", "AddonMediaListItemId", "ScopeItemId", "Role",
                   "Source", "FileName", "Sha256", "PublishedRevision"
               ) VALUES (?, NULL, 0, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(role)
        .bind(source)
        .bind(file_name)
        .bind(sha256)
        .bind(revision)
        .execute(pool)
        .await
        .repository()?;
    }
    Ok(())
}

async fn remove_parent_materialization(
    pool: &Pool<crate::db::Db>,
    list_id: EntityId,
    folder: &Path,
    role: AddonMediaListArtworkRole,
) -> Result<()> {
    if let Some(existing) = load_parent_materialization(pool, list_id, role).await? {
        let path = folder.join(&existing.file_name);
        if file_has_sha256(&path, &existing.sha256) {
            std::fs::remove_file(path).map_err(|error| {
                RepositoryError::unavailable("remove stale add-on artwork", error.to_string())
            })?;
        }
        sqlx::query(r#"DELETE FROM "AddonMediaListArtworkMaterialization" WHERE "Id" = ?"#)
            .bind(existing.id)
            .execute(pool)
            .await
            .repository()?;
    }
    Ok(())
}

fn parent_artwork_file_name(role: AddonMediaListArtworkRole, extension: &str) -> String {
    let name = match role {
        AddonMediaListArtworkRole::Poster => "poster",
        AddonMediaListArtworkRole::Fanart => "fanart",
        AddonMediaListArtworkRole::Thumb | AddonMediaListArtworkRole::Other => "thumb",
    };
    // `extension` comes exclusively from signature validation.
    format!("{name}.{extension}")
}

fn file_has_sha256(path: &Path, expected: &str) -> bool {
    std::fs::read(path).is_ok_and(|bytes| hex_hash(&bytes) == expected)
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
             AND "Published" = 1
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
            nfo_path: path,
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
    let result = sqlx::query(
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
    let artist_metadata_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
    replace_projected_metadata_relations(
        &mut tx,
        ProjectedMetadataRelations::ARTIST,
        artist_metadata_id,
        &artist_form.metadata,
        &artist_path,
    )
    .await?;

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
        let result = sqlx::query(
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
        let metadata_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
        replace_projected_metadata_relations(
            &mut tx,
            ProjectedMetadataRelations::MUSIC_VIDEO,
            metadata_id,
            &input.metadata,
            &input.nfo_path,
        )
        .await?;
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
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
             AND "Published" = 1"#,
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
    /// Shared related-row capabilities for this metadata record.
    metadata_relations: ProjectedMetadataRelations,
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
        metadata_relations: ProjectedMetadataRelations::MOVIE,
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
        metadata_relations: ProjectedMetadataRelations::OTHER_VIDEO,
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
        metadata_relations: ProjectedMetadataRelations::SONG,
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
        metadata_relations: ProjectedMetadataRelations::IMAGE,
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
        metadata_relations,
        nfo_root,
        descriptive_metadata,
        presentation,
    } = projection;
    let rows = sqlx::query(
        r#"SELECT "Id", "Title", "Year", "Availability"
           FROM "AddonMediaListItem"
           WHERE "AddonMediaListId" = ? AND "ParentProviderId" IS NULL
             AND "Published" = 1
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
            nfo_path: path,
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
        let metadata_id = if descriptive_metadata {
            let result = sqlx::query(sqlx::AssertSqlSafe(format!(
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
            crate::db::inserted_id(&mut tx, result).await.repository()?
        } else {
            let result = sqlx::query(sqlx::AssertSqlSafe(format!(
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
            crate::db::inserted_id(&mut tx, result).await.repository()?
        };
        replace_projected_metadata_relations(
            &mut tx,
            metadata_relations,
            metadata_id,
            &input.metadata,
            &input.nfo_path,
        )
        .await?;
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
               SET "PresentationKind" = ?, "PresentationError" = NULL WHERE "Id" = ?"#,
        )
        .bind(crate::repositories::common::encode_enum(
            AddonMediaListPresentationKind::RemoteStreams,
        ))
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
             AND "Published" = 1
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
            nfo_path: path,
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
    let result = sqlx::query(
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
    let show_metadata_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
    replace_projected_metadata_relations(
        &mut tx,
        ProjectedMetadataRelations::SHOW,
        show_metadata_id,
        &show_form.metadata,
        &show_path,
    )
    .await?;

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
        let result = sqlx::query(
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
        let metadata_id = crate::db::inserted_id(&mut tx, result).await.repository()?;
        replace_projected_metadata_relations(
            &mut tx,
            ProjectedMetadataRelations::EPISODE,
            metadata_id,
            &input.metadata,
            &input.nfo_path,
        )
        .await?;
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
           SET "PresentationKind" = ?, "PresentationError" = NULL WHERE "Id" = ?"#,
    )
    .bind(crate::repositories::common::encode_enum(
        AddonMediaListPresentationKind::TelevisionShow,
    ))
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
    use super::{is_safe_remote_artwork_url, FlatProjection};

    /// Every classification must map to a root the NFO serializer accepts.
    /// `Song` once mapped to an invented `song` root, so saving the NFO of any
    /// song-classified entry failed with "unsupported NFO root".
    #[test]
    fn every_projection_uses_a_serializable_nfo_root() {
        let roots = FlatProjection::ALL
            .into_iter()
            .map(|projection| projection.nfo_root)
            .chain(["tvshow", "episodedetails", "musicvideo"]);

        for root in roots {
            crate::addon_media_nfo::serialize(
                root,
                &ersatzrs_core::repositories::AddonMediaListNfoMetadata {
                    title: "Fixture".to_owned(),
                    ..Default::default()
                },
            )
            .unwrap_or_else(|error| panic!("projection uses unusable root {root}: {error}"));
        }
    }

    #[test]
    fn artwork_urls_require_bounded_https_without_credentials() {
        assert!(is_safe_remote_artwork_url(
            "https://media.example.test/poster.jpg"
        ));
        assert!(!is_safe_remote_artwork_url(
            "http://media.example.test/poster.jpg"
        ));
        assert!(!is_safe_remote_artwork_url(
            "https://user:secret@media.example.test/poster.jpg"
        ));
        assert!(!is_safe_remote_artwork_url("file:///tmp/poster.jpg"));
        assert!(!is_safe_remote_artwork_url(&format!(
            "https://media.example.test/{}",
            "x".repeat(2_048)
        )));
    }
}
