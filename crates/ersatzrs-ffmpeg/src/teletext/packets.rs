use ersatzrs_domain::{
    TeletextLanguage, TeletextNavigationRole, TeletextPageAddress, TeletextPageDraft,
    TeletextPageKind, TeletextPageNumber, TeletextPageSetDraft, TELETEXT_ROWS,
};

use super::{layout, presentation, TeletextEncodingError, TeletextEncodingRegion};

/// A validated page's EN 300 472 data units, without PES/TS publication authority.
///
/// The first unit is the serial-mode erase header. Its following units must
/// obey the carousel's header/body spacing and close-page timing. They cannot
/// simply be appended to arbitrary transport output without PSI/rate checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextPage {
    address: TeletextPageAddress,
    kind: TeletextPageKind,
    language: TeletextLanguage,
    units: Vec<[u8; 46]>,
}

impl EncodedTeletextPage {
    /// Original page and exact subpage identity.
    #[must_use]
    pub const fn address(&self) -> TeletextPageAddress {
        self.address
    }

    /// Original page role for PMT service signalling.
    #[must_use]
    pub const fn kind(&self) -> TeletextPageKind {
        self.kind
    }

    /// Original ISO 639 language, independent of national character tables.
    #[must_use]
    pub fn language(&self) -> &TeletextLanguage {
        &self.language
    }

    /// Fixed 46-byte units: identifier/length plus the 44-byte teletext field.
    #[must_use]
    pub fn data_units(&self) -> &[[u8; 46]] {
        &self.units
    }
}

/// A completely encoded page set, still requiring host generation fencing and publication.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EncodedTeletextPageSet {
    initial_page: TeletextPageNumber,
    language: TeletextLanguage,
    pages: Vec<EncodedTeletextPage>,
}

impl EncodedTeletextPageSet {
    /// Accepted initial display page, with at least one real subpage in the set.
    #[must_use]
    pub const fn initial_page(&self) -> TeletextPageNumber {
        self.initial_page
    }

    /// Original service language, separate from the explicit wire region.
    #[must_use]
    pub fn language(&self) -> &TeletextLanguage {
        &self.language
    }

    /// Complete immutable encoded pages; no partially successful set is returned.
    #[must_use]
    pub fn pages(&self) -> &[EncodedTeletextPage] {
        &self.pages
    }
}

/// Validate all identities/references, then encode the complete bounded set.
/// # Errors
/// Returns the first structural or encoding error; a later invalid page discards
/// the in-memory result rather than returning a partially encoded generation.
pub fn encode_teletext_page_set(
    draft: &TeletextPageSetDraft,
    region: TeletextEncodingRegion,
) -> Result<EncodedTeletextPageSet, TeletextEncodingError> {
    draft.validate_structure()?;
    let pages = draft
        .pages
        .iter()
        .map(|page| encode_teletext_page(page, region))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(EncodedTeletextPageSet {
        initial_page: draft.initial_page,
        language: draft.language.clone(),
        pages,
    })
}

