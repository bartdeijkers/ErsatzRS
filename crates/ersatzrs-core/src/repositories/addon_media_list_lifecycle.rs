use std::collections::BTreeMap;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use ersatzrs_domain::{
    AddonMediaListArtifactKind, AddonMediaListArtifactState, AddonMediaListAvailabilityReason,
    AddonMediaListContentKind, AddonMediaListImportMode, AddonMediaListImportPurpose,
    AddonMediaListIncrementalStrategy, AddonMediaListItemAvailability, AddonMediaListItemKind,
    AddonMediaListItemRole, AddonMediaListLifecycleJobKind, AddonMediaListLifecyclePhase,
    AddonMediaListOptionValue, AddonMediaListPresentationKind, AddonMediaListRefreshCadence,
    AddonMediaListRefreshPublicationPolicy, AddonToolMaintenanceOutcome,
    AddonToolMaintenanceRollback, AddonToolMaintenanceState, AddonToolMaintenanceTrigger,
    AddonToolMaintenanceValidation, EntityId, RemoteStreamLiveness,
};
use serde::{Deserialize, Serialize};

use super::Result;

pub type AddonMediaListOptionValues = BTreeMap<String, AddonMediaListOptionValue>;

pub const PROCESSED_ARTIFACT_GLOBAL_BYTE_LIMIT_KEY: &str =
    "addons.media_lists.processed_artifacts.global_byte_limit";
pub const PROCESSED_ARTIFACT_MINIMUM_FREE_BYTES_KEY: &str =
    "addons.media_lists.processed_artifacts.minimum_free_bytes";
pub const PROCESSED_ARTIFACT_RETENTION_SECONDS_KEY: &str =
    "addons.media_lists.processed_artifacts.retention_seconds";
pub const PROCESSED_ARTIFACT_MAX_CONCURRENCY_KEY: &str =
    "addons.media_lists.processed_artifacts.max_preparation_concurrency";

/// Immutable identity of one package-declared option schema.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListOptionSchemaRef {
    pub addon_id: String,
    pub capability_id: String,
    pub schema_digest: String,
}

/// Opaque provider-neutral option declaration retained for exact version matching.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListOptionSchema {
    pub schema: AddonMediaListOptionSchemaRef,
    pub definition_json: String,
    pub created_at: DateTime<Utc>,
}

/// Sparse draft or retained item-level option values.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListItemOptionValues {
    pub item_id: Option<EntityId>,
    pub provider_id: String,
    pub values: AddonMediaListOptionValues,
}

/// Current saved draft option state.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListDraftOptions {
    pub list_id: EntityId,
    pub draft_revision: i32,
    pub schema: Option<AddonMediaListOptionSchemaRef>,
    pub list_values: AddonMediaListOptionValues,
    pub item_values: Vec<AddonMediaListItemOptionValues>,
}

impl AddonMediaListDraftOptions {
    /// Resolve schema defaults, list values, then one item's sparse overrides.
    #[must_use]
    pub fn effective_values(
        &self,
        defaults: &AddonMediaListOptionValues,
        item_id: EntityId,
    ) -> AddonMediaListOptionValues {
        let mut result = defaults.clone();
        result.extend(self.list_values.clone());
        if let Some(item) = self
            .item_values
            .iter()
            .find(|candidate| candidate.item_id == Some(item_id))
        {
            result.extend(item.values.clone());
        }
        result
    }
}

/// Immutable option values associated with one completed publication revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListPublishedOptions {
    pub list_id: EntityId,
    pub revision: i32,
    pub source_draft_revision: Option<i32>,
    pub schema: Option<AddonMediaListOptionSchemaRef>,
    pub list_values: AddonMediaListOptionValues,
    pub item_values: Vec<AddonMediaListItemOptionValues>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Host-owned immutable content blob shared by retained revisions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListRevisionFileBlob {
    pub id: EntityId,
    pub list_id: EntityId,
    pub storage_relative_path: String,
    pub sha256: String,
    pub byte_length: u64,
    pub created_at: DateTime<Utc>,
}

/// One immutable file identity retained by a complete publication revision.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListRevisionFile {
    pub blob_id: EntityId,
    pub published_relative_path: String,
    pub provider_id: Option<String>,
    pub storage_relative_path: String,
    pub sha256: String,
    pub byte_length: u64,
}

