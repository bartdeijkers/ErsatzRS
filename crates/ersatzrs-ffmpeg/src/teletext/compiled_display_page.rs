//! Complete display units from the same checked normalization candidate.
//!
//! Service compilation owns MOT/resource pages and magazine scheduling. This
//! private constructor receives their bounded default reservation list rather
//! than accepting a detached allocation or source map from external callers.

use std::time::Instant;

use ersatzrs_domain::{
    TeletextDefaultObjectSelection, TeletextPageAddress, TeletextSemanticService,
};

use super::compile_context::TeletextCompileContext;
use super::normalized_main_program::compile_normalized_with_context;
use super::packets::{self, PageEnhancements, PagePacketMetadata};
use super::resource_transmission::ResourceTransmissionContext;
use super::{
    layout, presentation, EncodedTeletextNormalizedMainProgram, EncodedTeletextPage,
    TeletextEncodingError, TeletextEncodingRegion, TeletextNormalizedMainProgramBudget,
    TeletextNormalizedMainProgramError, TeletextNormalizedMainProgramFailure,
};

/// Owned display units and their checked original/actual models from one call.
/// The separate magazine units still require magazine-level scheduling.
#[derive(Debug)]
pub struct EncodedTeletextCompiledDisplayPage {
    normalized: EncodedTeletextNormalizedMainProgram,
    page: EncodedTeletextPage,
    magazine_units: Vec<[u8; 46]>,
}

impl EncodedTeletextCompiledDisplayPage {
    #[must_use]
    pub const fn normalized(&self) -> &EncodedTeletextNormalizedMainProgram {
        &self.normalized
    }

    #[must_use]
    pub const fn page(&self) -> &EncodedTeletextPage {
        &self.page
    }

    /// Actual M29/0, including when overridden by an actual page X28/0.
    #[must_use]
    pub fn magazine_units(&self) -> &[[u8; 46]] {
        &self.magazine_units
    }
}

#[derive(Debug, thiserror::Error)]
#[error("compiled display page {owner:?}: {reason}")]
pub struct TeletextCompiledDisplayPageEncodingError {
    pub owner: TeletextPageAddress,
    pub reason: TeletextCompiledDisplayPageEncodingFailure,
}

#[derive(Debug, thiserror::Error)]
pub enum TeletextCompiledDisplayPageEncodingFailure {
    #[error("compiled display exceeded its original deadline")]
    TimedOut,
    #[error("the exact page/subcode does not exist")]
    MissingPage,
    #[error("normalization failed: {0}")]
    Normalization(Box<TeletextNormalizedMainProgramError>),
    #[error(transparent)]
    Base(Box<TeletextEncodingError>),
}

/// Encode one exact display using the existing owned normalization boundary.
/// This standalone path reserves no MOT defaults. Complete service compilation
/// supplies its original default list through the private constructor instead.
/// Neither path grants whole-service admission or publication authority.
/// # Errors
/// Returns exact-owner, normalization or packet/layout errors without partial
/// display units. The immutable service and ordinary page encoder are unchanged.
pub fn encode_teletext_normalized_display_page(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    region: TeletextEncodingRegion,
    budget: TeletextNormalizedMainProgramBudget,
) -> Result<EncodedTeletextCompiledDisplayPage, TeletextCompiledDisplayPageEncodingError> {
    encode_teletext_compiled_display_page(service, owner, region, &[], &budget, None)
}

/// Assemble actual metadata, normalized X26 and original fallback/header rows.
/// No legacy X26 is reinserted and no effective fallback becomes a synthetic
/// page declaration. Resource allocations use the same default reservations in
/// both normalization passes; generated Local definitions remain unallocated.
/// Failures discard the complete owned candidate without returning page units.
pub(super) fn encode_teletext_compiled_display_page(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    region: TeletextEncodingRegion,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    resources: Option<&ResourceTransmissionContext<'_>>,
) -> Result<EncodedTeletextCompiledDisplayPage, TeletextCompiledDisplayPageEncodingError> {
    encode_teletext_compiled_display_page_with_context(
        service,
        owner,
        region,
        defaults,
        budget,
        &mut TeletextCompileContext::new(resources, None, None),
    )
}

