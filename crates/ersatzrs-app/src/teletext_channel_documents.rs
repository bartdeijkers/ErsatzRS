//! Unpublished channel-wide V1/V2 documents under one selection and deadline.
//!
//! Preparation neither merges services nor lifts, encodes or publishes pages.
//! The host must run synchronous validation off its async workers and recheck
//! the complete channel selection after CPU/native work and before promotion.

use crate::{
    teletext_guide::{
        build_request_for_schema, SuppliedTeletextGuideContext, TeletextGuideRequestError,
    },
    teletext_profile_documents::{
        BoundTeletextDocumentError, BoundTeletextDocumentInput, TeletextProfileDocument,
        TeletextProfileDocumentBudget, TeletextValidatedDocumentSource,
    },
    teletext_profiles::{TeletextProfileRuntimeError, TeletextProfileRuntimeReceipt},
    ApplicationServices,
};
use ersatzrs_addon_contract as wire;
use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_core::repositories::{
    TeletextProfileMutation as Mutation, TeletextProfileRejection, TeletextProfileRepository,
};
use ersatzrs_domain::{
    EntityId, TeletextChannelSelectionSnapshot, TeletextPageContract, TeletextSelectionSnapshot,
};
use std::{
    collections::BTreeSet,
    fmt,
    future::Future,
    time::{Duration, Instant},
};

/// Existing maximum lifetime of a complete channel preparation.
pub const PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);
const MAX_BINDINGS: usize = 1024;

/// Exact preflight request bytes for the contracts selected by one channel.
/// At most two independently capped 4 MiB templates are retained, separately
/// from the response budget. These bytes convey no durable guide authority.
pub struct TeletextChannelRequestTemplates {
    v1: Option<Vec<u8>>,
    v2: Option<Vec<u8>>,
}

impl TeletextChannelRequestTemplates {
    /// Borrow the exact template; unselected contracts and Off return `None`.
    #[must_use]
    pub fn request(&self, contract: TeletextPageContract) -> Option<&[u8]> {
        match contract {
            TeletextPageContract::V1 => self.v1.as_deref(),
            TeletextPageContract::V2 => self.v2.as_deref(),
        }
    }
}

impl fmt::Debug for TeletextChannelRequestTemplates {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextChannelRequestTemplates")
            .field("v1_bytes", &self.v1.as_ref().map(Vec::len))
            .field("v2_bytes", &self.v2.as_ref().map(Vec::len))
            .finish()
    }
}

/// Trusted selected-producer and retained response budgets, independent of wire caps.
/// At most two guide templates additionally obey the existing 4 MiB request cap.
/// This does not bound total process memory or parsed models; validation uses the
/// caller's separate semantic and declaration budgets.
#[derive(Debug, Clone, Copy)]
pub struct TeletextChannelDocumentInputBudget {
    /// Admit the complete selected channel before requests or producer invocation.
    /// The independent static binding ceiling still applies; zero accepts Off.
    pub max_producers: usize,
    pub max_response_bytes: usize,
}

/// Private, complete raw receipts. Validation has not yet interpreted documents.
pub struct BoundChannelTeletextDocumentInput {
    selection: TeletextChannelSelectionSnapshot,
    generation: String,
    receipts: Vec<TeletextProfileRuntimeReceipt>,
    requests: TeletextChannelRequestTemplates,
    entry_addon_id: Option<String>,
    deadline: Instant,
}

impl BoundChannelTeletextDocumentInput {
    /// Observe selected producer count without interpreting the retained receipts.
    #[must_use]
    pub const fn producer_count(&self) -> usize {
        self.selection.bindings.len()
    }

