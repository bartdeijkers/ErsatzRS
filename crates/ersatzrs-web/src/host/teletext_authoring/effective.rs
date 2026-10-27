//! Effective original roles reuse native compilation and independent scoped pointers.

use ersatzrs_domain::TeletextEffectiveBindingSnapshot;

use super::{
    before, channel_error, check_deadline, design_error, failure, generation_error,
    matches_original, parse_native_teletext_authoring_input, preview_with_operation,
    publication_error, publication_rejection, render_service, scope, teletext_work,
    ApplicationServices, AuthoringSelection, Instant, Operation, ProgrammeClock,
    SuppliedTeletextGuideContext, TeletextAuthoringError, TeletextAuthoringIdentity,
    TeletextAuthoringPending, TeletextAuthoringPrepare, TeletextAuthoringPreview,
    TeletextAuthoringPreviewRequest, TeletextAuthoringPublicationTarget,
    TeletextAuthoringPublishTarget, TeletextAuthoringSaved, TeletextAuthoringSelected,
    TeletextAuthoringStatus, TeletextDesignMutation, TeletextGenerationMutation,
    TeletextGenerationRejection, TeletextGenerationTarget, TeletextPublicationMutation, Utc,
};
use crate::host::{
    teletext_effective_channel_documents::prepare_effective_channel_teletext_document_candidate_checked_with_hold,
    teletext_effective_designs::{
        reload_effective_original_teletext_design_revision_with_hold,
        save_effective_original_teletext_design_candidate_with_hold,
        RestoredEffectiveOriginalDesign,
    },
    teletext_effective_generations::{
        accept_effective_original_teletext_generation_with_hold, frozen_original,
    },
    teletext_effective_publications::{
        request_effective_original_publication_with_hold, NativeEffectiveTeletextPublicationError,
        TeletextEffectivePublicationTarget,
    },
    teletext_runtime_policy::NativeTeletextAuthoringInput,
};

