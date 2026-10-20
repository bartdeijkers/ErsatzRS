//! Private historical role dispatch; effective origins never become base bindings.

use super::{
    teletext_channel_documents::TeletextChannelDocumentEncodingPolicy,
    teletext_designs::{
        compile_restored_parts, compile_restored_parts_with_resource_edits,
        NativeTeletextDesignError, RestoredOriginalTeletextDesign,
    },
    teletext_effective_designs::{
        compile_effective_restored_parts, compile_effective_restored_parts_with_resource_edits,
        RestoredEffectiveOriginalDesign,
    },
};
use ersatzrs_app::teletext_design_documents::{
    decode_teletext_design_documents, decode_teletext_effective_design_documents,
    TeletextDesignDocumentBudget, TeletextDesignDocumentError, TeletextDesignDocumentFailure,
    TeletextRestoredDesignDocumentParts, TeletextRestoredEffectiveDesignDocumentParts,
    TeletextSavedDocumentSource, TeletextSavedEditorialOverlay,
};
use ersatzrs_domain::{
    TeletextDesignRevision, TeletextPhysicalBaseOverlay, TeletextProducerCompositionMetadata,
    TeletextProducerPresentation, TeletextResourceObjectOperandOverlay,
};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextEncodingRegion};
use std::time::Instant;

pub(super) enum StoredDesignParts {
    Base(TeletextRestoredDesignDocumentParts),
    Effective(TeletextRestoredEffectiveDesignDocumentParts),
}

/// Exact stored resource intentions; this value alone grants no edit authority.
#[derive(Clone, Copy)]
pub(super) struct EditorialResourceIntents<'a> {
    pub(super) operands: &'a [TeletextResourceObjectOperandOverlay],
    pub(super) programs: &'a [ersatzrs_domain::TeletextResourceObjectProgramOverlay],
    pub(super) additions: &'a [ersatzrs_domain::TeletextResourceObjectDefinitionAddition],
}
impl StoredDesignParts {
    pub(super) fn decode(
        stored: &TeletextDesignRevision,
        budget: TeletextDesignDocumentBudget,
    ) -> Result<Self, TeletextDesignDocumentError> {
        match stored.document_version {
            ersatzrs_domain::TELETEXT_DESIGN_DOCUMENT_VERSION => Ok(Self::Base(
                decode_teletext_design_documents(&stored.document, budget)?.into_parts(),
            )),
            ersatzrs_domain::TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION => Ok(Self::Effective(
                decode_teletext_effective_design_documents(&stored.document, budget)?.into_parts(),
            )),
            _ => Err(TeletextDesignDocumentError {
                binding: None,
                reason: TeletextDesignDocumentFailure::Version,
            }),
        }
    }
    pub(super) fn overlay(&self) -> Option<&TeletextSavedEditorialOverlay> {
        match self {
            Self::Base(parts) => parts.editorial_overlay.as_ref(),
            Self::Effective(parts) => parts.editorial_overlay.as_ref(),
        }
    }
    pub(super) fn compile(
        self,
        stored: TeletextDesignRevision,
        policy: TeletextChannelDocumentEncodingPolicy,
        deadline: Instant,
        overlays: Vec<TeletextPhysicalBaseOverlay>,
    ) -> Result<RestoredStoredTeletextDesign, NativeTeletextDesignError> {
        match self {
            Self::Base(parts) => compile_restored_parts(stored, parts, policy, deadline, overlays)
                .map(RestoredStoredTeletextDesign::Base),
            Self::Effective(parts) => {
                compile_effective_restored_parts(stored, parts, policy, deadline, overlays)
                    .map(RestoredStoredTeletextDesign::Effective)
            }
        }
    }

