//! Static original-draft authoring with one admission and immutable previews.
//! Configuration observations are optimistic fences, never publication tokens.

use std::{
    fmt::{self, Write as _},
    time::Instant,
};

use chrono::{DateTime, Utc};
use ersatzrs_app::{
    teletext_channel_documents::ChannelTeletextDocumentError,
    teletext_generations::TeletextGenerationStoreError,
    teletext_guide::SuppliedTeletextGuideContext,
    teletext_publications::{TeletextPublicationStoreError, TeletextPublicationValidationError},
    ApplicationServices,
};
use ersatzrs_core::repositories::{
    TeletextDesignMutation, TeletextGenerationMutation, TeletextGenerationRejection,
    TeletextPublicationMutation, TeletextPublicationRejection,
};
use ersatzrs_core::{
    locking::{TeletextWorkGuard, TeletextWorkHold},
    programme::ProgrammeClock,
};
use ersatzrs_domain::{
    validate_teletext_profile_values, TeletextChannelSelectionSnapshot, TeletextColourPixelOptions,
    TeletextEffectiveChannelSelectionSnapshot, TeletextFlashPhase, TeletextNavigationLink,
    TeletextPageAddress, TeletextProfileValue, TeletextPublicationRequest,
    TeletextResourceIdentity, TeletextSecretReferenceKind, MAX_TELETEXT_DESIGN_NAME_BYTES,
    TELETEXT_DESIGN_DOCUMENT_VERSION, TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION,
};
use ersatzrs_ffmpeg::teletext::{
    TeletextAuthoredServiceEncodingFailure, TeletextCompiledDisplayPageEncodingFailure,
    TeletextComposedMainEncodingError, TeletextEncodingError, TeletextMainCompositionSource,
    TeletextNativeDefaultPageEncodingFailure, TeletextNormalizedMainProgramFailure, TeletextRaster,
};
use sha2::{Digest, Sha256};

use super::teletext_channel_documents::{
    prepare_channel_teletext_document_candidate_with_hold, ChannelTeletextDocumentEncodingError,
    TeletextChannelDocumentPreparation,
};
use super::teletext_designs::{
    reload_original_teletext_design_revision_with_hold, request_original_publication_with_hold,
    save_original_teletext_design_candidate_with_hold, NativeTeletextDesignError,
    NativeTeletextPublicationError, RestoredOriginalTeletextDesign,
    TeletextOriginalPublicationTarget,
};
use super::teletext_generations::{
    accept_original_teletext_generation_with_hold, frozen_original, NativeTeletextGenerationError,
    TeletextGenerationAcceptanceTarget, TeletextGenerationTarget,
};
use super::teletext_preview::{project_teletext_preview, TeletextPreviewFailure};
use super::teletext_runtime_policy::{
    parse_native_teletext_authoring_input, parse_native_teletext_authoring_policy,
    NativeTeletextAuthoringPolicy, NATIVE_TELETEXT_AUTHORING_POLICY_SETTING,
};
use super::teletext_work::{self, TeletextWorkError};

mod effective;
mod scope;
pub use scope::{
    observe_effective_teletext_authoring_selection, TeletextAuthoringEffectiveObservation,
};

/// Draft preparation intent; stored revision versions independently determine
/// every preview/publication role and cannot be relabelled by a submitted form.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAuthoringScope {
    #[default]
    Base,
    Effective,
}

enum AuthoringSelection {
    Base(TeletextChannelSelectionSnapshot),
    Effective(TeletextEffectiveChannelSelectionSnapshot),
}

/// No source/configuration strings enter operator error formatting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAuthoringStatus {
    TimedOut,
    Busy,
    WorkConfiguration,
    Unavailable,
    MissingPolicy,
    InvalidPolicy,
    InvalidInput,
    ResourceLimit,
    SelectionChanged,
    SelectionUnavailable,
    Off,
    MissingDesign,
    WrongChannel,
    InvalidPages,
    ProducerFailed,
    EncodingFailed,
    PreviewFailed,
    StorageFailed,
    WorkerFailed,
}

/// Original numeric engine locations; no offending text or setting values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextAuthoringLocation {
    pub owner: Option<TeletextPageAddress>,
    pub resource: Option<TeletextResourceIdentity>,
    pub position: Option<(u16, u16)>,
    pub source: Option<TeletextMainCompositionSource>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextAuthoringError {
    pub status: TeletextAuthoringStatus,
    pub location: Option<TeletextAuthoringLocation>,
}
impl fmt::Display for TeletextAuthoringError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "teletext authoring {:?}", self.status)
    }
}
impl TeletextAuthoringError {
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self.status {
            TeletextAuthoringStatus::TimedOut => "expired",
            TeletextAuthoringStatus::Busy => "busy",
            TeletextAuthoringStatus::MissingPolicy => "missing-policy",
            TeletextAuthoringStatus::InvalidPolicy => "invalid-policy",
            TeletextAuthoringStatus::InvalidInput => "invalid-input",
            TeletextAuthoringStatus::SelectionChanged => "conflict",
            TeletextAuthoringStatus::Off => "off",
            TeletextAuthoringStatus::MissingDesign => "missing-design",
            TeletextAuthoringStatus::WrongChannel => "foreign-channel",
            TeletextAuthoringStatus::WorkerFailed => "worker",
            TeletextAuthoringStatus::PreviewFailed => "preview",
            TeletextAuthoringStatus::ResourceLimit => "limit",
            TeletextAuthoringStatus::InvalidPages
            | TeletextAuthoringStatus::ProducerFailed
            | TeletextAuthoringStatus::EncodingFailed => "encoding",
            _ => "unavailable",
        }
    }
}
impl std::error::Error for TeletextAuthoringError {}
fn failure(status: TeletextAuthoringStatus) -> TeletextAuthoringError {
    TeletextAuthoringError {
        status,
        location: None,
    }
}

