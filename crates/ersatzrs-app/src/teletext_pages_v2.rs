//! Direct conversion of unpublished v2 declarations into an authenticated service.
//!
//! Producer assertions are checked against their actual MOT/X27 declarations.
//! Success does not activate, encode, compose or publish the service.

use ersatzrs_addon_contract as wire;
use ersatzrs_domain::teletext as d;

mod budget;
mod presentation;
mod program;
mod resources;

/// Original wire collection, independent of any physical packet allocation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextAuthoredSection {
    Service,
    Base,
    Pages,
    Magazines,
    Objects,
    Drcs,
    Mots,
    Selections,
    Companions,
}

/// Numeric location without producer text, JSON keys or generation data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextAuthoredLocation {
    pub section: TeletextAuthoredSection,
    pub record: usize,
    pub command: Option<usize>,
}

impl TeletextAuthoredLocation {
    const fn new(section: TeletextAuthoredSection, record: usize) -> Self {
        Self {
            section,
            record,
            command: None,
        }
    }
}

/// Checked constructor failure; raw values are never repaired or masked.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextAuthoredValueError {
    #[error(transparent)]
    Base(#[from] d::TeletextValueError),
    #[error(transparent)]
    Charset(#[from] d::TeletextCharsetValueError),
    #[error(transparent)]
    Enhancement(#[from] d::TeletextEnhancementValueError),
    #[error(transparent)]
    Geometry(#[from] d::TeletextEnhancementGeometryError),
    #[error(transparent)]
    Presentation(#[from] d::TeletextPresentationValueError),
    #[error(transparent)]
    Resource(#[from] d::TeletextResourceIdentityError),
    #[error(transparent)]
    Object(#[from] d::TeletextObjectIdentityError),
    #[error(transparent)]
    Program(#[from] d::TeletextEnhancementProgramError),
    #[error(transparent)]
    Drcs(#[from] d::TeletextDrcsResourceError),
    #[error(transparent)]
    X27(#[from] d::TeletextX27LinkError),
    #[error(transparent)]
    Mot(#[from] d::TeletextMotError),
}

/// Complete conversion failure; no partial service or raw authored text escapes.
#[derive(Debug, thiserror::Error)]
pub enum TeletextAuthoredPagesError {
    #[error(transparent)]
    Contract(#[from] wire::TeletextPagesV2Error),
    #[error("teletext authored input at {location:?}: {reason}")]
    Conversion {
        location: TeletextAuthoredLocation,
        reason: TeletextAuthoredValueError,
    },
    #[error("teletext authored input at {location:?} exceeds {dimension} budget {maximum}")]
    Budget {
        location: TeletextAuthoredLocation,
        dimension: &'static str,
        maximum: usize,
    },
    #[error(transparent)]
    Service(Box<d::TeletextServiceError>),
    #[error(transparent)]
    Declarations(Box<d::TeletextDeclaredServiceError>),
}

fn located<T, E: Into<TeletextAuthoredValueError>>(
    location: TeletextAuthoredLocation,
    result: Result<T, E>,
) -> Result<T, TeletextAuthoredPagesError> {
    result.map_err(|reason| TeletextAuthoredPagesError::Conversion {
        location,
        reason: reason.into(),
    })
}

/// Parse v2 once and authenticate every supplied declaration and resolved assertion.
///
/// Host budgets fence original wire counts/content before converted collections
/// or domain indexes are constructed. The raw parser has its own byte/record cap;
/// neither bound claims process RSS or physical output capacity. Mandatory v1
/// fallback conversion is shared, and null/present-empty X26 remain distinct.
///
/// # Errors
/// Returns a strict wire, located constructor/budget, semantic or declaration
/// error. There is no v1 retry, truncation, partial result or activation.
pub fn parse_declared_service(
    bytes: &[u8],
    expected_generation: &str,
    service_budget: d::TeletextServiceBudget,
    declared_budget: d::TeletextDeclaredServiceBudget,
) -> Result<d::TeletextDeclaredService, TeletextAuthoredPagesError> {
    let input = wire::TeletextPagesV2Response::from_json(bytes, expected_generation)?;
    budget::preflight(&input, service_budget, declared_budget)?;
    let base = convert_base(input.language, input.initial_page, input.pages)?;
    let pages = input
        .page_enhancements
        .into_iter()
        .enumerate()
        .map(|(index, page)| resources::page(page, index))
        .collect::<Result<_, _>>()?;
    let magazines = input
        .magazines
        .into_iter()
        .enumerate()
        .map(|(index, magazine)| {
            let location = TeletextAuthoredLocation::new(TeletextAuthoredSection::Magazines, index);
            Ok(d::TeletextMagazinePresentationDraft {
                magazine: magazine.magazine,
                presentation: located(location, presentation::convert(&magazine.presentation))?,
            })
        })
        .collect::<Result<_, TeletextAuthoredPagesError>>()?;
    let objects = input
        .objects
        .into_iter()
        .enumerate()
        .map(|(index, object)| program::definition(object, index))
        .collect::<Result<_, _>>()?;
    let drcs = convert_records(input.drcs, TeletextAuthoredSection::Drcs, resources::drcs)?;
    let mots = convert_records(input.mots, TeletextAuthoredSection::Mots, resources::mot)?;
    let selections = convert_records(
        input.mot_selections,
        TeletextAuthoredSection::Selections,
        |selection| resources::selection(&selection),
    )?;
    let companions = convert_records(
        input.empty_object_pages,
        TeletextAuthoredSection::Companions,
        |identity| resources::identity(&identity),
    )?;
    // This small checked identity list is shared by both constructors; no bulk
    // service clone or independent declaration authority is created.
    let service = d::TeletextSemanticService::new_with_object_companions(
        d::TeletextResolvedServiceDraft {
            base,
            pages,
            magazines,
            objects,
            drcs,
        },
        companions.clone(),
        service_budget,
    )
    .map_err(|error| TeletextAuthoredPagesError::Service(Box::new(error)))?;
    d::TeletextDeclaredService::new(service, mots, selections, companions, declared_budget)
        .map_err(|error| TeletextAuthoredPagesError::Declarations(Box::new(error)))
}

fn convert_records<T, U>(
    values: Vec<T>,
    section: TeletextAuthoredSection,
    convert: impl Fn(T) -> Result<U, TeletextAuthoredValueError>,
) -> Result<Vec<U>, TeletextAuthoredPagesError> {
    values
        .into_iter()
        .enumerate()
        .map(|(index, value)| {
            located(
                TeletextAuthoredLocation::new(section, index),
                convert(value),
            )
        })
        .collect()
}

fn convert_base(
    language: String,
    initial_page: u16,
    pages: Vec<wire::TeletextPageDraftWire>,
) -> Result<d::TeletextPageSetDraft, TeletextAuthoredPagesError> {
    let service_location = TeletextAuthoredLocation::new(TeletextAuthoredSection::Service, 0);
    Ok(d::TeletextPageSetDraft {
        language: located(service_location, d::TeletextLanguage::new(language))?,
        initial_page: located(service_location, d::TeletextPageNumber::new(initial_page))?,
        pages: pages
            .into_iter()
            .enumerate()
            .map(|(index, page)| {
                located(
                    TeletextAuthoredLocation::new(TeletextAuthoredSection::Base, index),
                    super::teletext_pages::convert_page(page),
                )
            })
            .collect::<Result<_, _>>()?,
    })
}

#[cfg(test)]
mod tests;
