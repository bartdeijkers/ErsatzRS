//! Read current saved state under the caller's transaction, without changing files.

use std::collections::{BTreeMap, BTreeSet};

use ersatzrs_core::repositories::{
    AddonMediaListArtworkRole, AddonMediaListOptionSchemaRef, AddonMediaListRevisionItem,
    RepositoryError, Result,
};
use ersatzrs_domain::EntityId;
use serde::de::DeserializeOwned;
use serde_json::{Map, Value};
use sqlx::{Row as _, Transaction};

use super::model::{
    SavedDraftItem, SavedFile, SavedItemContent, SavedListDraft, SavedListPresentation,
    SavedMetadata, SavedOptionSchema, SavedOptions, SavedReplacement, SavedSourceObservation,
};
use crate::db::{Db, DbRow};
use crate::repositories::common::{
    decode_bool_column, decode_datetime, decode_enum_column, decode_optional_datetime,
};
use crate::repositories::SqlxResultExt as _;
use crate::repositories::{load_current_revision_items_in_tx, load_draft_options_in_tx};

/// A draft candidate plus local sources still requiring owned asset materialization.
///
/// This deliberately does not implement Serialize. Only the assembled document,
/// after materializing and validating its assets and separate publication, may
/// become the saved file. No bytes are written by this read-only capture.
pub struct CapturedDraft {
    pub addon_id: String,
    pub provider_id: String,
    pub source_url: String,
    pub draft: SavedListDraft,
    pub artwork: Vec<CapturedArtwork>,
    pub replacements: Vec<CapturedReplacement>,
}

/// Installation-local upload source, not part of portable JSON.
pub struct CapturedArtwork {
    pub upload_id: EntityId,
    pub scope_item_id: EntityId,
    pub role: i32,
    pub file: SavedFile,
}

/// Installation-local selection to check for a portable list-owned file.
pub struct CapturedReplacement {
    pub provider_id: String,
    pub media_item_id: EntityId,
    pub media_version_id: Option<EntityId>,
}

/// Capture all current draft items and settings, retaining metadata provenance.
///
/// The caller owns a consistent transaction, content/publication fencing, file
/// materialization and the separate last-published capture. This is not an
/// export endpoint and must not publish or acknowledge a Save on its own.
///
/// # Errors
/// Rejects missing lists, inconsistent metadata/options and unowned artwork handles.
pub async fn draft_in_tx(tx: &mut Transaction<'_, Db>, list_id: EntityId) -> Result<CapturedDraft> {
    let row = sqlx::query(
        r#"SELECT "AddonId", "ProviderId", "SourceUrl", "Name", "Description",
           "GeneratePlaylist", "ContentKindOverride", "PresentationKind",
           "RequestedPresentationKind", "ProviderMetadataJson", "MetadataOverrideJson",
           "MetadataConflictJson", "DraftRevision", "DetailImportPolicy", "ChapterInput",
           "RefreshCadence", "RefreshPublicationPolicy", "DiskUsageWarningGb"
           FROM "AddonMediaList" WHERE "Id" = ?"#,
    )
    .bind(list_id)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or(RepositoryError::NotFound {
        kind: "add-on media list",
        id: list_id,
    })?;
    let mut artwork = ArtworkCapture::load(tx, list_id).await?;
    let metadata = artwork.metadata(&row, 0)?;
    let (items, replacements) = items_in_tx(tx, list_id, &mut artwork).await?;
    let options = load_draft_options_in_tx(tx, list_id, None)
        .await?
        .ok_or_else(inconsistent)?;
    let schema = option_schema(tx, options.schema.as_ref()).await?;
    let warning: i64 = row.try_get("DiskUsageWarningGb").repository()?;
    let artwork = artwork.finish();
    let captured = CapturedDraft {
        addon_id: row.try_get("AddonId").repository()?,
        provider_id: row.try_get("ProviderId").repository()?,
        source_url: row.try_get("SourceUrl").repository()?,
        draft: SavedListDraft {
            revision: row.try_get("DraftRevision").repository()?,
            presentation: presentation(&row)?,
            metadata,
            detail_import_policy: decode_enum_column("detail policy", &row, "DetailImportPolicy")?,
            chapter_input: row
                .try_get::<Option<String>, _>("ChapterInput")
                .repository()?
                .unwrap_or_default(),
            refresh_cadence: decode_enum_column("refresh cadence", &row, "RefreshCadence")?,
            refresh_publication_policy: decode_enum_column(
                "refresh publication policy",
                &row,
                "RefreshPublicationPolicy",
            )?,
            disk_warning_gb: u32::try_from(warning).map_err(|_| inconsistent())?,
            delivery: crate::addon_media_list_delivery::portable::capture_in_tx(tx, list_id)
                .await?,
            options: SavedOptions {
                schema,
                list_values: options.list_values,
                item_values: options
                    .item_values
                    .into_iter()
                    .map(|item| (item.provider_id, item.values))
                    .collect(),
            },
            items,
            files: artwork.iter().map(|asset| asset.file.clone()).collect(),
        },
        artwork,
        replacements,
    };
    // No sender-local handle may escape through an unsupported descriptive field.
    if serde_json::to_string(&captured.draft)
        .map_err(|_| inconsistent())?
        .contains(crate::addon_artwork::UPLOAD_URL_PREFIX)
    {
        return Err(inconsistent());
    }
    Ok(captured)
}

