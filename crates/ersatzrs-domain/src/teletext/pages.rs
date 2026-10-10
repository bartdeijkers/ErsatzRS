//! Typed authored drafts; no renderer arguments or transport bytes (ADR 0121).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use super::{
    TeletextLanguage, TeletextMosaic, TeletextNationalOption, TeletextPageAddress,
    TeletextPageNumber, TeletextSubcode,
};

/// Bounded page/subpage records accepted from one producer invocation.
pub const MAX_TELETEXT_PAGE_RECORDS: usize = 1_024;
/// Physical columns; the first eight header cells belong to the address/control area.
pub const TELETEXT_COLUMNS: usize = 40;
/// Display rows zero through twenty-four; navigation shares row twenty-four.
/// Packet X/25 carries non-display data and is not an extra authored display row.
pub const TELETEXT_ROWS: usize = 25;

/// Basic colour intent; DVB representability is checked by the encoder.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextColour {
    Black,
    Red,
    Green,
    Yellow,
    Blue,
    Magenta,
    Cyan,
    #[default]
    White,
}

/// Normal or two-row display height; not an arbitrary font size.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextHeight {
    #[default]
    Normal,
    Double,
}

/// Page header semantics; a newsflash is not a subtitle selection flag.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextPageKind {
    #[default]
    FullPage,
    Subtitle,
    Newsflash,
}

/// Declarative attributes, independently represented by browser and DVB output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextStyle {
    pub foreground: TeletextColour,
    pub background: TeletextColour,
    pub height: TeletextHeight,
    pub boxed: bool,
    pub flash: bool,
    pub conceal: bool,
}

impl Default for TeletextStyle {
    fn default() -> Self {
        Self {
            foreground: TeletextColour::White,
            background: TeletextColour::Black,
            height: TeletextHeight::Normal,
            boxed: false,
            flash: false,
            conceal: false,
        }
    }
}

/// Unicode text or typed six-block mosaics; strings remain literal text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeletextRunContent {
    Text {
        text: String,
    },
    Mosaics {
        cells: Vec<TeletextMosaic>,
        separated: bool,
    },
}

/// One declarative run at its intended display position.
///
/// Column is the first glyph, not a raw control-byte offset. A transport encoder
/// must fit its spacing attributes in unoccupied preceding cells or reject the
/// draft; structural validation cannot promise that an otherwise full row fits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextRun {
    pub column: u8,
    pub style: TeletextStyle,
    pub content: TeletextRunContent,
}

/// Sparse authored row; absent cells are blank, never synthetic guide content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextRow {
    /// Header zero uses only columns eight through thirty-nine.
    pub row: u8,
    pub runs: Vec<TeletextRun>,
}

/// Exact subpage or the first actual subpage of a display page.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextNavigationTarget {
    pub page: TeletextPageNumber,
    /// None means a page target, not an invented or merged subpage.
    pub subcode: Option<TeletextSubcode>,
}

/// Link intent independent of colour-only browser controls or raw packet slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TeletextNavigationRole {
    Index,
    Red,
    Green,
    Yellow,
    Cyan,
}

/// A typed page link. The browser supplies readable labels for each role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextNavigationLink {
    pub role: TeletextNavigationRole,
    pub target: TeletextNavigationTarget,
}

/// Unpublished page intent; includes no product-specific page map or layout.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextPageDraft {
    pub address: TeletextPageAddress,
    pub kind: TeletextPageKind,
    pub language: TeletextLanguage,
    pub national_option: TeletextNationalOption,
    pub rows: Vec<TeletextRow>,
    pub navigation: Vec<TeletextNavigationLink>,
}

/// A complete bounded producer draft, not a validated published generation.
///
/// Generation/configuration fencing, charset selection, encoded control-cell
/// occupancy and atomic promotion remain owned by the host engine boundaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TeletextPageSetDraft {
    pub language: TeletextLanguage,
    pub initial_page: TeletextPageNumber,
    pub pages: Vec<TeletextPageDraft>,
}

/// Structural failure with numeric location, without copying programme text.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TeletextStructureError {
    #[error("teletext draft must contain between one and 1024 page/subpage records")]
    RecordCount,
    #[error("teletext draft repeats page/subpage {address:?}")]
    DuplicatePage { address: TeletextPageAddress },
    #[error("teletext draft initial page is absent")]
    MissingInitialPage,
    #[error("teletext page {address:?} has an invalid or repeated row {row}")]
    Row {
        address: TeletextPageAddress,
        row: u8,
    },
    #[error("teletext page {address:?} row {row} column {column} has invalid text or extent")]
    Run {
        address: TeletextPageAddress,
        row: u8,
        column: u8,
    },
    #[error("teletext page {address:?} row {row} column {column} overlaps another run")]
    Overlap {
        address: TeletextPageAddress,
        row: u8,
        column: u8,
    },
    #[error("teletext page {address:?} row {row} has invalid or hidden double-height content")]
    DoubleHeight {
        address: TeletextPageAddress,
        row: u8,
    },
    #[error("teletext page {address:?} has an invalid or repeated navigation role")]
    Navigation { address: TeletextPageAddress },
    #[error("teletext page {address:?} links to absent target {target:?}")]
    MissingTarget {
        address: TeletextPageAddress,
        target: TeletextNavigationTarget,
    },
}