    /// Borrow retained preflight bytes before synchronous document validation.
    #[must_use]
    pub const fn requests(&self) -> &TeletextChannelRequestTemplates {
        &self.requests
    }

    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Validate every original document once, retaining its ordered association.
    /// An empty vector is the explicit fenced Off state. No partial success escapes.
    /// # Errors
    /// Rejects expiry, any selected-contract conversion or trusted semantic budget.
    pub fn validate(
        self,
        budget: TeletextProfileDocumentBudget,
    ) -> Result<PreparedChannelTeletextDocuments, ChannelTeletextDocumentError> {
        check_deadline(Instant::now(), self.deadline)?;
        let mut documents = Vec::with_capacity(self.receipts.len());
        let mut sources = Vec::with_capacity(self.receipts.len());
        for (binding, receipt) in self.receipts.into_iter().enumerate() {
            let prepared = BoundTeletextDocumentInput::from_channel_receipt(receipt, self.deadline)
                .validate(budget)
                .map_err(|error| ChannelTeletextDocumentError::Document {
                    binding,
                    error: Box::new(error),
                })?;
            let (_, _, document, source, _) = prepared.into_parts_with_source();
            documents.push(document);
            sources.push(source);
        }
        check_deadline(Instant::now(), self.deadline)?;
        Ok(PreparedChannelTeletextDocuments {
            selection: self.selection,
            generation: self.generation,
            documents,
            sources,
            requests: self.requests,
            entry_addon_id: self.entry_addon_id,
            deadline: self.deadline,
        })
    }
}

impl fmt::Debug for BoundChannelTeletextDocumentInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundChannelTeletextDocumentInput")
            .field("selection", &self.selection)
            .field("response_count", &self.receipts.len())
            .finish_non_exhaustive()
    }
}

/// Complete immutable original documents; this is not a composed generation.
/// `documents()[i]` belongs exactly to `selection().bindings[i]`.
pub struct PreparedChannelTeletextDocuments {
    selection: TeletextChannelSelectionSnapshot,
    generation: String,
    documents: Vec<TeletextProfileDocument>,
    sources: Vec<TeletextValidatedDocumentSource>,
    requests: TeletextChannelRequestTemplates,
    entry_addon_id: Option<String>,
    deadline: Instant,
}

impl PreparedChannelTeletextDocuments {
    /// Borrow exact request templates retained through validation.
    #[must_use]
    pub const fn requests(&self) -> &TeletextChannelRequestTemplates {
        &self.requests
    }

    /// Move originals, responses and exact requests together into host work.
    /// This association remains unpublished and requires another channel fence.
    #[must_use]
    pub fn into_parts_with_requests(self) -> PreparedChannelTeletextDocumentParts {
        PreparedChannelTeletextDocumentParts {
            selection: self.selection,
            generation: self.generation,
            documents: self.documents,
            sources: self.sources,
            requests: self.requests,
            entry_addon_id: self.entry_addon_id,
            deadline: self.deadline,
        }
    }

    /// Move ordered originals and sources under their complete observed selection.
    /// Both vectors have one element per binding in the original binding order.
    /// This compatibility projection drops request templates.
    #[must_use]
    pub fn into_parts_with_sources(
        self,
    ) -> (
        TeletextChannelSelectionSnapshot,
        String,
        Vec<TeletextProfileDocument>,
        Vec<TeletextValidatedDocumentSource>,
        Option<String>,
        Instant,
    ) {
        (
            self.selection,
            self.generation,
            self.documents,
            self.sources,
            self.entry_addon_id,
            self.deadline,
        )
    }
    #[must_use]
    pub const fn selection(&self) -> &TeletextChannelSelectionSnapshot {
        &self.selection
    }
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[must_use]
    pub fn documents(&self) -> &[TeletextProfileDocument] {
        &self.documents
    }
    /// `sources()[i]` belongs to `documents()[i]` and `selection().bindings[i]`.
    #[must_use]
    pub fn sources(&self) -> &[TeletextValidatedDocumentSource] {
        &self.sources
    }
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.entry_addon_id.as_deref()
    }
    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }
    /// Move the whole association into host work; another channel fence is required.
    /// This compatibility projection drops sources and request templates; use
    /// `into_parts_with_requests` to retain the complete original association.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        TeletextChannelSelectionSnapshot,
        String,
        Vec<TeletextProfileDocument>,
        Option<String>,
        Instant,
    ) {
        (
            self.selection,
            self.generation,
            self.documents,
            self.entry_addon_id,
            self.deadline,
        )
    }
}

/// Owned move projection of validated channel documents and exact wire inputs.
/// Public fields support host transfer, not construction of validated authority.
pub struct PreparedChannelTeletextDocumentParts {
    pub selection: TeletextChannelSelectionSnapshot,
    pub generation: String,
    pub documents: Vec<TeletextProfileDocument>,
    pub sources: Vec<TeletextValidatedDocumentSource>,
    pub requests: TeletextChannelRequestTemplates,
    pub entry_addon_id: Option<String>,
    pub deadline: Instant,
}