async fn items_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    artwork: &mut ArtworkCapture,
) -> Result<(Vec<SavedDraftItem>, Vec<CapturedReplacement>)> {
    let current_items = load_current_revision_items_in_tx(tx, list_id).await?;
    let state_rows = sqlx::query(
        r#"SELECT "item"."Id" AS "ScopeItemId", "item"."ProviderId", "item"."ProviderMetadataJson",
           "item"."MetadataOverrideJson", "item"."MetadataConflictJson",
           "item"."LivenessConflict", "absence"."MissingSince", "absence"."LastObservedAt",
           "absence"."ObservationCount", "absence"."ConfirmedAt", "absence"."ReappearedAt"
           FROM "AddonMediaListItem" AS "item"
           LEFT JOIN "AddonMediaListItemTombstone" AS "absence"
             ON "absence"."AddonMediaListItemId" = "item"."Id"
           WHERE "item"."AddonMediaListId" = ?"#,
    )
    .bind(list_id)
    .fetch_all(&mut **tx)
    .await
    .repository()?;
    let mut states = BTreeMap::new();
    for state in state_rows {
        states.insert(
            state.try_get::<String, _>("ProviderId").repository()?,
            state,
        );
    }
    let mut items = Vec::with_capacity(current_items.len());
    let mut replacements = Vec::new();
    for item in current_items {
        let state = states.remove(&item.provider_id).ok_or_else(inconsistent)?;
        if let Some(media_item_id) = item.replacement_media_item_id {
            replacements.push(CapturedReplacement {
                provider_id: item.provider_id.clone(),
                media_item_id,
                media_version_id: item.replacement_media_version_id,
            });
        }
        items.push(SavedDraftItem {
            content: item_content(&item)?,
            metadata: artwork.metadata(&state, state.try_get("ScopeItemId").repository()?)?,
            liveness_conflict: decode_bool_column(&state, "LivenessConflict")?,
            source_observation: observation(&state)?,
            replacement: item
                .replacement_media_item_id
                .map(|_| SavedReplacement { file: None }),
        });
    }
    Ok((items, replacements))
}

fn inconsistent() -> RepositoryError {
    RepositoryError::Conflict("saved list content is inconsistent".to_owned())
}

pub(super) fn decode_json<T: DeserializeOwned>(json: &str) -> Result<T> {
    serde_json::from_str(json).map_err(|_| inconsistent())
}

pub(super) fn presentation(row: &DbRow) -> Result<SavedListPresentation> {
    Ok(SavedListPresentation {
        name: row.try_get("Name").repository()?,
        description: row.try_get("Description").repository()?,
        generate_playlist: decode_bool_column(row, "GeneratePlaylist")?,
        content_kind_override: decode_enum_column(
            "content classification",
            row,
            "ContentKindOverride",
        )?,
        presentation_kind: decode_enum_column("presentation", row, "PresentationKind")?,
        requested_presentation_kind: decode_enum_column(
            "requested presentation",
            row,
            "RequestedPresentationKind",
        )?,
    })
}

