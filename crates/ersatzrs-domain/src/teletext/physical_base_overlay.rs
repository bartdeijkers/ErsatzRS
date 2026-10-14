//! Exact seven-bit physical base rows for trusted editorial concepts.
//!
//! These values preserve authored control bytes without assigning edit permission,
//! publication authority or receiver readiness. The native interpreter and encoder
//! must still validate supported control semantics against the complete service.

use std::fmt;

use super::TeletextPageAddress;

/// Numeric-only rejection; no supplied page content appears in diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextPhysicalBaseOverlayError {
    #[error("teletext physical base row {row} is outside 0..=24")]
    Row { row: u8 },
    #[error("teletext physical base row {row}, column {column} is not seven-bit")]
    Cell { row: u8, column: u8 },
    #[error("teletext physical header column {column} overlaps reserved identity fields")]
    Header { column: u8 },
    #[error("teletext physical base overlay requires 1..=25 distinct rows")]
    Shape,
    #[error("teletext physical base overlay repeats row {row}")]
    DuplicateRow { row: u8 },
}

/// One exact physical row; header identity/control columns remain reserved.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextPhysicalBaseRow {
    row: u8,
    cells: [u8; 40],
}

impl TeletextPhysicalBaseRow {
    /// Preserve seven-bit cells, including controls whose semantics are checked
    /// at the native interpretation boundary. Row zero reserves columns 0..7.
    /// # Errors
    /// Rejects invalid row numbers, non-seven-bit cells or header identity writes.
    pub fn new(row: u8, cells: [u8; 40]) -> Result<Self, TeletextPhysicalBaseOverlayError> {
        if row > 24 {
            return Err(TeletextPhysicalBaseOverlayError::Row { row });
        }
        for (column, cell) in (0_u8..40).zip(cells.iter()) {
            if *cell > 0x7f {
                return Err(TeletextPhysicalBaseOverlayError::Cell { row, column });
            }
            if row == 0 && column < 8 && *cell != b' ' {
                return Err(TeletextPhysicalBaseOverlayError::Header { column });
            }
        }
        Ok(Self { row, cells })
    }

    #[must_use]
    pub const fn row(&self) -> u8 {
        self.row
    }

    #[must_use]
    pub const fn cells(&self) -> &[u8; 40] {
        &self.cells
    }
}

impl fmt::Debug for TeletextPhysicalBaseRow {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextPhysicalBaseRow")
            .field("row", &self.row)
            .finish_non_exhaustive()
    }
}

/// Replacement rows for one exact page/subcode, separate from its semantic draft.
#[derive(Clone, PartialEq, Eq)]
pub struct TeletextPhysicalBaseOverlay {
    owner: TeletextPageAddress,
    rows: Vec<TeletextPhysicalBaseRow>,
}

impl TeletextPhysicalBaseOverlay {
    /// Validate row shape before constructing an immutable overlay.
    /// # Errors
    /// Rejects empty, oversized or repeated-row input without returning a prefix.
    pub fn new(
        owner: TeletextPageAddress,
        rows: Vec<TeletextPhysicalBaseRow>,
    ) -> Result<Self, TeletextPhysicalBaseOverlayError> {
        if rows.is_empty() || rows.len() > 25 {
            return Err(TeletextPhysicalBaseOverlayError::Shape);
        }
        let mut occupied = 0_u32;
        for row in &rows {
            let mask = 1_u32 << row.row();
            if occupied & mask != 0 {
                return Err(TeletextPhysicalBaseOverlayError::DuplicateRow { row: row.row() });
            }
            occupied |= mask;
        }
        Ok(Self { owner, rows })
    }

    #[must_use]
    pub const fn owner(&self) -> TeletextPageAddress {
        self.owner
    }

    #[must_use]
    pub fn rows(&self) -> &[TeletextPhysicalBaseRow] {
        &self.rows
    }
}

impl fmt::Debug for TeletextPhysicalBaseOverlay {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TeletextPhysicalBaseOverlay")
            .field("owner", &self.owner)
            .field("rows", &self.rows.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests;
