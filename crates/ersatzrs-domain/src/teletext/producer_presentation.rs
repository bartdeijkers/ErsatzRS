//! Producer-local full presentation snapshots before composition (ADR 0123).
//! Original declarations stay unchanged; no X28 or publication authority is made.

use super::{
    TeletextCharsetValueError, TeletextColourReference, TeletextColourRemapping,
    TeletextDeclaredService, TeletextEffectivePresentation, TeletextLevel25Palette,
    TeletextLevel25PresentationDefaults, TeletextMotConditionalPopFallback, TeletextNationalOption,
    TeletextPageAddress, TeletextPageCharsetDesignation, TeletextPanelStatus, TeletextPanelWidth,
    TeletextPresentationSource, TeletextSecondaryDefaultAgreement, TeletextSidePanels,
};
use std::{fmt, mem::size_of};

/// Trusted display snapshot limit; zero is meaningful.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextProducerPresentationBudget {
    pub max_pages: usize,
}

/// Original presentation source within this producer's immutable service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProducerPresentationSource {
    Page,
    Magazine {
        magazine: u8,
    },
    Header {
        national_option: TeletextNationalOption,
    },
    MotFallback {
        national_option: TeletextNationalOption,
        original_pop: TeletextMotConditionalPopFallback,
    },
}

/// Authored second-language information or an unresolved receiver default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProducerSecondarySelection {
    /// `None` explicitly declares no second G0 in an actual X28/M29 packet.
    Declared(Option<TeletextPageCharsetDesignation>),
    /// Annex D.2.1(2) does not specify this value without X28/M29.
    DecoderDependent,
}

/// Packet presence, independent of whether a program contains commands.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TeletextProducerLocalProgramPresence {
    Absent,
    PresentEmpty,
    PresentNonEmpty,
}

/// Full effective engine values and provenance for an exact display owner.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextProducerPagePresentation {
    address: TeletextPageAddress,
    source: TeletextProducerPresentationSource,
    defaults: TeletextLevel25PresentationDefaults,
    secondary: TeletextProducerSecondarySelection,
    local_program: TeletextProducerLocalProgramPresence,
}
impl TeletextProducerPagePresentation {
    #[must_use]
    pub const fn address(&self) -> TeletextPageAddress {
        self.address
    }
    #[must_use]
    pub const fn source(&self) -> TeletextProducerPresentationSource {
        self.source
    }
    /// Complete engine values, not an encodable authored declaration. Inspect
    /// `secondary_selection`: Header/POP `secondary: None` is retained engine
    /// policy, not normative evidence of an absent second G0.
    #[must_use]
    pub const fn effective_defaults(&self) -> &TeletextLevel25PresentationDefaults {
        &self.defaults
    }
    #[must_use]
    pub const fn secondary_selection(&self) -> TeletextProducerSecondarySelection {
        self.secondary
    }
    #[must_use]
    pub const fn local_program_presence(&self) -> TeletextProducerLocalProgramPresence {
        self.local_program
    }
}

/// Located failures without original document text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextProducerPresentationError {
    #[error("teletext producer display count {count} exceeds trusted maximum {maximum}")]
    PageBudget { count: usize, maximum: usize },
    #[error("teletext producer presentation snapshot storage exceeds addressable capacity")]
    StorageOverflow,
    #[error("teletext producer display {display:?} has no effective presentation")]
    MissingPresentation { display: TeletextPageAddress },
    #[error("teletext producer display {display:?} has no original POP fallback provenance")]
    MissingPopFallback { display: TeletextPageAddress },
    #[error("teletext producer display {display:?}: {error}")]
    Charset {
        display: TeletextPageAddress,
        error: TeletextCharsetValueError,
    },
}