impl fmt::Debug for PreparedChannelTeletextDocumentParts {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedChannelTeletextDocumentParts")
            .field("documents", &self.documents.len())
            .field("sources", &self.sources.len())
            .field("requests", &self.requests)
            .finish_non_exhaustive()
    }
}

impl fmt::Debug for PreparedChannelTeletextDocuments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedChannelTeletextDocuments")
            .field("selection", &self.selection)
            .field("entry_addon_id", &self.entry_addon_id)
            .finish_non_exhaustive()
    }
}

/// Static failures retain a numeric binding ordinal, never guide/producer text.
#[derive(Debug, thiserror::Error)]
pub enum ChannelTeletextDocumentError {
    #[error(transparent)]
    Request(#[from] TeletextGuideRequestError),
    #[error("teletext channel selection storage is unavailable")]
    StorageUnavailable,
    #[error("teletext channel selection was rejected: {0:?}")]
    SelectionRejected(TeletextProfileRejection),
    #[error("teletext channel selection changed during preparation")]
    SelectionChanged,
    #[error("teletext channel selection has invalid owner or repeated membership")]
    SelectionShape,
    #[error("teletext channel selection exceeds its binding limit")]
    BindingCount,
    #[error("teletext channel producer count {count} exceeds trusted budget {maximum}")]
    ProducerBudget { count: usize, maximum: usize },
    #[error("teletext channel requires an explicit entry add-on")]
    EntryAddonRequired,
    #[error("teletext channel entry add-on is not selected")]
    EntryAddonNotSelected,
    #[error("teletext channel preparation exceeded its original deadline")]
    TimedOut,
    #[error("teletext binding {binding} runtime: {error}")]
    Runtime {
        binding: usize,
        error: TeletextProfileRuntimeError,
    },
    #[error("teletext binding {binding} receipt does not match selection/correlation")]
    ReceiptMismatch { binding: usize },
    #[error("teletext binding {binding} response exceeds its raw wire cap")]
    ResponseTooLarge { binding: usize },
    #[error("teletext binding {binding} exceeds aggregate response budget {maximum}")]
    AggregateResponseBudget { binding: usize, maximum: usize },
    #[error("teletext binding {binding} document: {error}")]
    Document {
        binding: usize,
        error: Box<BoundTeletextDocumentError>,
    },
}

impl ApplicationServices {
    /// Invoke all enabled contracts serially under one full snapshot and deadline.
    /// # Errors
    /// Rejects invalid selection/requests, producer failures, raw response limits,
    /// receipt mismatch, expiry or changed complete channel membership.
    pub async fn invoke_channel_teletext_documents(
        &self,
        channel_id: EntityId,
        entry_addon_id: Option<&str>,
        context: &SuppliedTeletextGuideContext<'_>,
        budget: TeletextChannelDocumentInputBudget,
    ) -> Result<BoundChannelTeletextDocumentInput, ChannelTeletextDocumentError> {
        self.invoke_channel_teletext_documents_with_optional_hold(
            channel_id,
            entry_addon_id,
            context,
            budget,
            None,
            None,
        )
        .await
    }

