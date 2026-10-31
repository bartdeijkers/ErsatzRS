//! Content identity never confuses a derived snapshot with its manual origin.

use ersatzrs_core::repositories::{
    TeletextGuideRefreshCurrentMetadata, TeletextGuideRefreshOutputConfirmation,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct GuideOutputIdentity {
    seed_id: i64,
    generation_id: i64,
    runtime_id: Option<i64>,
    document_digest: String,
    artifact_digest: String,
}
impl GuideOutputIdentity {
    pub(crate) fn from_metadata(value: &TeletextGuideRefreshCurrentMetadata) -> Self {
        Self {
            seed_id: value.seed.id,
            generation_id: value.seed.generation.id,
            runtime_id: value.runtime.as_ref().map(|runtime| runtime.id),
            document_digest: value.runtime.as_ref().map_or_else(
                || value.seed.generation.request.digest.clone(),
                |runtime| runtime.document_digest.clone(),
            ),
            artifact_digest: value.runtime.as_ref().map_or_else(
                || value.seed.generation.artifact_digest.clone(),
                |runtime| runtime.artifact_digest.clone(),
            ),
        }
    }

    pub(crate) fn from_confirmation(value: &TeletextGuideRefreshOutputConfirmation) -> Self {
        Self {
            seed_id: value.seed.id,
            generation_id: value.seed.generation.id,
            runtime_id: value.runtime.as_ref().map(|runtime| runtime.id),
            document_digest: value.runtime.as_ref().map_or_else(
                || value.seed.generation.request.digest.clone(),
                |runtime| runtime.document_digest.clone(),
            ),
            artifact_digest: value.runtime.as_ref().map_or_else(
                || value.seed.generation.artifact_digest.clone(),
                |runtime| runtime.artifact_digest.clone(),
            ),
        }
    }
}
