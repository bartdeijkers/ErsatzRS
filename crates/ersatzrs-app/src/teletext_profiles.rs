//! ADR 0122 profile controls and unpublished, selection-bound preparation.
//!
//! Supplied generations are correlation values, not trusted generation owners.
//! Encoding and an authoritative selection recheck at promotion remain required.

use std::{fmt, future::Future};

use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, Result as RepositoryResult, TeletextProfileMutation as Mutation,
    TeletextProfileRejection, TeletextProfileRepository, TeletextProfileSave,
    TeletextProfileSaveResult,
};
use ersatzrs_domain::{
    teletext::TeletextPageSetDraft, ChannelTeletextBinding, ChannelTeletextBindingRead, EntityId,
    TeletextInstallationRead, TeletextProfileRead, TeletextSelectionSnapshot,
};

use crate::{
    teletext_guide::{build_request, SuppliedTeletextGuideContext, TeletextGuideRequestError},
    teletext_pages::{parse_page_set, TeletextPagesError},
    ApplicationServices,
};

/// Static runtime failures; cleanup uncertainty is never an ordinary rejection.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum TeletextProfileRuntimeError {
    #[error("teletext profile invocation is unsupported")]
    Unsupported,
    #[error("teletext profile invocation request is invalid")]
    InvalidRequest,
    #[error("teletext profile invocation admission was rejected")]
    AdmissionRejected,
    #[error("teletext profile invocation was cancelled")]
    Cancelled,
    #[error("teletext profile invocation exceeded its deadline")]
    TimedOut,
    #[error("teletext profile invocation cleanup could not be confirmed")]
    CleanupUnconfirmed,
    #[error("teletext profile command could not be started")]
    SpawnFailed,
    #[error("teletext profile command pipe or wait failed")]
    IoFailed,
    #[error("teletext profile output exceeded its byte limit")]
    OutputTooLarge,
    #[error("teletext profile diagnostics exceeded their byte limit")]
    StderrTooLarge,
    #[error("teletext profile command exited unsuccessfully")]
    Failed,
}

/// Raw producer output, exact invoked selection and correlation echo.
pub struct TeletextProfileRuntimeReceipt {
    pub stdout: Vec<u8>,
    pub selection: TeletextSelectionSnapshot,
    pub generation: String,
}

impl fmt::Debug for TeletextProfileRuntimeReceipt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextProfileRuntimeReceipt")
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}

/// Structurally validated draft; it has not been encoded or published.
pub struct PreparedBoundTeletextPages {
    pub draft: TeletextPageSetDraft,
    pub selection: TeletextSelectionSnapshot,
    pub generation: String,
}

impl fmt::Debug for PreparedBoundTeletextPages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedBoundTeletextPages")
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}