/// Encode one Western Latin Level 1/1.5 authored page, including editorial links.
///
/// This validates representability and physical control cells without publishing
/// any partial result. A caller encoding a whole generation must also validate
/// page-set references and retain all-or-nothing generation ownership.
/// # Errors
/// Returns a structural, character, spacing, header, link or enhancement-limit
/// diagnostic; never silently substitutes content or overwrites an authored cell.
pub fn encode_teletext_page(
    page: &TeletextPageDraft,
    region: TeletextEncodingRegion,
) -> Result<EncodedTeletextPage, TeletextEncodingError> {
    let TeletextEncodingRegion::WesternLatin = region;
    let compiled = layout::compile(page)?;
    let presentation =
        presentation::western_latin_v1_presentation(page.national_option).map_err(|_| {
            TeletextEncodingError::NationalOption {
                address: page.address,
            }
        })?;
    let subtitle = page.kind == TeletextPageKind::Subtitle;
    let mut rows = compiled.rows.map(|row| row.map(odd_parity));
    let address = page.address;
    let number = address.page.wire_page();
    let subcode = address.subcode.get();
    let [low, high] = subcode.to_le_bytes();
    let nibbles = [
        number & 15,
        number >> 4,
        low & 15,
        (low >> 4) | 8,
        high & 15,
        (high >> 4)
            | match page.kind {
                TeletextPageKind::FullPage => 0,
                TeletextPageKind::Newsflash => 4,
                TeletextPageKind::Subtitle => 8,
            },
        u8::from(page.kind != TeletextPageKind::FullPage),
        1 | (page.national_option.get().reverse_bits() >> 4),
    ];
    rows[0][..8].copy_from_slice(&nibbles.map(hamming_8_4));
    let mut units = Vec::with_capacity(TELETEXT_ROWS + 18);
    units.push(data_unit(
        address.page.wire_magazine(),
        0,
        &rows[0],
        subtitle,
    ));
    // EN 300 706 annex B.2: header, links, designation, enhancements, body.
    if !page.navigation.is_empty() {
        let links = editorial_links(page, &rows)?;
        units.push(data_unit(
            address.page.wire_magazine(),
            27,
            &links,
            subtitle,
        ));
    }
    // X/28 makes the Latin region explicit. Header national bits alone can
    // otherwise select Czech or Turkish according to a receiver's local default.
    units.push(data_unit(
        address.page.wire_magazine(),
        28,
        &presentation::encode_presentation(&presentation),
        subtitle,
    ));
    let mut enhancements = compiled.enhancements;
    enhancements.push(layout::triplet(63, 31, 0));
    for (designation, triplets) in (0_u8..16).zip(enhancements.chunks(13)) {
        let mut bytes = [0; 40];
        bytes[0] = hamming_8_4(designation);
        for (index, target) in bytes[1..].chunks_exact_mut(3).enumerate() {
            let value = triplets
                .get(index)
                .copied()
                .unwrap_or_else(|| layout::triplet(63, 31, 0));
            target.copy_from_slice(&hamming_24_18(value));
        }
        units.push(data_unit(
            address.page.wire_magazine(),
            26,
            &bytes,
            subtitle,
        ));
    }
    // Transmit blank body rows too: no stale pixels survive an authored update.
    for (packet, row) in (1_u8..25).zip(&rows[1..]) {
        units.push(data_unit(
            address.page.wire_magazine(),
            packet,
            row,
            subtitle,
        ));
    }
    Ok(EncodedTeletextPage {
        address,
        kind: page.kind,
        language: page.language.clone(),
        units,
    })
}

// EN 300 706 annex A.1: mFF with a non-wildcard subcode closes a page without
// acquiring a display page. Suppress the rolling header and retain serial mode.
pub(super) fn terminator(magazine: u8) -> [u8; 46] {
    let mut row = [odd_parity(b' '); 40];
    row[..8].copy_from_slice(&[15, 15, 0, 0, 0, 0, 1, 1].map(hamming_8_4));
    data_unit(magazine, 0, &row, false)
}

fn editorial_links(
    page: &TeletextPageDraft,
    rows: &[[u8; 40]; TELETEXT_ROWS],
) -> Result<[u8; 40], TeletextEncodingError> {
    let mut data = [0; 40];
    data[0] = hamming_8_4(0);
    let roles = [
        Some(TeletextNavigationRole::Red),
        Some(TeletextNavigationRole::Green),
        Some(TeletextNavigationRole::Yellow),
        Some(TeletextNavigationRole::Cyan),
        None,
        Some(TeletextNavigationRole::Index),
    ];
    for (slot, role) in roles.into_iter().enumerate() {
        let link = role.and_then(|role| page.navigation.iter().find(|link| link.role == role));
        let (number, subcode, relative) = if let Some(link) = link {
            if link
                .target
                .subcode
                .is_some_and(|subcode| subcode.get() == 0x3f7f)
            {
                return Err(TeletextEncodingError::LinkSubcode {
                    address: page.address,
                });
            }
            (
                link.target.page.wire_page(),
                link.target
                    .subcode
                    .map_or(0x3f7f, ersatzrs_domain::TeletextSubcode::get),
                page.address.page.wire_magazine() ^ link.target.page.wire_magazine(),
            )
        } else {
            (0xff, 0x3f7f, 0)
        };
        let [low, high] = subcode.to_le_bytes();
        let nibbles = [
            number & 15,
            number >> 4,
            low & 15,
            (low >> 4) | ((relative & 1) << 3),
            high & 15,
            (high >> 4) | ((relative & 6) << 1),
        ];
        data[1 + slot * 6..7 + slot * 6].copy_from_slice(&nibbles.map(hamming_8_4));
    }
    data[37] = hamming_8_4(15); // Defined links and display row 24.
    let crc = page_crc(rows);
    data[38..].copy_from_slice(&crc.to_be_bytes());
    Ok(data)
}

