//! Async application boundary bridged by the existing synchronous source resolver.

use async_trait::async_trait;
use ersatzrs_domain::{EntityId, TeletextProfileCandidateSnapshot, TeletextScriptedBaseSelection};

use crate::repositories::{Result, TeletextProfileMutation};

/// Read-only application controls for Scripted candidate generation. Implementors
/// share admin validation but never install, activate, invoke or publish packages.
#[async_trait]
pub trait ScriptedTeletextControl: Send + Sync {
    /// Resolve bound membership/base state without profile/package validation,
    /// so an explicit Off remains available for an unhealthy selection.
    ///
    /// # Errors
    /// Returns a storage failure; unavailable or missing targets remain explicit.
    async fn teletext_base(
        &self,
        channel_id: EntityId,
        addon_id: &str,
    ) -> Result<TeletextProfileMutation<TeletextScriptedBaseSelection>>;

    /// Validate an existing profile for this already-bound output channel/add-on.
    /// Returned observations cannot authorize ordinary base producer invocation.
    ///
    /// # Errors
    /// Returns a storage failure rather than substituting another profile.
    async fn teletext_candidate(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        profile_id: EntityId,
    ) -> Result<TeletextProfileMutation<TeletextProfileCandidateSnapshot>>;
}
