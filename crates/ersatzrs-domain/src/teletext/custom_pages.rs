//! Host-owned producer of operator-made custom pages (ADR 0120 amendment of
//! 2026-11-07).
//!
//! Channels link teletext elements with a page number. When a channel has at
//! least one enabled link, its invocation selection gains one reserved,
//! host-owned producer. It runs no child process and has no manifest, package
//! or settings. Its response carries exact seven-bit Level 1 rows, which the
//! engine applies as physical base rows on otherwise empty Base pages, so the
//! operator's page design reaches receivers byte for byte. Row 0 (the header)
//! stays the channel's. Enhancement packets are never carried here.

use std::{collections::BTreeSet, fmt};

use serde::{Deserialize, Serialize};

use super::{
    TeletextInstallationSnapshot, TeletextLanguage, TeletextNationalOption, TeletextPageAddress,
    TeletextPageContract, TeletextPageDraft, TeletextPageKind, TeletextPageNumber,
    TeletextPageSetDraft, TeletextPhysicalBaseOverlay, TeletextPhysicalBaseRow,
    TeletextSelectionSnapshot, TeletextSubcode,
};
use crate::EntityId;

/// Reserved producer id of the host-owned custom pages source.
pub const TELETEXT_CUSTOM_PAGES_PRODUCER_ID: &str = "ersatzrs.host.custom-pages";
/// Response schema written by the host producer and accepted only from it.
pub const TELETEXT_CUSTOM_PAGES_SCHEMA: &str = "ersatzrs.host.custom-pages/1";
/// Version of the host producer; part of the pinned selection identity.
const PRODUCER_VERSION: &str = "1.0.0";
/// Fixed 64-hex identity of the built-in producer. It is not a package digest;
/// it only keeps the pinned selection shape uniform with installed add-ons.
const PRODUCER_DIGEST: &str = "6f1b6a1f3c2d4e5a8b9c0d1e2f3a4b5c6d7e8f90a1b2c3d4e5f60718293a4b5c";
/// Upper bound on custom pages (including subpages) per channel.
pub const MAX_TELETEXT_CUSTOM_PAGE_RECORDS: usize = 256;

/// True for the reserved host producer id. Installable add-ons cannot use it.
#[must_use]
pub fn is_teletext_custom_pages_producer(addon_id: &str) -> bool {
    addon_id == TELETEXT_CUSTOM_PAGES_PRODUCER_ID
}

/// The selection member of the host producer on one channel. Its identity is
/// constant: editing an element or a link does not change it, so live output
/// continues until the next explicit publication.
#[must_use]
pub fn teletext_custom_pages_selection(channel_id: EntityId) -> TeletextSelectionSnapshot {
    TeletextSelectionSnapshot {
        contract: TeletextPageContract::V1,
        channel_id,
        addon_id: TELETEXT_CUSTOM_PAGES_PRODUCER_ID.to_owned(),
        binding_revision: 1,
        profile_id: 1,
        profile_revision: 1,
        installation: TeletextInstallationSnapshot {
            addon_id: TELETEXT_CUSTOM_PAGES_PRODUCER_ID.to_owned(),
            state_revision: "host".to_owned(),
            version: PRODUCER_VERSION.to_owned(),
            package_sha256: PRODUCER_DIGEST.to_owned(),
            content_sha256: PRODUCER_DIGEST.to_owned(),
            manifest_sha256: PRODUCER_DIGEST.to_owned(),
        },
        effective_settings: Vec::new(),
    }
}

/// Response of the host producer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextCustomPagesWire {
    pub schema: String,
    pub generation: String,
    pub pages: Vec<TeletextCustomPageWire>,
}

/// One page or subpage, with the element revision it was made from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextCustomPageWire {
    pub element_id: EntityId,
    pub element_revision: i64,
    /// Decimal display page 100..=899.
    pub page: u16,
    pub subcode: u16,
    pub national_option: u8,
    /// Display rows 1..=24, each 40 seven-bit cells as 80 hexadecimal digits.
    pub rows: Vec<TeletextCustomRowWire>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextCustomRowWire {
    pub row: u8,
    pub cells: String,
}

impl TeletextCustomRowWire {
    #[must_use]
    pub fn new(row: u8, cells: &[u8; 40]) -> Self {
        use std::fmt::Write as _;
        let mut hex = String::with_capacity(80);
        for cell in cells {
            let _ = write!(hex, "{:02x}", cell & 0x7f);
        }
        Self { row, cells: hex }
    }