pub(super) fn item_content(item: &AddonMediaListRevisionItem) -> Result<SavedItemContent> {
    Ok(SavedItemContent {
        provider_id: item.provider_id.clone(),
        guids: item.guids.clone(),
        rank: item.rank,
        kind: item.kind,
        content_kind: item.content_kind,
        included: item.included,
        display_title: item.display_title.clone(),
        title: item.title.clone(),
        year: item.year,
        season: item.season,
        episode: item.episode,
        source_url: item.source_url.clone(),
        additional_image_urls: item
            .additional_image_urls
            .as_deref()
            .map(decode_json)
            .transpose()?
            .unwrap_or_default(),
        duration_seconds: item.duration_seconds,
        parent_provider_id: item.parent_provider_id.clone(),
        fragment_start_seconds: item.fragment_start_seconds,
        fragment_end_seconds: item.fragment_end_seconds,
        chapter_input: item.chapter_input.clone(),
        provider_chapter_input: item.provider_chapter_input.clone(),
        collection_role: item.collection_role,
        availability: item.availability,
        availability_reason_code: item.availability_reason_code,
        provider_liveness: item.provider_liveness,
        liveness_override: item.liveness_override,
    })
}

fn observation(row: &DbRow) -> Result<Option<SavedSourceObservation>> {
    let Some(missing_since) = row
        .try_get::<Option<String>, _>("MissingSince")
        .repository()?
    else {
        return Ok(None);
    };
    let count: i64 = row.try_get("ObservationCount").repository()?;
    Ok(Some(SavedSourceObservation {
        missing_since: decode_datetime("source absence", &missing_since)?,
        last_observed_at: decode_datetime(
            "source observation",
            &row.try_get::<String, _>("LastObservedAt").repository()?,
        )?,
        observation_count: u32::try_from(count).map_err(|_| inconsistent())?,
        confirmed_at: decode_optional_datetime(
            "source confirmation",
            row.try_get("ConfirmedAt").repository()?,
        )?,
        reappeared_at: decode_optional_datetime(
            "source reappearance",
            row.try_get("ReappearedAt").repository()?,
        )?,
    }))
}

pub(super) async fn option_schema(
    tx: &mut Transaction<'_, Db>,
    identity: Option<&AddonMediaListOptionSchemaRef>,
) -> Result<Option<SavedOptionSchema>> {
    let Some(identity) = identity else {
        return Ok(None);
    };
    let definition: String = sqlx::query_scalar(
        r#"SELECT "DefinitionJson" FROM "AddonMediaListOptionSchema"
           WHERE "AddonId" = ? AND "CapabilityId" = ? AND "SchemaDigest" = ?"#,
    )
    .bind(&identity.addon_id)
    .bind(&identity.capability_id)
    .bind(&identity.schema_digest)
    .fetch_optional(&mut **tx)
    .await
    .repository()?
    .ok_or_else(inconsistent)?;
    Ok(Some(SavedOptionSchema {
        identity: identity.clone(),
        definition: decode_json(&definition)?,
    }))
}

struct ArtworkCapture {
    uploads: BTreeMap<String, CapturedArtwork>,
    used: BTreeSet<String>,
}

impl ArtworkCapture {
    async fn load(tx: &mut Transaction<'_, Db>, list_id: EntityId) -> Result<Self> {
        // Capture references, not every image BLOB in memory. Materialization must
        // verify/copy one selected upload at a time before writing the document.
        let rows = sqlx::query(
            r#"SELECT "Id", "ScopeItemId", "Role", "Sha256", "Extension", length("Bytes") AS "ByteLength"
            FROM "AddonMediaListArtworkUpload" WHERE "AddonMediaListId" = ? ORDER BY "Id""#,
        )
        .bind(list_id)
        .fetch_all(&mut **tx)
        .await
        .repository()?;
        let mut uploads = BTreeMap::new();
        for row in rows {
            let id: EntityId = row.try_get("Id").repository()?;
            let hash: String = row.try_get("Sha256").repository()?;
            let extension: String = row.try_get("Extension").repository()?;
            let size: i64 = row.try_get("ByteLength").repository()?;
            uploads.insert(
                format!("{}{id}", crate::addon_artwork::UPLOAD_URL_PREFIX),
                CapturedArtwork {
                    upload_id: id,
                    scope_item_id: row.try_get("ScopeItemId").repository()?,
                    role: row.try_get("Role").repository()?,
                    file: SavedFile {
                        relative_path: format!(".ersatzrs-saved/artwork/{hash}.{extension}"),
                        sha256: hash,
                        byte_length: u64::try_from(size).map_err(|_| inconsistent())?,
                    },
                },
            );
        }
        Ok(Self {
            uploads,
            used: BTreeSet::new(),
        })
    }

