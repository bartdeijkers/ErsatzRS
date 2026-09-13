//! Restore only portable artwork slots, leaving descriptive metadata untouched.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use ersatzrs_core::repositories::{AddonMediaListArtworkRole, Result};
use ersatzrs_domain::EntityId;
use serde_json::{Map, Value};
use sqlx::Transaction;

use super::{bind_artwork_in_tx, invalid, ArtworkStaging};
use crate::addon_media_list_saved::{
    model::{SavedFile, SavedListDocument, SavedMetadata},
    receiving::{layer_images, map_images},
};
use crate::db::Db;

impl ArtworkStaging {
    /// Stage the artwork referenced by both already-validated saved states.
    ///
    /// Keeps only a path index in memory and admits image bytes one at a time.
    /// Caller must first validate the document and retain its tree fence.
    ///
    /// # Errors
    /// Refuses undeclared references or unavailable/changed image bytes.
    pub async fn stage_document(
        &mut self,
        root: &Path,
        document: &SavedListDocument,
    ) -> Result<()> {
        let files: BTreeMap<_, _> = document
            .draft
            .files
            .iter()
            .chain(document.published.iter().flat_map(|state| &state.files))
            .map(|file| (file.relative_path.as_str(), file))
            .collect();
        let mut images = BTreeSet::new();
        layer_images(&document.draft.metadata, &mut images);
        for item in &document.draft.items {
            layer_images(&item.metadata, &mut images);
        }
        if let Some(published) = &document.published {
            map_images(&published.metadata, &mut images);
            for item in &published.items {
                map_images(&item.metadata, &mut images);
            }
        }
        for path in images {
            self.stage_file(root, files.get(path).ok_or_else(invalid)?)
                .await?;
        }
        Ok(())
    }
}

/// Bind a receiving draft's provider, override and conflict artwork independently.
///
/// Supply the validated state's own file index and owners created in `tx`.
/// Mutates the receiving working value, never the saved file. On failure discard
/// that value and roll back the transaction; do not reuse partial handles.
///
/// # Errors
/// Refuses undeclared, unstaged or invalid artwork and wrong receiving owners.
pub async fn bind_metadata_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    item_id: Option<EntityId>,
    metadata: &mut SavedMetadata,
    files: &BTreeMap<&str, &SavedFile>,
) -> Result<()> {
    bind_map_in_tx(tx, list_id, item_id, &mut metadata.provider, files).await?;
    bind_map_in_tx(tx, list_id, item_id, &mut metadata.overrides, files).await?;
    for field in ["artwork", "artwork_urls"] {
        if let Some(conflict) = metadata.conflicts.get_mut(field) {
            for value in [
                &mut conflict.provider_before,
                &mut conflict.provider_after,
                &mut conflict.operator_value,
            ] {
                bind_field(tx, list_id, item_id, field, value, files).await?;
            }
        }
    }
    Ok(())
}

/// Bind only artwork URLs in one receiving metadata map (including old publication).
/// Uses its own state's file index, not newer draft content. Person thumbnails,
/// unknown fields and remote URLs remain literal. On failure discard the working
/// value and roll back the transaction, as for [`bind_metadata_in_tx`].
///
/// # Errors
/// Refuses undeclared, unstaged or invalid artwork and wrong receiving owners.
pub async fn bind_map_in_tx(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    item_id: Option<EntityId>,
    metadata: &mut Map<String, Value>,
    files: &BTreeMap<&str, &SavedFile>,
) -> Result<()> {
    for field in ["artwork", "artwork_urls"] {
        if let Some(value) = metadata.get_mut(field) {
            bind_field(tx, list_id, item_id, field, value, files).await?;
        }
    }
    Ok(())
}

async fn bind_field(
    tx: &mut Transaction<'_, Db>,
    list_id: EntityId,
    item_id: Option<EntityId>,
    field: &str,
    value: &mut Value,
    files: &BTreeMap<&str, &SavedFile>,
) -> Result<()> {
    for entry in value.as_array_mut().into_iter().flatten() {
        let (url, role) = if field == "artwork" {
            // Only a local URL needs an owned role. Masked provider values retain
            // their tolerant stored shape, just as receiving validation allows.
            if !entry
                .get("url")
                .and_then(Value::as_str)
                .is_some_and(|url| url.starts_with("./"))
            {
                continue;
            }
            let role = serde_json::from_value(entry.get("role").cloned().ok_or_else(invalid)?)
                .map_err(|_| invalid())?;
            (entry.get_mut("url").ok_or_else(invalid)?, role)
        } else {
            (entry, AddonMediaListArtworkRole::Thumb)
        };
        let Some(path) = url.as_str().and_then(|url| url.strip_prefix("./")) else {
            continue;
        };
        let file = files.get(path).ok_or_else(invalid)?;
        *url = Value::String(bind_artwork_in_tx(tx, list_id, item_id, role, file).await?);
    }
    Ok(())
}
