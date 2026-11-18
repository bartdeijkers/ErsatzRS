//! Unpublished documents for a complete base/published Scripted observation.
//!
//! Active Off members remain in the fence without running a producer. Content
//! validation uses the same selected-contract parser as base preparation; it
//! never converts a Scripted origin into a base binding or publishes pages.

use std::{fmt, future::Future, sync::Arc, time::Instant};

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
use ersatzrs_time::Utc;

use crate::{
    teletext_channel_documents::{
        preflight_contracts, ChannelTeletextDocumentError as Error,
        TeletextChannelDocumentInputBudget, TeletextChannelRequestTemplates,
        TeletextCustomPageSource, PREPARATION_TIMEOUT,
    },
    teletext_effective_profiles::TeletextEffectiveProfileRuntimeReceipt,
    teletext_guide::SuppliedTeletextGuideContext,
    teletext_profile_documents::{
        validate_document_response, DocumentProducer, TeletextProfileDocument,
        TeletextProfileDocumentBudget, TeletextValidatedDocumentSource,
    },
    teletext_profiles::TeletextProfileRuntimeError,
    ApplicationServices,
};

/// Frozen manual host source for effective automatic guide preparation.
/// This data grants no invocation/publication authority; all real dependencies,
/// exact Lua scope, original deadline and actual work ownership stay mandatory.
pub struct TeletextEffectiveChannelDocumentRefresh<'a> {
    pub selection: &'a Selection,
    /// None preserves the manual original's explicit host absence.
    pub custom_pages: Option<&'a ersatzrs_domain::TeletextCustomPagesWire>,
    pub deadline: Instant,
    pub hold: TeletextWorkHold,
}

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
            let producer = match &self.selection.bindings[binding] {
                Binding::Base(base) => DocumentProducer::of(base),
                Binding::Scripted(_) => DocumentProducer::addon(selected_contract),
            };
            let (document, source) = validate_document_response(
                producer,
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
            TeletextCustomPageSource::Current,
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
            TeletextCustomPageSource::Current,
        )
        .await
    }

    /// Prepare automatic guide output from an immutable manual host source.
    /// # Errors
    /// Rejects changed real dependencies/Lua scope, invalid original association,
    /// deadline, budgets, runtime failures or a failed final dependency fence.
    pub async fn invoke_effective_channel_teletext_documents_for_refresh(
        &self,
        channel_id: EntityId,
        entry_addon_id: Option<&str>,
        context: &SuppliedTeletextGuideContext<'_>,
        budget: TeletextChannelDocumentInputBudget,
        refresh: TeletextEffectiveChannelDocumentRefresh<'_>,
    ) -> Result<BoundEffectiveChannelTeletextDocumentInput, Error> {
        self.invoke_effective_channel_teletext_documents_with_optional_expectation(
            channel_id,
            entry_addon_id,
            context,
            budget,
            (refresh.deadline, refresh.hold, Some(refresh.selection)),
            TeletextCustomPageSource::Published(refresh.custom_pages),
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
        custom_page_source: TeletextCustomPageSource<'_>,
    ) -> Result<BoundEffectiveChannelTeletextDocumentInput, Error> {
        let (deadline, hold, expected) = admission;
        prepare_with_ports_expected(
            self.ports().teletext_profiles(),
            channel_id,
            entry_addon_id,
            context,
            budget,
            |selection, addon_id, input, deadline| {
                let hold = hold.clone();
                async move {
                    if ersatzrs_domain::is_teletext_custom_pages_producer(&addon_id) {
                        let base = selection
                            .bindings
                            .iter()
                            .find_map(|binding| match binding {
                                Binding::Base(base) if base.addon_id == addon_id => {
                                    Some(base.clone())
                                }
                                _ => None,
                            })
                            .ok_or(TeletextProfileRuntimeError::InvalidRequest)?;
                        let receipt = if let TeletextCustomPageSource::Published(published) =
                            custom_page_source
                        {
                            super::teletext_elements::broadcast::reproduce_published_custom_pages(
                                base,
                                &input,
                                deadline,
                                published.ok_or(TeletextProfileRuntimeError::InvalidRequest)?,
                            )?
                        } else {
                            Box::pin(self.produce_teletext_custom_pages(
                                base,
                                &input,
                                deadline,
                                Some(&selection),
                            ))
                            .await?
                        };
                        return Ok(TeletextEffectiveProfileRuntimeReceipt {
                            selection,
                            addon_id,
                            stdout: receipt.stdout,
                            generation: receipt.generation,
                        });
                    }
                    match custom_page_source {
                        TeletextCustomPageSource::Current => {
                            self.invoke_effective_teletext_profile_before_with_hold(
                                selection, addon_id, input, deadline, hold,
                            )
                            .await
                        }
                        TeletextCustomPageSource::Published(_) => {
                            self.invoke_published_effective_teletext_profile_before_with_hold(
                                selection, addon_id, input, deadline, hold,
                            )
                            .await
                        }
                    }
                }
            },
            (deadline, expected, custom_page_source),
        )
        .await
    }

    /// Check dependencies of an accepted original without adopting latest host links.
    /// Every Scripted origin/boundary and real add-on dependency remains exact.
    /// This grants no new Check, publication or generation acceptance authority.
    /// # Errors
    /// Rejects expiry, deadline, storage failure or changed non-host dependencies.
    pub async fn recheck_published_effective_channel_teletext_document_selection(
        &self,
        selection: &Selection,
        deadline: Instant,
    ) -> Result<(), Error> {
        recheck_published(self.ports().teletext_profiles(), selection, deadline).await
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
        (supplied_deadline, None, TeletextCustomPageSource::Current),
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
    admission: (Instant, Option<&Selection>, TeletextCustomPageSource<'_>),
) -> Result<BoundEffectiveChannelTeletextDocumentInput, Error>
where
    F: FnMut(Arc<Selection>, String, Vec<u8>, Instant) -> Fut,
    Fut: Future<
        Output = Result<TeletextEffectiveProfileRuntimeReceipt, TeletextProfileRuntimeError>,
    >,
{
    let (supplied_deadline, expected, custom_page_source) = admission;
    let (selection, deadline) = observe_selection_before(
        repository,
        channel_id,
        supplied_deadline,
        matches!(custom_page_source, TeletextCustomPageSource::Published(_)),
    )
    .await?;
    let selection = preparation_selection(selection, expected, custom_page_source)?;
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
    match custom_page_source {
        TeletextCustomPageSource::Current => recheck(repository, &selection, deadline).await?,
        TeletextCustomPageSource::Published(_) => {
            recheck_published(repository, &selection, deadline).await?;
        }
    }
    Ok(BoundEffectiveChannelTeletextDocumentInput {
        selection,
        generation: context.generation.to_owned(),
        receipts,
        requests,
        entry_addon_id: entry,
        deadline,
    })
}

/// Bound the initial storage observation and producer lease with one original clock.
async fn observe_selection_before(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    supplied_deadline: Instant,
    published: bool,
) -> Result<(Arc<Selection>, Instant), Error> {
    let started = Instant::now();
    let maximum = started
        .checked_add(PREPARATION_TIMEOUT)
        .ok_or(Error::TimedOut)?;
    let mut deadline = maximum.min(supplied_deadline);
    check_deadline(deadline)?;
    let observed = if published {
        repository
            .published_effective_channel_dependencies(channel_id, Utc::now())
            .await
    } else {
        repository
            .effective_channel_snapshot(channel_id, Utc::now())
            .await
    };
    let selection = match observed.map_err(|_| Error::StorageUnavailable)? {
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
    Ok((selection, deadline))
}

fn preparation_selection(
    current: Arc<Selection>,
    expected: Option<&Selection>,
    source: TeletextCustomPageSource<'_>,
) -> Result<Arc<Selection>, Error> {
    match source {
        TeletextCustomPageSource::Current => {
            if expected.is_some_and(|expected| expected != current.as_ref()) {
                return Err(Error::SelectionChanged);
            }
            Ok(current)
        }
        TeletextCustomPageSource::Published(document) => {
            let expected = expected.ok_or(Error::SelectionShape)?;
            if !ersatzrs_domain::teletext_published_effective_selection_matches(&current, expected)
            {
                return Err(Error::SelectionChanged);
            }
            let has_host = expected.bindings.iter().any(|binding| {
                ersatzrs_domain::is_teletext_custom_pages_producer(binding.addon_id())
            });
            if has_host != document.is_some() {
                return Err(Error::SelectionShape);
            }
            Ok(Arc::new(expected.clone()))
        }
    }
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
        if ersatzrs_domain::is_teletext_custom_pages_producer(binding.addon_id())
            && *binding != Binding::Base(ersatzrs_domain::teletext_custom_pages_selection(channel))
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

async fn recheck_published(
    repository: &dyn TeletextProfileRepository,
    selection: &Selection,
    deadline: Instant,
) -> Result<(), Error> {
    check_deadline(deadline)?;
    let current = repository
        .published_effective_channel_dependencies(selection.channel_id, Utc::now())
        .await
        .map_err(|_| Error::StorageUnavailable)?;
    check_deadline(deadline)?;
    match current {
        Mutation::Saved(current)
            if selection
                .valid_until
                .is_some_and(|until| Utc::now() < until)
                && ersatzrs_domain::teletext_published_effective_selection_matches(
                    &current, selection,
                ) =>
        {
            Ok(())
        }
        _ => Err(Error::SelectionChanged),
    }
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
