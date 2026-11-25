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
    Cycle(#[from] d::TeletextSubpageCycleError),
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
    let mut service = checked_declared_service(input.service, service_budget, declared_budget)?;
    validate_panel_geometry(&service, &input.authoring)?;
    for page in input
        .authoring
        .pages
        .iter()
        .filter(|p| !p.channel_links.is_empty())
    {
        let owner = service
            .semantic()
            .base()
            .pages
            .iter()
            .find(|p| {
                p.address.page.get() == page.page.page
                    && p.address.subcode.get() == page.page.subcode
            })
            .map(|p| p.address)
            .ok_or(d::TeletextSubpageCycleError::MissingPage {
                page: page.page.page,
            })?;
        let links = page
            .channel_links
            .iter()
            .map(|(key, &number)| {
                let role = match key.as_str() {
                    "red" => d::TeletextNavigationRole::Red,
                    "green" => d::TeletextNavigationRole::Green,
                    "yellow" => d::TeletextNavigationRole::Yellow,
                    _ => d::TeletextNavigationRole::Cyan,
                };
                Ok::<_, d::TeletextSubpageCycleError>(d::TeletextNavigationLink {
                    role,
                    target: d::TeletextNavigationTarget {
                        page: d::TeletextPageNumber::new(number).map_err(|_| {
                            d::TeletextSubpageCycleError::MissingPage { page: number }
                        })?,
                        subcode: None,
                    },
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        if !service.stage_channel_links(owner, links) {
            return Err(d::TeletextSubpageCycleError::Count.into());
        }
    }
    let cycles = input
        .authoring
        .pages
        .iter()
        .filter_map(|p| p.cycle_seconds.map(|seconds| (p.page.page, seconds)))
        .map(|(page, seconds)| {
            let owner = d::TeletextPageNumber::new(page)
                .map_err(|_| d::TeletextSubpageCycleError::MissingPage { page })?;
            d::TeletextSubpageCycle::new(owner, seconds)
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok((service.with_subpage_cycles(cycles)?, input.authoring))
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
