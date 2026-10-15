//! Bounded report-only wire descriptors (EN 300 468 clauses 6.2.43/6.2.48).
//!
//! Tags 0x56 and 0x46 share five-byte entries, but retain distinct meanings.
//! In particular, 0x46 alone does not identify private stream type 0x06 as VBI
//! or EBU teletext. These entries grant no page readiness or subtitle policy.
//! Primary syntax: EN 300 468 V1.20.1 tables 101 and 108. `FFmpeg` n9.0
//! `libavformat/mpegts.c::ff_parse_mpeg2_descriptor` independently checks the
//! 0x56 payload's five-byte alignment and preserves its language/packed pair.

use ersatzrs_domain::{
    SourceTeletextDescriptor, TeletextLanguage, TeletextPageNumber,
    MAX_SOURCE_TELETEXT_HEADER_DESCRIPTOR_BYTES,
};
use mpeg2ts_reader::descriptor::DescriptorIter;

use super::signalling::RawDescriptors;

/// One exact teletext entry retaining wire identity, ordering and reserved values.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceTeletextWireDescriptor {
    /// Original 0x56 teletext or 0x46 VBI teletext descriptor tag.
    pub descriptor_tag: u8,
    /// Position among all TLV records, including unknown descriptor tags.
    pub descriptor_ordinal: usize,
    /// Position within this descriptor's five-byte entry list.
    pub entry_ordinal: usize,
    pub language: TeletextLanguage,
    /// Raw five-bit type; subtitle types 2 and 5 remain distinct.
    pub descriptor_type: u8,
    /// Raw three-bit magazine; zero represents magazine eight.
    pub wire_magazine: u8,
    /// Raw page byte, including non-BCD and nondisplay addresses.
    pub wire_page: u8,
}

impl SourceTeletextWireDescriptor {
    /// Convert only a decimal display address using the shared domain rule.
    /// An advertised address remains separate from an observed or ready page.
    #[must_use]
    pub fn display_page(&self) -> Option<TeletextPageNumber> {
        SourceTeletextDescriptor {
            projection_ordinal: 0,
            language: self.language.clone(),
            descriptor_type: self.descriptor_type,
            wire_magazine: self.wire_magazine,
            wire_page: self.wire_page,
        }
        .display_page()
    }
}

/// Static failures reject the entire supplied list without retaining wire bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextSourceDescriptorError {
    #[error("source teletext descriptor bytes exceed the acquisition limit")]
    ResourceLimit,
    #[error("source descriptor TLV framing is incomplete or invalid")]
    Framing,
    #[error("source teletext descriptor payload is not a complete five-byte list")]
    PayloadLength,
    #[error("source teletext descriptor language is invalid")]
    Language,
}

/// Parse complete wire TLV records without deduplicating their teletext entries.
///
/// Unknown descriptors participate in framing and ordinal assignment. Reserved
/// types, repeated entries, magazine eight and hexadecimal page bytes survive.
/// No subtitle flag becomes forced/default/SDH policy or a page C6 inference.
///
/// # Errors
/// Rejects excessive bytes, malformed TLV framing, partial five-byte entries or
/// invalid three-letter language codes. A later error returns no partial list.
pub fn parse_source_teletext_descriptors(
    bytes: &[u8],
) -> Result<Vec<SourceTeletextWireDescriptor>, TeletextSourceDescriptorError> {
    use TeletextSourceDescriptorError::{Framing, Language, PayloadLength, ResourceLimit};
    if bytes.len() > MAX_SOURCE_TELETEXT_HEADER_DESCRIPTOR_BYTES {
        return Err(ResourceLimit);
    }
    let mut entries = Vec::new();
    let mut consumed = 0;
    for (descriptor_ordinal, item) in DescriptorIter::<RawDescriptors<'_>>::new(bytes).enumerate() {
        let RawDescriptors::Other(descriptor) = item.map_err(|_| Framing)?;
        consumed += 2 + descriptor.payload.len();
        if !matches!(descriptor.tag, 0x56 | 0x46) {
            continue;
        }
        if !descriptor.payload.len().is_multiple_of(5) {
            return Err(PayloadLength);
        }
        for (entry_ordinal, raw) in descriptor.payload.chunks_exact(5).enumerate() {
            let language = std::str::from_utf8(&raw[..3]).map_err(|_| Language)?;
            let language = TeletextLanguage::new(language.to_owned()).map_err(|_| Language)?;
            entries.push(SourceTeletextWireDescriptor {
                descriptor_tag: descriptor.tag,
                descriptor_ordinal,
                entry_ordinal,
                language,
                descriptor_type: raw[3] >> 3,
                wire_magazine: raw[3] & 7,
                wire_page: raw[4],
            });
        }
    }
    // Some iterator implementations stop at a final tag without its length.
    // Every supplied byte must belong to a complete record, including unknowns.
    if consumed != bytes.len() {
        return Err(Framing);
    }
    Ok(entries)
}

#[cfg(test)]
mod tests;
