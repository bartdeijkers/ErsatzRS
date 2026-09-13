//! Saved-content validation, distinct from file integrity and playback readiness.

use std::collections::BTreeSet;

use ersatzrs_addon_contract::{validate_addon_id, validate_operator_source_url, AddonManifest};

use super::document_io::serialized_size;
use super::model::SavedListDocument;

mod items;
mod metadata;
mod options;

/// Safe receiving diagnostics; never include copied metadata or private paths.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SavedContentError {
    #[error("the saved list document is invalid or exceeds its size limit")]
    Document,
    #[error("the saved list identity does not match the installed add-on")]
    Identity,
    #[error("the saved list item relationships are invalid")]
    Items,
    #[error("the saved list metadata is invalid")]
    Metadata,
    #[error("the saved options do not match the installed add-on schema")]
    Options,
    #[error("the saved playback or subtitle timing is invalid")]
    Playback,
    #[error("the saved file references are invalid or inconsistent")]
    Files,
}

/// Check stored content without changing it or allocating receiving identities.
///
/// The caller supplies the manifest from the locally installed, enabled add-on,
/// never from the copied folder. It still owns that enabled-state check, file
/// confinement/integrity, image/subtitle validation, local media preparation and
/// durable projection. Success here alone never authorizes publication or jobs.
/// Saved drafts are not provider wire records: generated chapter identities,
/// retained tombstones and operator metadata can exceed provider ingestion bounds.
///
/// # Errors
/// Rejects mismatched identity/options, malformed stored content and inconsistent
/// relative references. Errors do not contain data from the copied document.
pub fn validate_content(
    document: &SavedListDocument,
    installed: &AddonManifest,
) -> Result<(), SavedContentError> {
    serialized_size(document).map_err(|_| SavedContentError::Document)?;
    validate_addon_id(&document.addon_id).map_err(|_| SavedContentError::Identity)?;
    if installed.id != document.addon_id
        || document.provider_id.trim().is_empty()
        || document.draft.presentation.name.trim().is_empty()
        || document.draft.revision < 0
    {
        return Err(SavedContentError::Identity);
    }
    validate_operator_source_url(&document.source_url, &[])
        .map_err(|_| SavedContentError::Identity)?;
    // Reuse the policy's existing duplicate/cohort/cutoff semantics. Its
    // normalized result is deliberately not substituted for the saved content.
    document
        .draft
        .delivery
        .validated()
        .map_err(|_| SavedContentError::Items)?;
    let identities: BTreeSet<_> = document
        .draft
        .delivery
        .before_cutoff
        .iter()
        .chain(&document.draft.delivery.after_cutoff)
        .collect();
    if identities.iter().any(|id| id.trim().is_empty())
        || document
            .draft
            .items
            .iter()
            .any(|item| !identities.contains(&item.content.provider_id))
    {
        return Err(SavedContentError::Items);
    }
    items::validate_items(document)?;
    metadata::validate_metadata(document)?;
    options::validate_options(&document.draft.options, &document.addon_id, installed)?;
    validate_option_owners(&document.draft.options)?;
    if let Some(published) = &document.published {
        if published.revision <= 0
            || published.source_draft_revision < 0
            || published.presentation.name.trim().is_empty()
        {
            return Err(SavedContentError::Identity);
        }
        options::validate_options(&published.options, &document.addon_id, installed)?;
        validate_option_owners(&published.options)?;
    }
    Ok(())
}

fn validate_option_owners(options: &super::model::SavedOptions) -> Result<(), SavedContentError> {
    if options.item_values.keys().any(|id| id.trim().is_empty()) {
        return Err(SavedContentError::Items);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
