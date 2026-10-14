//! Read-only Lua controls through the shared application configuration boundary.
//! Candidate validation grants no invocation, base mutation or publication authority.

use async_trait::async_trait;
use ersatzrs_core::{
    repositories::{Result, TeletextProfileMutation as Mutation},
    scheduling::ScriptedTeletextControl,
};
use ersatzrs_domain::{EntityId, TeletextProfileCandidateSnapshot, TeletextScriptedBaseSelection};

use crate::ApplicationServices;

impl ApplicationServices {
    /// Observe only channel/add-on membership and the retained base choice.
    /// Off remains available when the package or selected profile is unhealthy.
    /// # Errors
    /// Preserves repository errors and typed channel/membership rejections.
    pub async fn scripted_channel_teletext_base_selection(
        &self,
        channel_id: EntityId,
        addon_id: &str,
    ) -> Result<Mutation<TeletextScriptedBaseSelection>> {
        self.ports()
            .teletext_profiles()
            .scripted_base_selection(channel_id, addon_id)
            .await
    }
}

#[async_trait]
impl ScriptedTeletextControl for ApplicationServices {
    async fn teletext_base(
        &self,
        channel_id: EntityId,
        addon_id: &str,
    ) -> Result<Mutation<TeletextScriptedBaseSelection>> {
        self.scripted_channel_teletext_base_selection(channel_id, addon_id)
            .await
    }

    async fn teletext_candidate(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        profile_id: EntityId,
    ) -> Result<Mutation<TeletextProfileCandidateSnapshot>> {
        let candidate = match self
            .teletext_profile_candidate_snapshot(channel_id, addon_id, profile_id)
            .await?
        {
            Mutation::Saved(candidate) => candidate,
            Mutation::Rejected(reason) => return Ok(Mutation::Rejected(reason)),
        };
        self.recheck_teletext_profile_candidate_snapshot(&candidate)
            .await
    }
}