/// Checked at submission, then rechecked inside the existing invocation path.
/// Deliberately no Debug: names, input bytes and observation are private data.
pub struct TeletextAuthoringPrepare {
    pub scope: TeletextAuthoringScope,
    pub channel_id: i64,
    pub selection_digest: String,
    pub input_json: String,
    pub name: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAuthoringIdentity {
    pub channel_id: i64,
    pub design_id: i64,
    pub revision: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAuthoringPreviewRequest {
    pub channel_id: i64,
    pub design_id: i64,
    pub revision: i64,
    pub owner: Option<TeletextPageAddress>,
}

/// The raster owns its located diagnostics; navigation is copied only after
/// native preview admission from the same exact compiled original service.
pub struct TeletextAuthoringPreview {
    pub identity: TeletextAuthoringIdentity,
    digest: String,
    static_original: bool,
    selection: AuthoringSelection,
    publication: Option<TeletextAuthoringPublicationObservation>,
    raster: Option<TeletextRaster>,
    navigation: Vec<TeletextNavigationLink>,
    pages: Vec<TeletextPageAddress>,
    hold: TeletextWorkHold,
    deadline: Instant,
}
impl TeletextAuthoringPreview {
    /// Missing observation means unavailable metadata, never absent pointers.
    #[must_use]
    pub const fn publication(&self) -> Option<&TeletextAuthoringPublicationObservation> {
        self.publication.as_ref()
    }
    #[must_use]
    pub const fn raster(&self) -> Option<&TeletextRaster> {
        self.raster.as_ref()
    }
    #[must_use]
    pub const fn is_absent(&self) -> bool {
        self.raster.is_none()
    }
    #[must_use]
    pub const fn scope(&self) -> TeletextAuthoringScope {
        match &self.selection {
            AuthoringSelection::Base(_) => TeletextAuthoringScope::Base,
            AuthoringSelection::Effective(_) => TeletextAuthoringScope::Effective,
        }
    }
    #[must_use]
    pub const fn valid_until(&self) -> Option<DateTime<Utc>> {
        match &self.selection {
            AuthoringSelection::Base(_) => None,
            AuthoringSelection::Effective(selection) => selection.valid_until,
        }
    }
    #[must_use]
    pub fn navigation(&self) -> &[TeletextNavigationLink] {
        &self.navigation
    }
    #[must_use]
    pub fn pages(&self) -> &[TeletextPageAddress] {
        &self.pages
    }
    /// Project this owned raster off async workers under its original admission.
    /// The host callback must enforce its own derived output-storage limits.
    /// # Errors
    /// Preserves typed callback failures and rejects expiry or a failed worker.
    pub async fn project<T: Send + 'static>(
        self,
        projection: impl FnOnce(&Self) -> Result<T, TeletextAuthoringError> + Send + 'static,
    ) -> Result<T, TeletextAuthoringError> {
        let deadline = self.deadline;
        teletext_work::spawn_blocking(self.hold.clone(), move || {
            check_deadline(deadline)?;
            let output = projection(&self)?;
            check_deadline(deadline)?;
            Ok(output)
        })
        .await
        .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
    }
}
impl fmt::Debug for TeletextAuthoringPreview {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("TeletextAuthoringPreview")
            .field("identity", &self.identity)
            .field("owner", &self.raster.as_ref().map(TeletextRaster::owner))
            .field("width", &self.raster.as_ref().map(TeletextRaster::width))
            .field("scope", &self.scope())
            .field("valid_until", &self.valid_until())
            .field("pages", &self.pages.len())
            .field(
                "diagnostics",
                &self
                    .raster
                    .as_ref()
                    .map(|raster| raster.diagnostics().len()),
            )
            .finish_non_exhaustive()
    }
}

/// An admitted save is retained even if its subsequent preview cannot finish.
/// Retrying a failed preview must use this exact identity, not create a new draft.
#[derive(Debug)]
pub struct TeletextAuthoringSaved {
    pub identity: TeletextAuthoringIdentity,
    pub preview: Result<TeletextAuthoringPreview, TeletextAuthoringError>,
}

/// Optimistic publication metadata; no read field grants mutation authority.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAuthoringPublicationState {
    Unrequested,
    Pending,
    Selected,
    Unavailable,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeletextAuthoringPublicationObservation {
    pub digest: String,
    pub current_design_head: i64,
    pub current_pending_id: Option<i64>,
    pub current_accepted_id: Option<i64>,
    pub state: TeletextAuthoringPublicationState,
    pub can_request: bool,
    pub can_publish: bool,
    pub replaces_pending: bool,
}

/// Exact immutable target plus the rendered head and pending-pointer fences.
/// Requesting intent cannot change the independent accepted pointer.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextAuthoringPublicationTarget {
    pub identity: TeletextAuthoringIdentity,
    pub digest: String,
    pub expected_design_head: i64,
    pub expected_request_id: Option<i64>,
}
impl TeletextAuthoringPublicationTarget {
    fn validate(&self) -> Result<(), TeletextAuthoringError> {
        if self.identity.channel_id <= 0
            || self.identity.design_id <= 0
            || self.identity.revision <= 0
            || self.expected_design_head <= 0
            || self.expected_request_id.is_some_and(|id| id <= 0)
            || self.digest.len() != 64
            || !self
                .digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            return Err(failure(TeletextAuthoringStatus::InvalidInput));
        }
        Ok(())
    }
}

