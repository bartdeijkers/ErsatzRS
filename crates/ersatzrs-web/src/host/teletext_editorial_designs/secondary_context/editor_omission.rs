//! QTM1.0 omits X28/0 when packetExists reports only default presentation.
//! Restore only a sealed policy-derived packet, never uploaded author intent.
use super::{
    check_deadline, expected, Instant, NativeTeletextEditorialImportError, OriginalDesignView,
    Result, TeletextProducerPresentationSource,
};
use ersatzrs_app::teletext_tti::{
    parse_teletext_tti, write_teletext_tti, TeletextTtiBudget, TeletextTtiDocument,
    TeletextTtiRecord,
};
use ersatzrs_domain::{TeletextColourRemapping, TeletextLevel25Palette, TeletextPageAddress};
use std::borrow::Cow;

fn trusted_packet(
    original: OriginalDesignView<'_>,
    owner: TeletextPageAddress,
    deadline: Instant,
) -> Result<Option<[u32; 13]>> {
    for producer in original.producers() {
        check_deadline(deadline)?;
        // An absent companion never supplies consent. A designated choice must
        // keep its packet even when another editor default happens to match.
        if producer
            .secondary_default_agreement()
            .is_none_or(|agreement| agreement.secondary().is_some())
        {
            continue;
        }
        let Some(page) = producer.pages().iter().find(|page| page.address() == owner) else {
            continue;
        };
        if !matches!(
            page.source(),
            TeletextProducerPresentationSource::Header { .. }
                | TeletextProducerPresentationSource::MotFallback { .. }
        ) {
            return Ok(None);
        }
        let defaults =
            expected(producer, page).ok_or(NativeTeletextEditorialImportError::Association)?;
        // Exact QTM1.0 LevelOnePage::packetExists(28,0) predicate. Disabled
        // panel metadata and primary bits remain in the retained trusted packet.
        if defaults.panels.left
            || defaults.panels.right
            || defaults.screen_colour.get() != 0
            || defaults.row_colour.get() != 0
            || defaults.black_background_substitution
            || defaults.remapping != TeletextColourRemapping::FgClut0BgClut0
            || defaults.palette != TeletextLevel25Palette::standard()
        {
            return Ok(None);
        }
        let encoded = original
            .encoded()
            .ok_or(NativeTeletextEditorialImportError::Association)?;
        if encoded
            .declared()
            .semantic()
            .page_enhancements()
            .get(&owner)
            .and_then(|page| page.presentation)
            != Some(defaults)
        {
            return Err(NativeTeletextEditorialImportError::Association);
        }
        return Ok(Some(
            ersatzrs_ffmpeg::teletext::encode_teletext_presentation_words(&defaults),
        ));
    }
    Ok(None)
}

/// The caller has qualified the original receipt, producer companion and carrier.
/// Only one missing exact packet is restored before unchanged protected checks;
/// changed/present packets and all other records receive no normalization.
pub(in super::super) fn restore_default_packet<'a>(
    original: OriginalDesignView<'_>,
    owner: TeletextPageAddress,
    bytes: (&[u8], &'a [u8]),
    budget: TeletextTtiBudget,
    deadline: Instant,
) -> Result<Cow<'a, [u8]>> {
    let Some(expected) = trusted_packet(original, owner, deadline)? else {
        return Ok(Cow::Borrowed(bytes.1));
    };
    check_deadline(deadline)?;
    let (Ok(before), Ok(after)) = (
        parse_teletext_tti(bytes.0, budget),
        parse_teletext_tti(bytes.1, budget),
    ) else {
        // Keep the existing located syntax/identity error path authoritative.
        return Ok(Cow::Borrowed(bytes.1));
    };
    let packet = |record: &TeletextTtiRecord| {
        matches!(
            record,
            TeletextTtiRecord::Packet {
                row: 28,
                designation: 0,
                ..
            }
        )
    };
    if after.records().iter().any(packet) {
        return Ok(Cow::Borrowed(bytes.1));
    }
    let Some(record @ TeletextTtiRecord::Packet { words, .. }) =
        before.records().iter().find(|record| packet(record))
    else {
        return Ok(Cow::Borrowed(bytes.1));
    };
    if *words != expected {
        return Err(NativeTeletextEditorialImportError::Association);
    }
    let mut records = after.records().to_vec();
    records.push(record.clone());
    let document = TeletextTtiDocument::checked_from_records(records, budget)
        .map_err(|_| NativeTeletextEditorialImportError::Budget)?;
    let restored = write_teletext_tti(&document, budget)
        .map_err(|_| NativeTeletextEditorialImportError::Budget)?;
    check_deadline(deadline)?;
    Ok(Cow::Owned(restored))
}
