//! Explicit v1 presentation/presence, before any enhanced-service composition.

use ersatzrs_domain::{
    TeletextDrcsPageLinks, TeletextEnhancementProgram, TeletextPageSetDraft,
    TeletextResolvedObjectPageLinks, TeletextResolvedPageEnhancementDraft,
    TeletextResolvedPageResourceSelection, TeletextResolvedServiceDraft, TeletextSemanticService,
    TeletextServiceBudget, TeletextServiceError,
};

use super::{presentation, TeletextEncodingRegion};

/// Lift the unchanged base-only v1 draft into explicitly isolated semantic intent.
///
/// Every exact page/subcode receives the existing encoder's X28 presentation and
/// present empty authored X26 program. These prevent another producer's magazine
/// presentation/default objects from changing the lifted page. Character triplets
/// derived from base text remain encoder work; empty authored data does not remove
/// them. The base is consumed without cloning its content.
///
/// This is not an enhanced encoder, display validator or publication token.
/// # Errors
/// Budgets are checked before structural/designation work or metadata allocation;
/// invalid base structure and reserved Western Latin national option seven are
/// rejected. The complete semantic service is validated again before returning.
pub fn lift_teletext_v1_page_set(
    base: TeletextPageSetDraft,
    region: TeletextEncodingRegion,
    budget: TeletextServiceBudget,
) -> Result<TeletextSemanticService, TeletextServiceError> {
    let TeletextEncodingRegion::WesternLatin = region;
    budget.validate_base_with_page_metadata(&base)?;
    base.validate_structure()?;
    let pages = base
        .pages
        .iter()
        .map(|page| {
            let presentation = presentation::western_latin_v1_presentation(page.national_option)
                .map_err(|error| TeletextServiceError::Charset {
                    display: page.address,
                    error,
                })?;
            Ok(TeletextResolvedPageEnhancementDraft {
                address: page.address,
                presentation: Some(presentation),
                local_program: Some(TeletextEnhancementProgram::new(Vec::new())?),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [None; 4],
                },
            })
        })
        .collect::<Result<Vec<_>, TeletextServiceError>>()?;
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base,
            pages,
            magazines: Vec::new(),
            objects: Vec::new(),
            drcs: Vec::new(),
        },
        budget,
    )
}
