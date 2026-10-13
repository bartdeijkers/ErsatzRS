//! Selected V3 authored input with separately captured add-on authoring data.
//!
//! Metadata describes the original producer-owned template and page regions.
//! Its validation never grants an imported file edit or publication authority.

use ersatzrs_addon_contract as wire;
use ersatzrs_domain::teletext as d;

use crate::teletext_pages_v2::{checked_declared_service, TeletextAuthoredPagesError};

/// Complete V3 failure without authored text, metadata or partial results.
#[derive(Debug, thiserror::Error)]
pub enum TeletextAuthoredPagesV3Error {
    #[error(transparent)]
    Contract(#[from] wire::TeletextPagesV3Error),
    #[error(transparent)]
    Authored(#[from] TeletextAuthoredPagesError),
    #[error("teletext authoring region {region} of page {page} exceeds the effective Level 2.5 side panel")]
    PanelGeometry { page: usize, region: usize },
}

/// Parse the exact selected V3 response once, validating metadata against its
/// original display identities before converting the complete V2 service.
///
/// The returned metadata is historical description, not host edit policy.
/// No normalization, downgrade, encoding or publication occurs here.
/// # Errors
/// Returns strict V3 wire/metadata, existing authored budget/semantic or
/// declaration failures; neither component is returned on partial success.
pub fn parse_authored_service_with_metadata(
    bytes: &[u8],
    expected_generation: &str,
    service_budget: d::TeletextServiceBudget,
    declared_budget: d::TeletextDeclaredServiceBudget,
) -> Result<
    (
        d::TeletextDeclaredService,
        wire::TeletextAuthoringMetadataWire,
    ),
    TeletextAuthoredPagesV3Error,
> {
    let input = wire::TeletextPagesV3Response::from_json(bytes, expected_generation)?;
    let service = checked_declared_service(input.service, service_budget, declared_budget)?;
    validate_panel_geometry(&service, &input.authoring)?;
    Ok((service, input.authoring))
}

fn validate_panel_geometry(
    service: &d::TeletextDeclaredService,
    authoring: &wire::TeletextAuthoringMetadataWire,
) -> Result<(), TeletextAuthoredPagesV3Error> {
    for (page_index, page) in authoring.pages.iter().enumerate() {
        // The wire parser already requires exact display-address coverage and
        // checked_declared_service admits each original display identity.
        let display = service.semantic().base().pages.iter().find(|display| {
            display.address.page.get() == page.page.page
                && display.address.subcode.get() == page.page.subcode
        });
        let presentation =
            display.and_then(|display| service.semantic().effective_presentation(display.address));
        let (left, right) = match presentation {
            Some(
                d::TeletextEffectivePresentation::Extension { defaults, .. }
                | d::TeletextEffectivePresentation::MotFallback { defaults, .. },
            ) => defaults.panels.level25_widths(),
            _ => (0, 0),
        };
        for (region_index, region) in page.regions.iter().enumerate() {
            let width = match region.area {
                wire::TeletextAuthoringRegionAreaWire::Main => continue,
                wire::TeletextAuthoringRegionAreaWire::LeftPanel => left,
                wire::TeletextAuthoringRegionAreaWire::RightPanel => right,
            };
            if u16::from(region.column) + u16::from(region.width) > u16::from(width) {
                return Err(TeletextAuthoredPagesV3Error::PanelGeometry {
                    page: page_index,
                    region: region_index,
                });
            }
        }
    }
    Ok(())
}