/// Publish alone carries the independently observed accepted-pointer fence.
pub struct TeletextAuthoringPublishTarget {
    pub publication: TeletextAuthoringPublicationTarget,
    pub expected_generation_id: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAuthoringPending {
    pub identity: TeletextAuthoringIdentity,
    pub request_id: i64,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAuthoringSelected {
    pub identity: TeletextAuthoringIdentity,
    pub request_id: i64,
    pub generation_id: i64,
}

/// Create pending intent for an exact static original; selected output is unchanged.
/// No package runs, and submitted head/pending fences are never recaptured.
/// # Errors
/// Preserves located native failures and rejects stale fences without mutation.
pub async fn request_static_teletext_publication(
    application: &ApplicationServices,
    target: TeletextAuthoringPublicationTarget,
    deadline: Instant,
) -> Result<TeletextAuthoringPending, TeletextAuthoringError> {
    target.validate()?;
    let operation = Operation::begin(application, deadline).await?;
    if stored_scope(application, target.identity, &operation).await?
        == TeletextAuthoringScope::Effective
    {
        return effective::request_publication(application, target, &operation).await;
    }
    let identity = target.identity;
    let mutation = request_original_publication_with_hold(
        application,
        TeletextOriginalPublicationTarget {
            design_id: identity.design_id,
            revision: identity.revision,
            expected_digest: target.digest,
            expected_design_head: target.expected_design_head,
            expected_request_id: target.expected_request_id,
            expected_channel_id: Some(identity.channel_id),
            require_static_original: true,
        },
        operation.policy.design,
        operation.policy.reload_encoding_policy(),
        None,
        operation.deadline,
        operation.guard.hold_work(),
    )
    .await
    .map_err(publication_error)?;
    match mutation {
        TeletextPublicationMutation::Requested(requested) => Ok(TeletextAuthoringPending {
            identity,
            request_id: requested.request().id,
        }),
        TeletextPublicationMutation::Rejected(reason) => Err(publication_rejection(reason)),
    }
}

/// Explicitly select exact pending output for compatible consumers.
/// Version-2 live output may adopt it at its existing safe boundary; this
/// metadata selection does not acknowledge application by any stream.
/// # Errors
/// Rejects pending/accepted/head/selection races using existing atomic acceptance.
pub async fn publish_static_teletext_generation(
    application: &ApplicationServices,
    target: TeletextAuthoringPublishTarget,
    deadline: Instant,
) -> Result<TeletextAuthoringSelected, TeletextAuthoringError> {
    target.publication.validate()?;
    if target.expected_generation_id.is_some_and(|id| id <= 0)
        || target.publication.expected_request_id.is_none()
    {
        return Err(failure(TeletextAuthoringStatus::InvalidInput));
    }
    let operation = Operation::begin(application, deadline).await?;
    if stored_scope(application, target.publication.identity, &operation).await?
        == TeletextAuthoringScope::Effective
    {
        return effective::publish_generation(application, target, &operation).await;
    }
    let publication = target.publication;
    let request = before(
        operation.deadline,
        application.teletext_publication_request(publication.identity.channel_id),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::StorageFailed))?
    .filter(|request| Some(request.id) == publication.expected_request_id)
    .ok_or_else(|| failure(TeletextAuthoringStatus::SelectionChanged))?;
    if !matches_original(&request, publication.identity, &publication.digest) {
        return Err(failure(TeletextAuthoringStatus::SelectionChanged));
    }
    let mutation = accept_original_teletext_generation_with_hold(
        application,
        TeletextGenerationAcceptanceTarget {
            target: TeletextGenerationTarget {
                request,
                expected_design_head: publication.expected_design_head,
                expected_generation_id: target.expected_generation_id,
            },
            expected_channel_id: Some(publication.identity.channel_id),
        },
        operation.policy.design,
        operation.policy.reload_encoding_policy(),
        operation.deadline,
        operation.guard.hold_work(),
    )
    .await
    .map_err(generation_error)?;
    match mutation {
        TeletextGenerationMutation::Accepted(accepted) => Ok(TeletextAuthoringSelected {
            identity: publication.identity,
            request_id: accepted.generation().request.id,
            generation_id: accepted.generation().id,
        }),
        TeletextGenerationMutation::Rejected(reason) => Err(match reason {
            TeletextGenerationRejection::Publication(reason) => publication_rejection(reason),
            TeletextGenerationRejection::GuideFreshnessRequired => {
                failure(TeletextAuthoringStatus::SelectionUnavailable)
            }
            TeletextGenerationRejection::GenerationConflict
            | TeletextGenerationRejection::AlreadyAccepted => {
                failure(TeletextAuthoringStatus::SelectionChanged)
            }
        }),
    }
}

fn matches_original(
    request: &TeletextPublicationRequest,
    identity: TeletextAuthoringIdentity,
    digest: &str,
) -> bool {
    request.channel_id == identity.channel_id
        && request.design_id == identity.design_id
        && request.revision == identity.revision
        && request.digest == digest
}

async fn publication_observation(
    application: &ApplicationServices,
    preview: &TeletextAuthoringPreview,
    deadline: Instant,
) -> Result<TeletextAuthoringPublicationObservation, TeletextAuthoringError> {
    let head = before(
        deadline,
        application.teletext_design_head(preview.identity.design_id),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::StorageFailed))?
    .ok_or_else(|| failure(TeletextAuthoringStatus::MissingDesign))?;
    let pending = before(deadline, async {
        match preview.scope() {
            TeletextAuthoringScope::Base => {
                application
                    .teletext_publication_request(preview.identity.channel_id)
                    .await
            }
            TeletextAuthoringScope::Effective => {
                application
                    .effective_teletext_publication_request(preview.identity.channel_id)
                    .await
            }
        }
    })
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::StorageFailed))?;
    let accepted = before(deadline, async {
        match preview.scope() {
            TeletextAuthoringScope::Base => {
                application
                    .accepted_teletext_generation(preview.identity.channel_id)
                    .await
            }
            TeletextAuthoringScope::Effective => {
                application
                    .effective_teletext_accepted_generation(preview.identity.channel_id)
                    .await
            }
        }
    })
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::StorageFailed))?;
    let selection_current = preview_selection_current(application, preview, deadline).await?;
    let pending_matches = pending
        .as_ref()
        .is_some_and(|request| matches_original(request, preview.identity, &preview.digest));
    let selected = accepted.as_ref().is_some_and(|generation| {
        matches_original(&generation.request, preview.identity, &preview.digest)
    });
    let already_accepted_request = pending.as_ref().is_some_and(|request| {
        accepted
            .as_ref()
            .is_some_and(|generation| &generation.request == request)
    });
    let eligible = preview.static_original && !head.deleted && selection_current;
    let state = if !selection_current {
        TeletextAuthoringPublicationState::Unavailable
    } else if selected {
        TeletextAuthoringPublicationState::Selected
    } else if pending_matches {
        TeletextAuthoringPublicationState::Pending
    } else if eligible {
        TeletextAuthoringPublicationState::Unrequested
    } else {
        TeletextAuthoringPublicationState::Unavailable
    };
    Ok(TeletextAuthoringPublicationObservation {
        digest: preview.digest.clone(),
        current_design_head: head.revision,
        current_pending_id: pending.as_ref().map(|request| request.id),
        current_accepted_id: accepted.as_ref().map(|generation| generation.id),
        state,
        can_request: eligible && !pending_matches,
        can_publish: eligible && pending_matches && !already_accepted_request,
        replaces_pending: pending.is_some() && !pending_matches,
    })
}

