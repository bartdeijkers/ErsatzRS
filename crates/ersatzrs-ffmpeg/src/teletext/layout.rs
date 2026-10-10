use ersatzrs_domain::{
    TeletextColour, TeletextHeight, TeletextPageDraft, TeletextPageKind, TeletextRunContent,
    TeletextStyle, TELETEXT_COLUMNS, TELETEXT_ROWS,
};

use super::{charset, TeletextEncodingError};

pub(super) struct Layout {
    pub rows: [[u8; TELETEXT_COLUMNS]; TELETEXT_ROWS],
    pub enhancements: Vec<u32>,
}

pub(super) fn compile(page: &TeletextPageDraft) -> Result<Layout, TeletextEncodingError> {
    page.validate_structure()?;
    if page.national_option.get() >= 7 {
        return Err(TeletextEncodingError::NationalOption {
            address: page.address,
        });
    }
    let mut layout = Layout {
        rows: [[b' '; TELETEXT_COLUMNS]; TELETEXT_ROWS],
        enhancements: Vec::new(),
    };
    let mut rows: Vec<_> = page.rows.iter().collect();
    rows.sort_by_key(|row| row.row);
    for row in rows {
        let mut runs: Vec<_> = row.runs.iter().collect();
        runs.sort_by_key(|run| run.column);
        let mut state = State::default();
        let mut end = if row.row == 0 { 8 } else { 0 };
        let mut active = false;
        for run in runs {
            let error = || TeletextEncodingError::ControlCells {
                address: page.address,
                row: row.row,
                column: run.column,
            };
            if page.kind != TeletextPageKind::FullPage && (row.row == 0 || !run.style.boxed) {
                return Err(TeletextEncodingError::HiddenContent {
                    address: page.address,
                    row: row.row,
                    column: run.column,
                });
            }
            if row.row == 0 && run.style.height != TeletextHeight::Normal {
                return Err(TeletextEncodingError::HeaderHeight {
                    address: page.address,
                });
            }
            if run.style.foreground == TeletextColour::Black {
                return Err(TeletextEncodingError::BlackForeground {
                    address: page.address,
                    row: row.row,
                    column: run.column,
                });
            }
            let (graphics, separated) = match &run.content {
                TeletextRunContent::Text { .. } => (false, false),
                TeletextRunContent::Mosaics { separated, .. } => (true, *separated),
            };
            let controls = state.transition(run.style, graphics, separated);
            let start = usize::from(run.column);
            if controls.len() > start - end {
                return Err(error());
            }
            layout.rows[usize::from(row.row)][start - controls.len()..start]
                .copy_from_slice(&controls);
            match &run.content {
                TeletextRunContent::Text { text } => {
                    for (column, character) in (run.column..).zip(text.chars()) {
                        let encoded = charset::encode(character, page.national_option.get())
                            .ok_or(TeletextEncodingError::Character {
                                address: page.address,
                                row: row.row,
                                column,
                                codepoint: u32::from(character),
                            })?;
                        layout.rows[usize::from(row.row)][usize::from(column)] = encoded.fallback;
                        if let Some((mode, data)) = encoded.enhancement {
                            if !active {
                                let (address, row_mode) = if row.row == 0 {
                                    (63, 7)
                                } else if row.row == 24 {
                                    (40, 4)
                                } else {
                                    (row.row + 40, 4)
                                };
                                layout.enhancements.push(triplet(address, row_mode, 0));
                                active = true;
                            }
                            layout.enhancements.push(triplet(column, mode, data));
                        }
                        end = usize::from(column) + 1;
                    }
                }
                TeletextRunContent::Mosaics { cells, .. } => {
                    for (column, cell) in (run.column..).zip(cells) {
                        let bits = cell.blocks();
                        layout.rows[usize::from(row.row)][usize::from(column)] =
                            0x20 | (bits & 0x1f) | ((bits & 0x20) << 1);
                        end = usize::from(column) + 1;
                    }
                }
            }
        }
    }
    // Sixteen designation codes, thirteen triplets each, including termination.
    if layout.enhancements.len() >= 16 * 13 {
        return Err(TeletextEncodingError::Enhancements {
            address: page.address,
        });
    }
    Ok(layout)
}

pub(super) const fn triplet(address: u8, mode: u8, data: u8) -> u32 {
    (address as u32) | ((mode as u32) << 6) | ((data as u32) << 11)
}

#[derive(Default)]
struct State {
    style: TeletextStyle,
    graphics: bool,
    separated: bool,
}

impl State {
    fn colour(&mut self, colour: TeletextColour, graphics: bool, output: &mut Vec<u8>) {
        output.push(colour_code(colour) | if graphics { 0x10 } else { 0 });
        self.style.foreground = colour;
        self.style.conceal = false;
        self.graphics = graphics;
    }

    fn transition(&mut self, target: TeletextStyle, graphics: bool, separated: bool) -> Vec<u8> {
        let mut output = Vec::new();
        if self.style.background != target.background {
            if target.background == TeletextColour::Black {
                output.push(0x1c);
            } else {
                if self.style.foreground != target.background {
                    self.colour(target.background, graphics, &mut output);
                }
                output.push(0x1d);
            }
        }
        if self.style.foreground != target.foreground
            || self.graphics != graphics
            || (self.style.conceal && !target.conceal)
        {
            self.colour(target.foreground, graphics, &mut output);
        }
        if self.style.height != target.height {
            output.push(if target.height == TeletextHeight::Double {
                0x0d
            } else {
                0x0c
            });
        }
        if self.style.boxed != target.boxed {
            if target.boxed {
                output.extend([0x0b, 0x0b]);
            } else {
                output.push(0x0a);
            }
        }
        if self.style.flash != target.flash {
            output.push(if target.flash { 0x08 } else { 0x09 });
        }
        if graphics && self.separated != separated {
            output.push(if separated { 0x1a } else { 0x19 });
        }
        if target.conceal && !self.style.conceal {
            output.push(0x18);
        }
        self.style = target;
        if graphics {
            self.separated = separated;
        }
        output
    }
}

const fn colour_code(colour: TeletextColour) -> u8 {
    match colour {
        TeletextColour::Black => 0,
        TeletextColour::Red => 1,
        TeletextColour::Green => 2,
        TeletextColour::Yellow => 3,
        TeletextColour::Blue => 4,
        TeletextColour::Magenta => 5,
        TeletextColour::Cyan => 6,
        TeletextColour::White => 7,
    }
}