/// Keep the service's original deadline through normalization and page assembly.
/// Checks are cooperative between bounded stages, without preempting a running
/// stage or returning a partial candidate. `None` preserves standalone encoding.
#[cfg(test)]
pub(super) fn encode_teletext_compiled_display_page_before(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    region: TeletextEncodingRegion,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    resources: Option<&ResourceTransmissionContext<'_>>,
    deadline: Option<Instant>,
) -> Result<EncodedTeletextCompiledDisplayPage, TeletextCompiledDisplayPageEncodingError> {
    encode_teletext_compiled_display_page_with_context(
        service,
        owner,
        region,
        defaults,
        budget,
        &mut TeletextCompileContext::new(resources, deadline, None),
    )
}

pub(super) fn encode_teletext_compiled_display_page_with_context(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    region: TeletextEncodingRegion,
    defaults: &[TeletextDefaultObjectSelection],
    budget: &TeletextNormalizedMainProgramBudget,
    context: &mut TeletextCompileContext<'_, '_>,
) -> Result<EncodedTeletextCompiledDisplayPage, TeletextCompiledDisplayPageEncodingError> {
    let error = |reason| TeletextCompiledDisplayPageEncodingError { owner, reason };
    check_deadline(owner, context.deadline)?;
    let page = service
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)
        .ok_or_else(|| error(TeletextCompiledDisplayPageEncodingFailure::MissingPage))?;
    let TeletextEncodingRegion::WesternLatin = region;
    let normalized = compile_normalized_with_context(service, owner, defaults, budget, context)
        .map_err(|reason| {
            if matches!(
                &reason.reason,
                TeletextNormalizedMainProgramFailure::TimedOut
            ) {
                error(TeletextCompiledDisplayPageEncodingFailure::TimedOut)
            } else {
                error(TeletextCompiledDisplayPageEncodingFailure::Normalization(
                    Box::new(reason),
                ))
            }
        })?;
    check_deadline(owner, context.deadline)?;
    let compiled = layout::compile_for_composition(page).map_err(|reason| {
        error(TeletextCompiledDisplayPageEncodingFailure::Base(Box::new(
            reason,
        )))
    })?;
    check_deadline(owner, context.deadline)?;
    let metadata = service.page_enhancements().get(&owner);
    let page = packets::encode_compiled_page(
        page,
        &compiled,
        &PagePacketMetadata {
            presentation: metadata.and_then(|metadata| metadata.presentation.as_ref()),
            compositional_links: metadata
                .and_then(|metadata| metadata.compositional_links.as_ref()),
            enhancements: PageEnhancements::Compiled(normalized.encoded().packet_bodies()),
        },
    )
    .map_err(|reason| {
        error(TeletextCompiledDisplayPageEncodingFailure::Base(Box::new(
            reason,
        )))
    })?;
    check_deadline(owner, context.deadline)?;
    let magazine_units = service
        .magazine_presentations()
        .get(&owner.page.magazine())
        .map(|defaults| {
            packets::data_unit(
                owner.page.wire_magazine(),
                29,
                &presentation::encode_presentation(defaults),
                false,
            )
        })
        .into_iter()
        .collect();
    check_deadline(owner, context.deadline)?;
    Ok(EncodedTeletextCompiledDisplayPage {
        normalized,
        page,
        magazine_units,
    })
}

fn check_deadline(
    owner: TeletextPageAddress,
    deadline: Option<Instant>,
) -> Result<(), TeletextCompiledDisplayPageEncodingError> {
    if deadline.is_some_and(|deadline| Instant::now() >= deadline) {
        return Err(TeletextCompiledDisplayPageEncodingError {
            owner,
            reason: TeletextCompiledDisplayPageEncodingFailure::TimedOut,
        });
    }
    Ok(())
}