async fn preview_selection_current(
    application: &ApplicationServices,
    preview: &TeletextAuthoringPreview,
    deadline: Instant,
) -> Result<bool, TeletextAuthoringError> {
    Ok(match &preview.selection {
        AuthoringSelection::Base(selection) => match before(
            deadline,
            application.observe_channel_teletext_document_selection(preview.identity.channel_id),
        )
        .await?
        {
            Ok(current) => current == *selection,
            Err(ChannelTeletextDocumentError::SelectionRejected(_)) => false,
            Err(_) => return Err(failure(TeletextAuthoringStatus::StorageFailed)),
        },
        AuthoringSelection::Effective(selection) => match before(
            deadline,
            application.observe_effective_channel_teletext_selection(
                preview.identity.channel_id,
                Utc::now(),
            ),
        )
        .await?
        {
            Ok(ersatzrs_core::repositories::TeletextProfileMutation::Saved(current)) => {
                current == *selection && current.valid_until.is_some_and(|until| until > Utc::now())
            }
            Ok(ersatzrs_core::repositories::TeletextProfileMutation::Rejected(_)) => false,
            Err(_) => return Err(failure(TeletextAuthoringStatus::StorageFailed)),
        },
    })
}

fn publication_rejection(reason: TeletextPublicationRejection) -> TeletextAuthoringError {
    failure(match reason {
        TeletextPublicationRejection::InvalidValues => TeletextAuthoringStatus::InvalidInput,
        TeletextPublicationRejection::MissingDesign
        | TeletextPublicationRejection::MissingRevision => TeletextAuthoringStatus::MissingDesign,
        TeletextPublicationRejection::DesignConflict
        | TeletextPublicationRejection::DigestConflict
        | TeletextPublicationRejection::SelectionChanged
        | TeletextPublicationRejection::RequestConflict
        | TeletextPublicationRejection::DeletedDesign => TeletextAuthoringStatus::SelectionChanged,
        TeletextPublicationRejection::GuideTimedOut => TeletextAuthoringStatus::TimedOut,
        _ => TeletextAuthoringStatus::SelectionUnavailable,
    })
}
fn publication_error(error: NativeTeletextPublicationError) -> TeletextAuthoringError {
    match error {
        NativeTeletextPublicationError::Design(error) => design_error(error),
        NativeTeletextPublicationError::Request(TeletextPublicationStoreError::Validation(
            TeletextPublicationValidationError::TimedOut,
        )) => failure(TeletextAuthoringStatus::TimedOut),
        NativeTeletextPublicationError::Request(TeletextPublicationStoreError::Validation(
            TeletextPublicationValidationError::Association,
        )) => failure(TeletextAuthoringStatus::SelectionChanged),
        NativeTeletextPublicationError::Request(_) => {
            failure(TeletextAuthoringStatus::StorageFailed)
        }
    }
}
fn generation_error(error: NativeTeletextGenerationError) -> TeletextAuthoringError {
    match error {
        NativeTeletextGenerationError::Design(error) => design_error(error),
        NativeTeletextGenerationError::Storage(TeletextGenerationStoreError::Validation(
            TeletextPublicationValidationError::TimedOut,
        )) => failure(TeletextAuthoringStatus::TimedOut),
        NativeTeletextGenerationError::Storage(TeletextGenerationStoreError::Validation(
            TeletextPublicationValidationError::Association,
        ))
        | NativeTeletextGenerationError::ArtifactMismatch => {
            failure(TeletextAuthoringStatus::SelectionChanged)
        }
        NativeTeletextGenerationError::Storage(_) => {
            failure(TeletextAuthoringStatus::StorageFailed)
        }
        NativeTeletextGenerationError::GuideFreshnessRequired => {
            failure(TeletextAuthoringStatus::SelectionUnavailable)
        }
        NativeTeletextGenerationError::WorkerFailed => {
            failure(TeletextAuthoringStatus::WorkerFailed)
        }
    }
}

struct Operation {
    deadline: Instant,
    guard: TeletextWorkGuard,
    policy: NativeTeletextAuthoringPolicy,
}
impl Operation {
    async fn begin(
        application: &ApplicationServices,
        deadline: Instant,
    ) -> Result<Self, TeletextAuthoringError> {
        let started = Instant::now();
        check_deadline(deadline)?;
        let guard = teletext_work::begin(application, Some(deadline))
            .await
            .map_err(|error| work_error(&error))?;
        let raw = before(
            deadline,
            application.get_optional_setting(NATIVE_TELETEXT_AUTHORING_POLICY_SETTING),
        )
        .await?
        .map_err(|_| failure(TeletextAuthoringStatus::Unavailable))?
        .ok_or_else(|| failure(TeletextAuthoringStatus::MissingPolicy))?;
        let policy = parse_native_teletext_authoring_policy(&raw)
            .map_err(|_| failure(TeletextAuthoringStatus::InvalidPolicy))?;
        let deadline = deadline.min(
            started
                .checked_add(policy.operation_timeout)
                .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidPolicy))?,
        );
        check_deadline(deadline)?;
        Ok(Self {
            deadline,
            guard,
            policy,
        })
    }
}

