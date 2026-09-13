//! Explicit portable fields; never serialize database rows or revision DTOs wholesale.

use std::collections::BTreeMap;
use std::ops::Range;

use chrono::{DateTime, Utc};
use ersatzrs_addon_contract::MediaListSubtitleTimeline;
use ersatzrs_core::addon_media_list_playback::{
    AddonMediaListPlaybackTimelinePlan, AddonMediaListPlaybackTimelinePlanError,
};
use ersatzrs_core::repositories::{
    AddonMediaListMetadataConflict, AddonMediaListOptionSchemaRef, AddonMediaListOptionValues,
};
use ersatzrs_domain::{
    AddonMediaListAvailabilityReason, AddonMediaListContentKind, AddonMediaListDetailPolicy,
    AddonMediaListItemAvailability, AddonMediaListItemKind, AddonMediaListItemRole,
    AddonMediaListPresentationKind, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, RemoteStreamLiveness,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::addon_media_list_delivery::portable::PortableDeliveryPolicy;

/// Non-YAML reserved name: ignored by media collection and published fingerprints.
pub const SAVED_LIST_FILE_NAME: &str = ".ersatzrs-list.json";
/// Reserved list-owned assets, independent of the active published tree fingerprint.
pub const SAVED_ASSETS_DIRECTORY: &str = ".ersatzrs-saved";
/// Initial saved-document version; receiving validation must reject other versions.
pub const SAVED_LIST_VERSION: u32 = 1;

/// Complete saved draft and independently retained publication for one provider list.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedListDocument {
    pub version: u32,
    pub addon_id: String,
    pub provider_id: String,
    pub source_url: String,
    pub draft: SavedListDraft,
    pub published: Option<SavedListPublication>,
}

/// Preserve metadata provenance and unresolved provider/operator conflicts.
///
/// Metadata artwork upload URLs must be mapped to contained saved file references
/// before serialization; a receiving projection allocates its own upload handles.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedMetadata {
    pub provider: Map<String, Value>,
    pub overrides: Map<String, Value>,
    pub conflicts: BTreeMap<String, AddonMediaListMetadataConflict>,
}

/// Non-local list presentation fields shared by draft and published content.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedListPresentation {
    pub name: String,
    pub description: Option<String>,
    pub generate_playlist: bool,
    pub content_kind_override: AddonMediaListContentKind,
    pub presentation_kind: AddonMediaListPresentationKind,
    pub requested_presentation_kind: AddonMediaListPresentationKind,
}

/// Exact schema declaration, retained as data rather than installed executable authority.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedOptionSchema {
    pub identity: AddonMediaListOptionSchemaRef,
    pub definition: Value,
}

/// Sparse values use provider identities, not local item or schema IDs.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedOptions {
    pub schema: Option<SavedOptionSchema>,
    pub list_values: AddonMediaListOptionValues,
    pub item_values: BTreeMap<String, AddonMediaListOptionValues>,
}

/// All acknowledged administrative content, including unpublished and excluded items.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedListDraft {
    /// Sender provenance only; never a receiving installation's edit fence.
    pub revision: i32,
    pub presentation: SavedListPresentation,
    pub metadata: SavedMetadata,
    pub detail_import_policy: AddonMediaListDetailPolicy,
    pub chapter_input: String,
    pub refresh_cadence: AddonMediaListRefreshCadence,
    pub refresh_publication_policy: AddonMediaListRefreshPublicationPolicy,
    pub disk_warning_gb: u32,
    pub delivery: PortableDeliveryPolicy,
    pub options: SavedOptions,
    pub items: Vec<SavedDraftItem>,
    /// Referenced list-owned assets that are not necessarily published yet.
    pub files: Vec<SavedFile>,
}

/// Provider item identity and content fields, without any local media binding.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedItemContent {
    pub provider_id: String,
    pub guids: Vec<String>,
    pub rank: i32,
    pub kind: AddonMediaListItemKind,
    pub content_kind: AddonMediaListContentKind,
    pub included: bool,
    pub display_title: String,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub source_url: Option<String>,
    pub additional_image_urls: Vec<String>,
    pub duration_seconds: Option<u64>,
    pub parent_provider_id: Option<String>,
    pub fragment_start_seconds: Option<u64>,
    pub fragment_end_seconds: Option<u64>,
    pub chapter_input: String,
    pub provider_chapter_input: String,
    pub collection_role: AddonMediaListItemRole,
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason_code: Option<AddonMediaListAvailabilityReason>,
    pub provider_liveness: RemoteStreamLiveness,
    pub liveness_override: Option<RemoteStreamLiveness>,
}

