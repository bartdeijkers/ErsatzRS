//! Strict add-on response conversion to unpublished authored drafts (ADR 0121).
//!
//! Structural acceptance preserves literal content and exact identities. The
//! native host must separately validate encoding and generation promotion;
//! this module does not establish charset, transport or publication readiness.

use ersatzrs_addon_contract as wire;
use ersatzrs_domain::teletext::{
    TeletextColour, TeletextHeight, TeletextLanguage, TeletextMosaic, TeletextNationalOption,
    TeletextNavigationLink, TeletextNavigationRole, TeletextNavigationTarget, TeletextPageAddress,
    TeletextPageDraft, TeletextPageKind, TeletextPageNumber, TeletextPageSetDraft, TeletextRow,
    TeletextRun, TeletextRunContent, TeletextStructureError, TeletextStyle, TeletextSubcode,
    TeletextValueError,
};

/// Response failure without producer text or generation values in diagnostics.
#[derive(Debug, thiserror::Error)]
pub enum TeletextPagesError {
    /// Raw JSON, schema, generation or resource-bound failure.
    #[error(transparent)]
    Contract(#[from] wire::TeletextPagesError),
    /// An authored scalar cannot be constructed without changing its value.
    #[error(transparent)]
    DomainValue(#[from] TeletextValueError),
    /// The complete authored draft has invalid structure or references.
    #[error(transparent)]
    Structure(#[from] TeletextStructureError),
}

/// Parse one bounded response and return its complete structurally valid draft.
///
/// No partial set escapes on failure. Missing fields, malformed values and
/// generation mismatches are never repaired or replaced with defaults.
///
/// # Errors
/// Returns a contract, scalar or structural failure. Encoding and publication
/// checks remain required after this function succeeds.
pub fn parse_page_set(
    bytes: &[u8],
    expected_generation: &str,
) -> Result<TeletextPageSetDraft, TeletextPagesError> {
    let response = wire::TeletextPagesResponse::from_json(bytes, expected_generation)?;
    let draft = TeletextPageSetDraft {
        language: TeletextLanguage::new(response.language)?,
        initial_page: TeletextPageNumber::new(response.initial_page)?,
        pages: response
            .pages
            .into_iter()
            .map(convert_page)
            .collect::<Result<_, _>>()?,
    };
    draft.validate_structure()?;
    Ok(draft)
}

fn convert_page(
    page: wire::TeletextPageDraftWire,
) -> Result<TeletextPageDraft, TeletextValueError> {
    Ok(TeletextPageDraft {
        address: TeletextPageAddress {
            page: TeletextPageNumber::new(page.address.page)?,
            subcode: TeletextSubcode::new(page.address.subcode)?,
        },
        kind: match page.kind {
            wire::TeletextPageKindWire::FullPage => TeletextPageKind::FullPage,
            wire::TeletextPageKindWire::Subtitle => TeletextPageKind::Subtitle,
            wire::TeletextPageKindWire::Newsflash => TeletextPageKind::Newsflash,
        },
        language: TeletextLanguage::new(page.language)?,
        national_option: TeletextNationalOption::new(page.national_option)?,
        rows: page
            .rows
            .into_iter()
            .map(convert_row)
            .collect::<Result<_, _>>()?,
        navigation: page
            .navigation
            .iter()
            .map(convert_navigation)
            .collect::<Result<_, _>>()?,
    })
}

fn convert_row(row: wire::TeletextRowWire) -> Result<TeletextRow, TeletextValueError> {
    Ok(TeletextRow {
        row: row.row,
        runs: row
            .runs
            .into_iter()
            .map(convert_run)
            .collect::<Result<_, _>>()?,
    })
}

fn convert_run(run: wire::TeletextRunWire) -> Result<TeletextRun, TeletextValueError> {
    let content = match run.content {
        wire::TeletextRunContentWire::Text { text } => TeletextRunContent::Text { text },
        wire::TeletextRunContentWire::Mosaics { cells, separated } => TeletextRunContent::Mosaics {
            cells: cells
                .into_iter()
                .map(TeletextMosaic::new)
                .collect::<Result<_, _>>()?,
            separated,
        },
    };
    Ok(TeletextRun {
        column: run.column,
        style: TeletextStyle {
            foreground: convert_colour(run.style.foreground),
            background: convert_colour(run.style.background),
            height: match run.style.height {
                wire::TeletextHeightWire::Normal => TeletextHeight::Normal,
                wire::TeletextHeightWire::Double => TeletextHeight::Double,
            },
            boxed: run.style.boxed,
            flash: run.style.flash,
            conceal: run.style.conceal,
        },
        content,
    })
}

fn convert_colour(colour: wire::TeletextColourWire) -> TeletextColour {
    match colour {
        wire::TeletextColourWire::Black => TeletextColour::Black,
        wire::TeletextColourWire::Red => TeletextColour::Red,
        wire::TeletextColourWire::Green => TeletextColour::Green,
        wire::TeletextColourWire::Yellow => TeletextColour::Yellow,
        wire::TeletextColourWire::Blue => TeletextColour::Blue,
        wire::TeletextColourWire::Magenta => TeletextColour::Magenta,
        wire::TeletextColourWire::Cyan => TeletextColour::Cyan,
        wire::TeletextColourWire::White => TeletextColour::White,
    }
}

fn convert_navigation(
    link: &wire::TeletextNavigationLinkWire,
) -> Result<TeletextNavigationLink, TeletextValueError> {
    Ok(TeletextNavigationLink {
        role: match link.role {
            wire::TeletextNavigationRoleWire::Index => TeletextNavigationRole::Index,
            wire::TeletextNavigationRoleWire::Red => TeletextNavigationRole::Red,
            wire::TeletextNavigationRoleWire::Green => TeletextNavigationRole::Green,
            wire::TeletextNavigationRoleWire::Yellow => TeletextNavigationRole::Yellow,
            wire::TeletextNavigationRoleWire::Cyan => TeletextNavigationRole::Cyan,
        },
        target: TeletextNavigationTarget {
            page: TeletextPageNumber::new(link.target.page)?,
            subcode: link.target.subcode.map(TeletextSubcode::new).transpose()?,
        },
    })
}

#[cfg(test)]
#[path = "teletext_pages_tests.rs"]
mod tests;