/// Render one opaque full-enabled-membership fence without producer invocation.
/// Raw unresolved settings are framed into the digest, never returned to HTML.
/// # Errors
/// Rejects unavailable policy/storage, limits, Off, invalid configuration or expiry.
pub async fn observe_teletext_authoring_selection(
    application: &ApplicationServices,
    channel_id: i64,
    deadline: Instant,
) -> Result<String, TeletextAuthoringError> {
    let operation = Operation::begin(application, deadline).await?;
    let selection = observed_selection(application, channel_id, operation.deadline).await?;
    let maximum = operation.policy.maximum_producers();
    teletext_work::spawn_blocking(operation.guard.hold_work(), move || {
        selection_digest(&selection, maximum, operation.deadline)
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
}

/// Invoke explicitly selected installed profiles with channels:[], validate,
/// save NEW immutable originals and recompile/render that exact stored revision.
/// One hold covers policy I/O, every native stage and the joined raster worker.
/// Saving grants no publication, package activation or settings authority.
/// # Errors
/// Pre-save failures return no draft; post-save failures retain its identity.
pub async fn prepare_static_teletext_design(
    application: &ApplicationServices,
    request: TeletextAuthoringPrepare,
    deadline: Instant,
) -> Result<TeletextAuthoringSaved, TeletextAuthoringError> {
    check_deadline(deadline)?;
    // The repository's existing design-name rule allows line breaks. The
    // stricter profile-name rule would silently reject previously valid names.
    if request.name.trim().is_empty()
        || request.name.len() > MAX_TELETEXT_DESIGN_NAME_BYTES
        || request.name.contains('\0')
    {
        return Err(failure(TeletextAuthoringStatus::InvalidInput));
    }
    let operation = Operation::begin(application, deadline).await?;
    if request.scope == TeletextAuthoringScope::Effective {
        return effective::prepare_design(application, request, &operation).await;
    }
    let input = parse_native_teletext_authoring_input(&request.input_json, &operation.policy)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    if request.channel_id <= 0 || input.channel_id() != request.channel_id {
        return Err(failure(TeletextAuthoringStatus::WrongChannel));
    }
    let owner = input.entry();
    let selection = observed_selection(application, request.channel_id, operation.deadline).await?;
    let selection = checked_selection(selection, &request.selection_digest, &operation).await?;
    let locale_raw = before(
        operation.deadline,
        application.get_optional_setting("pages.language"),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::Unavailable))?;
    let locale = super::teletext_guide::resolve_locale(locale_raw.as_deref())
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let clock =
        ProgrammeClock::system().map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let timezone = clock
        .iana_name()
        .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidInput))?;
    let locales = super::teletext_guide::supported_locales();
    let generation = uuid::Uuid::new_v4().to_string();
    let context = SuppliedTeletextGuideContext {
        generation: &generation,
        locale,
        supported_locales: &locales,
        timezone,
        captured_at: Utc::now(),
        channels: &[],
    };
    let policy = operation
        .policy
        .encoding_policy(&input)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let candidate = prepare_channel_teletext_document_candidate_with_hold(
        application,
        request.channel_id,
        Some(input.entry_addon_id()),
        TeletextChannelDocumentPreparation {
            context: &context,
            expected_selection: Some(&selection),
            deadline: operation.deadline,
        },
        policy,
        operation.guard.hold_work(),
    )
    .await
    .map_err(channel_error)?;
    if candidate.encoded().is_none() {
        return Err(failure(TeletextAuthoringStatus::Off));
    }
    let saved = save_original_teletext_design_candidate_with_hold(
        application,
        request.name,
        None,
        candidate,
        operation.policy.design,
        operation.deadline,
        operation.guard.hold_work(),
    )
    .await
    .map_err(design_error)?;
    let TeletextDesignMutation::Saved(saved) = saved else {
        return Err(failure(TeletextAuthoringStatus::InvalidInput));
    };
    let identity = TeletextAuthoringIdentity {
        channel_id: request.channel_id,
        design_id: saved.head().id,
        revision: saved.head().revision,
    };
    // The complete preparation candidate is no longer needed while historical
    // reload builds its independently checked model. Keep only durable identity.
    drop(saved);
    let preview = preview_with_operation(
        application,
        TeletextAuthoringPreviewRequest {
            channel_id: identity.channel_id,
            design_id: identity.design_id,
            revision: identity.revision,
            owner: Some(owner),
        },
        &operation,
    )
    .await;
    Ok(TeletextAuthoringSaved { identity, preview })
}

/// Recompile a historical immutable revision without invoking any package.
/// Exact channel and page/subcode are checked; no first-page fallback exists.
/// # Errors
/// Rejects policy, ownership, missing revisions, bounds, worker or render failures.
pub async fn preview_original_teletext_design(
    application: &ApplicationServices,
    request: TeletextAuthoringPreviewRequest,
    deadline: Instant,
) -> Result<TeletextAuthoringPreview, TeletextAuthoringError> {
    let operation = Operation::begin(application, deadline).await?;
    preview_with_operation(application, request, &operation).await
}

async fn preview_with_operation(
    application: &ApplicationServices,
    request: TeletextAuthoringPreviewRequest,
    operation: &Operation,
) -> Result<TeletextAuthoringPreview, TeletextAuthoringError> {
    if request.channel_id <= 0 || request.design_id <= 0 || request.revision <= 0 {
        return Err(failure(TeletextAuthoringStatus::InvalidInput));
    }
    check_deadline(operation.deadline)?;
    if stored_scope(
        application,
        TeletextAuthoringIdentity {
            channel_id: request.channel_id,
            design_id: request.design_id,
            revision: request.revision,
        },
        operation,
    )
    .await?
        == TeletextAuthoringScope::Effective
    {
        let preview = effective::preview_design(application, request, operation).await?;
        return finish_preview_observation(application, preview, operation.deadline).await;
    }
    let restored = reload_original_teletext_design_revision_with_hold(
        application,
        request.design_id,
        request.revision,
        operation.policy.design,
        operation.policy.reload_encoding_policy(),
        operation.deadline,
        operation.guard.hold_work(),
    )
    .await
    .map_err(design_error)?
    .ok_or_else(|| failure(TeletextAuthoringStatus::MissingDesign))?;
    if restored.selection().channel_id != request.channel_id {
        return Err(failure(TeletextAuthoringStatus::WrongChannel));
    }
    let budget = operation.policy.preview;
    let deadline = operation.deadline;
    let hold = operation.guard.hold_work();
    let preview = teletext_work::spawn_blocking(hold.clone(), move || {
        render_original(&restored, request, budget, deadline, hold)
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))??;
    finish_preview_observation(application, preview, deadline).await
}