    /// Invoke under the host's existing admission without consuming a new slot.
    /// Owned adapters retain cloned holds through actual native cleanup.
    /// # Errors
    /// Preserves original preparation errors and rejects incapable adapters.
    pub async fn invoke_channel_teletext_documents_with_hold(
        &self,
        channel_id: EntityId,
        entry_addon_id: Option<&str>,
        context: &SuppliedTeletextGuideContext<'_>,
        budget: TeletextChannelDocumentInputBudget,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<BoundChannelTeletextDocumentInput, ChannelTeletextDocumentError> {
        self.invoke_channel_teletext_documents_with_optional_hold(
            channel_id,
            entry_addon_id,
            context,
            budget,
            Some(deadline),
            Some(hold),
        )
        .await
    }

    async fn invoke_channel_teletext_documents_with_optional_hold(
        &self,
        channel_id: EntityId,
        entry_addon_id: Option<&str>,
        context: &SuppliedTeletextGuideContext<'_>,
        budget: TeletextChannelDocumentInputBudget,
        deadline: Option<Instant>,
        hold: Option<TeletextWorkHold>,
    ) -> Result<BoundChannelTeletextDocumentInput, ChannelTeletextDocumentError> {
        let invoke = |selected, input, deadline| {
            let hold = hold.clone();
            async move {
                match hold {
                    Some(hold) => {
                        self.ports()
                            .addons()
                            .invoke_teletext_profile_before_with_hold(
                                selected, input, deadline, hold,
                            )
                            .await
                    }
                    None => {
                        self.ports()
                            .addons()
                            .invoke_teletext_profile_before(selected, input, deadline)
                            .await
                    }
                }
            }
        };
        match deadline {
            None => {
                prepare_with_ports(
                    self.ports().teletext_profiles(),
                    channel_id,
                    entry_addon_id,
                    context,
                    budget,
                    invoke,
                    Instant::now,
                )
                .await
            }
            Some(deadline) => {
                prepare_with_ports_before(
                    self.ports().teletext_profiles(),
                    channel_id,
                    entry_addon_id,
                    context,
                    budget,
                    invoke,
                    (Instant::now, Some(deadline)),
                )
                .await
            }
        }
    }

    /// Observe the complete channel again after validation/native work, without
    /// authorizing publication or resetting the original preparation deadline.
    /// # Errors
    /// Rejects expiry, unavailable storage or any changed channel selection.
    pub async fn recheck_channel_teletext_document_selection(
        &self,
        selection: &TeletextChannelSelectionSnapshot,
        deadline: Instant,
    ) -> Result<(), ChannelTeletextDocumentError> {
        recheck_with_ports(
            self.ports().teletext_profiles(),
            selection,
            deadline,
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
    budget: TeletextChannelDocumentInputBudget,
    invoke: F,
    now: Clock,
) -> Result<BoundChannelTeletextDocumentInput, ChannelTeletextDocumentError>
where
    F: FnMut(TeletextSelectionSnapshot, Vec<u8>, Instant) -> Fut,
    Fut: Future<Output = Result<TeletextProfileRuntimeReceipt, TeletextProfileRuntimeError>>,
    Clock: FnMut() -> Instant,
{
    prepare_with_ports_before(
        repository,
        channel_id,
        entry_addon_id,
        context,
        budget,
        invoke,
        (now, None),
    )
    .await
}

async fn prepare_with_ports_before<F, Fut, Clock>(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    budget: TeletextChannelDocumentInputBudget,
    mut invoke: F,
    timing: (Clock, Option<Instant>),
) -> Result<BoundChannelTeletextDocumentInput, ChannelTeletextDocumentError>
where
    F: FnMut(TeletextSelectionSnapshot, Vec<u8>, Instant) -> Fut,
    Fut: Future<Output = Result<TeletextProfileRuntimeReceipt, TeletextProfileRuntimeError>>,
    Clock: FnMut() -> Instant,
{
    let (mut now, supplied_deadline) = timing;
    let started = now();
    let maximum = started
        .checked_add(PREPARATION_TIMEOUT)
        .ok_or(ChannelTeletextDocumentError::TimedOut)?;
    let deadline = supplied_deadline.map_or(maximum, |deadline| maximum.min(deadline));
    if supplied_deadline.is_some() {
        check_deadline(started, deadline)?;
    }
    let selection = snapshot(repository, channel_id).await?;
    check_deadline(now(), deadline)?;
    validate_selection(&selection, channel_id, budget.max_producers)?;
    let entry = select_entry(&selection, entry_addon_id)?;
    let requests = preflight_requests(&selection, context)?;
    check_deadline(now(), deadline)?;
    let mut receipts = Vec::with_capacity(selection.bindings.len());
    let mut remaining = budget.max_response_bytes;
    for (binding, selected) in selection.bindings.iter().enumerate() {
        check_deadline(now(), deadline)?;
        let input = requests
            .request(selected.contract)
            .ok_or(ChannelTeletextDocumentError::SelectionShape)?
            .to_vec();
        let receipt = invoke(selected.clone(), input, deadline)
            .await
            .map_err(|error| ChannelTeletextDocumentError::Runtime { binding, error })?;
        check_deadline(now(), deadline)?;
        if receipt.selection != *selected || receipt.generation != context.generation {
            return Err(ChannelTeletextDocumentError::ReceiptMismatch { binding });
        }
        if receipt.stdout.len() > wire::MAX_TELETEXT_PAGES_OUTPUT_BYTES {
            return Err(ChannelTeletextDocumentError::ResponseTooLarge { binding });
        }
        remaining = remaining.checked_sub(receipt.stdout.len()).ok_or(
            ChannelTeletextDocumentError::AggregateResponseBudget {
                binding,
                maximum: budget.max_response_bytes,
            },
        )?;
        receipts.push(receipt);
    }
    recheck_with_ports(repository, &selection, deadline, &mut now).await?;
    Ok(BoundChannelTeletextDocumentInput {
        selection,
        generation: context.generation.to_owned(),
        receipts,
        requests,
        entry_addon_id: entry,
        deadline,
    })
}

async fn snapshot(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
) -> Result<TeletextChannelSelectionSnapshot, ChannelTeletextDocumentError> {
    match repository
        .channel_invocation_snapshot(channel_id)
        .await
        .map_err(|_| ChannelTeletextDocumentError::StorageUnavailable)?
    {
        Mutation::Saved(selection) => Ok(selection),
        Mutation::Rejected(reason) => Err(ChannelTeletextDocumentError::SelectionRejected(reason)),
    }
}

fn validate_selection(
    selection: &TeletextChannelSelectionSnapshot,
    channel_id: EntityId,
    maximum: usize,
) -> Result<(), ChannelTeletextDocumentError> {
    if selection.bindings.len() > MAX_BINDINGS {
        return Err(ChannelTeletextDocumentError::BindingCount);
    }
    if selection.bindings.len() > maximum {
        return Err(ChannelTeletextDocumentError::ProducerBudget {
            count: selection.bindings.len(),
            maximum,
        });
    }
    let mut seen = BTreeSet::new();
    if selection.channel_id != channel_id
        || selection
            .bindings
            .iter()
            .any(|binding| binding.channel_id != channel_id || !seen.insert(&binding.addon_id))
    {
        return Err(ChannelTeletextDocumentError::SelectionShape);
    }
    Ok(())
}

fn select_entry(
    selection: &TeletextChannelSelectionSnapshot,
    entry: Option<&str>,
) -> Result<Option<String>, ChannelTeletextDocumentError> {
    if selection.bindings.is_empty() {
        return Ok(None);
    }
    let entry = entry.ok_or(ChannelTeletextDocumentError::EntryAddonRequired)?;
    if !selection
        .bindings
        .iter()
        .any(|binding| binding.addon_id == entry)
    {
        return Err(ChannelTeletextDocumentError::EntryAddonNotSelected);
    }
    Ok(Some(entry.to_owned()))
}

pub(super) fn preflight_requests(
    selection: &TeletextChannelSelectionSnapshot,
    context: &SuppliedTeletextGuideContext<'_>,
) -> Result<TeletextChannelRequestTemplates, ChannelTeletextDocumentError> {
    // Off still retains this opaque correlation value, but builds no guide.
    if context.generation.is_empty()
        || context.generation.len() > wire::MAX_TELETEXT_PAGES_GENERATION_BYTES
    {
        return Err(TeletextGuideRequestError::from(
            wire::TeletextPagesRequestError::InvalidGeneration,
        )
        .into());
    }
    let mut requests = TeletextChannelRequestTemplates { v1: None, v2: None };
    for contract in [TeletextPageContract::V1, TeletextPageContract::V2] {
        if selection
            .bindings
            .iter()
            .any(|binding| binding.contract == contract)
        {
            let input = build_request_for_schema(context, contract.schema_version())?;
            match contract {
                TeletextPageContract::V1 => requests.v1 = Some(input),
                TeletextPageContract::V2 => requests.v2 = Some(input),
            }
        }
    }
    Ok(requests)
}

async fn recheck_with_ports<Clock: FnMut() -> Instant>(
    repository: &dyn TeletextProfileRepository,
    selection: &TeletextChannelSelectionSnapshot,
    deadline: Instant,
    mut now: Clock,
) -> Result<(), ChannelTeletextDocumentError> {
    check_deadline(now(), deadline)?;
    let current = repository
        .channel_invocation_snapshot(selection.channel_id)
        .await
        .map_err(|_| ChannelTeletextDocumentError::StorageUnavailable)?;
    check_deadline(now(), deadline)?;
    if current != Mutation::Saved(selection.clone()) {
        return Err(ChannelTeletextDocumentError::SelectionChanged);
    }
    Ok(())
}

fn check_deadline(now: Instant, deadline: Instant) -> Result<(), ChannelTeletextDocumentError> {
    if now >= deadline {
        Err(ChannelTeletextDocumentError::TimedOut)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "teletext_channel_documents_tests.rs"]
mod tests;