/// Owned producer contribution and immutable snapshots. All original X28/M29/
/// X26 presence, resources and object/DRCS/MOT bodies remain in `original`.
/// No full draft is cloned or cross-producer context read.
pub struct TeletextProducerPresentation {
    original: TeletextDeclaredService,
    pages: Vec<TeletextProducerPagePresentation>,
    secondary_default_agreement: Option<TeletextSecondaryDefaultAgreement>,
}
impl TeletextProducerPresentation {
    /// Snapshot every original display before cross-producer composition.
    /// # Errors
    /// Fences count/storage before allocation, then checks presentation, original
    /// POP provenance and header charset. No override or partial result escapes.
    pub fn new(
        original: TeletextDeclaredService,
        budget: TeletextProducerPresentationBudget,
    ) -> Result<Self, TeletextProducerPresentationError> {
        let count = original.semantic().base().pages.len();
        if count > budget.max_pages {
            return Err(TeletextProducerPresentationError::PageBudget {
                count,
                maximum: budget.max_pages,
            });
        }
        count
            .checked_mul(size_of::<TeletextProducerPagePresentation>())
            .filter(|&bytes| isize::try_from(bytes).is_ok())
            .ok_or(TeletextProducerPresentationError::StorageOverflow)?;
        let mut pages = Vec::with_capacity(count);
        for page in &original.semantic().base().pages {
            pages.push(snapshot(&original, page.address)?);
        }
        Ok(Self {
            original,
            pages,
            secondary_default_agreement: None,
        })
    }
    /// Attach or remove an explicit author/receiver choice for this producer's
    /// Header/POP owners. Original packets, snapshots and decoder-dependent
    /// provenance are unchanged. Callers must authenticate author consent and
    /// retain this value in their versioned profile/design receipt; publication
    /// and warm receiver equivalence require their separate existing gates.
    #[must_use]
    pub const fn with_secondary_default_agreement(
        mut self,
        agreement: Option<TeletextSecondaryDefaultAgreement>,
    ) -> Self {
        self.secondary_default_agreement = agreement;
        self
    }
    #[must_use]
    pub const fn secondary_default_agreement(&self) -> Option<TeletextSecondaryDefaultAgreement> {
        self.secondary_default_agreement
    }
    #[must_use]
    pub const fn original(&self) -> &TeletextDeclaredService {
        &self.original
    }
    /// Original base-page order, including distinct raw subcodes.
    #[must_use]
    pub fn pages(&self) -> &[TeletextProducerPagePresentation] {
        &self.pages
    }
    #[must_use]
    pub fn page(&self, address: TeletextPageAddress) -> Option<&TeletextProducerPagePresentation> {
        self.pages.iter().find(|page| page.address == address)
    }
    /// Recover the original unchanged service without cloning declarations.
    #[must_use]
    pub fn into_original(self) -> TeletextDeclaredService {
        self.original
    }
    /// Move the original model and its associated snapshots into native work.
    /// All intent is retained; these values grant no publication authority.
    #[must_use]
    pub fn into_parts(
        self,
    ) -> (
        TeletextDeclaredService,
        Vec<TeletextProducerPagePresentation>,
        Option<TeletextSecondaryDefaultAgreement>,
    ) {
        (self.original, self.pages, self.secondary_default_agreement)
    }
}
impl fmt::Debug for TeletextProducerPresentation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextProducerPresentation")
            .field("pages", &self.pages.len())
            .finish_non_exhaustive()
    }
}

fn snapshot(
    original: &TeletextDeclaredService,
    address: TeletextPageAddress,
) -> Result<TeletextProducerPagePresentation, TeletextProducerPresentationError> {
    let service = original.semantic();
    let presentation = service
        .effective_presentation(address)
        .ok_or(TeletextProducerPresentationError::MissingPresentation { display: address })?;
    let (source, defaults, secondary) = match presentation {
        TeletextEffectivePresentation::Extension { source, defaults } => (
            match source {
                TeletextPresentationSource::Page => TeletextProducerPresentationSource::Page,
                TeletextPresentationSource::Magazine => {
                    TeletextProducerPresentationSource::Magazine {
                        magazine: address.page.magazine(),
                    }
                }
            },
            *defaults,
            TeletextProducerSecondarySelection::Declared(defaults.secondary),
        ),
        TeletextEffectivePresentation::Header { national_option } => (
            TeletextProducerPresentationSource::Header { national_option },
            header_defaults(national_option).map_err(|error| {
                TeletextProducerPresentationError::Charset {
                    display: address,
                    error,
                }
            })?,
            TeletextProducerSecondarySelection::DecoderDependent,
        ),
        TeletextEffectivePresentation::MotFallback {
            national_option,
            defaults,
        } => {
            let original_pop = original
                .resolutions()
                .iter()
                .find(|value| value.display() == address)
                .and_then(super::TeletextDeclaredDisplayResolution::conditional_pop_fallback)
                .ok_or(TeletextProducerPresentationError::MissingPopFallback {
                    display: address,
                })?;
            (
                TeletextProducerPresentationSource::MotFallback {
                    national_option,
                    original_pop,
                },
                *defaults,
                TeletextProducerSecondarySelection::DecoderDependent,
            )
        }
    };
    let local_program = match service
        .page_enhancements()
        .get(&address)
        .and_then(|page| page.local_program.as_ref())
    {
        None => TeletextProducerLocalProgramPresence::Absent,
        Some(program) if program.commands().is_empty() => {
            TeletextProducerLocalProgramPresence::PresentEmpty
        }
        Some(_) => TeletextProducerLocalProgramPresence::PresentNonEmpty,
    };
    Ok(TeletextProducerPagePresentation {
        address,
        source,
        defaults,
        secondary,
        local_program,
    })
}

// EN 300 706 Annex D.2.1(1,3–8), printed page 133, absent X28/M29/MOT.
// Secondary remains DecoderDependent; this does not manufacture an X28 packet.
fn header_defaults(
    national: TeletextNationalOption,
) -> Result<TeletextLevel25PresentationDefaults, TeletextCharsetValueError> {
    Ok(TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(national.get())?,
        secondary: None,
        palette: TeletextLevel25Palette::standard(),
        screen_colour: TeletextColourReference::BLACK,
        row_colour: TeletextColourReference::BLACK,
        black_background_substitution: false,
        remapping: TeletextColourRemapping::FgClut0BgClut0,
        panels: TeletextSidePanels {
            left: false,
            right: false,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::ZERO,
        },
    })
}

#[cfg(test)]
#[path = "producer_presentation_tests.rs"]
mod tests;
