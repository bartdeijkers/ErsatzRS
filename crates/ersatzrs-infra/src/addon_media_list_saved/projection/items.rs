use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::Result;
use ersatzrs_domain::EntityId;
use sqlx::Transaction;

use super::{invalid, json};
use crate::addon_media_list_saved::model::{SavedFile, SavedListDocument};
use crate::db::Db;
use crate::repositories::common::{encode_datetime, encode_enum};
use crate::repositories::SqlxResultExt as _;

pub(super) async fn owners(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    document: &SavedListDocument,
    now: DateTime<Utc>,
) -> Result<BTreeMap<String, EntityId>> {
    let draft_ids: BTreeSet<_> = document
        .draft
        .items
        .iter()
        .map(|item| item.content.provider_id.as_str())
        .collect();
    if draft_ids.len() != document.draft.items.len() {
        return Err(invalid());
    }
    let mut union = BTreeMap::new();
    if let Some(published) = &document.published {
        for item in &published.items {
            union.insert(item.content.provider_id.as_str(), &item.content);
        }
    }
    for item in &document.draft.items {
        union.insert(item.content.provider_id.as_str(), &item.content);
    }
    let existing: Vec<(EntityId, String, bool, bool)> = sqlx::query_as(
        r#"SELECT "Id", "ProviderId", "InDraft", "Published" FROM "AddonMediaListItem"
           WHERE "AddonMediaListId"=?"#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    for (item_id, provider_id, in_draft, published) in &existing {
        if !*published && !union.contains_key(provider_id.as_str()) {
            // No incoming state or active local publication owns this row.
            // Item children cascade; retained artifacts keep their revision pins
            // and lose only the nullable item link.
            sqlx::query(r#"DELETE FROM "AddonMediaListItem" WHERE "Id"=? AND "Published"=0"#)
                .bind(item_id)
                .execute(&mut **tx)
                .await
                .repository()?;
            continue;
        }
        let incoming_draft = draft_ids.contains(provider_id.as_str());
        if *in_draft != incoming_draft {
            crate::addon_media_list_item_jobs::cancel_item_jobs_in_tx(tx, list_id, *item_id, now)
                .await?;
            sqlx::query(r#"DELETE FROM "AddonMediaListDetailJob" WHERE "AddonMediaListItemId"=?"#)
                .bind(item_id)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
        if *in_draft && !incoming_draft {
            sqlx::query(r#"UPDATE "AddonMediaListItem" SET "InDraft"=0 WHERE "Id"=?"#)
                .bind(item_id)
                .execute(&mut **tx)
                .await
                .repository()?;
        }
    }
    let existing: BTreeMap<_, _> = existing
        .into_iter()
        .map(|(id, provider_id, _, _)| (provider_id, id))
        .collect();
    let mut ids = BTreeMap::new();
    for (provider_id, item) in union {
        let id = if let Some(id) = existing.get(provider_id) {
            *id
        } else {
            sqlx::query_scalar(
                r#"INSERT INTO "AddonMediaListItem"
                   ("AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title", "InDraft")
                   VALUES (?, ?, ?, ?, ?, ?, 0) RETURNING "Id""#,
            )
            .bind(list_id)
            .bind(provider_id)
            .bind(&item.display_title)
            .bind(encode_enum(item.kind))
            .bind(item.rank)
            .bind(&item.title)
            .fetch_one(&mut **tx)
            .await
            .repository()?
        };
        ids.insert(provider_id.to_owned(), id);
    }
    Ok(ids)
}

#[expect(
    clippy::too_many_lines,
    reason = "Explicit portable item columns and their owned rows are projected together"
)]
pub(super) async fn content(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    document: &SavedListDocument,
    ids: &BTreeMap<String, EntityId>,
    files: &BTreeMap<&str, &SavedFile>,
) -> Result<()> {
    for item in &document.draft.items {
        let content = &item.content;
        let id = *ids.get(&content.provider_id).ok_or_else(invalid)?;
        let mut metadata = item.metadata.clone();
        crate::addon_media_list_saved::receiving_artwork::bind_metadata_in_tx(
            tx,
            list_id,
            Some(id),
            &mut metadata,
            files,
        )
        .await?;
        sqlx::query(
            r#"UPDATE "AddonMediaListItem" SET "InDraft"=1, "DisplayTitle"=?, "Kind"=?,
               "Rank"=?, "Title"=?, "Year"=?, "Season"=?, "Episode"=?, "SourceUrl"=?,
               "AdditionalImageUrls"=?, "ContentKind"=?, "Included"=?, "DurationSeconds"=?,
               "ParentProviderId"=?, "FragmentStartSeconds"=?, "FragmentEndSeconds"=?,
               "ChapterInput"=?, "ProviderChapterInput"=?, "CollectionRole"=?,
               "Availability"=?, "AvailabilityReason"=NULL, "AvailabilityReasonCode"=?,
               "ProviderLiveness"=?, "LivenessOverride"=?, "EffectiveLiveness"=?,
               "LivenessConflict"=?, "ProviderMetadataJson"=?, "MetadataOverrideJson"=?,
               "MetadataConflictJson"=?, "SavedReplacementJson"=?
               WHERE "Id"=? AND "AddonMediaListId"=?"#,
        )
        .bind(&content.display_title)
        .bind(encode_enum(content.kind))
        .bind(content.rank)
        .bind(&content.title)
        .bind(content.year)
        .bind(content.season)
        .bind(content.episode)
        .bind(&content.source_url)
        .bind(json(&content.additional_image_urls)?)
        .bind(encode_enum(content.content_kind))
        .bind(content.included)
        .bind(bound(content.duration_seconds)?)
        .bind(&content.parent_provider_id)
        .bind(bound(content.fragment_start_seconds)?)
        .bind(bound(content.fragment_end_seconds)?)
        .bind(&content.chapter_input)
        .bind(&content.provider_chapter_input)
        .bind(encode_enum(content.collection_role))
        .bind(encode_enum(content.availability))
        .bind(content.availability_reason_code.map(encode_enum))
        .bind(encode_enum(content.provider_liveness))
        .bind(content.liveness_override.map(encode_enum))
        .bind(encode_enum(
            content
                .liveness_override
                .unwrap_or(content.provider_liveness),
        ))
        .bind(item.liveness_conflict)
        .bind(json(&metadata.provider)?)
        .bind(json(&metadata.overrides)?)
        .bind(json(&metadata.conflicts)?)
        .bind(item.replacement.as_ref().map(json).transpose()?)
        .bind(id)
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
        sqlx::query(
            r#"DELETE FROM "AddonMediaListItemReplacement" WHERE "AddonMediaListItemId"=?"#,
        )
        .bind(id)
        .execute(&mut **tx)
        .await
        .repository()?;
        sqlx::query(r#"DELETE FROM "AddonMediaListItemGuid" WHERE "AddonMediaListItemId"=?"#)
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        for guid in &content.guids {
            sqlx::query(r#"INSERT INTO "AddonMediaListItemGuid" ("AddonMediaListItemId", "Guid") VALUES (?, ?)"#)
                .bind(id).bind(guid).execute(&mut **tx).await.repository()?;
        }
        if let Some(observation) = &item.source_observation {
            sqlx::query(
                r#"INSERT INTO "AddonMediaListItemTombstone"
                   ("AddonMediaListItemId", "MissingSince", "LastObservedAt", "ObservationCount",
                    "LastReconcileGeneration", "ConfirmedAt", "ReappearedAt")
                   VALUES (?, ?, ?, ?, COALESCE((SELECT "LastReconcileGeneration"
                           FROM "AddonMediaListItem" WHERE "Id"=?), 0), ?, ?)
                   ON CONFLICT ("AddonMediaListItemId") DO UPDATE SET
                     "MissingSince"=excluded."MissingSince", "LastObservedAt"=excluded."LastObservedAt",
                     "ObservationCount"=excluded."ObservationCount", "ConfirmedAt"=excluded."ConfirmedAt",
                     "ReappearedAt"=excluded."ReappearedAt""#,
            )
            .bind(id).bind(encode_datetime(observation.missing_since))
            .bind(encode_datetime(observation.last_observed_at)).bind(i64::from(observation.observation_count))
            .bind(id).bind(observation.confirmed_at.map(encode_datetime))
            .bind(observation.reappeared_at.map(encode_datetime))
            .execute(&mut **tx).await.repository()?;
        } else {
            sqlx::query(
                r#"DELETE FROM "AddonMediaListItemTombstone" WHERE "AddonMediaListItemId"=?"#,
            )
            .bind(id)
            .execute(&mut **tx)
            .await
            .repository()?;
        }
    }
    chapters(tx, list_id, document, ids).await?;
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "ItemCount"=(
        SELECT COUNT(*) FROM "AddonMediaListItem" WHERE "AddonMediaListId"=? AND "InDraft"=1)
        WHERE "Id"=?"#,
    )
    .bind(list_id)
    .bind(list_id)
    .execute(&mut **tx)
    .await
    .repository()?;
    Ok(())
}

async fn chapters(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    document: &SavedListDocument,
    ids: &BTreeMap<String, EntityId>,
) -> Result<()> {
    sqlx::query(r#"DELETE FROM "AddonMediaListChapter" WHERE "AddonMediaListId"=?"#)
        .bind(list_id)
        .execute(&mut **tx)
        .await
        .repository()?;
    let draft_ids: BTreeSet<_> = document
        .draft
        .items
        .iter()
        .map(|item| item.content.provider_id.as_str())
        .collect();
    let mut fragments: Vec<_> = document
        .draft
        .items
        .iter()
        .map(|item| &item.content)
        .filter(|item| item.parent_provider_id.is_some())
        .collect();
    fragments.sort_by_key(|item| {
        (
            &item.parent_provider_id,
            item.fragment_start_seconds,
            item.rank,
            &item.provider_id,
        )
    });
    let mut indexes = BTreeMap::<&str, i32>::new();
    for item in fragments {
        let parent = item.parent_provider_id.as_deref().ok_or_else(invalid)?;
        if !draft_ids.contains(parent) {
            return Err(invalid());
        }
        let index = indexes.entry(parent).or_default();
        sqlx::query(
            r#"INSERT INTO "AddonMediaListChapter"
               ("AddonMediaListId", "SourceItemId", "FragmentItemId", "ChapterIndex",
                "Title", "StartSeconds", "EndSeconds") VALUES (?, ?, ?, ?, ?, ?, ?)"#,
        )
        .bind(list_id)
        .bind(*ids.get(parent).ok_or_else(invalid)?)
        .bind(*ids.get(&item.provider_id).ok_or_else(invalid)?)
        .bind(*index)
        .bind(&item.title)
        .bind(bound(item.fragment_start_seconds)?.ok_or_else(invalid)?)
        .bind(bound(item.fragment_end_seconds)?)
        .execute(&mut **tx)
        .await
        .repository()?;
        *index = index.checked_add(1).ok_or_else(invalid)?;
    }
    Ok(())
}

fn bound(value: Option<u64>) -> Result<Option<i64>> {
    value
        .map(|value| i64::try_from(value).map_err(|_| invalid()))
        .transpose()
}