// EN 300 706 9.6.1: Fibonacci register stages 7,9,12,16, zero seed;
// 24 header bytes + 25*40 body bytes, MSB first, including odd parity.
fn page_crc(rows: &[[u8; 40]; TELETEXT_ROWS]) -> u16 {
    let mut register = 0_u16;
    let missing_row = [odd_parity(b' '); 40];
    let bytes = rows[0][8..32]
        .iter()
        .chain(rows[1..].iter().flatten())
        .chain(&missing_row);
    for &byte in bytes {
        for bit in (0..8).rev() {
            let feedback = u16::from((byte >> bit) & 1)
                ^ ((register >> 6) & 1)
                ^ ((register >> 8) & 1)
                ^ ((register >> 11) & 1)
                ^ ((register >> 15) & 1);
            register = (register << 1) | feedback;
        }
    }
    register
}

fn data_unit(magazine: u8, packet: u8, body: &[u8; 40], subtitle: bool) -> [u8; 46] {
    let mut unit = [0; 46];
    unit[..4].copy_from_slice(&[if subtitle { 3 } else { 2 }, 44, 0xe0, 0xe4]);
    let address = magazine | (packet << 3);
    unit[4] = hamming_8_4(address & 15).reverse_bits();
    unit[5] = hamming_8_4(address >> 4).reverse_bits();
    for (target, value) in unit[6..].iter_mut().zip(body) {
        *target = value.reverse_bits();
    }
    unit
}

pub(super) const fn odd_parity(byte: u8) -> u8 {
    byte | if byte.count_ones().is_multiple_of(2) {
        0x80
    } else {
        0
    }
}

pub(super) fn hamming_8_4(data: u8) -> u8 {
    let d1 = data & 1;
    let d2 = (data >> 1) & 1;
    let d3 = (data >> 2) & 1;
    let d4 = (data >> 3) & 1;
    let p1 = 1 ^ d1 ^ d3 ^ d4;
    let p2 = 1 ^ d1 ^ d2 ^ d4;
    let p3 = 1 ^ d1 ^ d2 ^ d3;
    let p4 = 1 ^ p1 ^ d1 ^ p2 ^ d2 ^ p3 ^ d3 ^ d4;
    p1 | (d1 << 1) | (p2 << 2) | (d2 << 3) | (p3 << 4) | (d3 << 5) | (p4 << 6) | (d4 << 7)
}

pub(super) fn hamming_24_18(data: u32) -> [u8; 3] {
    let mut word = 0_u32;
    let mut source = 0;
    for position in 1_u32..24 {
        if !position.is_power_of_two() {
            word |= ((data >> source) & 1) << (position - 1);
            source += 1;
        }
    }
    for parity in [1_u32, 2, 4, 8, 16] {
        let mut value = 1;
        for position in 1_u32..24 {
            if position & parity != 0 {
                value ^= (word >> (position - 1)) & 1;
            }
        }
        word |= value << (parity - 1);
    }
    word |= u32::from(word.count_ones().is_multiple_of(2)) << 23;
    let [first, second, third, _] = word.to_le_bytes();
    [first, second, third]
}