/// Provider-keyed saved-draft state captured by a completed publication.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListRevisionItem {
    pub provider_id: String,
    pub guids: Vec<String>,
    pub rank: i32,
    pub kind: AddonMediaListItemKind,
    pub content_kind: AddonMediaListContentKind,
    pub included: bool,
    /// Actual membership after publication validation and restricted-item filtering.
    pub published: bool,
    pub display_title: String,
    pub title: String,
    pub year: Option<i32>,
    pub season: Option<i32>,
    pub episode: Option<i32>,
    pub source_url: Option<String>,
    pub additional_image_urls: Option<String>,
    pub duration_seconds: Option<u64>,
    pub parent_provider_id: Option<String>,
    pub fragment_start_seconds: Option<u64>,
    pub fragment_end_seconds: Option<u64>,
    pub chapter_input: String,
    pub provider_chapter_input: String,
    pub collection_role: AddonMediaListItemRole,
    pub availability: AddonMediaListItemAvailability,
    pub availability_reason: Option<String>,
    pub availability_reason_code: Option<AddonMediaListAvailabilityReason>,
    pub provider_liveness: RemoteStreamLiveness,
    pub liveness_override: Option<RemoteStreamLiveness>,
    pub effective_liveness: RemoteStreamLiveness,
    /// Canonical effective provider-plus-operator metadata JSON.
    pub metadata_json: String,
    pub metadata_digest: String,
    pub replacement_media_item_id: Option<EntityId>,
    pub replacement_media_version_id: Option<EntityId>,
    /// Media identity actually used by the published projection.
    pub published_media_item_id: Option<EntityId>,
}

/// Digest-covered, restorable publication snapshot.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListRevisionSnapshot {
    pub list_id: EntityId,
    pub revision: i32,
    pub source_draft_revision: i32,
    pub completed_at: DateTime<Utc>,
    pub name: String,
    pub description: Option<String>,
    pub generate_playlist: bool,
    pub content_kind_override: AddonMediaListContentKind,
    pub presentation_kind: AddonMediaListPresentationKind,
    pub requested_presentation_kind: AddonMediaListPresentationKind,
    /// Canonical effective provider-plus-operator metadata JSON.
    pub metadata_json: String,
    pub options: AddonMediaListPublishedOptions,
    pub items: Vec<AddonMediaListRevisionItem>,
    pub files: Vec<AddonMediaListRevisionFile>,
    pub artifact_ids: Vec<EntityId>,
    pub snapshot_digest: String,
}

/// Bounded history row. A legacy row is visible but cannot be restored.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListRevisionSummary {
    pub revision: i32,
    pub source_draft_revision: Option<i32>,
    pub completed_at: Option<DateTime<Utc>>,
    pub snapshot_digest: Option<String>,
    /// The required snapshot content and digest validated successfully.
    pub restorable: bool,
    pub published_item_count: u64,
    pub file_count: u64,
    pub artifact_count: u64,
}

/// Exact comparison between two digest-validated retained snapshots.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListRevisionDiff {
    pub list_metadata_changed: bool,
    pub effective_options_changed: bool,
    pub items_added: u64,
    pub items_removed: u64,
    pub items_changed: u64,
    pub files_added: u64,
    pub files_removed: u64,
    pub files_changed: u64,
    pub artifacts_added: u64,
    pub artifacts_removed: u64,
}

/// Saved draft compared with the exact current complete publication snapshot.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListDraftDiff {
    pub draft_revision: i32,
    pub published_revision: Option<i32>,
    pub exact: bool,
    pub added: u64,
    pub metadata_changed: u64,
    pub excluded: u64,
    pub unavailable: u64,
    pub conflicted: u64,
    pub removal_candidates: u64,
    pub added_item_ids: Vec<EntityId>,
    pub metadata_changed_item_ids: Vec<EntityId>,
    pub excluded_item_ids: Vec<EntityId>,
    pub unavailable_item_ids: Vec<EntityId>,
    pub conflicted_item_ids: Vec<EntityId>,
    pub removal_candidate_item_ids: Vec<EntityId>,
}

/// Current dependency counts shown before a destructive reset.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListResetImpact {
    pub draft_revision: i32,
    pub published_revision: i32,
    pub published_items: u64,
    pub generated_playlists: u64,
    pub channels: u64,
    pub schedules: u64,
    pub active_or_future_playouts: u64,
}

/// Why a physical artifact or generated file must not be reclaimed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListReferencePins {
    pub retained_revision: bool,
    pub active_work: bool,
    pub published_projection: bool,
    pub active_or_future_playout: bool,
}

/// Exact durable publication claim used to materialize a retained revision as a new draft.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListRevisionRestoreFence {
    pub list_id: EntityId,
    pub source_revision: i32,
    pub target_draft_revision: i32,
    pub operation_id: EntityId,
    pub generation: i32,
    pub owner_token: String,
    pub lease_owner: String,
    pub active_at: DateTime<Utc>,
}