/// Static preparation failures without producer/configuration diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum BoundTeletextPagesError {
    #[error(transparent)]
    Request(#[from] TeletextGuideRequestError),
    #[error("teletext profile storage is unavailable")]
    StorageUnavailable,
    #[error("teletext profile selection was rejected: {0:?}")]
    SelectionRejected(TeletextProfileRejection),
    #[error(transparent)]
    Runtime(#[from] TeletextProfileRuntimeError),
    #[error("teletext producer receipt does not match the requested selection")]
    ReceiptMismatch,
    #[error("teletext selection changed during preparation")]
    SelectionChanged,
    #[error(transparent)]
    Response(#[from] TeletextPagesError),
}

impl ApplicationServices {
    /// Observe configuration eligibility without invocation or activation.
    pub async fn teletext_installation_snapshot(
        &self,
        addon_id: &str,
    ) -> RepositoryResult<TeletextInstallationRead> {
        self.ports()
            .teletext_profiles()
            .installation_snapshot(addon_id)
            .await
    }
    /// Include retained tombstones and explicit unavailable states.
    pub async fn list_teletext_profiles(
        &self,
        addon_id: &str,
    ) -> RepositoryResult<Vec<TeletextProfileRead>> {
        self.ports()
            .teletext_profiles()
            .list_profiles(addon_id)
            .await
    }
    /// Read the requested output channel's retained base selections.
    pub async fn channel_teletext_bindings(
        &self,
        channel_id: EntityId,
    ) -> RepositoryResult<Vec<ChannelTeletextBindingRead>> {
        self.ports()
            .teletext_profiles()
            .channel_bindings(channel_id)
            .await
    }
    /// Replace overrides under the repository's transaction and CAS policy.
    pub async fn save_teletext_profile(
        &self,
        save: &TeletextProfileSave,
    ) -> RepositoryResult<Mutation<TeletextProfileSaveResult>> {
        self.ports().teletext_profiles().save_profile(save).await
    }
    /// Select a same-add-on profile without activating the installed package.
    pub async fn save_channel_teletext_binding(
        &self,
        save: &ChannelTeletextBindingSave,
    ) -> RepositoryResult<Mutation<ChannelTeletextBinding>> {
        self.ports().teletext_profiles().save_binding(save).await
    }
    /// Turn Off even an unavailable selection, retaining its profile identity.
    pub async fn disable_channel_teletext_binding(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_revision: i64,
    ) -> RepositoryResult<Mutation<ChannelTeletextBinding>> {
        self.ports()
            .teletext_profiles()
            .disable_binding(channel_id, addon_id, expected_revision)
            .await
    }
    /// Retain deleted configuration and invalidate exactly its dependents.
    pub async fn tombstone_teletext_profile(
        &self,
        addon_id: &str,
        profile_id: EntityId,
        expected_revision: i64,
    ) -> RepositoryResult<Mutation<TeletextProfileSaveResult>> {
        self.ports()
            .teletext_profiles()
            .tombstone_profile(addon_id, profile_id, expected_revision)
            .await
    }
    /// Prepare one unpublished set for the explicit output binding. Guide channels
    /// may contain other sources and mirrors; they grant no target ownership.
    pub async fn prepare_bound_teletext_pages(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
        context: &SuppliedTeletextGuideContext<'_>,
    ) -> Result<PreparedBoundTeletextPages, BoundTeletextPagesError> {
        prepare_with_ports(
            self.ports().teletext_profiles(),
            channel_id,
            addon_id,
            expected_binding_revision,
            context,
            |selection, input| {
                self.ports()
                    .addons()
                    .invoke_teletext_profile(selection, input)
            },
        )
        .await
    }
}

async fn prepare_with_ports<F, Fut>(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    addon_id: &str,
    expected_revision: i64,
    context: &SuppliedTeletextGuideContext<'_>,
    invoke: F,
) -> Result<PreparedBoundTeletextPages, BoundTeletextPagesError>
where
    F: FnOnce(TeletextSelectionSnapshot, Vec<u8>) -> Fut,
    Fut: Future<Output = Result<TeletextProfileRuntimeReceipt, TeletextProfileRuntimeError>>,
{
    let input = build_request(context)?;
    let selection = match repository
        .invocation_snapshot(channel_id, addon_id, expected_revision)
        .await
        .map_err(|_| BoundTeletextPagesError::StorageUnavailable)?
    {
        Mutation::Saved(selection) => selection,
        Mutation::Rejected(reason) => {
            return Err(BoundTeletextPagesError::SelectionRejected(reason))
        }
    };
    let receipt = invoke(selection.clone(), input).await?;
    if receipt.selection != selection || receipt.generation != context.generation {
        return Err(BoundTeletextPagesError::ReceiptMismatch);
    }
    let current = repository
        .invocation_snapshot(channel_id, addon_id, expected_revision)
        .await
        .map_err(|_| BoundTeletextPagesError::StorageUnavailable)?;
    if current != Mutation::Saved(selection.clone()) {
        return Err(BoundTeletextPagesError::SelectionChanged);
    }
    let draft = parse_page_set(&receipt.stdout, context.generation)?;
    Ok(PreparedBoundTeletextPages {
        draft,
        selection,
        generation: receipt.generation,
    })
}

#[cfg(test)]
#[path = "teletext_profiles_tests.rs"]
mod tests;
