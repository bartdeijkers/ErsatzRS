//! Unpublished documents for a complete base/published Scripted observation.
//!
//! Active Off members remain in the fence without running a producer. Content
//! validation uses the same selected-contract parser as base preparation; it
//! never converts a Scripted origin into a base binding or publishes pages.

use std::{fmt, future::Future, sync::Arc, time::Instant};

use chrono::Utc;
use ersatzrs_addon_contract as wire;
use ersatzrs_core::{
    locking::TeletextWorkHold,
    repositories::{TeletextProfileMutation as Mutation, TeletextProfileRepository},
};
use ersatzrs_domain::{
    EntityId, TeletextEffectiveBindingSnapshot as Binding,
    TeletextEffectiveChannelSelectionSnapshot as Selection, TeletextOverrideState,
    TeletextPageContract,
};

use crate::{
    teletext_channel_documents::{
        preflight_contracts, ChannelTeletextDocumentError as Error,
        TeletextChannelDocumentInputBudget, TeletextChannelRequestTemplates, PREPARATION_TIMEOUT,
    },
    teletext_effective_profiles::TeletextEffectiveProfileRuntimeReceipt,
    teletext_guide::SuppliedTeletextGuideContext,
    teletext_profile_documents::{
        validate_document_response, TeletextProfileDocument, TeletextProfileDocumentBudget,
        TeletextValidatedDocumentSource,
    },
    teletext_profiles::TeletextProfileRuntimeError,
    ApplicationServices,
};

/// Complete fenced raw output, still uninterpreted and unpublished.
pub struct BoundEffectiveChannelTeletextDocumentInput {
    selection: Arc<Selection>,
    generation: String,
    receipts: Vec<(usize, TeletextEffectiveProfileRuntimeReceipt)>,
    requests: TeletextChannelRequestTemplates,
    entry_addon_id: Option<String>,
    deadline: Instant,
}

impl BoundEffectiveChannelTeletextDocumentInput {
    #[must_use]
    pub fn selection(&self) -> &Selection {
        &self.selection
    }

    #[must_use]
    pub const fn producer_count(&self) -> usize {
        self.receipts.len()
    }

    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }

    #[must_use]
    pub const fn requests(&self) -> &TeletextChannelRequestTemplates {
        &self.requests
    }

    /// Run CPU validation off async workers, retaining their actual work hold.
    /// No partial result escapes; the host must recheck after validation/encoding.
    /// # Errors
    /// Rejects expiry or any selected-contract content or semantic-budget failure.
    pub fn validate(
        self,
        budget: TeletextProfileDocumentBudget,
    ) -> Result<PreparedEffectiveChannelTeletextDocuments, Error> {
        check_deadline(self.deadline)?;
        let mut producer_bindings = Vec::with_capacity(self.receipts.len());
        let mut documents = Vec::with_capacity(self.receipts.len());
        let mut sources = Vec::with_capacity(self.receipts.len());
        for (binding, receipt) in self.receipts {
            let selected_contract =
                contract(&self.selection.bindings[binding]).ok_or(Error::SelectionShape)?;
            let (document, source) = validate_document_response(
                selected_contract,
                &receipt.generation,
                receipt.stdout,
                budget,
                self.deadline,
            )
            .map_err(|error| Error::Document {
                binding,
                error: Box::new(error),
            })?;
            producer_bindings.push(binding);
            documents.push(document);
            sources.push(source);
        }
        check_deadline(self.deadline)?;
        Ok(PreparedEffectiveChannelTeletextDocuments {
            selection: self.selection,
            generation: self.generation,
            producer_bindings,
            documents,
            sources,
            requests: self.requests,
            entry_addon_id: self.entry_addon_id,
            deadline: self.deadline,
        })
    }
}

impl fmt::Debug for BoundEffectiveChannelTeletextDocumentInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundEffectiveChannelTeletextDocumentInput")
            .field("selection", &self.selection)
            .field("producer_count", &self.receipts.len())
            .finish_non_exhaustive()
    }
}

