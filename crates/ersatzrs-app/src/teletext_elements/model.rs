//! Page model for teletext elements and its canonical TTI representation.

use crate::teletext_tti::{
    parse_teletext_tti, write_teletext_tti, TeletextTtiBudget, TeletextTtiCycleUnit,
    TeletextTtiDocument, TeletextTtiRecord,
};
use ersatzrs_domain::{MAX_TELETEXT_ELEMENT_SUBPAGES, MAX_TELETEXT_ELEMENT_UPLOAD_BYTES};

/// Rows 0..=25 of a page; row 0 is the header row owned by the channel.
pub const ELEMENT_ROWS: usize = 26;
/// Default page status: transmit flag as written by `QTeletextMaker`.
pub const DEFAULT_PAGE_STATUS: u16 = 0x8000;

/// Canonical storage budget for one element.
#[must_use]
pub const fn element_tti_budget() -> TeletextTtiBudget {
    TeletextTtiBudget {
        max_bytes: MAX_TELETEXT_ELEMENT_UPLOAD_BYTES * 2,
        max_records: MAX_TELETEXT_ELEMENT_SUBPAGES * 64,
    }
}

/// One enhancement packet kept verbatim (X/26, X/27, X/28).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementPacket {
    pub row: u8,
    pub designation: u8,
    pub words: [u32; 13],
}

/// One subpage of an element: 7-bit Level 1 cells plus kept metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementSubpage {
    pub rows: [[u8; 40]; ELEMENT_ROWS],
    pub present: [bool; ELEMENT_ROWS],
    pub page_status: u16,
    pub subcode: u64,
    pub region: Option<u64>,
    pub cycle: Option<(u64, TeletextTtiCycleUnit)>,
    pub fast_links: Option<[u16; 6]>,
    pub packets: Vec<ElementPacket>,
}

impl Default for ElementSubpage {
    fn default() -> Self {
        Self {
            rows: [[0x20; 40]; ELEMENT_ROWS],
            present: [false; ELEMENT_ROWS],
            page_status: DEFAULT_PAGE_STATUS,
            subcode: 0,
            region: None,
            cycle: None,
            fast_links: None,
            packets: Vec::new(),
        }
    }
}

impl ElementSubpage {
    /// Store one Level 1 row; bytes are masked to 7 bits.
    pub fn set_row(&mut self, row: usize, cells: &[u8]) {
        if row >= ELEMENT_ROWS {
            return;
        }
        let mut target = [0x20; 40];
        for (cell, byte) in target.iter_mut().zip(cells) {
            *cell = byte & 0x7f;
        }
        self.rows[row] = target;
        self.present[row] = true;
    }

    /// National option bits C12..C14 as `QTeletextMaker` stores them in PS.
    #[must_use]
    pub const fn national_option(&self) -> u8 {
        let status = self.page_status;
        ((status & 0x0200 != 0) as u8)
            | (((status & 0x0100 != 0) as u8) << 1)
            | (((status & 0x0080 != 0) as u8) << 2)
    }

    /// Set the national option bits in the page status.
    pub fn set_national_option(&mut self, option: u8) {
        self.page_status &= !(0x0200 | 0x0100 | 0x0080);
        if option & 1 != 0 {
            self.page_status |= 0x0200;
        }
        if option & 2 != 0 {
            self.page_status |= 0x0100;
        }
        if option & 4 != 0 {
            self.page_status |= 0x0080;
        }
    }

    /// Table 32 character set designation: region << 3 | national option.
    #[must_use]
    pub fn charset_designation(&self) -> u8 {
        let region = self.region.unwrap_or(0).min(15);
        let region = u8::try_from(region).unwrap_or(0);
        (region << 3) | self.national_option()
    }

    /// Whether the subpage carries enhancement packets beyond Level 1.
    #[must_use]
    pub fn has_enhancements(&self) -> bool {
        !self.packets.is_empty()
    }
}

/// A complete element page with all its subpages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ElementPage {
    pub description: String,
    /// Hexadecimal magazine/page, 0x100..=0x8FF.
    pub page: u16,
    pub subpages: Vec<ElementSubpage>,
}

/// Failures converting the model into canonical TTI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ElementModelError {
    #[error("teletext element has no subpages")]
    Empty,
    #[error("teletext element has too many subpages")]
    TooManySubpages,
    #[error("teletext element page number is invalid")]
    PageNumber,
    #[error("teletext element could not be written as TTI")]
    Tti,
}