impl AddonMediaListReferencePins {
    #[must_use]
    pub const fn is_reclaimable(self) -> bool {
        !self.retained_revision
            && !self.active_work
            && !self.published_projection
            && !self.active_or_future_playout
    }
}

/// Persisted refresh and per-list processed-artifact policy.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListLifecycleSettings {
    pub cadence: AddonMediaListRefreshCadence,
    pub publication_policy: AddonMediaListRefreshPublicationPolicy,
    pub next_refresh_at: Option<DateTime<Utc>>,
    pub retry_at: Option<DateTime<Utc>>,
    pub last_success_at: Option<DateTime<Utc>>,
    pub processed_artifact_byte_limit: Option<u64>,
}

/// Global processed-artifact admission policy stored as typed configuration keys.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonProcessedArtifactPolicy {
    pub global_byte_limit: Option<u64>,
    pub minimum_free_bytes: Option<u64>,
    pub retention_seconds: Option<u64>,
    pub max_preparation_concurrency: Option<u32>,
}

/// Host-owned promoted checkpoint descriptor; archive contents remain on disk.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListImportCheckpoint {
    pub id: EntityId,
    pub list_id: EntityId,
    pub relative_path: String,
    pub generation: i32,
    pub sha256: String,
    pub entry_count: u64,
    pub promoted_at: DateTime<Utc>,
}

/// K-01 fields appended to the existing resumable import job.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListImportLifecycle {
    pub job_id: EntityId,
    pub purpose: AddonMediaListImportPurpose,
    pub mode: AddonMediaListImportMode,
    pub strategy: AddonMediaListIncrementalStrategy,
    pub examined_count: u64,
    pub archived_skipped_count: u64,
    pub emitted_count: u64,
    pub checkpoint_id: Option<EntityId>,
}

/// Durable reset/reconcile state linked to the shared managed-library operation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListLifecycleJob {
    pub id: EntityId,
    pub operation_id: EntityId,
    pub list_id: EntityId,
    pub generation: i32,
    pub kind: AddonMediaListLifecycleJobKind,
    pub phase: AddonMediaListLifecyclePhase,
    pub saved_draft_revision: i32,
    pub saved_published_revision: i32,
    pub candidate_import_job_id: Option<EntityId>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Provider-neutral admission command for one durable reset or reconcile job.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CreateAddonMediaListLifecycleJob {
    pub operation_id: EntityId,
    pub list_id: EntityId,
    pub generation: i32,
    pub kind: AddonMediaListLifecycleJobKind,
    pub saved_draft_revision: i32,
    pub saved_published_revision: i32,
    pub created_at: DateTime<Utc>,
}

/// Two-observation source-absence evidence for one current item.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListItemTombstone {
    pub item_id: EntityId,
    pub missing_since: DateTime<Utc>,
    pub last_observed_at: DateTime<Utc>,
    pub observation_count: u32,
    pub last_reconcile_generation: i32,
    pub confirmed_at: Option<DateTime<Utc>>,
}

