//! Level 1 spacing-attribute decoder for browser previews (EN 300 706 §12.2).
//!
//! This decodes the coded Level 1 plane only. Enhancement packets, DRCS and
//! receiver-specific header formatting are not applied, so the result is an
//! editorial preview, not engine or receiver admission.

/// One decoded display cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Level1Cell {
    /// 7-bit character code to show (0x20 for a blank control cell).
    pub code: u8,
    /// Colour index 0..=7 (black, red, green, yellow, blue, magenta, cyan, white).
    pub foreground: u8,
    pub background: u8,
    /// Draw `code` as a block mosaic instead of a character.
    pub mosaic: bool,
    pub separated: bool,
    pub double_height: bool,
    pub flash: bool,
    pub conceal: bool,
}

impl Level1Cell {
    const BLANK: Self = Self {
        code: 0x20,
        foreground: 7,
        background: 0,
        mosaic: false,
        separated: false,
        double_height: false,
        flash: false,
        conceal: false,
    };
}

/// A decoded page: 25 rows of 40 cells plus rows hidden by double height.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Level1Screen {
    pub rows: Vec<[Level1Cell; 40]>,
    /// Row is covered by the lower half of a double-height row above it.
    pub covered: [bool; 25],
}

/// Decode one row of 7-bit codes.
#[must_use]
pub fn decode_level1_row(codes: &[u8; 40]) -> [Level1Cell; 40] {
    let mut cells = [Level1Cell::BLANK; 40];
    let mut foreground = 7_u8;
    let mut background = 0_u8;
    let mut mosaic = false;
    let mut separated = false;
    let mut double_height = false;
    let mut flash = false;
    let mut conceal = false;
    let mut hold = false;
    let mut held_code = 0x20_u8;
    let mut held_separated = false;

    for (cell, &raw) in cells.iter_mut().zip(codes) {
        let code = raw & 0x7f;
        if code >= 0x20 {
            let is_mosaic = mosaic && !(0x40..0x60).contains(&code);
            if is_mosaic {
                held_code = code;
                held_separated = separated;
            }
            *cell = Level1Cell {
                code,
                foreground,
                background,
                mosaic: is_mosaic,
                separated: is_mosaic && separated,
                double_height,
                flash,
                conceal,
            };
            continue;
        }

        // "Set-at" attributes take effect in this control cell.
        match code {
            0x09 => flash = false,
            0x0c => {
                if double_height {
                    held_code = 0x20;
                }
                double_height = false;
            }
            0x18 => conceal = true,
            0x19 => separated = false,
            0x1a => separated = true,
            0x1c => background = 0,
            0x1d => background = foreground,
            0x1e => hold = true,
            _ => {}
        }

        let (shown, shown_separated, shown_mosaic) = if hold && mosaic {
            (held_code, held_separated, held_code != 0x20)
        } else {
            (0x20, false, false)
        };
        *cell = Level1Cell {
            code: shown,
            foreground,
            background,
            mosaic: shown_mosaic,
            separated: shown_separated,
            double_height,
            flash,
            conceal,
        };

        // "Set-after" attributes take effect from the next cell.
        match code {
            0x00..=0x07 => {
                foreground = code;
                if mosaic {
                    held_code = 0x20;
                }
                mosaic = false;
                conceal = false;
            }
            0x10..=0x17 => {
                foreground = code - 0x10;
                mosaic = true;
                conceal = false;
            }
            0x08 => flash = true,
            0x0d => {
                if !double_height {
                    held_code = 0x20;
                }
                double_height = true;
            }
            0x1f => hold = false,
            _ => {}
        }
    }
    cells
}

/// Decode a whole page. Row 0's first eight cells are left blank for the
/// channel's own page header.
#[must_use]
pub fn decode_level1_page(rows: &[[u8; 40]]) -> Level1Screen {
    let mut decoded = Vec::with_capacity(25);
    let mut covered = [false; 25];
    for row in 0..25 {
        let mut codes = rows.get(row).copied().unwrap_or([0x20; 40]);
        if row == 0 {
            codes[..8].fill(0x20);
        }
        if covered[row] {
            decoded.push([Level1Cell::BLANK; 40]);
            continue;
        }
        let cells = decode_level1_row(&codes);
        if (1..=22).contains(&row) && cells.iter().any(|cell| cell.double_height) {
            covered[row + 1] = true;
        }
        decoded.push(cells);
    }
    Level1Screen {
        rows: decoded,
        covered,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(text: &[u8]) -> [u8; 40] {
        let mut row = [0x20; 40];
        row[..text.len()].copy_from_slice(text);
        row
    }

    #[test]
    fn colours_change_after_the_control_cell_and_new_background_is_immediate() {
        let cells = decode_level1_row(&row(b"\x01A\x1dB\x1cC"));
        assert_eq!((cells[0].foreground, cells[0].code), (7, 0x20));
        assert_eq!((cells[1].foreground, cells[1].background), (1, 0));
        // New background is set-at: the control cell itself is red.
        assert_eq!(cells[2].background, 1);
        assert_eq!(cells[3].background, 1);
        assert_eq!(cells[4].background, 0);
        assert_eq!(cells[5].background, 0);
    }

    #[test]
    fn mosaics_blast_through_capitals_and_hold_repeats_the_last_block() {
        let cells = decode_level1_row(&row(b"\x12\x7fA\x1e\x13\x35"));
        assert!(cells[1].mosaic);
        assert_eq!(cells[1].foreground, 2);
        assert!(!cells[2].mosaic, "0x40-0x5F blast through as characters");
        // Hold mosaics: the colour change cell shows the held 0x7F block.
        assert!(cells[4].mosaic);
        assert_eq!(cells[4].code, 0x7f);
        assert_eq!(cells[5].foreground, 3);
    }

    #[test]
    fn separated_mosaics_and_conceal_reset_by_colour() {
        let cells = decode_level1_row(&row(b"\x1a\x11\x23\x18\x01X"));
        assert!(cells[2].separated);
        assert!(cells[3].conceal);
        assert!(!cells[5].conceal);
    }

    #[test]
    fn double_height_covers_the_next_row_only_between_rows_one_and_twenty_two() {
        let mut rows = vec![[0x20; 40]; 25];
        rows[2] = row(b"\x0dBIG");
        rows[23] = row(b"\x0dLOW");
        let screen = decode_level1_page(&rows);
        assert!(screen.covered[3]);
        assert!(!screen.covered[24]);
        assert!(screen.rows[2][1].double_height);
    }
}
