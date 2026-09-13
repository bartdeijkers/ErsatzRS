//! Receiving draft SQL projection; never activates the independent publication.

mod items;
#[cfg(test)]
mod tests;

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ersatzrs_addon_contract::AddonCapability;
use ersatzrs_core::repositories::{
    AddonMediaListDraftOptions, AddonMediaListItemOptionValues, AddonMediaListOptionSchema,
    RepositoryError, Result,
};
use ersatzrs_domain::{AddonMediaListRefreshCadence, EntityId};
use serde::Serialize;
use sqlx::{Row as _, Transaction};

use super::model::{SavedListDocument, SavedReplacement};
use crate::db::Db;
use crate::repositories::common::{decode_enum_column, encode_datetime, encode_enum};
use crate::repositories::SqlxResultExt as _;

/// Receiving identities and portable replacement choices for optional local binding.
///
/// Replacement intent is persisted without fabricated media IDs. A concrete
/// receiving media binding is separate from preserving the saved draft choice.
/// This is not publication activation. The separate published reconstruction owns activation
/// and the active presentation kind; this operation preserves that kind while a
/// local publication exists, even if the incoming draft records another value.
pub struct ProjectedDraft {
    pub item_ids: BTreeMap<String, EntityId>,
    pub replacement_choices: BTreeMap<String, Option<SavedReplacement>>,
}

/// Project validated saved draft content using an existing receiving list owner.
///
/// The caller validates the document, installed add-on and assets, fences the
/// mapped folder/list, allocates its own local revision and stages artwork before
/// opening this transaction through `ArtworkStaging::begin_projection`. The
/// supplied revision must already equal the receiving row's revision. Sender
/// revisions never become local edit fences. No source files, network calls,
/// nested transaction or publication activation occur here.
///
/// All item owners exist before metadata artwork is bound. Existing published
/// bindings, local hiding, leases and runtime history remain local. The caller
/// must roll back on any error; this helper
/// never commits. Repeated identical projection reuses owner identities.
///
/// # Errors
/// Refuses mismatched receiving identity/revision, malformed row references or
/// unavailable option/artwork bindings, and propagates database errors.
#[expect(
    clippy::too_many_lines,
    reason = "Keep receiving header, options and item projection in one caller-owned transaction"
)]
pub async fn project_draft_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    document: &SavedListDocument,
    local_revision: i32,
    now: DateTime<Utc>,
) -> Result<ProjectedDraft> {
    let current = sqlx::query(
        r#"SELECT "AddonId", "ProviderId", "DraftRevision", "RefreshCadence", "NextRefreshAt"
           FROM "AddonMediaList" WHERE "Id"=?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(invalid)?;
    if local_revision < 0
        || current.try_get::<String, _>("AddonId").repository()? != document.addon_id
        || current.try_get::<String, _>("ProviderId").repository()? != document.provider_id
        || current.try_get::<i32, _>("DraftRevision").repository()? != local_revision
    {
        return Err(invalid());
    }
    crate::addon_media_list_sync::ensure_not_publishing_tx(tx, list_id).await?;
    let item_ids = items::owners(tx, list_id, document, now).await?;
    let files = document
        .draft
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file))
        .collect();
    let mut metadata = document.draft.metadata.clone();
    super::receiving_artwork::bind_metadata_in_tx(tx, list_id, None, &mut metadata, &files).await?;
    let draft = &document.draft;
    let cadence: AddonMediaListRefreshCadence =
        decode_enum_column("refresh cadence", &current, "RefreshCadence")?;
    let old_due: Option<String> = current.try_get("NextRefreshAt").repository()?;
    let due = if draft.refresh_cadence == AddonMediaListRefreshCadence::Never {
        None
    } else if cadence == draft.refresh_cadence && old_due.is_some() {
        old_due
    } else {
        match draft.refresh_cadence {
            AddonMediaListRefreshCadence::Never => None,
            AddonMediaListRefreshCadence::Adaptive => Some(encode_datetime(now)),
            cadence => crate::addon_media_list_refresh::next_fixed_refresh_at(cadence, now)
                .map(encode_datetime),
        }
    };
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "SourceUrl"=?, "Name"=?, "Description"=?,
           "GeneratePlaylist"=?, "ContentKindOverride"=?,
           "PresentationKind"=CASE WHEN "PublishedRevision"=0 THEN ? ELSE "PresentationKind" END,
           "RequestedPresentationKind"=?, "ProviderMetadataJson"=?, "MetadataOverrideJson"=?,
           "MetadataConflictJson"=?, "DetailImportPolicy"=?, "ChapterInput"=?,
           "AutoRefresh"=?, "RefreshCadence"=?, "RefreshPublicationPolicy"=?,
           "DiskUsageWarningGb"=?, "NextRefreshAt"=?
           WHERE "Id"=? AND "DraftRevision"=?"#,
    )
    .bind(&document.source_url)
    .bind(&draft.presentation.name)
    .bind(&draft.presentation.description)
    .bind(draft.presentation.generate_playlist)
    .bind(encode_enum(draft.presentation.content_kind_override))
    .bind(encode_enum(draft.presentation.presentation_kind))
    .bind(encode_enum(draft.presentation.requested_presentation_kind))
    .bind(json(&metadata.provider)?)
    .bind(json(&metadata.overrides)?)
    .bind(json(&metadata.conflicts)?)
    .bind(encode_enum(draft.detail_import_policy))
    .bind(&draft.chapter_input)
    .bind(draft.refresh_cadence == AddonMediaListRefreshCadence::Daily)
    .bind(encode_enum(draft.refresh_cadence))
    .bind(encode_enum(draft.refresh_publication_policy))
    .bind(i64::from(draft.disk_warning_gb))
    .bind(due)
    .bind(list_id)
    .bind(local_revision)
    .execute(&mut **tx)
    .await
    .repository()?;
    items::content(tx, list_id, document, &item_ids, &files).await?;
    crate::addon_media_list_delivery::portable::restore_in_tx(tx, list_id, &draft.delivery).await?;
    if let Some(schema) = &draft.options.schema {
        let definition: AddonCapability =
            serde_json::from_value(schema.definition.clone()).map_err(|_| invalid())?;
        crate::repositories::register_option_schema_in_tx(
            tx,
            &AddonMediaListOptionSchema {
                schema: schema.identity.clone(),
                definition_json: json(&definition)?,
                created_at: now,
            },
        )
        .await?;
    }
    let item_values = draft
        .options
        .item_values
        .iter()
        .map(|(provider_id, values)| {
            Ok(AddonMediaListItemOptionValues {
                item_id: Some(*item_ids.get(provider_id).ok_or_else(invalid)?),
                provider_id: provider_id.clone(),
                values: values.clone(),
            })
        })
        .collect::<Result<_>>()?;
    crate::repositories::save_draft_options_tx(
        tx,
        &AddonMediaListDraftOptions {
            list_id,
            draft_revision: local_revision,
            schema: draft
                .options
                .schema
                .as_ref()
                .map(|schema| schema.identity.clone()),
            list_values: draft.options.list_values.clone(),
            item_values,
        },
        false,
    )
    .await?;
    Ok(ProjectedDraft {
        item_ids,
        replacement_choices: draft
            .items
            .iter()
            .map(|item| (item.content.provider_id.clone(), item.replacement.clone()))
            .collect(),
    })
}

fn json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|_| invalid())
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("receiving saved draft projection is inconsistent".into())
}