/// Retained source absence/reappearance evidence, without import-job generations.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSourceObservation {
    pub missing_since: DateTime<Utc>,
    pub last_observed_at: DateTime<Utc>,
    pub observation_count: u32,
    pub confirmed_at: Option<DateTime<Utc>>,
    pub reappeared_at: Option<DateTime<Utc>>,
}

/// A replacement choice survives copying, but may require a new local binding.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedReplacement {
    /// None means the sender's replacement was not a portable list-owned file.
    /// The receiver must show it unresolved, never silently remove the choice.
    pub file: Option<SavedFile>,
}

/// Saved provider and operator state; publication membership is stored separately.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedDraftItem {
    pub content: SavedItemContent,
    pub metadata: SavedMetadata,
    pub liveness_conflict: bool,
    pub source_observation: Option<SavedSourceObservation>,
    pub replacement: Option<SavedReplacement>,
}

/// Last completed publication, not the current draft and not a copied active job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedListPublication {
    /// Sender provenance only; receiving publication allocates its own revision.
    pub revision: i32,
    /// Sender provenance, not a receiving draft or operation identity.
    pub source_draft_revision: i32,
    pub completed_at: DateTime<Utc>,
    pub presentation: SavedListPresentation,
    pub metadata: Map<String, Value>,
    pub options: SavedOptions,
    pub items: Vec<SavedPublishedItem>,
    pub files: Vec<SavedFile>,
}

/// One actually published item, with no sender artifact pins.
/// Unpublished/excluded saved content belongs to the independent draft.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedPublishedItem {
    pub content: SavedItemContent,
    pub metadata: Map<String, Value>,
    pub playback: SavedPlayback,
    pub subtitles: Vec<SavedSubtitle>,
}

/// Manual SRT sidecar with its language and source/prepared timing preserved.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedSubtitle {
    pub file: SavedFile,
    pub language: String,
    pub timeline: MediaListSubtitleTimeline,
}

/// Absence of prepared bytes never implies permission to stream instead.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "mode", rename_all = "snake_case", deny_unknown_fields)]
pub enum SavedPlayback {
    Stream {
        timeline: Option<SavedPlaybackTimeline>,
    },
    DownloadAhead {
        file: SavedFile,
        duration_milliseconds: u64,
    },
    Replacement {
        replacement: SavedReplacement,
    },
}

/// Relative path and byte identity, not a local database blob ID or storage root.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedFile {
    pub relative_path: String,
    pub sha256: String,
    pub byte_length: u64,
}

/// Portable cut geometry, intentionally excluding the sender's execution-context hash.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SavedPlaybackTimeline {
    pub source_duration_milliseconds: u64,
    pub source_window_milliseconds: Range<u64>,
    pub excluded_intervals_milliseconds: Vec<Range<u64>>,
}

impl From<&AddonMediaListPlaybackTimelinePlan> for SavedPlaybackTimeline {
    fn from(plan: &AddonMediaListPlaybackTimelinePlan) -> Self {
        Self {
            source_duration_milliseconds: plan.source_duration_milliseconds(),
            source_window_milliseconds: plan.source_window_milliseconds(),
            excluded_intervals_milliseconds: plan.excluded_intervals_milliseconds().to_vec(),
        }
    }
}

impl SavedPlaybackTimeline {
    /// Validate through the existing timeline contract using a receiving-local context.
    ///
    /// # Errors
    /// Returns the existing timing or context validation error without importing a
    /// sender's execution identity or inventing a separate interval validator.
    pub fn bind(
        &self,
        context_fingerprint: String,
    ) -> Result<AddonMediaListPlaybackTimelinePlan, AddonMediaListPlaybackTimelinePlanError> {
        AddonMediaListPlaybackTimelinePlan::new(
            self.source_duration_milliseconds,
            self.source_window_milliseconds.clone(),
            &self.excluded_intervals_milliseconds,
            context_fingerprint,
        )
    }
}

#[cfg(test)]
mod tests;
