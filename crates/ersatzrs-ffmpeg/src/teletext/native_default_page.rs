//! Display units preserving the absence of local X26 and original metadata.
//!
//! This is a native default-object packet boundary, not MOT/resource encoding,
//! selected-link authorization, complete service admission or publication.

use ersatzrs_domain::{
    TeletextDefaultObjectSelection, TeletextEnhancementPosition, TeletextPageAddress,
    TeletextSemanticService,
};

use super::packets::{self, LegacyEnhancements, PagePacketMetadata};
use super::{
    layout, presentation, EncodedTeletextPage, TeletextEncodingError, TeletextEncodingRegion,
};

/// Owned display and separate original magazine declaration, without a carousel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextNativeDefaultPage {
    page: EncodedTeletextPage,
    magazine_units: Vec<[u8; 46]>,
    defaults: [Option<TeletextDefaultObjectSelection>; 4],
}

impl EncodedTeletextNativeDefaultPage {
    /// Exact original page/subcode, kind, language and fallback units; no X26.
    #[must_use]
    pub const fn page(&self) -> &EncodedTeletextPage {
        &self.page
    }

    /// Present original M29/0, even when overridden by a page X28/0.
    /// These units still require magazine-level transport scheduling.
    #[must_use]
    pub fn magazine_units(&self) -> &[[u8; 46]] {
        &self.magazine_units
    }

    /// Original sparse GPOP1/GPOP2/POP1/POP2 targets, never rebound to X27.
    #[must_use]
    pub const fn defaults(&self) -> &[Option<TeletextDefaultObjectSelection>; 4] {
        &self.defaults
    }
}

/// Located boundary failure, without authored program or text in diagnostics.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("native default page {owner:?}: {reason}")]
pub struct TeletextNativeDefaultPageEncodingError {
    pub owner: TeletextPageAddress,
    pub reason: TeletextNativeDefaultPageEncodingFailure,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextNativeDefaultPageEncodingFailure {
    #[error("the exact page/subcode does not exist")]
    MissingPage,
    #[error("a present local program suppresses native defaults, including an empty one")]
    LocalProgramPresent,
    #[error("base replacement at {position:?} requires the X26 composition path")]
    DerivedGlyphNeedsX26 {
        position: TeletextEnhancementPosition,
    },
    #[error(transparent)]
    Base(Box<TeletextEncodingError>),
}

/// Encode original fallback/header/link rows without manufacturing local X26.
///
/// Only actual page presentation/link declarations produce X28/0 and X27/4.
/// Original M29/0 is returned separately, not promoted to artificial page data.
/// Checked default identities are retained as metadata: this neither encodes
/// their MOT/resource pages nor exempts explicit calls from selected-link checks.
/// # Errors
/// Rejects unknown exact owners, any present local program, a base glyph needing
/// X26, or existing checked layout/header/editorial-link encoding errors. Pages
/// needing X26 must use the composition path rather than losing replacements.
pub fn encode_teletext_native_default_page(
    service: &TeletextSemanticService,
    owner: TeletextPageAddress,
    region: TeletextEncodingRegion,
) -> Result<EncodedTeletextNativeDefaultPage, TeletextNativeDefaultPageEncodingError> {
    let error = |reason| TeletextNativeDefaultPageEncodingError { owner, reason };
    let page = service
        .base()
        .pages
        .iter()
        .find(|page| page.address == owner)
        .ok_or_else(|| error(TeletextNativeDefaultPageEncodingFailure::MissingPage))?;
    let metadata = service.page_enhancements().get(&owner);
    if metadata.is_some_and(|metadata| metadata.local_program.is_some()) {
        return Err(error(
            TeletextNativeDefaultPageEncodingFailure::LocalProgramPresent,
        ));
    }
    let TeletextEncodingRegion::WesternLatin = region;
    let compiled = layout::compile_for_composition(page).map_err(|reason| {
        error(TeletextNativeDefaultPageEncodingFailure::Base(Box::new(
            reason,
        )))
    })?;
    if let Some(glyph) = compiled.base_glyphs().first() {
        return Err(error(
            TeletextNativeDefaultPageEncodingFailure::DerivedGlyphNeedsX26 {
                position: glyph.position(),
            },
        ));
    }
    let encoded = packets::encode_compiled_page(
        page,
        &compiled,
        &PagePacketMetadata {
            presentation: metadata.and_then(|metadata| metadata.presentation.as_ref()),
            compositional_links: metadata
                .and_then(|metadata| metadata.compositional_links.as_ref()),
            enhancements: LegacyEnhancements::Omit,
        },
    )
    .map_err(|reason| {
        error(TeletextNativeDefaultPageEncodingFailure::Base(Box::new(
            reason,
        )))
    })?;
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
    Ok(EncodedTeletextNativeDefaultPage {
        page: encoded,
        magazine_units,
        defaults: metadata.map_or([None; 4], |metadata| metadata.resources.defaults),
    })
}
