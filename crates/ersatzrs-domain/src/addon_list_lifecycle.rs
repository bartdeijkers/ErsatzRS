use serde::{Deserialize, Serialize};

/// Calendar cadence for provider refreshes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListRefreshCadence {
    Daily,
    Weekly,
    Monthly,
    Yearly,
    #[default]
    Never,
}

/// Whether a successful refresh always enters review or may publish pure additions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListRefreshPublicationPolicy {
    #[default]
    Review,
    AutoPublishAdditions,
}

/// Provider-neutral option primitives supported by the first option contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AddonMediaListOptionValue {
    Boolean(bool),
    Enum(String),
}

/// Why an overview import generation exists.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListImportPurpose {
    #[default]
    Refresh,
    Reconcile,
    ResetReload,
}

/// Whether discovery reads the complete source or a provider delta.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListImportMode {
    #[default]
    Full,
    Incremental,
}

/// Provider-declared strategy used by one incremental import.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListIncrementalStrategy {
    #[default]
    None,
    SkipKnown,
    NewestFirstEarlyStop,
}

/// Destructive reset/reload or non-destructive source reconciliation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListLifecycleJobKind {
    ResetReload,
    Reconcile,
}

/// Durable phase vocabulary shared by the two list-lifecycle jobs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListLifecyclePhase {
    #[default]
    AwaitingLease,
    Quiescing,
    UnpublishingFiles,
    UnpublishingProjection,
    ClearingProviderState,
    ClearingCheckpoint,
    VerifyingEmpty,
    QueueingInitialImport,
    EnumeratingSource,
    ComparingCandidate,
    Complete,
}

/// Host-owned artifact categories; provider identities never select behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListArtifactKind {
    Subtitle,
    ProcessedMedia,
}

/// Durable ownership state for one staged or reusable artifact.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonMediaListArtifactState {
    #[default]
    Reserved,
    Staged,
    Ready,
    Failed,
}

/// Source of one required tool-maintenance attempt.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonToolMaintenanceTrigger {
    Startup,
    Daily,
    Manual,
}

/// Durable queue state for a tool-maintenance attempt.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonToolMaintenanceState {
    #[default]
    Queued,
    Running,
    WaitingToRetry,
    Completed,
    Failed,
}

/// Result of validating a candidate tool after maintenance.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonToolMaintenanceValidation {
    #[default]
    NotRun,
    Passed,
    Failed,
}

/// Result of restoring the last-known-good tool bytes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonToolMaintenanceRollback {
    #[default]
    NotRequired,
    Succeeded,
    Failed,
}

/// Typed terminal result retained without provider output.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonToolMaintenanceOutcome {
    Updated,
    AlreadyCurrent,
    ManualUpdateRequired,
    RolledBack,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::{AddonMediaListOptionValue, AddonMediaListRefreshCadence};

    #[test]
    fn lifecycle_values_keep_provider_neutral_wire_names() {
        assert_eq!(
            serde_json::to_string(&AddonMediaListRefreshCadence::Monthly).unwrap(),
            "\"monthly\""
        );
        assert_eq!(
            serde_json::to_string(&AddonMediaListOptionValue::Boolean(true)).unwrap(),
            "true"
        );
        assert_eq!(
            serde_json::to_string(&AddonMediaListOptionValue::Enum("choice".into())).unwrap(),
            "\"choice\""
        );
    }
}