/// Validated originals with explicit producer-to-whole-membership association.
/// Scripted Off has no document but remains part of `selection` at every fence.
/// This move projection grants no encoding or publication authority.
pub struct PreparedEffectiveChannelTeletextDocuments {
    selection: Arc<Selection>,
    generation: String,
    producer_bindings: Vec<usize>,
    documents: Vec<TeletextProfileDocument>,
    sources: Vec<TeletextValidatedDocumentSource>,
    requests: TeletextChannelRequestTemplates,
    entry_addon_id: Option<String>,
    deadline: Instant,
}

impl PreparedEffectiveChannelTeletextDocuments {
    #[must_use]
    pub fn selection(&self) -> &Selection {
        &self.selection
    }
    #[must_use]
    pub fn producer_bindings(&self) -> &[usize] {
        &self.producer_bindings
    }
    #[must_use]
    pub fn documents(&self) -> &[TeletextProfileDocument] {
        &self.documents
    }
    #[must_use]
    pub fn sources(&self) -> &[TeletextValidatedDocumentSource] {
        &self.sources
    }
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }
    #[must_use]
    pub const fn requests(&self) -> &TeletextChannelRequestTemplates {
        &self.requests
    }
    #[must_use]
    pub fn entry_addon_id(&self) -> Option<&str> {
        self.entry_addon_id.as_deref()
    }
    /// Move the complete association into host work without granting authority.
    #[must_use]
    pub fn into_parts(self) -> PreparedEffectiveChannelTeletextDocumentParts {
        PreparedEffectiveChannelTeletextDocumentParts {
            selection: self.selection,
            generation: self.generation,
            producer_bindings: self.producer_bindings,
            documents: self.documents,
            sources: self.sources,
            requests: self.requests,
            entry_addon_id: self.entry_addon_id,
            deadline: self.deadline,
        }
    }
}

/// Move projection for host composition; fields do not authorize publication.
pub struct PreparedEffectiveChannelTeletextDocumentParts {
    pub selection: Arc<Selection>,
    pub generation: String,
    pub producer_bindings: Vec<usize>,
    pub documents: Vec<TeletextProfileDocument>,
    pub sources: Vec<TeletextValidatedDocumentSource>,
    pub requests: TeletextChannelRequestTemplates,
    pub entry_addon_id: Option<String>,
    pub deadline: Instant,
}

impl fmt::Debug for PreparedEffectiveChannelTeletextDocuments {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedEffectiveChannelTeletextDocuments")
            .field("selection", &self.selection)
            .field("producer_bindings", &self.producer_bindings)
            .finish_non_exhaustive()
    }
}

impl ApplicationServices {
    /// Prepare all effective producers under the original admission and deadline.
    /// No profile mutation, package activation or generation publication occurs.
    /// # Errors
    /// Rejects invalid whole membership, expiry, runtime/content framing errors,
    /// budgets, receipt mismatch or any change to the whole observed selection.
    pub async fn invoke_effective_channel_teletext_documents_with_hold(
        &self,
        channel_id: EntityId,
        entry_addon_id: Option<&str>,
        context: &SuppliedTeletextGuideContext<'_>,
        budget: TeletextChannelDocumentInputBudget,
        admission: (Instant, TeletextWorkHold),
    ) -> Result<BoundEffectiveChannelTeletextDocumentInput, Error> {
        self.invoke_effective_channel_teletext_documents_with_optional_expectation(
            channel_id,
            entry_addon_id,
            context,
            budget,
            (admission.0, admission.1, None),
        )
        .await
    }

    /// Invoke only if the fresh complete scope still equals the observed scope.
    /// The expectation includes Off members and Scripted ownership/intervals;
    /// comparison precedes contract preflight and every producer invocation.
    /// # Errors
    /// Rejects changed scope before invocation, and retains all ordinary
    /// membership, deadline, runtime, content and final selection fences.
    pub async fn invoke_effective_channel_teletext_documents_with_expected_hold(
        &self,
        channel_id: EntityId,
        entry_addon_id: Option<&str>,
        context: &SuppliedTeletextGuideContext<'_>,
        budget: TeletextChannelDocumentInputBudget,
        admission: (Instant, TeletextWorkHold, &Selection),
    ) -> Result<BoundEffectiveChannelTeletextDocumentInput, Error> {
        self.invoke_effective_channel_teletext_documents_with_optional_expectation(
            channel_id,
            entry_addon_id,
            context,
            budget,
            (admission.0, admission.1, Some(admission.2)),
        )
        .await
    }