async fn finish_preview_observation(
    application: &ApplicationServices,
    mut preview: TeletextAuthoringPreview,
    deadline: Instant,
) -> Result<TeletextAuthoringPreview, TeletextAuthoringError> {
    // Optional metadata gets at most half the remaining original operation bound.
    // A failed read preserves the historical raster and never invents None IDs.
    let now = Instant::now();
    let observation_deadline = deadline
        .checked_duration_since(now)
        .and_then(|remaining| now.checked_add(remaining / 2));
    if let Some(observation_deadline) = observation_deadline {
        preview.publication =
            publication_observation(application, &preview, observation_deadline.min(deadline))
                .await
                .ok();
    }
    Ok(preview)
}

fn render_original(
    restored: &RestoredOriginalTeletextDesign,
    request: TeletextAuthoringPreviewRequest,
    budget: super::teletext_preview::TeletextPreviewBudget,
    deadline: Instant,
    hold: TeletextWorkHold,
) -> Result<TeletextAuthoringPreview, TeletextAuthoringError> {
    check_deadline(deadline)?;
    let service = restored
        .encoded()
        .ok_or_else(|| failure(TeletextAuthoringStatus::Off))?;
    let owner = request
        .owner
        .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidInput))?;
    let (raster, navigation, pages) = render_service(service, owner, budget)?;
    check_deadline(deadline)?;
    Ok(TeletextAuthoringPreview {
        identity: TeletextAuthoringIdentity {
            channel_id: request.channel_id,
            design_id: request.design_id,
            revision: request.revision,
        },
        digest: restored.digest().to_owned(),
        static_original: frozen_original(restored),
        selection: AuthoringSelection::Base(restored.selection().clone()),
        publication: None,
        raster: Some(raster),
        navigation,
        pages,
        hold,
        deadline,
    })
}

fn render_service(
    service: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    owner: TeletextPageAddress,
    budget: super::teletext_preview::TeletextPreviewBudget,
) -> Result<
    (
        TeletextRaster,
        Vec<TeletextNavigationLink>,
        Vec<TeletextPageAddress>,
    ),
    TeletextAuthoringError,
> {
    let preview = project_teletext_preview(
        service,
        owner,
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        budget,
    )
    .map_err(|error| {
        let status = match error.reason {
            TeletextPreviewFailure::Budget { .. } | TeletextPreviewFailure::Size => {
                TeletextAuthoringStatus::ResourceLimit
            }
            _ => TeletextAuthoringStatus::PreviewFailed,
        };
        TeletextAuthoringError {
            status,
            location: Some(TeletextAuthoringLocation {
                owner: Some(error.owner),
                resource: None,
                position: None,
                source: None,
            }),
        }
    })?;
    let (raster, navigation) = preview.into_parts();
    let pages = service
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .map(|page| page.address)
        .collect();
    Ok((raster, navigation.to_vec(), pages))
}

async fn stored_scope(
    application: &ApplicationServices,
    identity: TeletextAuthoringIdentity,
    operation: &Operation,
) -> Result<TeletextAuthoringScope, TeletextAuthoringError> {
    let revision = before(
        operation.deadline,
        application.teletext_design_revision(
            identity.design_id,
            identity.revision,
            operation.policy.design.max_serialized_bytes,
        ),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::StorageFailed))?
    .ok_or_else(|| failure(TeletextAuthoringStatus::MissingDesign))?;
    match revision.document_version {
        TELETEXT_DESIGN_DOCUMENT_VERSION => Ok(TeletextAuthoringScope::Base),
        TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION => Ok(TeletextAuthoringScope::Effective),
        _ => Err(failure(TeletextAuthoringStatus::InvalidInput)),
    }
}

async fn observed_selection(
    application: &ApplicationServices,
    channel_id: i64,
    deadline: Instant,
) -> Result<TeletextChannelSelectionSnapshot, TeletextAuthoringError> {
    if channel_id <= 0 {
        return Err(failure(TeletextAuthoringStatus::WrongChannel));
    }
    before(
        deadline,
        application.observe_channel_teletext_document_selection(channel_id),
    )
    .await?
    .map_err(|error| preparation_error(&error))
}

async fn checked_selection(
    selection: TeletextChannelSelectionSnapshot,
    expected: &str,
    operation: &Operation,
) -> Result<TeletextChannelSelectionSnapshot, TeletextAuthoringError> {
    if expected.len() != 64
        || !expected
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Err(failure(TeletextAuthoringStatus::InvalidInput));
    }
    let maximum = operation.policy.maximum_producers();
    let deadline = operation.deadline;
    let (selection, digest) =
        teletext_work::spawn_blocking(operation.guard.hold_work(), move || {
            let digest = selection_digest(&selection, maximum, deadline)?;
            Ok::<_, TeletextAuthoringError>((selection, digest))
        })
        .await
        .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))??;
    if digest != expected {
        return Err(failure(TeletextAuthoringStatus::SelectionChanged));
    }
    Ok(selection)
}

