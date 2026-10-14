//! Selection-bound authored input, with a separate synchronous validation stage.
//!
//! The native host runs CPU-heavy validation off async workers. Neither stage
//! encodes, composes or publishes pages; promotion still needs a fresh selection.

use std::{
    fmt,
    future::Future,
    time::{Duration, Instant},
};

use ersatzrs_core::locking::TeletextWorkHold;
use ersatzrs_core::repositories::{TeletextProfileMutation as Mutation, TeletextProfileRepository};
use ersatzrs_domain::{
    EntityId, TeletextDeclaredService, TeletextDeclaredServiceBudget, TeletextPageContract,
    TeletextPageSetDraft, TeletextSelectionSnapshot, TeletextServiceBudget, TeletextServiceError,
};

use crate::{
    teletext_guide::{build_request_for_schema, SuppliedTeletextGuideContext},
    teletext_pages::{parse_page_set, TeletextPagesError},
    teletext_pages_v2::{parse_declared_service, TeletextAuthoredPagesError},
    teletext_pages_v3::{parse_authored_service_with_metadata, TeletextAuthoredPagesV3Error},
    teletext_profiles::{
        BoundTeletextPagesError, TeletextProfileRuntimeError, TeletextProfileRuntimeReceipt,
    },
    ApplicationServices,
};

/// Existing maximum lifetime of a bound document preparation.
pub const PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);

/// Trusted host policy, separate from producer payloads and wire limits.
#[derive(Clone, Copy, Debug)]
pub struct TeletextProfileDocumentBudget {
    pub service: TeletextServiceBudget,
    pub declarations: TeletextDeclaredServiceBudget,
}

/// Exact producer document; base-only input requires the host's explicit region
/// when lifted into the enhanced model. Authored input retains all declarations.
pub enum TeletextProfileDocument {
    Base(TeletextPageSetDraft),
    Authored(Box<TeletextDeclaredService>),
}

/// Exact bounded UTF-8 producer response after selected-contract/model validation.
/// Private construction preserves the original schema and correlation; neither
/// field establishes expiry, revision ownership or publication eligibility.
pub struct TeletextValidatedDocumentSource {
    contract: TeletextPageContract,
    generation: String,
    bytes: Vec<u8>,
    authoring: Option<ersatzrs_addon_contract::TeletextAuthoringMetadataWire>,
}

impl TeletextValidatedDocumentSource {
    #[must_use]
    pub const fn contract(&self) -> TeletextPageContract {
        self.contract
    }
    /// Original opaque correlation, never a durable design revision.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    /// Exact original response bytes, without serialization or normalization.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }
    /// Validated original add-on description, not an import edit permission.
    #[must_use]
    pub const fn authoring(
        &self,
    ) -> Option<&ersatzrs_addon_contract::TeletextAuthoringMetadataWire> {
        self.authoring.as_ref()
    }
}

impl fmt::Debug for TeletextValidatedDocumentSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextValidatedDocumentSource")
            .field("contract", &self.contract)
            .field("response_bytes", &self.bytes.len())
            .field("has_authoring", &self.authoring.is_some())
            .finish_non_exhaustive()
    }
}

/// Owned raw receipt, obtainable only after invocation and selection fencing.
/// Content stays private until exact-contract validation completes.
pub struct BoundTeletextDocumentInput {
    receipt: TeletextProfileRuntimeReceipt,
    deadline: Instant,
}

impl BoundTeletextDocumentInput {
    // Channel preparation authenticates the complete receipt and membership first.
    // This internal constructor is not detached public receipt authority.
    pub(super) const fn from_channel_receipt(
        receipt: TeletextProfileRuntimeReceipt,
        deadline: Instant,
    ) -> Self {
        Self { receipt, deadline }
    }

    /// The original budget includes invocation, validation and native encoding.
    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }

    /// Validate once under the selected contract, without retry or downgrade.
    ///
    /// This is CPU-heavy synchronous work: native hosts must run it off async
    /// workers. Deadline checks surround bounded validation; they do not preempt
    /// CPU work. A selection must be checked again after encoding and at promotion.
    /// # Errors
    /// Returns exact response/budget failures or an expired original deadline.
    pub fn validate(
        self,
        budget: TeletextProfileDocumentBudget,
    ) -> Result<PreparedBoundTeletextDocument, BoundTeletextDocumentError> {
        let (document, source) = validate_document_response(
            self.receipt.selection.contract,
            &self.receipt.generation,
            self.receipt.stdout,
            budget,
            self.deadline,
        )?;
        Ok(PreparedBoundTeletextDocument {
            selection: self.receipt.selection,
            generation: self.receipt.generation,
            document,
            source,
            deadline: self.deadline,
        })
    }
}

