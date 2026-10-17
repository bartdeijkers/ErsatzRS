//! Explicit pinned-client preview policy; never an authored X28 declaration.
//!
//! Original POP fallback keeps decoder-dependent secondary selection. QTM 1.0
//! represents its own absent-X28 default as region F/NOS 7, exactly as it reads
//! the encoded 0x7f sentinel. Only the editor carrier receives that value. The
//! immutable original, its provenance and emitted service remain unchanged.

use super::{
    NativeTeletextExportProjectError, OriginalDesignView, Result, TeletextEditorCarrier,
    TeletextPageAddress, TeletextProducerPagePresentation,
};
use ersatzrs_app::teletext_export_project::TeletextExportEditorPresentation;
use ersatzrs_domain::{TeletextProducerPresentationSource, TeletextProducerSecondarySelection};
use ersatzrs_ffmpeg::teletext::encode_teletext_presentation_words;

fn needed(presentation: &TeletextProducerPagePresentation) -> bool {
    let defaults = presentation.effective_defaults();
    matches!(
        presentation.source(),
        TeletextProducerPresentationSource::MotFallback { .. }
    ) && (defaults.panels.left || defaults.panels.right || defaults.black_background_substitution)
}

/// New exports choose complete resource views before presentation-only carriers.
/// Historical receipt reconstruction continues to use its stored carrier.
pub(in super::super) fn carrier(
    original: OriginalDesignView<'_>,
    selected: Option<&[TeletextPageAddress]>,
    deadline: std::time::Instant,
) -> Result<TeletextEditorCarrier> {
    let mut has_objects = false;
    if let Some(encoded) = original.encoded() {
        for page in encoded.pages() {
            super::check_deadline(deadline)?;
            if super::resource_objects::requires_fragments(page, deadline)? {
                return Ok(TeletextEditorCarrier::V7);
            }
            has_objects |= matches!(page, ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredPage::Object(page)
                if !page.definition_positions().is_empty());
        }
    }
    if has_objects {
        return Ok(TeletextEditorCarrier::V6);
    }
    Ok(
        if original.producers().iter().any(|producer| {
            producer.pages().iter().any(|page| {
                selected.is_none_or(|owners| owners.contains(&page.address())) && needed(page)
            })
        }) {
            TeletextEditorCarrier::V5
        } else {
            TeletextEditorCarrier::V4
        },
    )
}

pub(super) fn projection(
    presentation: &TeletextProducerPagePresentation,
    carrier: TeletextEditorCarrier,
    has_encoded_presentation: bool,
) -> Result<Option<(TeletextExportEditorPresentation, [u32; 13])>> {
    if !matches!(
        carrier,
        TeletextEditorCarrier::V5 | TeletextEditorCarrier::V6 | TeletextEditorCarrier::V7
    ) || has_encoded_presentation
        || !needed(presentation)
    {
        return Ok(None);
    }
    let TeletextProducerPresentationSource::MotFallback {
        national_option,
        original_pop,
    } = presentation.source()
    else {
        return Err(NativeTeletextExportProjectError::Association);
    };
    if presentation.secondary_selection() != TeletextProducerSecondarySelection::DecoderDependent
        || presentation.effective_defaults().primary.get() != national_option.get()
    {
        return Err(NativeTeletextExportProjectError::Association);
    }
    let mapping = TeletextExportEditorPresentation::Qtm10OriginalPopFallbackV1 {
        national_option: national_option.get(),
        pop_flags: original_pop.flags.get(),
    };
    let mut editor_defaults = *presentation.effective_defaults();
    // Deliberate QTM-only view policy, named/versioned in the sealed mapping.
    // It is not an interpretation of native DecoderDependent as Declared(None).
    editor_defaults.secondary = None;
    Ok(Some((
        mapping,
        encode_teletext_presentation_words(&editor_defaults),
    )))
}