fn selection_digest(
    selection: &TeletextChannelSelectionSnapshot,
    maximum: usize,
    deadline: Instant,
) -> Result<String, TeletextAuthoringError> {
    check_deadline(deadline)?;
    if selection.bindings.is_empty() {
        return Err(failure(TeletextAuthoringStatus::Off));
    }
    if selection.bindings.len() > maximum {
        return Err(failure(TeletextAuthoringStatus::ResourceLimit));
    }
    let mut hash = Sha256::new();
    hash.update(b"ErsatzRS.Teletext.AuthoringSelection.v1\0");
    hash.update(selection.channel_id.to_be_bytes());
    hash.update(
        u64::try_from(selection.bindings.len())
            .unwrap_or(u64::MAX)
            .to_be_bytes(),
    );
    for binding in &selection.bindings {
        check_deadline(deadline)?;
        if validate_teletext_profile_values("observed selection", &binding.effective_settings)
            .is_err()
        {
            return Err(failure(TeletextAuthoringStatus::SelectionUnavailable));
        }
        hash.update(binding.contract.schema_version().to_be_bytes());
        for value in [
            binding.channel_id,
            binding.binding_revision,
            binding.profile_id,
            binding.profile_revision,
        ] {
            hash.update(value.to_be_bytes());
        }
        let pin = &binding.installation;
        for value in [
            &binding.addon_id,
            &pin.addon_id,
            &pin.state_revision,
            &pin.version,
            &pin.package_sha256,
            &pin.content_sha256,
            &pin.manifest_sha256,
        ] {
            frame(&mut hash, value.as_bytes());
        }
        let mut settings: Vec<_> = binding.effective_settings.iter().collect();
        settings.sort_unstable_by(|left, right| left.key.cmp(&right.key));
        hash.update(
            u64::try_from(settings.len())
                .unwrap_or(u64::MAX)
                .to_be_bytes(),
        );
        for setting in settings {
            frame(&mut hash, setting.key.as_bytes());
            let (kind, value) = match &setting.value {
                TeletextProfileValue::Literal(value) => (0, value),
                TeletextProfileValue::SecretReference {
                    kind: TeletextSecretReferenceKind::Environment,
                    reference,
                } => (1, reference),
                TeletextProfileValue::SecretReference {
                    kind: TeletextSecretReferenceKind::File,
                    reference,
                } => (2, reference),
            };
            hash.update([kind]);
            frame(&mut hash, value.as_bytes());
        }
    }
    check_deadline(deadline)?;
    let digest = hash.finalize();
    let mut result = String::with_capacity(64);
    for byte in digest {
        write!(result, "{byte:02x}").map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?;
    }
    Ok(result)
}
fn frame(hash: &mut Sha256, value: &[u8]) {
    hash.update(u64::try_from(value.len()).unwrap_or(u64::MAX).to_be_bytes());
    hash.update(value);
}

fn check_deadline(deadline: Instant) -> Result<(), TeletextAuthoringError> {
    if Instant::now() >= deadline {
        Err(failure(TeletextAuthoringStatus::TimedOut))
    } else {
        Ok(())
    }
}
async fn before<T>(
    deadline: Instant,
    work: impl std::future::Future<Output = T>,
) -> Result<T, TeletextAuthoringError> {
    check_deadline(deadline)?;
    let result = tokio::time::timeout_at(tokio::time::Instant::from_std(deadline), work)
        .await
        .map_err(|_| failure(TeletextAuthoringStatus::TimedOut))?;
    check_deadline(deadline)?;
    Ok(result)
}
fn work_error(error: &TeletextWorkError) -> TeletextAuthoringError {
    failure(match error {
        TeletextWorkError::TimedOut => TeletextAuthoringStatus::TimedOut,
        TeletextWorkError::Saturated => TeletextAuthoringStatus::Busy,
        TeletextWorkError::InvalidConfiguration => TeletextAuthoringStatus::WorkConfiguration,
        TeletextWorkError::ConfigurationUnavailable => TeletextAuthoringStatus::Unavailable,
    })
}
fn preparation_error(error: &ChannelTeletextDocumentError) -> TeletextAuthoringError {
    failure(match error {
        ChannelTeletextDocumentError::TimedOut => TeletextAuthoringStatus::TimedOut,
        ChannelTeletextDocumentError::SelectionChanged => TeletextAuthoringStatus::SelectionChanged,
        ChannelTeletextDocumentError::SelectionRejected(_) => {
            TeletextAuthoringStatus::SelectionUnavailable
        }
        ChannelTeletextDocumentError::StorageUnavailable => TeletextAuthoringStatus::Unavailable,
        ChannelTeletextDocumentError::Runtime { .. } => TeletextAuthoringStatus::ProducerFailed,
        ChannelTeletextDocumentError::ProducerBudget { .. }
        | ChannelTeletextDocumentError::BindingCount => TeletextAuthoringStatus::ResourceLimit,
        ChannelTeletextDocumentError::Document { .. } => TeletextAuthoringStatus::InvalidPages,
        _ => TeletextAuthoringStatus::InvalidInput,
    })
}
pub(crate) fn design_error(error: NativeTeletextDesignError) -> TeletextAuthoringError {
    match error {
        NativeTeletextDesignError::Work(error) => work_error(&error),
        NativeTeletextDesignError::Encoding(error) => channel_error(*error),
        NativeTeletextDesignError::TimedOut => failure(TeletextAuthoringStatus::TimedOut),
        NativeTeletextDesignError::WorkerFailed => failure(TeletextAuthoringStatus::WorkerFailed),
        NativeTeletextDesignError::Storage(_) => failure(TeletextAuthoringStatus::StorageFailed),
        _ => failure(TeletextAuthoringStatus::InvalidPages),
    }
}
fn channel_error(error: ChannelTeletextDocumentEncodingError) -> TeletextAuthoringError {
    match error {
        ChannelTeletextDocumentEncodingError::Work(error) => work_error(&error),
        ChannelTeletextDocumentEncodingError::Preparation(error) => preparation_error(&error),
        ChannelTeletextDocumentEncodingError::TimedOut => {
            failure(TeletextAuthoringStatus::TimedOut)
        }
        ChannelTeletextDocumentEncodingError::WorkerFailed => {
            failure(TeletextAuthoringStatus::WorkerFailed)
        }
        ChannelTeletextDocumentEncodingError::Encoding(error) => {
            let mut location = TeletextAuthoringLocation {
                owner: error.display,
                resource: error.resource,
                position: None,
                source: None,
            };
            authored_location(&error.reason, &mut location);
            if let Some(TeletextMainCompositionSource::BaseCell { position, .. }) = location.source
            {
                location.position = Some((u16::from(position.row()), u16::from(position.column())));
            }
            let status = match error.reason {
                TeletextAuthoredServiceEncodingFailure::TimedOut => {
                    TeletextAuthoringStatus::TimedOut
                }
                TeletextAuthoredServiceEncodingFailure::Budget { .. }
                | TeletextAuthoredServiceEncodingFailure::Size { .. } => {
                    TeletextAuthoringStatus::ResourceLimit
                }
                _ => TeletextAuthoringStatus::EncodingFailed,
            };
            TeletextAuthoringError {
                status,
                location: Some(location),
            }
        }
        _ => failure(TeletextAuthoringStatus::EncodingFailed),
    }
}
fn authored_location(
    reason: &TeletextAuthoredServiceEncodingFailure,
    location: &mut TeletextAuthoringLocation,
) {
    match reason {
        TeletextAuthoredServiceEncodingFailure::Layout(error) => layout_location(error, location),
        TeletextAuthoredServiceEncodingFailure::Composition(error) => {
            location.owner = Some(error.owner);
            location.source = error.location;
        }
        TeletextAuthoredServiceEncodingFailure::Native(error) => {
            if let TeletextNativeDefaultPageEncodingFailure::Base(error) = &error.reason {
                layout_location(error, location);
            }
        }
        TeletextAuthoredServiceEncodingFailure::Compiled(error) => match &error.reason {
            TeletextCompiledDisplayPageEncodingFailure::Base(error) => {
                layout_location(error, location);
            }
            TeletextCompiledDisplayPageEncodingFailure::Normalization(error) => {
                if let TeletextNormalizedMainProgramFailure::Original(error) = &error.reason {
                    match error.as_ref() {
                        TeletextComposedMainEncodingError::Composition(error) => {
                            location.owner = Some(error.owner);
                            location.source = error.location;
                        }
                        TeletextComposedMainEncodingError::Packing(error) => {
                            location.owner = Some(error.owner);
                            location.source = error.location;
                        }
                        TeletextComposedMainEncodingError::Resource(_) => (),
                    }
                }
            }
            _ => (),
        },
        _ => (),
    }
}
fn layout_location(error: &TeletextEncodingError, location: &mut TeletextAuthoringLocation) {
    match error {
        TeletextEncodingError::Character {
            address,
            row,
            column,
            ..
        }
        | TeletextEncodingError::ControlCells {
            address,
            row,
            column,
        }
        | TeletextEncodingError::BlackForeground {
            address,
            row,
            column,
        }
        | TeletextEncodingError::HiddenContent {
            address,
            row,
            column,
        } => {
            location.owner = Some(*address);
            location.position = Some((u16::from(*row), u16::from(*column)));
        }
        _ => (),
    }
}