impl ElementPage {
    /// Write canonical TTI bytes through the checked TTI writer.
    /// # Errors
    /// Rejects empty, oversized or structurally invalid pages.
    pub fn to_tti(&self) -> Result<Vec<u8>, ElementModelError> {
        if self.subpages.is_empty() {
            return Err(ElementModelError::Empty);
        }
        if self.subpages.len() > MAX_TELETEXT_ELEMENT_SUBPAGES {
            return Err(ElementModelError::TooManySubpages);
        }
        if !(0x100..=0x8ff).contains(&self.page) {
            return Err(ElementModelError::PageNumber);
        }
        let mut records = Vec::new();
        let description: String = self
            .description
            .chars()
            .filter(|character| !character.is_control())
            .take(64)
            .collect();
        if !description.is_empty() {
            records.push(TeletextTtiRecord::Description(description));
        }
        let multiple = self.subpages.len() > 1;
        for (index, subpage) in self.subpages.iter().enumerate() {
            let ordinal = if multiple { index as u64 + 1 } else { 0 };
            records.push(TeletextTtiRecord::PageNumber {
                page: self.page,
                editor_ordinal: ordinal,
            });
            records.push(TeletextTtiRecord::SubpageOrdinal(if multiple {
                ordinal
            } else {
                subpage.subcode
            }));
            records.push(TeletextTtiRecord::PageStatus(subpage.page_status));
            if let Some(region) = subpage.region {
                records.push(TeletextTtiRecord::Region(region));
            }
            if let Some((value, unit)) = subpage.cycle {
                records.push(TeletextTtiRecord::Cycle { value, unit });
            }
            if let Some(links) = subpage.fast_links {
                records.push(TeletextTtiRecord::FastLinks(links));
            }
            for (row, cells) in subpage.rows.iter().enumerate() {
                if subpage.present[row] {
                    records.push(TeletextTtiRecord::BaseRow {
                        row: u8::try_from(row).map_err(|_| ElementModelError::Tti)?,
                        cells: *cells,
                    });
                }
            }
            for packet in &subpage.packets {
                records.push(TeletextTtiRecord::Packet {
                    row: packet.row,
                    designation: packet.designation,
                    words: packet.words,
                });
            }
        }
        let document = TeletextTtiDocument::checked_from_records(records, element_tti_budget())
            .map_err(|_| ElementModelError::Tti)?;
        write_teletext_tti(&document, element_tti_budget()).map_err(|_| ElementModelError::Tti)
    }

    /// Read canonical TTI bytes previously produced by [`Self::to_tti`].
    /// # Errors
    /// Rejects bytes that the checked TTI parser does not accept.
    pub fn from_canonical_tti(bytes: &[u8]) -> Result<Self, ElementModelError> {
        let document =
            parse_teletext_tti(bytes, element_tti_budget()).map_err(|_| ElementModelError::Tti)?;
        let mut page = Self {
            description: String::new(),
            page: 0,
            subpages: Vec::new(),
        };
        for record in document.records() {
            match record {
                TeletextTtiRecord::Description(value) => page.description.clone_from(value),
                TeletextTtiRecord::PageNumber { page: number, .. } => {
                    page.page = *number;
                    page.subpages.push(ElementSubpage::default());
                }
                other => {
                    let subpage = page.subpages.last_mut().ok_or(ElementModelError::Tti)?;
                    apply_record(subpage, other);
                }
            }
        }
        if page.subpages.is_empty() {
            return Err(ElementModelError::Empty);
        }
        Ok(page)
    }
}

/// Apply a non-PN, non-DE record to a subpage.
pub(super) fn apply_record(subpage: &mut ElementSubpage, record: &TeletextTtiRecord) {
    match record {
        TeletextTtiRecord::SubpageOrdinal(value) => subpage.subcode = *value,
        TeletextTtiRecord::PageStatus(value) => subpage.page_status = *value,
        TeletextTtiRecord::Region(value) => subpage.region = Some(*value),
        TeletextTtiRecord::Cycle { value, unit } => subpage.cycle = Some((*value, *unit)),
        TeletextTtiRecord::FastLinks(links) => subpage.fast_links = Some(*links),
        TeletextTtiRecord::BaseRow { row, cells } => subpage.set_row(usize::from(*row), cells),
        TeletextTtiRecord::Packet {
            row,
            designation,
            words,
        } => subpage.packets.push(ElementPacket {
            row: *row,
            designation: *designation,
            words: *words,
        }),
        TeletextTtiRecord::Description(_) | TeletextTtiRecord::PageNumber { .. } => {}
    }
}