    fn decode(&self) -> Option<[u8; 40]> {
        let bytes = self.cells.as_bytes();
        if bytes.len() != 80 {
            return None;
        }
        let mut cells = [0_u8; 40];
        for (cell, pair) in cells.iter_mut().zip(bytes.chunks_exact(2)) {
            let text = std::str::from_utf8(pair).ok()?;
            *cell = u8::from_str_radix(text, 16).ok()?;
        }
        Some(cells)
    }
}

/// Numeric-only failure; no page content is copied into diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextCustomPagesError {
    #[error("custom pages response is not valid JSON for its schema")]
    Syntax,
    #[error("custom pages response has an unexpected schema or generation")]
    Correlation,
    #[error("custom pages response has no pages or too many pages")]
    RecordCount,
    #[error("custom page {page} has an invalid page number, subcode or national option")]
    Address { page: u16 },
    #[error("custom page {page} repeats subcode {subcode}")]
    Duplicate { page: u16, subcode: u16 },
    #[error("custom page {page} has an invalid row {row}")]
    Row { page: u16, row: u8 },
}

/// A checked host response: empty Base pages plus their exact rows.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextCustomPagesDocument {
    draft: TeletextPageSetDraft,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
    origins: Vec<TeletextCustomPageOrigin>,
}

/// Which element revision produced one page; recorded with the publication.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextCustomPageOrigin {
    pub address: TeletextPageAddress,
    pub element_id: EntityId,
    pub element_revision: i64,
}

impl fmt::Debug for TeletextCustomPagesDocument {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextCustomPagesDocument")
            .field("pages", &self.draft.pages.len())
            .field("origins", &self.origins)
            .finish_non_exhaustive()
    }
}

impl TeletextCustomPagesDocument {
    /// Check a host response value for the expected correlation.
    /// # Errors
    /// Rejects correlation, counts, addresses and invalid rows.
    pub fn from_wire(
        wire: TeletextCustomPagesWire,
        generation: &str,
    ) -> Result<Self, TeletextCustomPagesError> {
        if wire.schema != TELETEXT_CUSTOM_PAGES_SCHEMA || wire.generation != generation {
            return Err(TeletextCustomPagesError::Correlation);
        }
        if wire.pages.is_empty() || wire.pages.len() > MAX_TELETEXT_CUSTOM_PAGE_RECORDS {
            return Err(TeletextCustomPagesError::RecordCount);
        }
        let language = TeletextLanguage::new("und".to_owned())
            .map_err(|_| TeletextCustomPagesError::RecordCount)?;
        let mut seen = BTreeSet::new();
        let mut pages = Vec::with_capacity(wire.pages.len());
        let mut overlays = Vec::with_capacity(wire.pages.len());
        let mut origins = Vec::with_capacity(wire.pages.len());
        for page in wire.pages {
            let invalid = TeletextCustomPagesError::Address { page: page.page };
            let address = TeletextPageAddress {
                page: TeletextPageNumber::new(page.page).map_err(|_| invalid)?,
                subcode: TeletextSubcode::new(page.subcode).map_err(|_| invalid)?,
            };
            let national_option =
                TeletextNationalOption::new(page.national_option).map_err(|_| invalid)?;
            if page.element_id <= 0 || page.element_revision <= 0 {
                return Err(invalid);
            }
            if !seen.insert(address) {
                return Err(TeletextCustomPagesError::Duplicate {
                    page: page.page,
                    subcode: page.subcode,
                });
            }
            let mut rows = Vec::with_capacity(page.rows.len());
            for row in &page.rows {
                let bad_row = TeletextCustomPagesError::Row {
                    page: page.page,
                    row: row.row,
                };
                if !(1..=24).contains(&row.row) {
                    return Err(bad_row);
                }
                let cells = row.decode().ok_or(bad_row)?;
                rows.push(TeletextPhysicalBaseRow::new(row.row, cells).map_err(|_| bad_row)?);
            }
            if !rows.is_empty() {
                overlays.push(
                    TeletextPhysicalBaseOverlay::new(address, rows).map_err(|_| {
                        TeletextCustomPagesError::Row {
                            page: page.page,
                            row: 0,
                        }
                    })?,
                );
            }
            pages.push(TeletextPageDraft {
                address,
                kind: TeletextPageKind::FullPage,
                language: language.clone(),
                national_option,
                rows: Vec::new(),
                navigation: Vec::new(),
            });
            origins.push(TeletextCustomPageOrigin {
                address,
                element_id: page.element_id,
                element_revision: page.element_revision,
            });
        }
        let initial_page = pages
            .iter()
            .map(|page| page.address.page)
            .min()
            .ok_or(TeletextCustomPagesError::RecordCount)?;
        Ok(Self {
            draft: TeletextPageSetDraft {
                language,
                initial_page,
                pages,
            },
            overlays,
            origins,
        })
    }

