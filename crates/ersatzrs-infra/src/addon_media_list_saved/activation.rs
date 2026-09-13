//! Translate supported saved publication content without replacing the draft.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use ersatzrs_addon_contract::AddonCapability;
use ersatzrs_core::repositories::{
    AddonMediaListItemOptionValues, AddonMediaListOptionSchema, AddonMediaListPublishedOptions,
    AddonMediaListRevisionItem, AddonMediaListRevisionSnapshot, RepositoryError, Result,
};
use ersatzrs_domain::EntityId;
use serde::Serialize;
use sha2::{Digest as _, Sha256};
use sqlx::Transaction;

use super::model::{SavedListPublication, SavedPlayback};
use crate::db::Db;
use crate::repositories::SqlxResultExt as _;

/// Build an independent receiving publication from already-validated saved content.
///
/// The caller supplies local revision identities and owners from draft projection,
/// validates the installed add-on and files, and stages artwork on this same
/// connection before beginning the transaction. This only supports plain Stream
/// playback: intervals, prepared media, replacements and subtitles refuse until
/// their receiving-local bindings are supplied by a supported admission path.
/// No sender execution fingerprint or media identity is fabricated.
///
/// This does not insert or activate the snapshot, change draft content, or commit.
/// The caller must discard the result and roll back on any error.
///
/// # Errors
/// Refuses unsupported playback, mismatched receiving owners/revisions, or
/// unavailable schema/artwork bindings. The saved publication is never mutated.
#[expect(
    clippy::too_many_lines,
    reason = "Translate a complete publication without changing the draft"
)]
pub async fn snapshot_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    publication: &SavedListPublication,
    item_ids: &BTreeMap<String, EntityId>,
    local_revision: i32,
    local_draft_revision: i32,
    completed_at: DateTime<Utc>,
) -> Result<AddonMediaListRevisionSnapshot> {
    // Refuse unsupported bindings before any schema/upload writes.
    validate_supported_playback(publication)?;
    if list_id <= 0 || local_revision <= 0 || local_draft_revision < 0 {
        return Err(invalid());
    }
    let completed_at = crate::repositories::common::decode_datetime(
        "receiving publication completion",
        &crate::repositories::common::encode_datetime(completed_at),
    )?;
    let addon_id: String = sqlx::query_scalar(
        r#"SELECT "AddonId" FROM "AddonMediaList" WHERE "Id"=? AND "DraftRevision"=?"#,
    )
    .bind(list_id)
    .bind(local_draft_revision)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(invalid)?;
    let owners: Vec<(String, EntityId)> = sqlx::query_as(
        r#"SELECT "ProviderId", "Id" FROM "AddonMediaListItem" WHERE "AddonMediaListId"=?"#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let mut owners: BTreeMap<_, _> = owners.into_iter().collect();
    for item in &publication.items {
        let provider_id = &item.content.provider_id;
        let owner = owners.remove(provider_id).ok_or_else(invalid)?;
        if item_ids.get(provider_id) != Some(&owner) {
            return Err(invalid());
        }
    }
    if let Some(schema) = &publication.options.schema {
        if schema.identity.addon_id != addon_id {
            return Err(invalid());
        }
        let definition: AddonCapability =
            serde_json::from_value(schema.definition.clone()).map_err(|_| invalid())?;
        crate::repositories::register_option_schema_in_tx(
            tx,
            &AddonMediaListOptionSchema {
                schema: schema.identity.clone(),
                definition_json: json(&definition)?,
                created_at: completed_at,
            },
        )
        .await?;
    }
    let files = publication
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file))
        .collect();
    let mut metadata = publication.metadata.clone();
    super::receiving_artwork::bind_map_in_tx(tx, list_id, None, &mut metadata, &files).await?;
    let mut items = Vec::with_capacity(publication.items.len());
    for item in &publication.items {
        let content = &item.content;
        let owner = *item_ids.get(&content.provider_id).ok_or_else(invalid)?;
        let mut metadata = item.metadata.clone();
        super::receiving_artwork::bind_map_in_tx(tx, list_id, Some(owner), &mut metadata, &files)
            .await?;
        let metadata_json = json(&metadata)?;
        items.push(AddonMediaListRevisionItem {
            provider_id: content.provider_id.clone(),
            guids: content.guids.clone(),
            rank: content.rank,
            kind: content.kind,
            content_kind: content.content_kind,
            included: content.included,
            published: true,
            display_title: content.display_title.clone(),
            title: content.title.clone(),
            year: content.year,
            season: content.season,
            episode: content.episode,
            source_url: content.source_url.clone(),
            additional_image_urls: Some(json(&content.additional_image_urls)?),
            duration_seconds: content.duration_seconds,
            parent_provider_id: content.parent_provider_id.clone(),
            fragment_start_seconds: content.fragment_start_seconds,
            fragment_end_seconds: content.fragment_end_seconds,
            chapter_input: content.chapter_input.clone(),
            provider_chapter_input: content.provider_chapter_input.clone(),
            collection_role: content.collection_role,
            availability: content.availability,
            availability_reason: None,
            availability_reason_code: content.availability_reason_code,
            provider_liveness: content.provider_liveness,
            liveness_override: content.liveness_override,
            effective_liveness: content
                .liveness_override
                .unwrap_or(content.provider_liveness),
            metadata_digest: sha256(metadata_json.as_bytes()),
            metadata_json,
            replacement_media_item_id: None,
            replacement_media_version_id: None,
            published_media_item_id: None,
            prepared_artifact_id: None,
            subtitle_artifact_ids: Vec::new(),
            playback_timeline: None,
        });
    }
    items.sort_by(|left, right| {
        left.rank
            .cmp(&right.rank)
            .then_with(|| left.provider_id.cmp(&right.provider_id))
    });
    let presentation = &publication.presentation;
    let mut snapshot = AddonMediaListRevisionSnapshot {
        list_id,
        revision: local_revision,
        source_draft_revision: local_draft_revision,
        completed_at,
        name: presentation.name.clone(),
        description: presentation.description.clone(),
        generate_playlist: presentation.generate_playlist,
        content_kind_override: presentation.content_kind_override,
        presentation_kind: presentation.presentation_kind,
        requested_presentation_kind: presentation.requested_presentation_kind,
        metadata_json: json(&metadata)?,
        options: AddonMediaListPublishedOptions {
            list_id,
            revision: local_revision,
            source_draft_revision: Some(local_draft_revision),
            schema: publication
                .options
                .schema
                .as_ref()
                .map(|schema| schema.identity.clone()),
            list_values: publication.options.list_values.clone(),
            item_values: publication
                .options
                .item_values
                .iter()
                .map(|(provider_id, values)| AddonMediaListItemOptionValues {
                    item_id: None,
                    provider_id: provider_id.clone(),
                    values: values.clone(),
                })
                .collect(),
            completed_at: Some(completed_at),
        },
        items,
        files: Vec::new(),
        artifact_ids: Vec::new(),
        snapshot_digest: String::new(),
    };
    snapshot.snapshot_digest = crate::repositories::revision_snapshot_digest(&snapshot)?;
    Ok(snapshot)
}

/// Refuse unsupported receiving bindings before the caller creates derived files.
pub(super) fn validate_supported_playback(publication: &SavedListPublication) -> Result<()> {
    if publication.items.iter().any(|item| {
        !matches!(&item.playback, SavedPlayback::Stream { timeline: None })
            || !item.subtitles.is_empty()
    }) {
        return Err(RepositoryError::Conflict(
            "the saved publication requires receiving playback bindings that are not yet supported"
                .into(),
        ));
    }
    Ok(())
}

fn json(value: &impl Serialize) -> Result<String> {
    serde_json::to_string(value).map_err(|_| invalid())
}

fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut digest = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        let _ = write!(digest, "{byte:02x}");
    }
    digest
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("the receiving publication identity is inconsistent".into())
}

#[cfg(test)]
mod tests;