// Shared content validation only: callers authenticate their distinct base or
// published-interval receipt first. This function grants no producer authority.
pub(super) fn validate_document_response(
    contract: TeletextPageContract,
    generation: &str,
    stdout: Vec<u8>,
    budget: TeletextProfileDocumentBudget,
    deadline: Instant,
) -> Result<(TeletextProfileDocument, TeletextValidatedDocumentSource), BoundTeletextDocumentError>
{
    check_deadline(Instant::now(), deadline)?;
    let mut authoring = None;
    let document = match contract {
        TeletextPageContract::V1 => {
            let base = parse_page_set(&stdout, generation)?;
            budget
                .service
                .validate_base_with_page_metadata(&base)
                .map_err(|error| BoundTeletextDocumentError::ServiceBudget(Box::new(error)))?;
            TeletextProfileDocument::Base(base)
        }
        TeletextPageContract::V2 => TeletextProfileDocument::Authored(Box::new(
            parse_declared_service(&stdout, generation, budget.service, budget.declarations)
                .map_err(|error| BoundTeletextDocumentError::AuthoredResponse(Box::new(error)))?,
        )),
        TeletextPageContract::V3 => {
            let (service, metadata) = parse_authored_service_with_metadata(
                &stdout,
                generation,
                budget.service,
                budget.declarations,
            )
            .map_err(|error| {
                BoundTeletextDocumentError::AuthoredMetadataResponse(Box::new(error))
            })?;
            authoring = Some(metadata);
            TeletextProfileDocument::Authored(Box::new(service))
        }
    };
    check_deadline(Instant::now(), deadline)?;
    let source = TeletextValidatedDocumentSource {
        contract,
        generation: generation.to_owned(),
        bytes: stdout,
        authoring,
    };
    Ok((document, source))
}

impl fmt::Debug for BoundTeletextDocumentInput {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundTeletextDocumentInput")
            .field("selection", &self.receipt.selection)
            .field("response_bytes", &self.receipt.stdout.len())
            .finish_non_exhaustive()
    }
}

/// Immutable complete validated document, without encoding/publication authority.
pub struct PreparedBoundTeletextDocument {
    selection: TeletextSelectionSnapshot,
    generation: String,
    document: TeletextProfileDocument,
    source: TeletextValidatedDocumentSource,
    deadline: Instant,
}

impl PreparedBoundTeletextDocument {
    /// Move the complete association and exact source without copying responses.
    /// Subsequent work still needs the existing selection/deadline fences.
    #[must_use]
    pub fn into_parts_with_source(
        self,
    ) -> (
        TeletextSelectionSnapshot,
        String,
        TeletextProfileDocument,
        TeletextValidatedDocumentSource,
        Instant,
    ) {
        (
            self.selection,
            self.generation,
            self.document,
            self.source,
            self.deadline,
        )
    }
    /// Move the complete document and its observed association into native work.
    /// The caller must retain this association and recheck it after encoding;
    /// these components grant no publication authority.
    /// This compatibility projection drops the source; retain it with
    /// `into_parts_with_source` when preparing durable original documents.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        TeletextSelectionSnapshot,
        String,
        TeletextProfileDocument,
        Instant,
    ) {
        (
            self.selection,
            self.generation,
            self.document,
            self.deadline,
        )
    }
    /// Exact observed output binding, profile, installation, contract and settings.
    #[must_use]
    pub const fn selection(&self) -> &TeletextSelectionSnapshot {
        &self.selection
    }
    /// Opaque producer correlation, never trusted generation ownership.
    #[must_use]
    pub fn generation(&self) -> &str {
        &self.generation
    }
    /// Borrow the complete immutable document used for subsequent native work.
    #[must_use]
    pub const fn document(&self) -> &TeletextProfileDocument {
        &self.document
    }
    /// The source belongs to this exact document and observed selection.
    #[must_use]
    pub const fn source(&self) -> &TeletextValidatedDocumentSource {
        &self.source
    }
    /// Original shared deadline; subsequent stages must not restart this budget.
    #[must_use]
    pub const fn deadline(&self) -> Instant {
        self.deadline
    }
}