    fn metadata(&mut self, row: &DbRow, scope: EntityId) -> Result<SavedMetadata> {
        let mut value = SavedMetadata {
            provider: decode_json(
                &row.try_get::<String, _>("ProviderMetadataJson")
                    .repository()?,
            )?,
            overrides: decode_json(
                &row.try_get::<String, _>("MetadataOverrideJson")
                    .repository()?,
            )?,
            conflicts: row
                .try_get::<Option<String>, _>("MetadataConflictJson")
                .repository()?
                .filter(|v| !v.trim().is_empty())
                .as_deref()
                .map(decode_json)
                .transpose()?
                .unwrap_or_default(),
        };
        self.metadata_map(&mut value.provider, scope)?;
        self.metadata_map(&mut value.overrides, scope)?;
        for field in ["artwork", "artwork_urls"] {
            if let Some(conflict) = value.conflicts.get_mut(field) {
                self.field(field, &mut conflict.provider_before, scope)?;
                self.field(field, &mut conflict.provider_after, scope)?;
                self.field(field, &mut conflict.operator_value, scope)?;
            }
        }
        Ok(value)
    }

    fn metadata_map(&mut self, value: &mut Map<String, Value>, scope: EntityId) -> Result<()> {
        for field in ["artwork", "artwork_urls"] {
            if let Some(value) = value.get_mut(field) {
                self.field(field, value, scope)?;
            }
        }
        Ok(())
    }

    fn field(&mut self, field: &str, value: &mut Value, scope: EntityId) -> Result<()> {
        if field == "artwork" {
            if let Some(artwork) = value.as_array_mut() {
                for entry in artwork {
                    let role: AddonMediaListArtworkRole = serde_json::from_value(
                        entry.get("role").cloned().ok_or_else(inconsistent)?,
                    )
                    .map_err(|_| inconsistent())?;
                    self.rewrite(
                        entry,
                        scope,
                        Some(crate::repositories::artwork_role_value(role)),
                    )?;
                }
                return Ok(());
            }
        }
        self.rewrite(value, scope, None)
    }

    fn rewrite(&mut self, value: &mut Value, scope: EntityId, role: Option<i32>) -> Result<()> {
        match value {
            Value::String(url) if url.starts_with(crate::addon_artwork::UPLOAD_URL_PREFIX) => {
                let owned = self.uploads.get(url).ok_or_else(inconsistent)?;
                if owned.scope_item_id != scope || role.is_some_and(|role| role != owned.role) {
                    return Err(inconsistent());
                }
                let asset = owned;
                let file = &asset.file;
                if file.sha256.len() != 64
                    || !file
                        .sha256
                        .bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
                    || !["jpg", "png", "webp"].iter().any(|extension| {
                        file.relative_path
                            == format!(".ersatzrs-saved/artwork/{}.{extension}", file.sha256)
                    })
                    || file.byte_length == 0
                    || file.byte_length > crate::addon_artwork::MAX_ARTWORK_BYTES as u64
                {
                    return Err(inconsistent());
                }
                self.used.insert(url.clone());
                *url = format!("./{}", asset.file.relative_path);
            }
            Value::Array(values) => {
                for value in values {
                    self.rewrite(value, scope, role)?;
                }
            }
            Value::Object(values) => {
                for value in values.values_mut() {
                    self.rewrite(value, scope, role)?;
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn finish(self) -> Vec<CapturedArtwork> {
        let mut files = BTreeMap::new();
        for (url, owned) in self.uploads {
            if self.used.contains(&url) {
                let asset = owned;
                files
                    .entry(asset.file.relative_path.clone())
                    .or_insert(asset);
            }
        }
        files.into_values().collect()
    }
}

#[cfg(test)]
mod tests;
