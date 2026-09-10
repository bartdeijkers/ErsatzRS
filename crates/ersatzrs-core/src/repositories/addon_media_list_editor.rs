//! Coherent task saves for the ADR 0091 add-on list editor.

use ersatzrs_domain::{
    AddonMediaListDeliveryMode, AddonMediaListDetailPolicy, AddonMediaListItemRole,
    AddonMediaListPresentationKind, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, EntityId,
};
use serde::{Deserialize, Serialize};

use super::{AddonMediaListDraftOptions, AddonMediaListItemReviewEdit, AddonMediaListNfoMetadata};

/// Metadata plus the exact provider/override version displayed to the visitor.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddonMediaListEditorMetadata {
    pub metadata: AddonMediaListNfoMetadata,
    pub etag: String,
}

/// List settings edited together, independently of playback and publication.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddonMediaListEditorSettings {
    pub refresh_cadence: AddonMediaListRefreshCadence,
    pub refresh_publication_policy: AddonMediaListRefreshPublicationPolicy,
    pub detail_policy: AddonMediaListDetailPolicy,
    pub generate_playlist: bool,
    pub presentation_kind: AddonMediaListPresentationKind,
}

/// Operator-owned values for one item; absent optional groups stay unchanged.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddonMediaListEditorItem {
    pub review: AddonMediaListItemReviewEdit,
    /// Original row values protect edits retained while another task is saved.
    pub expected_review: Option<AddonMediaListItemReviewEdit>,
    pub role: Option<AddonMediaListItemRole>,
    pub metadata: Option<AddonMediaListEditorMetadata>,
    pub chapters: Option<String>,
    /// Outer None preserves the mapping; Some(None) explicitly clears it.
    pub replacement: Option<Option<(EntityId, EntityId)>>,
}

/// One validated editing task committed under a single saved-revision fence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AddonMediaListEditorSave {
    pub list_id: EntityId,
    pub expected_revision: i32,
    pub metadata: Option<AddonMediaListEditorMetadata>,
    pub settings: Option<AddonMediaListEditorSettings>,
    pub delivery_mode: Option<AddonMediaListDeliveryMode>,
    pub disk_warning_gb: Option<u32>,
    pub options: Option<AddonMediaListDraftOptions>,
    pub items: Vec<AddonMediaListEditorItem>,
}