    /// Dispatch exact original-bound scalar and structural resource intent.
    pub(super) fn compile_with_resource_edits(
        self,
        stored: TeletextDesignRevision,
        policy: TeletextChannelDocumentEncodingPolicy,
        deadline: Instant,
        overlays: Vec<TeletextPhysicalBaseOverlay>,
        resource_operands: &[TeletextResourceObjectOperandOverlay],
        resource_programs: &[ersatzrs_domain::TeletextResourceObjectProgramOverlay],
    ) -> Result<RestoredStoredTeletextDesign, NativeTeletextDesignError> {
        match self {
            Self::Base(parts) => compile_restored_parts_with_resource_edits(
                stored,
                parts,
                policy,
                deadline,
                overlays,
                resource_operands,
                resource_programs,
            )
            .map(RestoredStoredTeletextDesign::Base),
            Self::Effective(parts) => compile_effective_restored_parts_with_resource_edits(
                stored,
                parts,
                policy,
                deadline,
                overlays,
                resource_operands,
                resource_programs,
            )
            .map(RestoredStoredTeletextDesign::Effective),
        }
    }

    pub(super) fn compile_with_resource_intents(
        self,
        stored: TeletextDesignRevision,
        policy: TeletextChannelDocumentEncodingPolicy,
        deadline: Instant,
        overlays: Vec<TeletextPhysicalBaseOverlay>,
        resources: EditorialResourceIntents<'_>,
    ) -> Result<RestoredStoredTeletextDesign, NativeTeletextDesignError> {
        if resources.additions.is_empty() {
            return self.compile_with_resource_edits(
                stored,
                policy,
                deadline,
                overlays,
                resources.operands,
                resources.programs,
            );
        }
        match self {
            Self::Base(parts) => super::teletext_designs::compile_restored_parts_with_resource_intents(
                stored, parts, policy, deadline, overlays, resources,
            ).map(RestoredStoredTeletextDesign::Base),
            Self::Effective(parts) => super::teletext_effective_designs::compile_effective_restored_parts_with_resource_intents(
                stored, parts, policy, deadline, overlays, resources,
            ).map(RestoredStoredTeletextDesign::Effective),
        }
    }
}

/// Match all stored resource intent exactly before admitting authenticated replay.
pub(super) fn check_authenticated_editorial_resource_replay(
    overlay: Option<&TeletextSavedEditorialOverlay>,
    has_physical_rows: bool,
    resource_operands: &[TeletextResourceObjectOperandOverlay],
    resource_programs: &[ersatzrs_domain::TeletextResourceObjectProgramOverlay],
) -> Result<(), NativeTeletextDesignError> {
    check_authenticated_editorial_resource_intents(
        overlay,
        has_physical_rows,
        EditorialResourceIntents {
            operands: resource_operands,
            programs: resource_programs,
            additions: &[],
        },
    )
}

pub(super) fn check_authenticated_editorial_resource_intents(
    overlay: Option<&TeletextSavedEditorialOverlay>,
    has_physical_rows: bool,
    intents: EditorialResourceIntents<'_>,
) -> Result<(), NativeTeletextDesignError> {
    let resource_operands = intents.operands;
    let resource_programs = intents.programs;
    if overlay.is_some_and(|overlay| !overlay.object_resource_additions().is_empty())
        || overlay.map_or(
            &[][..],
            TeletextSavedEditorialOverlay::resource_object_operands,
        ) != resource_operands
        || overlay.map_or(
            &[][..],
            TeletextSavedEditorialOverlay::resource_object_program_overlays,
        ) != resource_programs
        || overlay.is_some_and(|overlay| !overlay.pages().is_empty() && !has_physical_rows)
        || overlay.map_or(
            &[][..],
            TeletextSavedEditorialOverlay::resource_object_definition_additions,
        ) != intents.additions
    {
        return Err(NativeTeletextDesignError::EditorialValidationUnavailable);
    }
    Ok(())
}