/// Host/list/item-owned artifact metadata. Paths must remain relative to host storage.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonMediaListArtifact {
    pub id: EntityId,
    pub list_id: EntityId,
    pub item_id: Option<EntityId>,
    pub cache_key: String,
    pub kind: AddonMediaListArtifactKind,
    pub state: AddonMediaListArtifactState,
    pub relative_path: Option<String>,
    pub reserved_bytes: u64,
    pub byte_length: Option<u64>,
    pub sha256: Option<String>,
    pub diagnostic_code: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub last_used_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Bounded durable attempt that also serves as the maintenance history row.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonToolMaintenanceAttempt {
    pub id: EntityId,
    pub addon_id: String,
    pub tool_key: String,
    pub trigger: AddonToolMaintenanceTrigger,
    pub state: AddonToolMaintenanceState,
    pub attempt_count: i32,
    pub next_eligible_at: Option<DateTime<Utc>>,
    pub before_version: Option<String>,
    pub before_sha256: Option<String>,
    pub after_version: Option<String>,
    pub after_sha256: Option<String>,
    pub validation: AddonToolMaintenanceValidation,
    pub rollback: AddonToolMaintenanceRollback,
    pub outcome: Option<AddonToolMaintenanceOutcome>,
    pub diagnostic_code: Option<String>,
    pub diagnostic: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Storage-only AMM-K foundation. Later milestones own execution and scheduling.
#[async_trait]
pub trait AddonMediaListLifecycleRepository: Send + Sync {
    async fn lifecycle_settings(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListLifecycleSettings>>;

    async fn save_lifecycle_settings(
        &self,
        list_id: EntityId,
        settings: &AddonMediaListLifecycleSettings,
    ) -> Result<()>;

    async fn processed_artifact_policy(&self) -> Result<AddonProcessedArtifactPolicy>;

    async fn save_processed_artifact_policy(
        &self,
        policy: &AddonProcessedArtifactPolicy,
    ) -> Result<()>;

    /// Insert an immutable schema. Reusing a capability ID with different bytes fails closed.
    async fn register_option_schema(&self, schema: &AddonMediaListOptionSchema) -> Result<()>;

    /// Load and revalidate the exact stored schema definition.
    async fn option_schema(
        &self,
        schema: &AddonMediaListOptionSchemaRef,
    ) -> Result<Option<AddonMediaListOptionSchema>>;

    async fn draft_options(
        &self,
        list_id: EntityId,
        expected_schema: Option<&AddonMediaListOptionSchemaRef>,
    ) -> Result<Option<AddonMediaListDraftOptions>>;

    /// Atomically replace the complete saved draft option snapshot.
    async fn save_draft_options(&self, options: &AddonMediaListDraftOptions) -> Result<i32>;

    /// Retain the current option snapshot without changing publication state.
    async fn retain_published_options(
        &self,
        list_id: EntityId,
        revision: i32,
        source_draft_revision: i32,
        completed_at: DateTime<Utc>,
    ) -> Result<()>;

    async fn published_options(
        &self,
        list_id: EntityId,
        revision: i32,
        expected_schema: Option<&AddonMediaListOptionSchemaRef>,
    ) -> Result<Option<AddonMediaListPublishedOptions>>;

    /// Register host-owned immutable bytes before linking them to a revision.
    async fn register_revision_file_blob(
        &self,
        list_id: EntityId,
        storage_relative_path: &str,
        sha256: &str,
        byte_length: u64,
        created_at: DateTime<Utc>,
    ) -> Result<AddonMediaListRevisionFileBlob>;

    /// Atomically capture the complete current saved draft and publication state.
    async fn retain_published_revision(
        &self,
        list_id: EntityId,
        revision: i32,
        source_draft_revision: i32,
        completed_at: DateTime<Utc>,
        files: &[AddonMediaListRevisionFile],
        artifact_ids: &[EntityId],
    ) -> Result<AddonMediaListRevisionSnapshot>;

    /// List all completed history rows, including truthful non-restorable legacy rows.
    async fn revision_history(
        &self,
        list_id: EntityId,
    ) -> Result<Vec<AddonMediaListRevisionSummary>>;

    /// Load and digest-validate one complete restorable snapshot.
    async fn revision_snapshot(
        &self,
        list_id: EntityId,
        revision: i32,
    ) -> Result<Option<AddonMediaListRevisionSnapshot>>;

    /// Replace the saved draft from a retained snapshot under the normal live publication lease.
    async fn materialize_revision_as_draft(
        &self,
        fence: &AddonMediaListRevisionRestoreFence,
    ) -> Result<i32>;

    async fn compare_revisions(
        &self,
        list_id: EntityId,
        left_revision: i32,
        right_revision: i32,
    ) -> Result<AddonMediaListRevisionDiff>;

    async fn saved_draft_diff(&self, list_id: EntityId) -> Result<AddonMediaListDraftDiff>;

    async fn reset_impact(
        &self,
        list_id: EntityId,
        as_of: DateTime<Utc>,
    ) -> Result<AddonMediaListResetImpact>;

    /// Prune old revision/reference rows only when neither work nor playout pins them.
    async fn prune_unpinned_revisions(
        &self,
        list_id: EntityId,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<i32>>;

    /// Hourly post-operation retention pass across every list.
    async fn prune_all_unpinned_revisions(&self, as_of: DateTime<Utc>) -> Result<u64>;

    async fn artifact_reference_pins(
        &self,
        artifact_id: EntityId,
        as_of: DateTime<Utc>,
    ) -> Result<AddonMediaListReferencePins>;

    async fn revision_file_blob_reference_pins(
        &self,
        blob_id: EntityId,
        as_of: DateTime<Utc>,
    ) -> Result<AddonMediaListReferencePins>;

    async fn import_checkpoint(
        &self,
        list_id: EntityId,
    ) -> Result<Option<AddonMediaListImportCheckpoint>>;

    async fn replace_import_checkpoint(
        &self,
        list_id: EntityId,
        relative_path: &str,
        generation: i32,
        sha256: &str,
        entry_count: u64,
        promoted_at: DateTime<Utc>,
    ) -> Result<AddonMediaListImportCheckpoint>;

    async fn import_lifecycle(
        &self,
        job_id: EntityId,
    ) -> Result<Option<AddonMediaListImportLifecycle>>;

    async fn save_import_lifecycle(
        &self,
        lifecycle: &AddonMediaListImportLifecycle,
        expected_lease_owner: &str,
        active_at: DateTime<Utc>,
    ) -> Result<()>;

    async fn lifecycle_job(
        &self,
        operation_id: EntityId,
    ) -> Result<Option<AddonMediaListLifecycleJob>>;

    async fn create_lifecycle_job(
        &self,
        command: &CreateAddonMediaListLifecycleJob,
    ) -> Result<AddonMediaListLifecycleJob>;

    async fn save_lifecycle_job(
        &self,
        job: &AddonMediaListLifecycleJob,
        expected_updated_at: DateTime<Utc>,
    ) -> Result<()>;

    async fn item_tombstone(
        &self,
        item_id: EntityId,
    ) -> Result<Option<AddonMediaListItemTombstone>>;

    async fn save_item_tombstone(
        &self,
        tombstone: &AddonMediaListItemTombstone,
        expected_generation: Option<i32>,
    ) -> Result<()>;

    async fn confirm_item_tombstone(
        &self,
        item_id: EntityId,
        expected_generation: i32,
        confirmed_at: DateTime<Utc>,
    ) -> Result<()>;

    async fn clear_item_tombstone(&self, item_id: EntityId, expected_generation: i32)
        -> Result<()>;

    async fn artifact(&self, artifact_id: EntityId) -> Result<Option<AddonMediaListArtifact>>;

    async fn create_artifact(
        &self,
        list_id: EntityId,
        item_id: Option<EntityId>,
        cache_key: &str,
        kind: AddonMediaListArtifactKind,
        reserved_bytes: u64,
        created_at: DateTime<Utc>,
    ) -> Result<AddonMediaListArtifact>;

    async fn save_artifact(
        &self,
        artifact: &AddonMediaListArtifact,
        expected_updated_at: DateTime<Utc>,
    ) -> Result<()>;

    async fn revision_artifact_ids(
        &self,
        list_id: EntityId,
        revision: i32,
    ) -> Result<Vec<EntityId>>;

    async fn link_revision_artifact(
        &self,
        list_id: EntityId,
        revision: i32,
        artifact_id: EntityId,
    ) -> Result<()>;

    async fn unlink_revision_artifact(
        &self,
        list_id: EntityId,
        revision: i32,
        artifact_id: EntityId,
    ) -> Result<()>;

    /// Return at most the fixed storage-history cap for one package tool.
    async fn tool_maintenance_history(
        &self,
        addon_id: &str,
        tool_key: &str,
    ) -> Result<Vec<AddonToolMaintenanceAttempt>>;

    async fn create_tool_maintenance_attempt(
        &self,
        addon_id: &str,
        tool_key: &str,
        trigger: AddonToolMaintenanceTrigger,
        created_at: DateTime<Utc>,
    ) -> Result<AddonToolMaintenanceAttempt>;

    async fn save_tool_maintenance_attempt(
        &self,
        attempt: &AddonToolMaintenanceAttempt,
        expected_updated_at: DateTime<Utc>,
    ) -> Result<()>;
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use ersatzrs_domain::AddonMediaListOptionValue;

    use super::{AddonMediaListDraftOptions, AddonMediaListItemOptionValues};

    #[test]
    fn effective_options_apply_item_then_list_then_schema_inheritance() {
        let options = AddonMediaListDraftOptions {
            list_id: 1,
            draft_revision: 2,
            schema: None,
            list_values: BTreeMap::from([
                ("enabled".into(), AddonMediaListOptionValue::Boolean(false)),
                (
                    "profile".into(),
                    AddonMediaListOptionValue::Enum("list".into()),
                ),
            ]),
            item_values: vec![AddonMediaListItemOptionValues {
                item_id: Some(4),
                provider_id: "item-4".into(),
                values: BTreeMap::from([(
                    "profile".into(),
                    AddonMediaListOptionValue::Enum("item".into()),
                )]),
            }],
        };
        let defaults = BTreeMap::from([
            ("enabled".into(), AddonMediaListOptionValue::Boolean(true)),
            (
                "profile".into(),
                AddonMediaListOptionValue::Enum("default".into()),
            ),
        ]);

        assert_eq!(
            options.effective_values(&defaults, 4),
            BTreeMap::from([
                ("enabled".into(), AddonMediaListOptionValue::Boolean(false)),
                (
                    "profile".into(),
                    AddonMediaListOptionValue::Enum("item".into()),
                ),
            ])
        );
    }
}
