//! Selection-bound authored input, with a separate synchronous validation stage.
//!
//! The native host runs CPU-heavy validation off async workers. Neither stage
//! encodes, composes or publishes pages; promotion still needs a fresh selection.

use std::{
    fmt,
    future::Future,
    time::{Duration, Instant},
};

use ersatzrs_core::repositories::{TeletextProfileMutation as Mutation, TeletextProfileRepository};
use ersatzrs_domain::{
    EntityId, TeletextDeclaredService, TeletextDeclaredServiceBudget, TeletextPageContract,
    TeletextPageSetDraft, TeletextSelectionSnapshot, TeletextServiceBudget, TeletextServiceError,
};

use crate::{
    teletext_guide::{build_request_for_schema, SuppliedTeletextGuideContext},
    teletext_pages::{parse_page_set, TeletextPagesError},
    teletext_pages_v2::{parse_declared_service, TeletextAuthoredPagesError},
    teletext_profiles::{
        BoundTeletextPagesError, TeletextProfileRuntimeError, TeletextProfileRuntimeReceipt,
    },
    ApplicationServices,
};

const PREPARATION_TIMEOUT: Duration = Duration::from_secs(30);

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

/// Owned raw receipt, obtainable only after invocation and selection fencing.
/// Content stays private until exact-contract validation completes.
pub struct BoundTeletextDocumentInput {
    receipt: TeletextProfileRuntimeReceipt,
    deadline: Instant,
}

impl BoundTeletextDocumentInput {
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
        check_deadline(Instant::now(), self.deadline)?;
        let document = match self.receipt.selection.contract {
            TeletextPageContract::V1 => {
                let base = parse_page_set(&self.receipt.stdout, &self.receipt.generation)?;
                budget
                    .service
                    .validate_base_with_page_metadata(&base)
                    .map_err(|error| BoundTeletextDocumentError::ServiceBudget(Box::new(error)))?;
                TeletextProfileDocument::Base(base)
            }
            TeletextPageContract::V2 => TeletextProfileDocument::Authored(Box::new(
                parse_declared_service(
                    &self.receipt.stdout,
                    &self.receipt.generation,
                    budget.service,
                    budget.declarations,
                )
                .map_err(|error| BoundTeletextDocumentError::AuthoredResponse(Box::new(error)))?,
            )),
        };
        check_deadline(Instant::now(), self.deadline)?;
        Ok(PreparedBoundTeletextDocument {
            selection: self.receipt.selection,
            generation: self.receipt.generation,
            document,
            deadline: self.deadline,
        })
    }
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
    deadline: Instant,
}

impl PreparedBoundTeletextDocument {
    /// Move the complete document and its observed association into native work.
    /// The caller must retain this association and recheck it after encoding;
    /// these components grant no publication authority.
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
        prepare_input_with_ports(
            self.ports().teletext_profiles(),
            channel_id,
            addon_id,
            expected_binding_revision,
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

pub(super) async fn prepare_input_with_ports<F, Fut, Clock>(
    repository: &dyn TeletextProfileRepository,
    channel_id: EntityId,
    addon_id: &str,
    expected_revision: i64,
    context: &SuppliedTeletextGuideContext<'_>,
    invoke: F,
    mut now: Clock,
) -> Result<BoundTeletextDocumentInput, BoundTeletextDocumentError>
where
    F: FnOnce(TeletextSelectionSnapshot, Vec<u8>, Instant) -> Fut,
    Fut: Future<Output = Result<TeletextProfileRuntimeReceipt, TeletextProfileRuntimeError>>,
    Clock: FnMut() -> Instant,
{
    let deadline = now()
        .checked_add(PREPARATION_TIMEOUT)
        .ok_or(BoundTeletextDocumentError::TimedOut)?;
    let selection = match repository
        .invocation_snapshot(channel_id, addon_id, expected_revision)
        .await
        .map_err(|_| BoundTeletextPagesError::StorageUnavailable)?
    {
        Mutation::Saved(selection) => selection,
        Mutation::Rejected(reason) => {
            return Err(BoundTeletextPagesError::SelectionRejected(reason).into())
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