impl TeletextPageSetDraft {
    /// Check count, identities, logical extents, height and internal references.
    ///
    /// This preserves Unicode exactly. It does not establish DVB character-set
    /// encoding or spacing-control occupancy, nor authorize publishing a draft.
    /// # Errors
    /// Returns the first structural failure with its page/row/column location.
    pub fn validate_structure(&self) -> Result<(), TeletextStructureError> {
        if self.pages.is_empty() || self.pages.len() > MAX_TELETEXT_PAGE_RECORDS {
            return Err(TeletextStructureError::RecordCount);
        }
        let mut addresses = BTreeSet::new();
        for page in &self.pages {
            if !addresses.insert(page.address) {
                return Err(TeletextStructureError::DuplicatePage {
                    address: page.address,
                });
            }
            page.validate_structure()?;
        }
        if !addresses
            .iter()
            .any(|address| address.page == self.initial_page)
        {
            return Err(TeletextStructureError::MissingInitialPage);
        }
        for page in &self.pages {
            for link in &page.navigation {
                let exists = match link.target.subcode {
                    Some(subcode) => addresses.contains(&TeletextPageAddress {
                        page: link.target.page,
                        subcode,
                    }),
                    None => addresses
                        .iter()
                        .any(|address| address.page == link.target.page),
                };
                if !exists {
                    return Err(TeletextStructureError::MissingTarget {
                        address: page.address,
                        target: link.target,
                    });
                }
            }
        }
        Ok(())
    }
}

impl TeletextPageDraft {
    /// Validate one page's logical layout before character/transport validation.
    /// # Errors
    /// Returns a location-bearing error without truncating or changing content.
    pub fn validate_structure(&self) -> Result<(), TeletextStructureError> {
        let address = self.address;
        let mut present = [false; TELETEXT_ROWS];
        let mut content = [false; TELETEXT_ROWS];
        let mut double_height = [false; TELETEXT_ROWS];
        for row in &self.rows {
            let row_index = usize::from(row.row);
            if row_index >= TELETEXT_ROWS || present[row_index] {
                return Err(TeletextStructureError::Row {
                    address,
                    row: row.row,
                });
            }
            present[row_index] = true;
            let mut occupied = [false; TELETEXT_COLUMNS];
            for run in &row.runs {
                let invalid = || TeletextStructureError::Run {
                    address,
                    row: row.row,
                    column: run.column,
                };
                let cells = match &run.content {
                    TeletextRunContent::Text { text } => {
                        // Forty scalar cells occupy at most 160 UTF-8 bytes.
                        if text.len() > TELETEXT_COLUMNS * 4 || text.chars().any(char::is_control) {
                            return Err(invalid());
                        }
                        text.chars().count()
                    }
                    TeletextRunContent::Mosaics { cells, .. } => cells.len(),
                };
                let start = usize::from(run.column);
                if cells == 0
                    || cells > TELETEXT_COLUMNS
                    || start >= TELETEXT_COLUMNS
                    || start + cells > TELETEXT_COLUMNS
                    || (row.row == 0 && start < 8)
                {
                    return Err(invalid());
                }
                for cell in &mut occupied[start..start + cells] {
                    if *cell {
                        return Err(TeletextStructureError::Overlap {
                            address,
                            row: row.row,
                            column: run.column,
                        });
                    }
                    *cell = true;
                }
                content[row_index] = true;
                if run.style.height == TeletextHeight::Double {
                    // EN 300 706 table 26: lower-row data is ignored; do not
                    // author double-height controls in rows 23/24 or below.
                    if row.row >= 23 {
                        return Err(TeletextStructureError::DoubleHeight {
                            address,
                            row: row.row,
                        });
                    }
                    double_height[row_index] = true;
                }
            }
        }
        for row in 0_u8..24 {
            let index = usize::from(row);
            if double_height[index] && content[index + 1] {
                return Err(TeletextStructureError::DoubleHeight {
                    address,
                    row: row + 1,
                });
            }
        }
        if self.navigation.len() > 5 {
            return Err(TeletextStructureError::Navigation { address });
        }
        let mut roles = BTreeSet::new();
        for link in &self.navigation {
            if !roles.insert(link.role) {
                return Err(TeletextStructureError::Navigation { address });
            }
        }
        Ok(())
    }
}