/// Resolve existing operator-owned authoring bounds for an HTTP editor transfer.
/// No admission is retained: export/import have their own existing work owners.
/// Transfer caps constrain storage/TTI bounds without inventing engine defaults.
/// # Errors
/// Rejects unavailable/invalid policy, zero bounds or the original deadline.
pub(crate) async fn editorial_teletext_policy(
    application: &ApplicationServices,
    deadline: Instant,
    max_transfer_bytes: usize,
) -> Result<
    (
        super::teletext_design_exports::NativeTeletextExportPolicy,
        Instant,
    ),
    TeletextAuthoringError,
> {
    let started = Instant::now();
    check_deadline(deadline)?;
    if max_transfer_bytes == 0 {
        return Err(failure(TeletextAuthoringStatus::InvalidInput));
    }
    let raw = before(
        deadline,
        application.get_optional_setting(NATIVE_TELETEXT_AUTHORING_POLICY_SETTING),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::Unavailable))?
    .ok_or_else(|| failure(TeletextAuthoringStatus::MissingPolicy))?;
    let policy = parse_native_teletext_authoring_policy(&raw)
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidPolicy))?;
    let deadline = deadline.min(
        started
            .checked_add(policy.operation_timeout)
            .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidPolicy))?,
    );
    check_deadline(deadline)?;
    let encoding = policy.reload_encoding_policy();
    let mut document = policy.design;
    document.max_serialized_bytes = document.max_serialized_bytes.min(max_transfer_bytes);
    let project = ersatzrs_app::teletext_export_project::TeletextExportProjectBudget {
        max_bytes: document.max_serialized_bytes,
        max_files: encoding.encoding.max_units,
        max_producers: document.max_documents,
        max_file_bytes: document.max_serialized_bytes,
        max_units: encoding.encoding.max_units,
    };
    let tti = ersatzrs_app::teletext_tti::TeletextTtiBudget {
        max_bytes: document.max_serialized_bytes,
        max_records: encoding.encoding.max_units,
    };
    Ok((
        super::teletext_design_exports::NativeTeletextExportPolicy {
            document,
            encoding,
            project,
            tti,
        },
        deadline,
    ))
}

/// Join bounded editor transfer work using its caller's real admission hold.
/// The hold stays with the actual worker after cancellation; no native export or
/// import admission is nested here. Callbacks perform no durable mutations.
pub(crate) async fn editorial_transfer_work<T: Send + 'static>(
    hold: TeletextWorkHold,
    deadline: Instant,
    work: impl FnOnce() -> T + Send + 'static,
) -> Result<T, TeletextAuthoringError> {
    teletext_work::spawn_blocking(hold, move || {
        check_deadline(deadline)?;
        let result = work();
        check_deadline(deadline)?;
        Ok(result)
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
}