    async fn invoke_effective_channel_teletext_documents_with_optional_expectation(
        &self,
        channel_id: EntityId,
        entry_addon_id: Option<&str>,
        context: &SuppliedTeletextGuideContext<'_>,
        budget: TeletextChannelDocumentInputBudget,
        admission: (Instant, TeletextWorkHold, Option<&Selection>),
    ) -> Result<BoundEffectiveChannelTeletextDocumentInput, Error> {
        let (deadline, hold, expected) = admission;
        prepare_with_ports_expected(
            self.ports().teletext_profiles(),
            channel_id,
            entry_addon_id,
            context,
            budget,
            |selection, addon_id, input, deadline| {
                self.invoke_effective_teletext_profile_before_with_hold(
                    selection,
                    addon_id,
                    input,
                    deadline,
                    hold.clone(),
                )
            },
            (deadline, expected),
        )
        .await
    }

    /// Check the complete scope after CPU/encoding without renewing its lease.
    /// # Errors
    /// Rejects expiry, unavailable storage or changed membership/interval targets.
    pub async fn recheck_effective_channel_teletext_document_selection(
        &self,
        selection: &Selection,
        deadline: Instant,
    ) -> Result<(), Error> {
        recheck(self.ports().teletext_profiles(), selection, deadline).await
    }
}

#[cfg(test)]
async fn prepare_with_ports<F, Fut>(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    budget: TeletextChannelDocumentInputBudget,
    invoke: F,
    supplied_deadline: Instant,
) -> Result<BoundEffectiveChannelTeletextDocumentInput, Error>
where
    F: FnMut(Arc<Selection>, String, Vec<u8>, Instant) -> Fut,
    Fut: Future<
        Output = Result<TeletextEffectiveProfileRuntimeReceipt, TeletextProfileRuntimeError>,
    >,
{
    prepare_with_ports_expected(
        repository,
        channel_id,
        entry_addon_id,
        context,
        budget,
        invoke,
        (supplied_deadline, None),
    )
    .await
}