    #[must_use]
    pub const fn draft(&self) -> &TeletextPageSetDraft {
        &self.draft
    }

    #[must_use]
    pub fn overlays(&self) -> &[TeletextPhysicalBaseOverlay] {
        &self.overlays
    }

    #[must_use]
    pub fn origins(&self) -> &[TeletextCustomPageOrigin] {
        &self.origins
    }

    /// Move the empty Base pages and their exact rows to the encoder.
    #[must_use]
    pub fn into_parts(self) -> (TeletextPageSetDraft, Vec<TeletextPhysicalBaseOverlay>) {
        (self.draft, self.overlays)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page(number: u16, subcode: u16) -> TeletextCustomPageWire {
        let mut cells = [b' '; 40];
        cells[0] = 0x01; // alpha red
        cells[1..6].copy_from_slice(b"Hello");
        TeletextCustomPageWire {
            element_id: 7,
            element_revision: 3,
            page: number,
            subcode,
            national_option: 0,
            rows: vec![TeletextCustomRowWire::new(1, &cells)],
        }
    }

    fn wire(pages: Vec<TeletextCustomPageWire>) -> TeletextCustomPagesWire {
        TeletextCustomPagesWire {
            schema: TELETEXT_CUSTOM_PAGES_SCHEMA.to_owned(),
            generation: "g".to_owned(),
            pages,
        }
    }

    #[test]
    fn exact_rows_become_overlays_on_empty_base_pages() {
        let document =
            TeletextCustomPagesDocument::from_wire(wire(vec![page(150, 0)]), "g").unwrap();
        assert_eq!(document.draft().initial_page.get(), 150);
        assert!(document.draft().pages[0].rows.is_empty());
        document.draft().validate_structure().unwrap();
        let overlay = &document.overlays()[0];
        assert_eq!(overlay.rows()[0].row(), 1);
        assert_eq!(overlay.rows()[0].cells()[0], 0x01);
        assert_eq!(&overlay.rows()[0].cells()[1..6], b"Hello");
        assert_eq!(document.origins()[0].element_revision, 3);
    }

    #[test]
    fn rejects_wrong_correlation_duplicates_and_bad_rows() {
        assert_eq!(
            TeletextCustomPagesDocument::from_wire(wire(vec![page(150, 0)]), "other"),
            Err(TeletextCustomPagesError::Correlation)
        );
        assert_eq!(
            TeletextCustomPagesDocument::from_wire(wire(vec![page(150, 0), page(150, 0)]), "g"),
            Err(TeletextCustomPagesError::Duplicate {
                page: 150,
                subcode: 0
            })
        );
        assert_eq!(
            TeletextCustomPagesDocument::from_wire(wire(vec![page(99, 0)]), "g"),
            Err(TeletextCustomPagesError::Address { page: 99 })
        );
        let mut header = page(150, 0);
        header.rows[0].row = 0;
        assert_eq!(
            TeletextCustomPagesDocument::from_wire(wire(vec![header]), "g"),
            Err(TeletextCustomPagesError::Row { page: 150, row: 0 })
        );
        let mut eight_bit = page(150, 0);
        eight_bit.rows[0].cells.replace_range(0..2, "ff");
        assert_eq!(
            TeletextCustomPagesDocument::from_wire(wire(vec![eight_bit]), "g"),
            Err(TeletextCustomPagesError::Row { page: 150, row: 1 })
        );
    }

    #[test]
    fn selection_identity_is_constant_and_valid() {
        let selection = teletext_custom_pages_selection(4);
        assert_eq!(selection, teletext_custom_pages_selection(4));
        assert!(is_teletext_custom_pages_producer(&selection.addon_id));
        assert_eq!(selection.installation.package_sha256.len(), 64);
        assert!(selection
            .installation
            .package_sha256
            .bytes()
            .all(|byte| byte.is_ascii_hexdigit()));
    }
}
