//! Unpublished channel-wide composition of selected teletext producers (ADR 0122).
//!
//! One supplied correlation value and one deadline cover the complete preparation.
//! Promotion still needs encoding and authoritative generation/selection fencing.

use std::{
    collections::BTreeSet,
    fmt,
    future::Future,
    time::{Duration, Instant},
};

use ersatzrs_core::repositories::{
    TeletextProfileMutation as Mutation, TeletextProfileRejection, TeletextProfileRepository,
};
use ersatzrs_domain::{
    teletext::{TeletextPageSetDraft, TeletextStructureError, MAX_TELETEXT_PAGE_RECORDS},
    EntityId, TeletextChannelSelectionSnapshot, TeletextSelectionSnapshot,
};

use crate::{
    teletext_guide::{build_request, SuppliedTeletextGuideContext, TeletextGuideRequestError},
    teletext_pages::{parse_page_set, TeletextPagesError},
    teletext_profiles::{TeletextProfileRuntimeError, TeletextProfileRuntimeReceipt},
    ApplicationServices,
};

const PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Complete unpublished candidate. `None` draft is an explicitly fenced Off state.
/// Private fields retain the association; no member grants publication authority.
pub struct PreparedChannelTeletextPages {
    selection: TeletextChannelSelectionSnapshot,
    generation: String,
    draft: Option<TeletextPageSetDraft>,
    entry_addon_id: Option<String>,
    deadline: Instant,
}

impl PreparedChannelTeletextPages {
    /// Exact enabled membership and identities captured for this candidate.
    #[must_use]
    pub const fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        &self.selection
    }
    /// Supplied opaque correlation; this is not a generation owner.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    /// Borrow the complete unpublished aggregate, or the explicit Off state.
    #[must_use]
    pub const fn draft(&self) -> Option<&TeletextPageSetDraft> {
        self.draft.as_ref()
    }
    /// Original budget metadata for subsequent host preparation work.
    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }
    /// Producer supplying service language and initial page, absent when Off.
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.entry_addon_id.as_deref()
    }
}

impl fmt::Debug for PreparedChannelTeletextPages {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedChannelTeletextPages")
            .field("selection", &self.selection)
            .field("entry_addon_id", &self.entry_addon_id)
            .finish_non_exhaustive()
    }
}

