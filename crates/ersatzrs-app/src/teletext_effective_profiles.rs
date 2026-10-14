//! Whole-channel observations and unpublished effective-profile invocation receipts.

use std::{fmt, sync::Arc};

use chrono::{DateTime, Utc};
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{Result as RepositoryResult, TeletextProfileMutation},
};
use ersatzrs_domain::{EntityId, TeletextEffectiveChannelSelectionSnapshot};

use crate::{teletext_profiles::TeletextProfileRuntimeError, ApplicationServices};

/// Raw producer output retaining the exact whole-channel observation.
/// Correlation is not generation ownership, native acceptance or publication authority.
pub struct TeletextEffectiveProfileRuntimeReceipt {
    pub stdout: Vec<u8>,
    pub selection: Arc<TeletextEffectiveChannelSelectionSnapshot>,
    pub addon_id: String,
    pub generation: String,
}

impl fmt::Debug for TeletextEffectiveProfileRuntimeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextEffectiveProfileRuntimeReceipt")
            .field("stdout_bytes", &self.stdout.len())
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}

impl ApplicationServices {
    /// Observe base and accepted Scripted origins together at the supplied clock.
    pub async fn observe_effective_channel_teletext_selection(
        &self,
        channel_id: EntityId,
        when: DateTime<Utc>,
    ) -> RepositoryResult<TeletextProfileMutation<TeletextEffectiveChannelSelectionSnapshot>> {
        self.ports()
            .teletext_profiles()
            .effective_channel_snapshot(channel_id, when)
            .await
    }

    /// Recheck the complete captured observation without replacing its target.
    pub async fn recheck_effective_channel_teletext_selection(
        &self,
        expected: &TeletextEffectiveChannelSelectionSnapshot,
        when: DateTime<Utc>,
    ) -> RepositoryResult<TeletextProfileMutation<TeletextEffectiveChannelSelectionSnapshot>> {
        self.ports()
            .teletext_profiles()
            .recheck_effective_channel_snapshot(expected, when)
            .await
    }

    /// Invoke through an explicitly capable adapter under the original admission.
    /// No base fallback, setting mutation or automatic package activation occurs.
    pub async fn invoke_effective_teletext_profile_before_with_hold(
        &self,
        selection: Arc<TeletextEffectiveChannelSelectionSnapshot>,
        addon_id: String,
        input: Vec<u8>,
        deadline: std::time::Instant,
        hold: TeletextWorkHold,
    ) -> Result<TeletextEffectiveProfileRuntimeReceipt, TeletextProfileRuntimeError> {
        self.ports()
            .addons()
            .invoke_effective_teletext_profile_before_with_hold(
                selection, addon_id, input, deadline, hold,
            )
            .await
    }
}
