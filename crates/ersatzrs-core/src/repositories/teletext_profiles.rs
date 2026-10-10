//! Narrow transactional configuration port for ADR 0122.

use async_trait::async_trait;
use ersatzrs_domain::{
    ChannelTeletextBinding, ChannelTeletextBindingRead, EntityId, TeletextInstallationRead,
    TeletextInstallationSnapshot, TeletextProfile, TeletextProfileRead, TeletextProfileSetting,
    TeletextSelectionSnapshot,
};

use super::Result;

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
    /// Future Lua candidate validation must reuse this policy; candidate/base
    /// transitions and their publication intervals are not represented here.
    async fn invocation_snapshot(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
    ) -> Result<TeletextProfileMutation<TeletextSelectionSnapshot>>;
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