pub(super) async fn prepare_design(
    application: &ApplicationServices,
    request: TeletextAuthoringPrepare,
    operation: &Operation,
) -> Result<TeletextAuthoringSaved, TeletextAuthoringError> {
    let selection = scope::observed(application, request.channel_id, operation.deadline).await?;
    let selection = scope::checked(selection, request.selection_digest, operation).await?;
    let absent = !selection
        .bindings
        .iter()
        .any(TeletextEffectiveBindingSnapshot::enabled);
    let input = if absent {
        if !request.input_json.is_empty() {
            return Err(failure(TeletextAuthoringStatus::InvalidInput));
        }
        None
    } else {
        let input = parse_native_teletext_authoring_input(&request.input_json, &operation.policy)
            .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
        if input.channel_id() != request.channel_id {
            return Err(failure(TeletextAuthoringStatus::WrongChannel));
        }
        Some(input)
    };
    let policy = input
        .as_ref()
        .map_or_else(
            || Ok(operation.policy.reload_encoding_policy()),
            |input| operation.policy.encoding_policy(input),
        )
        .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
    let (locale, timezone, locales) = context_configuration(application, operation, absent).await?;
    let generation = uuid::Uuid::new_v4().to_string();
    let context = SuppliedTeletextGuideContext {
        generation: &generation,
        locale,
        supported_locales: &locales,
        timezone: &timezone,
        captured_at: Utc::now(),
        channels: &[],
    };
    let candidate = prepare_effective_channel_teletext_document_candidate_checked_with_hold(
        application,
        request.channel_id,
        input
            .as_ref()
            .map(NativeTeletextAuthoringInput::entry_addon_id),
        &context,
        policy,
        &selection,
        (operation.deadline, operation.guard.hold_work()),
    )
    .await
    .map_err(channel_error)?;
    if candidate.selection() != &selection || candidate.encoded().is_none() != absent {
        return Err(failure(TeletextAuthoringStatus::SelectionChanged));
    }
    let saved = save_effective_original_teletext_design_candidate_with_hold(
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
    drop(saved);
    // A settled save is truthful even when later scope/preview checks fail.
    let preview = match application
        .recheck_effective_channel_teletext_document_selection(&selection, operation.deadline)
        .await
    {
        Ok(()) => {
            preview_with_operation(
                application,
                TeletextAuthoringPreviewRequest {
                    channel_id: identity.channel_id,
                    design_id: identity.design_id,
                    revision: identity.revision,
                    owner: input.as_ref().map(NativeTeletextAuthoringInput::entry),
                },
                operation,
            )
            .await
        }
        Err(error) => Err(super::preparation_error(&error)),
    };
    Ok(TeletextAuthoringSaved { identity, preview })
}

async fn context_configuration(
    application: &ApplicationServices,
    operation: &Operation,
    absent: bool,
) -> Result<(&'static str, String, Vec<&'static str>), TeletextAuthoringError> {
    Ok(if absent {
        // These fields are unused: zero producer contracts create no guide
        // request. No fictitious content/entry or regional choice is captured.
        ("", String::new(), Vec::new())
    } else {
        let locale = before(
            operation.deadline,
            application.get_optional_setting("pages.language"),
        )
        .await?
        .map_err(|_| failure(TeletextAuthoringStatus::Unavailable))?;
        let locale = crate::host::teletext_guide::resolve_locale(locale.as_deref())
            .map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
        let clock =
            ProgrammeClock::system().map_err(|_| failure(TeletextAuthoringStatus::InvalidInput))?;
        let timezone = clock
            .iana_name()
            .ok_or_else(|| failure(TeletextAuthoringStatus::InvalidInput))?
            .to_owned();
        (
            locale,
            timezone,
            crate::host::teletext_guide::supported_locales(),
        )
    })
}

pub(super) async fn preview_design(
    application: &ApplicationServices,
    request: TeletextAuthoringPreviewRequest,
    operation: &Operation,
) -> Result<TeletextAuthoringPreview, TeletextAuthoringError> {
    let restored = reload_effective_original_teletext_design_revision_with_hold(
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
    let deadline = operation.deadline;
    let hold = operation.guard.hold_work();
    let budget = operation.policy.preview;
    teletext_work::spawn_blocking(hold.clone(), move || {
        render_original(&restored, request, budget, deadline, hold)
    })
    .await
    .map_err(|_| failure(TeletextAuthoringStatus::WorkerFailed))?
}

fn render_original(
    restored: &RestoredEffectiveOriginalDesign,
    request: TeletextAuthoringPreviewRequest,
    budget: crate::host::teletext_preview::TeletextPreviewBudget,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<TeletextAuthoringPreview, TeletextAuthoringError> {
    check_deadline(deadline)?;
    let (raster, navigation, pages) = match (restored.encoded(), request.owner) {
        (Some(service), Some(owner)) => {
            let (raster, navigation, pages) = render_service(service, owner, budget)?;
            (Some(raster), navigation, pages)
        }
        (None, None)
            if !restored
                .selection()
                .bindings
                .iter()
                .any(TeletextEffectiveBindingSnapshot::enabled) =>
        {
            (None, Vec::new(), Vec::new())
        }
        _ => return Err(failure(TeletextAuthoringStatus::InvalidInput)),
    };
    check_deadline(deadline)?;
    Ok(TeletextAuthoringPreview {
        identity: TeletextAuthoringIdentity {
            channel_id: request.channel_id,
            design_id: request.design_id,
            revision: request.revision,
        },
        digest: restored.digest().to_owned(),
        static_original: frozen_original(restored),
        selection: AuthoringSelection::Effective(restored.selection().clone()),
        publication: None,
        raster,
        navigation,
        pages,
        resource_objects: restored
            .encoded()
            .map(super::resource_objects)
            .unwrap_or_default(),
        removable_resource_objects: restored
            .encoded()
            .map(|service| {
                super::object_removals::observe(
                    service,
                    restored.added_resource_objects(),
                    deadline,
                )
            })
            .transpose()?
            .unwrap_or_default(),
        object_resources: restored
            .encoded()
            .map(super::object_resources)
            .unwrap_or_default(),
        object_choices: restored
            .encoded()
            .map(|service| {
                super::object_choices::observe(
                    service,
                    restored.object_page_attachments(),
                    deadline,
                )
            })
            .transpose()?
            .unwrap_or_default(),
        authoring: super::metadata::effective(restored, request.owner, deadline)?,
        hold,
        deadline,
    })
}

pub(super) async fn request_publication(
    application: &ApplicationServices,
    target: TeletextAuthoringPublicationTarget,
    operation: &Operation,
) -> Result<TeletextAuthoringPending, TeletextAuthoringError> {
    let identity = target.identity;
    let mutation = request_effective_original_publication_with_hold(
        application,
        TeletextEffectivePublicationTarget {
            channel_id: identity.channel_id,
            design_id: identity.design_id,
            revision: identity.revision,
            expected_digest: target.digest,
            expected_design_head: target.expected_design_head,
            expected_request_id: target.expected_request_id,
        },
        operation.policy.design,
        operation.policy.reload_encoding_policy(),
        operation.deadline,
        operation.guard.hold_work(),
    )
    .await
    .map_err(|error| match error {
        NativeEffectiveTeletextPublicationError::Design(error) => design_error(error),
        NativeEffectiveTeletextPublicationError::Storage(error) => {
            publication_error(super::NativeTeletextPublicationError::Request(error))
        }
        NativeEffectiveTeletextPublicationError::GuideFreshnessRequired => {
            failure(TeletextAuthoringStatus::SelectionUnavailable)
        }
    })?;
    match mutation {
        TeletextPublicationMutation::Requested(requested) => Ok(TeletextAuthoringPending {
            identity,
            request_id: requested.request().id,
        }),
        TeletextPublicationMutation::Rejected(reason) => Err(publication_rejection(reason)),
    }
}

pub(super) async fn publish_generation(
    application: &ApplicationServices,
    target: TeletextAuthoringPublishTarget,
    operation: &Operation,
) -> Result<TeletextAuthoringSelected, TeletextAuthoringError> {
    let publication = target.publication;
    let request = before(
        operation.deadline,
        application.effective_teletext_publication_request(publication.identity.channel_id),
    )
    .await?
    .map_err(|_| failure(TeletextAuthoringStatus::StorageFailed))?
    .filter(|request| Some(request.id) == publication.expected_request_id)
    .ok_or_else(|| failure(TeletextAuthoringStatus::SelectionChanged))?;
    if !matches_original(&request, publication.identity, &publication.digest) {
        return Err(failure(TeletextAuthoringStatus::SelectionChanged));
    }
    let mutation = accept_effective_original_teletext_generation_with_hold(
        application,
        TeletextGenerationTarget {
            request,
            expected_design_head: publication.expected_design_head,
            expected_generation_id: target.expected_generation_id,
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