/// Static errors without producer payload, guide text or secret locations.
#[derive(Debug, thiserror::Error)]
pub enum ChannelTeletextPagesError {
    #[error(transparent)]
    Request(#[from] TeletextGuideRequestError),
    #[error("teletext channel selection storage is unavailable")]
    StorageUnavailable,
    #[error("teletext channel selection was rejected: {0:?}")]
    SelectionRejected(TeletextProfileRejection),
    #[error("teletext channel selection changed during preparation")]
    SelectionChanged,
    #[error("teletext channel requires an explicit entry add-on")]
    EntryAddonRequired,
    #[error("teletext channel entry add-on is not selected")]
    EntryAddonNotSelected,
    #[error("teletext channel selection exceeds its binding limit")]
    BindingCount,
    #[error("selected teletext contract requires authored channel composition")]
    EnhancedCompositionRequired,
    #[error("teletext channel preparation exceeded its shared deadline")]
    TimedOut,
    #[error(transparent)]
    Runtime(#[from] TeletextProfileRuntimeError),
    #[error("teletext producer receipt does not match the requested selection")]
    ReceiptMismatch,
    #[error(transparent)]
    Response(#[from] TeletextPagesError),
    #[error("teletext channel aggregate exceeds its page record limit")]
    RecordCount,
    #[error("teletext channel producers have colliding page addresses")]
    AddressCollision,
    #[error(transparent)]
    Structure(#[from] TeletextStructureError),
}

impl ApplicationServices {
    /// Prepare every enabled base binding serially under one deadline, without
    /// altering profiles, selections or publication state. Guide channel IDs
    /// describe sources and mirrors; `channel_id` owns the output selection.
    pub async fn prepare_channel_teletext_pages(
        &self,
        channel_id: EntityId,
        entry_addon_id: Option<&str>,
        context: &SuppliedTeletextGuideContext<'_>,
    ) -> Result<PreparedChannelTeletextPages, ChannelTeletextPagesError> {
        prepare_with_ports(
            self.ports().teletext_profiles(),
            channel_id,
            entry_addon_id,
            context,
            |selection, input, deadline| {
                self.ports()
                    .addons()
                    .invoke_teletext_profile_before(selection, input, deadline)
            },
            Instant::now,
        )
        .await
    }
}

async fn prepare_with_ports<F, Fut, Clock>(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    mut invoke: F,
    mut now: Clock,
) -> Result<PreparedChannelTeletextPages, ChannelTeletextPagesError>
where
    F: FnMut(TeletextSelectionSnapshot, Vec<u8>, Instant) -> Fut,
    Fut: Future<Output = Result<TeletextProfileRuntimeReceipt, TeletextProfileRuntimeError>>,
    Clock: FnMut() -> Instant,
{
    let deadline = now()
        .checked_add(PREPARATION_TIMEOUT)
        .ok_or(ChannelTeletextPagesError::TimedOut)?;
    let input = build_request(context)?;
    check_deadline(now(), deadline)?;
    let selection = match repository
        .channel_invocation_snapshot(channel_id)
        .await
        .map_err(|_| ChannelTeletextPagesError::StorageUnavailable)?
    {
        Mutation::Saved(selection) => selection,
        Mutation::Rejected(reason) => {
            return Err(ChannelTeletextPagesError::SelectionRejected(reason))
        }
    };
    check_deadline(now(), deadline)?;
    if selection.bindings.len() > MAX_TELETEXT_PAGE_RECORDS {
        return Err(ChannelTeletextPagesError::BindingCount);
    }
    if selection
        .bindings
        .iter()
        .any(|binding| binding.contract != ersatzrs_domain::TeletextPageContract::V1)
    {
        return Err(ChannelTeletextPagesError::EnhancedCompositionRequired);
    }
    let entry = select_entry(&selection, entry_addon_id)?;
    let mut aggregate = None;
    let mut pages = Vec::new();
    let mut addresses = BTreeSet::new();
    for binding in &selection.bindings {
        check_deadline(now(), deadline)?;
        let receipt = invoke(binding.clone(), input.clone(), deadline).await?;
        check_deadline(now(), deadline)?;
        if receipt.selection != *binding || receipt.generation != context.generation {
            return Err(ChannelTeletextPagesError::ReceiptMismatch);
        }
        let draft = parse_page_set(&receipt.stdout, context.generation)?;
        check_deadline(now(), deadline)?;
        if draft.pages.len() > MAX_TELETEXT_PAGE_RECORDS - pages.len() {
            return Err(ChannelTeletextPagesError::RecordCount);
        }
        if draft
            .pages
            .iter()
            .any(|page| !addresses.insert(page.address))
        {
            return Err(ChannelTeletextPagesError::AddressCollision);
        }
        if entry == Some(binding.addon_id.as_str()) {
            aggregate = Some((draft.language, draft.initial_page));
        }
        pages.extend(draft.pages);
    }
    check_deadline(now(), deadline)?;
    let current = repository
        .channel_invocation_snapshot(channel_id)
        .await
        .map_err(|_| ChannelTeletextPagesError::StorageUnavailable)?;
    check_deadline(now(), deadline)?;
    if current != Mutation::Saved(selection.clone()) {
        return Err(ChannelTeletextPagesError::SelectionChanged);
    }
    let draft = aggregate.map(|(language, initial_page)| TeletextPageSetDraft {
        language,
        initial_page,
        pages,
    });
    if let Some(draft) = &draft {
        draft.validate_structure()?;
    }
    check_deadline(now(), deadline)?;
    let entry_addon_id = entry.map(str::to_owned);
    Ok(PreparedChannelTeletextPages {
        selection,
        generation: context.generation.to_owned(),
        draft,
        entry_addon_id,
        deadline,
    })
}

fn select_entry<'a>(
    selection: &TeletextChannelSelectionSnapshot,
    entry: Option<&'a str>,
) -> Result<Option<&'a str>, ChannelTeletextPagesError> {
    if selection.bindings.is_empty() {
        return Ok(None);
    }
    let entry = entry.ok_or(ChannelTeletextPagesError::EntryAddonRequired)?;
    if !selection
        .bindings
        .iter()
        .any(|binding| binding.addon_id == entry)
    {
        return Err(ChannelTeletextPagesError::EntryAddonNotSelected);
    }
    Ok(Some(entry))
}

fn check_deadline(now: Instant, deadline: Instant) -> Result<(), ChannelTeletextPagesError> {
    if now >= deadline {
        Err(ChannelTeletextPagesError::TimedOut)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "teletext_channels_tests.rs"]
mod tests;