pub(super) enum RestoredStoredTeletextDesign {
    Base(RestoredOriginalTeletextDesign),
    Effective(RestoredEffectiveOriginalDesign),
}
impl RestoredStoredTeletextDesign {
    pub(super) fn view(&self) -> OriginalDesignView<'_> {
        match self {
            Self::Base(value) => OriginalDesignView::Base(value),
            Self::Effective(value) => OriginalDesignView::Effective(value),
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum OriginalDesignView<'a> {
    Base(&'a RestoredOriginalTeletextDesign),
    Effective(&'a RestoredEffectiveOriginalDesign),
}
impl<'a> From<&'a RestoredOriginalTeletextDesign> for OriginalDesignView<'a> {
    fn from(value: &'a RestoredOriginalTeletextDesign) -> Self {
        Self::Base(value)
    }
}

pub(super) async fn reload_stored_with_hold(
    application: &ersatzrs_app::ApplicationServices,
    id: i64,
    revision: i64,
    budget: TeletextDesignDocumentBudget,
    policy: TeletextChannelDocumentEncodingPolicy,
    deadline: Instant,
    hold: ersatzrs_core::locking::TeletextWorkHold,
) -> Result<Option<RestoredStoredTeletextDesign>, NativeTeletextDesignError> {
    let Some(stored) = application
        .teletext_design_revision(id, revision, budget.max_serialized_bytes)
        .await?
    else {
        return Ok(None);
    };
    match stored.document_version {
        ersatzrs_domain::TELETEXT_DESIGN_DOCUMENT_VERSION => super::teletext_designs::reload_original_teletext_design_revision_with_hold(application,id,revision,budget,policy,deadline,hold).await.map(|value| value.map(RestoredStoredTeletextDesign::Base)),
        ersatzrs_domain::TELETEXT_EFFECTIVE_DESIGN_DOCUMENT_VERSION => super::teletext_effective_designs::reload_effective_original_teletext_design_revision_with_hold(application,id,revision,budget,policy,deadline,hold).await.map(|value| value.map(RestoredStoredTeletextDesign::Effective)),
        _ => Err(TeletextDesignDocumentError { binding:None,reason:TeletextDesignDocumentFailure::Version }.into()),
    }
}
impl<'a> OriginalDesignView<'a> {
    pub(super) fn design_id(self) -> i64 {
        match self {
            Self::Base(v) => v.design_id(),
            Self::Effective(v) => v.design_id(),
        }
    }
    pub(super) fn revision(self) -> i64 {
        match self {
            Self::Base(v) => v.revision(),
            Self::Effective(v) => v.revision(),
        }
    }
    pub(super) fn digest(self) -> &'a str {
        match self {
            Self::Base(v) => v.digest(),
            Self::Effective(v) => v.digest(),
        }
    }
    pub(super) fn channel_id(self) -> i64 {
        match self {
            Self::Base(v) => v.selection().channel_id,
            Self::Effective(v) => v.selection().channel_id,
        }
    }
    pub(super) fn document_version(self) -> u16 {
        match self {
            Self::Base(_) => 1,
            Self::Effective(_) => 2,
        }
    }
    pub(super) fn producer_count(self) -> usize {
        match self {
            Self::Base(v) => v.selection().bindings.len(),
            Self::Effective(v) => v.producer_bindings().len(),
        }
    }
    pub(super) fn sources(self) -> &'a [TeletextSavedDocumentSource] {
        match self {
            Self::Base(v) => v.sources(),
            Self::Effective(v) => v.sources(),
        }
    }
    pub(super) fn producers(self) -> &'a [TeletextProducerPresentation] {
        match self {
            Self::Base(v) => v.producers(),
            Self::Effective(v) => v.producers(),
        }
    }
    pub(super) fn encoded(self) -> Option<&'a EncodedTeletextAuthoredService> {
        match self {
            Self::Base(v) => v.encoded(),
            Self::Effective(v) => v.encoded(),
        }
    }
    pub(super) fn metadata(self) -> Option<&'a TeletextProducerCompositionMetadata> {
        match self {
            Self::Base(v) => v.metadata(),
            Self::Effective(v) => v.metadata(),
        }
    }
    pub(super) fn v1_region(self) -> Option<TeletextEncodingRegion> {
        match self {
            Self::Base(v) => v.v1_region(),
            Self::Effective(v) => v.v1_region(),
        }
    }
}

#[cfg(test)]
#[path = "teletext_stored_designs/addition_tests.rs"]
mod addition_tests;
