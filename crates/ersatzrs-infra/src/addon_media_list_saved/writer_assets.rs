//! Asset preflight for ordinary Save; applying the result performs no I/O.

use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

use ersatzrs_core::repositories::{AddonMediaListEditorSave, RepositoryError, Result};

use super::{
    assembly::ManagedTreeGuard,
    assets,
    capture::{ArtworkCapture, CapturedArtwork, CapturedDraft, CapturedReplacement},
    model::{SavedFile, SavedListDocument, SavedReplacement},
    receiving, sources,
};
use crate::db::DbPool;

/// Exact local sources and portable references prepared before the write transaction.
pub(crate) struct PreparedAssets {
    identity: (String, String, String),
    artwork: Vec<CapturedArtwork>,
    files: Vec<SavedFile>,
    replacements: BTreeMap<String, (Option<CapturedReplacement>, Option<SavedReplacement>)>,
}

impl PreparedAssets {
    /// Install newly selected uploads and inspect only changed replacement choices.
    /// The caller retains its content/revision fence and tree ownership until Save commits.
    #[allow(clippy::too_many_lines)] // Keep read capture, I/O and final reference preparation in order.
    pub(crate) async fn prepare(
        pool: &DbPool,
        tree: Arc<ManagedTreeGuard>,
        current_document: &SavedListDocument,
        current_capture: &CapturedDraft,
        save: &AddonMediaListEditorSave,
    ) -> Result<Self> {
        if current_document.addon_id != current_capture.addon_id
            || current_document.provider_id != current_capture.provider_id
            || current_document.source_url != current_capture.source_url
        {
            return Err(invalid());
        }
        let mut prepared = Self {
            identity: (
                current_capture.addon_id.clone(),
                current_capture.provider_id.clone(),
                current_capture.source_url.clone(),
            ),
            artwork: current_capture.artwork.clone(),
            files: current_document.draft.files.clone(),
            replacements: BTreeMap::new(),
        };
        for item in &current_capture.draft.items {
            let provider = &item.content.provider_id;
            let source = current_capture
                .replacements
                .iter()
                .find(|source| &source.provider_id == provider)
                .cloned();
            let saved = current_document
                .draft
                .items
                .iter()
                .find(|saved| &saved.content.provider_id == provider)
                .ok_or_else(invalid)?;
            if item.replacement.is_some() != saved.replacement.is_some() {
                return Err(invalid());
            }
            prepared
                .replacements
                .insert(provider.clone(), (source, saved.replacement.clone()));
        }
        // Capture submitted handles using the same list/scope/role rules as final capture.
        // Finish this read transaction before any upload or video bytes are inspected.
        let mut tx = pool.begin().await.map_err(|_| invalid())?;
        let mut uploads = ArtworkCapture::load(&mut tx, save.list_id).await?;
        if let Some(metadata) = &save.metadata {
            let mut value = serde_json::to_value(&metadata.metadata).map_err(|_| invalid())?;
            uploads.metadata_map(value.as_object_mut().ok_or_else(invalid)?, 0)?;
        }
        let mut changed = Vec::new();
        for item in &save.items {
            let provider: String = sqlx::query_scalar(
                r#"SELECT "ProviderId" FROM "AddonMediaListItem" WHERE "Id"=? AND "AddonMediaListId"=? AND "InDraft"=1"#,
            ).bind(item.review.item_id).bind(save.list_id).fetch_optional(&mut *tx).await.map_err(|_| invalid())?.ok_or_else(invalid)?;
            if let Some(metadata) = &item.metadata {
                let mut value = serde_json::to_value(&metadata.metadata).map_err(|_| invalid())?;
                uploads.metadata_map(
                    value.as_object_mut().ok_or_else(invalid)?,
                    item.review.item_id,
                )?;
            }
            if let Some(choice) = item.replacement {
                let selected =
                    choice.map(|(media_item_id, media_version_id)| CapturedReplacement {
                        provider_id: provider.clone(),
                        media_item_id,
                        media_version_id: Some(media_version_id),
                    });
                let existing = prepared
                    .replacements
                    .get_mut(&provider)
                    .ok_or_else(invalid)?;
                if existing.0 != selected || (selected.is_none() && existing.1.is_some()) {
                    *existing = (
                        selected.clone(),
                        selected.as_ref().map(|_| SavedReplacement { file: None }),
                    );
                    if let Some(selected) = selected {
                        changed.push(selected);
                    }
                }
            }
        }
        let uploads = uploads.finish_sources();
        tx.commit().await.map_err(|_| invalid())?;
        for upload in uploads {
            if prepared.artwork.contains(&upload) && prepared.files.contains(&upload.file) {
                continue;
            }
            let bytes = sources::load_owned_artwork(pool, save.list_id, &upload).await?;
            let guard = Arc::clone(&tree);
            let file = upload.file.clone();
            tokio::task::spawn_blocking(move || {
                assets::install(guard.managed_root(), &file, &mut bytes.as_slice())
                    .map_err(|_| invalid())
            })
            .await
            .map_err(|_| invalid())??;
            prepared.files.push(upload.file.clone());
            prepared.artwork.push(upload);
        }
        for selected in changed {
            let file =
                sources::inspect_owned_replacement(pool, tree.managed_root(), &selected).await?;
            if let Some(file) = &file {
                prepared.files.push(file.clone());
            }
            prepared.replacements.insert(
                selected.provider_id.clone(),
                (Some(selected), Some(SavedReplacement { file })),
            );
        }
        Ok(prepared)
    }

    /// Recheck post-edit local ownership and attach only prepared/reused references.
    /// This is deliberately synchronous: no filesystem, network or SQL work occurs here.
    pub(crate) fn apply(&self, captured: &mut CapturedDraft) -> Result<()> {
        if self.identity
            != (
                captured.addon_id.clone(),
                captured.provider_id.clone(),
                captured.source_url.clone(),
            )
            || captured
                .artwork
                .iter()
                .any(|upload| !self.artwork.contains(upload))
        {
            return Err(invalid());
        }
        for item in &mut captured.draft.items {
            let provider = &item.content.provider_id;
            let (expected_source, replacement) =
                self.replacements.get(provider).ok_or_else(invalid)?;
            let actual_source = captured
                .replacements
                .iter()
                .find(|source| &source.provider_id == provider);
            if actual_source != expected_source.as_ref()
                || item.replacement.is_some() != replacement.is_some()
                || (actual_source.is_none() && &item.replacement != replacement)
            {
                return Err(invalid());
            }
            item.replacement.clone_from(replacement);
        }
        let mut required = BTreeSet::new();
        receiving::layer_images(&captured.draft.metadata, &mut required);
        for item in &captured.draft.items {
            receiving::layer_images(&item.metadata, &mut required);
            if let Some(file) = item
                .replacement
                .as_ref()
                .and_then(|replacement| replacement.file.as_ref())
            {
                required.insert(file.relative_path.as_str());
            }
        }
        let mut files = Vec::new();
        for path in required {
            let mut candidates = self.files.iter().filter(|file| file.relative_path == path);
            let file = candidates.next().ok_or_else(invalid)?;
            if candidates.any(|other| other != file) {
                return Err(invalid());
            }
            files.push(file.clone());
        }
        captured.draft.files = files;
        Ok(())
    }
}

fn invalid() -> RepositoryError {
    RepositoryError::Conflict("saved list asset selection changed or is unavailable".into())
}

#[cfg(test)]
mod tests;
