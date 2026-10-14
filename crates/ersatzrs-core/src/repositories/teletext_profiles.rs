//! Narrow transactional configuration port for ADR 0122.

use async_trait::async_trait;
use ersatzrs_domain::{
    ChannelTeletextBinding, ChannelTeletextBindingRead, EntityId, TeletextChannelSelectionSnapshot,
    TeletextInstallationRead, TeletextInstallationSnapshot, TeletextProfile,
    TeletextProfileCandidateSnapshot, TeletextProfileRead, TeletextProfileSetting,
    TeletextSelectionSnapshot,
};

use super::{RepositoryError, Result};

/// Complete override replacement; `existing=None` creates a new named profile.
/// Expected identities never relabel validation performed for another revision.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextProfileSave {
    pub addon_id: String,
    pub name: String,
    pub settings: Vec<TeletextProfileSetting>,
    pub expected_installation: TeletextInstallationSnapshot,
    pub existing: Option<(EntityId, i64)>,
}

/// Validated enable/selection; an absent expected binding revision means create.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ChannelTeletextBindingSave {
    pub channel_id: EntityId,
    pub addon_id: String,
    pub profile_id: EntityId,
    pub enabled: bool,
    pub expected_installation: TeletextInstallationSnapshot,
    pub expected_profile_revision: i64,
    pub expected_binding_revision: Option<i64>,
}

/// Static control rejection, without package/configuration content.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TeletextProfileRejection {
    MissingChannel,
    MissingBinding,
    DisabledBinding,
    BindingLimit,
    MissingProfile,
    WrongAddon,
    DeletedProfile,
    InstallationUnavailable,
    InvalidSettings,
    InvalidValues,
    Conflict,
    NameConflict,
    RevisionLimit,
}

/// Rejected controls mutate no selections, settings or revisions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TeletextProfileMutation<T> {
    Saved(T),
    Rejected(TeletextProfileRejection),
}

/// Persisted profile and number of dependent bindings invalidated atomically.
/// The count avoids materializing every dependent channel in a write result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TeletextProfileSaveResult {
    pub profile: TeletextProfile,
    pub dependent_bindings_changed: u64,
}

/// Repository owns validation and all writes; callers cannot bypass admission
/// with a prepared DTO. This port never installs/enables or invokes a package.
#[async_trait]
pub trait TeletextProfileRepository: Send + Sync {
    /// Observe current package identity and configuration eligibility.
    async fn installation_snapshot(&self, addon_id: &str) -> Result<TeletextInstallationRead>;
    /// Include tombstones and explicit unavailable states, without fallbacks.
    async fn list_profiles(&self, addon_id: &str) -> Result<Vec<TeletextProfileRead>>;
    /// Read base bindings by output channel, including disabled selections.
    async fn channel_bindings(
        &self,
        channel_id: EntityId,
    ) -> Result<Vec<ChannelTeletextBindingRead>>;
    /// Capture an exact enabled base selection and its bounded effective values
    /// in one read transaction. Rejection writes nothing and implies no fallback.
    /// Candidate/base transitions and their publication intervals are not
    /// represented here; a candidate snapshot cannot authorize this invocation.
    async fn invocation_snapshot(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
    ) -> Result<TeletextProfileMutation<TeletextSelectionSnapshot>>;
    /// Capture all enabled base bindings in canonical add-on identity order in
    /// one read transaction. Reuse single-binding validation, reject the whole
    /// snapshot on any invalid enabled member, and never inspect disabled profiles.
    /// The page-record budget bounds membership before resolving effective bags.
    async fn channel_invocation_snapshot(
        &self,
        channel_id: EntityId,
    ) -> Result<TeletextProfileMutation<TeletextChannelSelectionSnapshot>>;
    /// Validate an existing profile for an already-bound channel/add-on in one
    /// read transaction, including a disabled base or a different base profile.
    /// Reuse the enabled-selection installation/settings checks without granting
    /// producer authority, writing a selection or activating a package. This is
    /// profile-selection/On validation; Off does not require a healthy profile.
    async fn candidate_snapshot(
        &self,
        _channel_id: EntityId,
        _addon_id: &str,
        _profile_id: EntityId,
    ) -> Result<TeletextProfileMutation<TeletextProfileCandidateSnapshot>> {
        Err(RepositoryError::unavailable(
            "teletext candidate profile validation",
            "candidate profile validation is unsupported",
        ))
    }
    /// Revalidate channel/binding presence and the candidate's exact profile,
    /// installation, contract and effective settings in one read transaction.
    /// Changes only to the admin base enabled/profile/revision are independent.
    /// Reject a changed candidate rather than granting its replacement authority.
    async fn recheck_candidate_snapshot(
        &self,
        _candidate: &TeletextProfileCandidateSnapshot,
    ) -> Result<TeletextProfileMutation<TeletextProfileCandidateSnapshot>> {
        Err(RepositoryError::unavailable(
            "teletext candidate profile recheck",
            "candidate profile validation is unsupported",
        ))
    }
    /// Replace a complete override bag and invalidate only its dependents.
    async fn save_profile(
        &self,
        save: &TeletextProfileSave,
    ) -> Result<TeletextProfileMutation<TeletextProfileSaveResult>>;
    /// Select an existing same-add-on profile under installation/profile/binding CAS.
    async fn save_binding(
        &self,
        save: &ChannelTeletextBindingSave,
    ) -> Result<TeletextProfileMutation<ChannelTeletextBinding>>;
    /// Keep the selected profile while turning Off, even when it is unavailable.
    async fn disable_binding(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
    ) -> Result<TeletextProfileMutation<ChannelTeletextBinding>>;
    /// Retain settings/selections and atomically invalidate dependent revisions.
    async fn tombstone_profile(
        &self,
        addon_id: &str,
        profile_id: EntityId,
        expected_revision: i64,
    ) -> Result<TeletextProfileMutation<TeletextProfileSaveResult>>;
}