impl fmt::Debug for PreparedBoundTeletextDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedBoundTeletextDocument")
            .field("selection", &self.selection)
            .finish_non_exhaustive()
    }
}

/// Located validation failures omit raw guide/configuration/producer values.
#[derive(Debug, thiserror::Error)]
pub enum BoundTeletextDocumentError {
    #[error(transparent)]
    Preparation(#[from] BoundTeletextPagesError),
    #[error(transparent)]
    BaseResponse(#[from] TeletextPagesError),
    #[error(transparent)]
    AuthoredResponse(Box<TeletextAuthoredPagesError>),
    #[error(transparent)]
    AuthoredMetadataResponse(Box<TeletextAuthoredPagesV3Error>),
    #[error(transparent)]
    ServiceBudget(Box<TeletextServiceError>),
    #[error("teletext document preparation exceeded its shared deadline")]
    TimedOut,
}

impl ApplicationServices {
    /// Recheck the full observed binding after synchronous validation/encoding.
    /// This is an eligibility observation, never generation/publication admission.
    pub async fn recheck_teletext_document_selection(
        &self,
        selection: &TeletextSelectionSnapshot,
        deadline: Instant,
    ) -> Result<(), BoundTeletextDocumentError> {
        recheck_with_ports(
            self.ports().teletext_profiles(),
            selection,
            deadline,
            Instant::now,
        )
        .await
    }
    /// Invoke the selected installed contract under one absolute deadline.
    ///
    /// The returned private input still needs synchronous validation, native
    /// encoding, another selection fence and explicit publication admission.
    /// This function only reads configuration and never activates packages.
    pub async fn invoke_bound_teletext_document(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
        context: &SuppliedTeletextGuideContext<'_>,
    ) -> Result<BoundTeletextDocumentInput, BoundTeletextDocumentError> {
        self.invoke_bound_teletext_document_with_optional_hold(
            channel_id,
            addon_id,
            expected_binding_revision,
            context,
            None,
            None,
        )
        .await
    }

    /// Invoke under an existing host admission and original operation deadline.
    /// The native adapter retains the hold until its owned cleanup has settled.
    /// # Errors
    /// Preserves original bound preparation errors and rejects incapable adapters.
    pub async fn invoke_bound_teletext_document_with_hold(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
        context: &SuppliedTeletextGuideContext<'_>,
        deadline: Instant,
        hold: TeletextWorkHold,
    ) -> Result<BoundTeletextDocumentInput, BoundTeletextDocumentError> {
        self.invoke_bound_teletext_document_with_optional_hold(
            channel_id,
            addon_id,
            expected_binding_revision,
            context,
            Some(deadline),
            Some(hold),
        )
        .await
    }

    async fn invoke_bound_teletext_document_with_optional_hold(
        &self,
        channel_id: EntityId,
        addon_id: &str,
        expected_binding_revision: i64,
        context: &SuppliedTeletextGuideContext<'_>,
        deadline: Option<Instant>,
        hold: Option<TeletextWorkHold>,
    ) -> Result<BoundTeletextDocumentInput, BoundTeletextDocumentError> {
        let invoke = |selection, input, deadline| {
            let hold = hold.clone();
            async move {
                match hold {
                    Some(hold) => {
                        self.ports()
                            .addons()
                            .invoke_teletext_profile_before_with_hold(
                                selection, input, deadline, hold,
                            )
                            .await
                    }
                    None => {
                        self.ports()
                            .addons()
                            .invoke_teletext_profile_before(selection, input, deadline)
                            .await
                    }
                }
            }
        };
        match deadline {
            None => {
                prepare_input_with_ports(
                    self.ports().teletext_profiles(),
                    channel_id,
                    addon_id,
                    expected_binding_revision,
                    context,
                    invoke,
                    Instant::now,
                )
                .await
            }
            Some(deadline) => {
                prepare_input_with_ports_before(
                    self.ports().teletext_profiles(),
                    channel_id,
                    addon_id,
                    expected_binding_revision,
                    context,
                    invoke,
                    (Instant::now, Some(deadline)),
                )
                .await
            }
        }
    }
}

pub(super) async fn prepare_input_with_ports<F, Fut, Clock>(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    addon_id: &str,
    expected_revision: i64,
    context: &SuppliedTeletextGuideContext<'_>,
    invoke: F,
    now: Clock,
) -> Result<BoundTeletextDocumentInput, BoundTeletextDocumentError>
where
    F: FnOnce(TeletextSelectionSnapshot, Vec<u8>, Instant) -> Fut,
    Fut: Future<Output = Result<TeletextProfileRuntimeReceipt, TeletextProfileRuntimeError>>,
    Clock: FnMut() -> Instant,
{
    prepare_input_with_ports_before(
        repository,
        channel_id,
        addon_id,
        expected_revision,
        context,
        invoke,
        (now, None),
    )
    .await
}

pub(super) async fn prepare_input_with_ports_before<F, Fut, Clock>(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    addon_id: &str,
    expected_revision: i64,
    context: &SuppliedTeletextGuideContext<'_>,
    invoke: F,
    timing: (Clock, Option<Instant>),
) -> Result<BoundTeletextDocumentInput, BoundTeletextDocumentError>
where
    F: FnOnce(TeletextSelectionSnapshot, Vec<u8>, Instant) -> Fut,
    Fut: Future<Output = Result<TeletextProfileRuntimeReceipt, TeletextProfileRuntimeError>>,
    Clock: FnMut() -> Instant,
{
    let (mut now, supplied_deadline) = timing;
    let started = now();
    let maximum = started
        .checked_add(PREPARATION_TIMEOUT)
        .ok_or(BoundTeletextDocumentError::TimedOut)?;
    let deadline = supplied_deadline.map_or(maximum, |deadline| maximum.min(deadline));
    if supplied_deadline.is_some() {
        check_deadline(started, deadline)?;
    }
    let selection = match repository
        .invocation_snapshot(channel_id, addon_id, expected_revision)
        .await
        .map_err(|_| BoundTeletextPagesError::StorageUnavailable)?
    {
        Mutation::Saved(selection) => selection,
        Mutation::Rejected(reason) => {
            return Err(BoundTeletextPagesError::SelectionRejected(reason).into());
        }
    };
    check_deadline(now(), deadline)?;
    let input = build_request_for_schema(context, selection.contract.schema_version())
        .map_err(BoundTeletextPagesError::Request)?;
    check_deadline(now(), deadline)?;
    let receipt = invoke(selection.clone(), input, deadline)
        .await
        .map_err(BoundTeletextPagesError::Runtime)?;
    check_deadline(now(), deadline)?;
    if receipt.selection != selection || receipt.generation != context.generation {
        return Err(BoundTeletextPagesError::ReceiptMismatch.into());
    }
    let current = repository
        .invocation_snapshot(channel_id, addon_id, expected_revision)
        .await
        .map_err(|_| BoundTeletextPagesError::StorageUnavailable)?;
    check_deadline(now(), deadline)?;
    if current != Mutation::Saved(selection) {
        return Err(BoundTeletextPagesError::SelectionChanged.into());
    }
    Ok(BoundTeletextDocumentInput { receipt, deadline })
}

fn check_deadline(now: Instant, deadline: Instant) -> Result<(), BoundTeletextDocumentError> {
    if now >= deadline {
        Err(BoundTeletextDocumentError::TimedOut)
    } else {
        Ok(())
    }
}

pub(super) async fn recheck_with_ports<Clock: FnMut() -> Instant>(
    repository: &dyn TeletextProfileRepository,
    selection: &TeletextSelectionSnapshot,
    deadline: Instant,
    mut now: Clock,
) -> Result<(), BoundTeletextDocumentError> {
    check_deadline(now(), deadline)?;
    let current = repository
        .invocation_snapshot(
            selection.channel_id,
            &selection.addon_id,
            selection.binding_revision,
        )
        .await
        .map_err(|_| BoundTeletextPagesError::StorageUnavailable)?;
    check_deadline(now(), deadline)?;
    if current != Mutation::Saved(selection.clone()) {
        return Err(BoundTeletextPagesError::SelectionChanged.into());
    }
    Ok(())
}