async fn prepare_with_ports_expected<F, Fut>(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    entry_addon_id: Option<&str>,
    context: &SuppliedTeletextGuideContext<'_>,
    budget: TeletextChannelDocumentInputBudget,
    mut invoke: F,
    admission: (Instant, Option<&Selection>),
) -> Result<BoundEffectiveChannelTeletextDocumentInput, Error>
where
    F: FnMut(Arc<Selection>, String, Vec<u8>, Instant) -> Fut,
    Fut: Future<
        Output = Result<TeletextEffectiveProfileRuntimeReceipt, TeletextProfileRuntimeError>,
    >,
{
    let (supplied_deadline, expected) = admission;
    let started = Instant::now();
    let maximum = started
        .checked_add(PREPARATION_TIMEOUT)
        .ok_or(Error::TimedOut)?;
    let mut deadline = maximum.min(supplied_deadline);
    check_deadline(deadline)?;
    let selection = match repository
        .effective_channel_snapshot(channel_id, Utc::now())
        .await
        .map_err(|_| Error::StorageUnavailable)?
    {
        Mutation::Saved(selection) => Arc::new(selection),
        Mutation::Rejected(reason) => return Err(Error::SelectionRejected(reason)),
    };
    if let Some(boundary) = selection.valid_until {
        // Measure after observation: storage time is part of the original budget.
        let observed_monotonic = Instant::now();
        let remaining = (boundary - Utc::now())
            .to_std()
            .map_err(|_| Error::TimedOut)?;
        deadline = deadline.min(
            observed_monotonic
                .checked_add(remaining.min(PREPARATION_TIMEOUT))
                .ok_or(Error::TimedOut)?,
        );
    }
    check_deadline(deadline)?;
    if expected.is_some_and(|expected| expected != selection.as_ref()) {
        return Err(Error::SelectionChanged);
    }
    validate_selection(&selection, channel_id, budget.max_producers)?;
    let producers = selection
        .bindings
        .iter()
        .filter(|binding| binding.enabled())
        .count();
    let entry = if producers == 0 {
        None
    } else {
        let entry = entry_addon_id.ok_or(Error::EntryAddonRequired)?;
        if !selection
            .bindings
            .iter()
            .any(|binding| binding.enabled() && binding.addon_id() == entry)
        {
            return Err(Error::EntryAddonNotSelected);
        }
        Some(entry.to_owned())
    };
    let requests = preflight_contracts(selection.bindings.iter().filter_map(contract), context)?;
    let mut receipts = Vec::with_capacity(producers);
    let mut remaining = budget.max_response_bytes;
    for (binding, selected) in selection.bindings.iter().enumerate() {
        let Some(contract) = contract(selected) else {
            continue;
        };
        check_deadline(deadline)?;
        let input = requests
            .request(contract)
            .ok_or(Error::SelectionShape)?
            .to_vec();
        let receipt = invoke(
            Arc::clone(&selection),
            selected.addon_id().to_owned(),
            input,
            deadline,
        )
        .await
        .map_err(|error| Error::Runtime { binding, error })?;
        check_deadline(deadline)?;
        if receipt.selection.as_ref() != selection.as_ref()
            || receipt.addon_id != selected.addon_id()
            || receipt.generation != context.generation
        {
            return Err(Error::ReceiptMismatch { binding });
        }
        if receipt.stdout.len() > wire::MAX_TELETEXT_PAGES_OUTPUT_BYTES {
            return Err(Error::ResponseTooLarge { binding });
        }
        remaining =
            remaining
                .checked_sub(receipt.stdout.len())
                .ok_or(Error::AggregateResponseBudget {
                    binding,
                    maximum: budget.max_response_bytes,
                })?;
        receipts.push((binding, receipt));
    }
    recheck(repository, &selection, deadline).await?;
    Ok(BoundEffectiveChannelTeletextDocumentInput {
        selection,
        generation: context.generation.to_owned(),
        receipts,
        requests,
        entry_addon_id: entry,
        deadline,
    })
}

fn contract(binding: &Binding) -> Option<TeletextPageContract> {
    match binding {
        Binding::Base(base) => Some(base.contract),
        Binding::Scripted(published) => match &published.interval.state {
            TeletextOverrideState::Enabled(candidate) => Some(candidate.contract),
            TeletextOverrideState::Off { .. } => None,
        },
    }
}

fn validate_selection(
    selection: &Selection,
    channel: EntityId,
    maximum: usize,
) -> Result<(), Error> {
    // Each bounded source admits at most 1024 members; Off does not consume a
    // producer slot, so the complete fence can contain 2048 distinct members.
    if selection.bindings.len() > 2048 {
        return Err(Error::BindingCount);
    }
    if channel <= 0 || selection.channel_id != channel {
        return Err(Error::SelectionShape);
    }
    let mut previous = None;
    for binding in &selection.bindings {
        if binding.channel_id() != channel
            || binding.addon_id().is_empty()
            || previous.is_some_and(|last| last >= binding.addon_id())
        {
            return Err(Error::SelectionShape);
        }
        previous = Some(binding.addon_id());
    }
    let count = selection
        .bindings
        .iter()
        .filter(|binding| binding.enabled())
        .count();
    let maximum = maximum.min(1024);
    if count > maximum {
        return Err(Error::ProducerBudget { count, maximum });
    }
    Ok(())
}

async fn recheck(
    repository: &dyn TeletextProfileRepository,
    selection: &Selection,
    deadline: Instant,
) -> Result<(), Error> {
    check_deadline(deadline)?;
    let current = repository
        .recheck_effective_channel_snapshot(selection, Utc::now())
        .await
        .map_err(|_| Error::StorageUnavailable)?;
    check_deadline(deadline)?;
    if current != Mutation::Saved(selection.clone()) {
        return Err(Error::SelectionChanged);
    }
    Ok(())
}

fn check_deadline(deadline: Instant) -> Result<(), Error> {
    if Instant::now() >= deadline {
        Err(Error::TimedOut)
    } else {
        Ok(())
    }
}

#[cfg(test)]
#[path = "teletext_effective_channel_documents_tests.rs"]
mod tests;
