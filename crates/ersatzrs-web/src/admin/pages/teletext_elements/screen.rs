//! HTML cell-grid rendering of a decoded Level 1 teletext page.
//!
//! The grid goes into the shared `preview_frame` content slot. It is plain
//! SSR markup: no interactivity, no hydration state.

use leptos::prelude::*;

stylance::import_crate_style!(style, "src/admin/pages/teletext_elements/screen.module.css");

/// One display cell, already resolved to a glyph.
#[allow(clippy::struct_excessive_bools)] // independent Level 1 display attributes
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenCell {
    pub glyph: char,
    pub foreground: u8,
    pub background: u8,
    /// Sixel bits (b0..b4, b6 as 0..=5) and separated flag for mosaics.
    pub mosaic: Option<(u8, bool)>,
    pub double_height: bool,
    pub flash: bool,
    pub conceal: bool,
    /// Cell belongs to a fillable field.
    pub field: bool,
}

/// A whole page ready to render.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ScreenView {
    /// Rows 0..=24; a covered row (below double height) is empty.
    pub rows: Vec<Vec<ScreenCell>>,
    /// Row contains double height and spans two grid rows.
    pub tall: Vec<bool>,
}

const FOREGROUND: [&str; 8] = [
    style::f0,
    style::f1,
    style::f2,
    style::f3,
    style::f4,
    style::f5,
    style::f6,
    style::f7,
];
const BACKGROUND: [&str; 8] = [
    style::b0,
    style::b1,
    style::b2,
    style::b3,
    style::b4,
    style::b5,
    style::b6,
    style::b7,
];
const SIXEL: [&str; 6] = [
    style::s0,
    style::s1,
    style::s2,
    style::s3,
    style::s4,
    style::s5,
];

fn cell_view(cell: &ScreenCell, row: usize, column: usize, tall: bool) -> AnyView {
    let mut class = format!(
        "{} {} {}",
        style::cell,
        FOREGROUND[usize::from(cell.foreground & 7)],
        BACKGROUND[usize::from(cell.background & 7)]
    );
    for (flag, name) in [
        (cell.double_height, style::double),
        (cell.flash, style::flash),
        (cell.conceal, style::conceal),
        (cell.field, style::field),
        (
            cell.mosaic.is_some_and(|(_, separated)| separated),
            style::separated,
        ),
    ] {
        if flag {
            class.push(' ');
            class.push_str(name);
        }
    }
    let grid = format!(
        "grid-row:{} / span {};grid-column:{}",
        row + 1,
        if tall { 2 } else { 1 },
        column + 1
    );
    match cell.mosaic {
        Some((bits, _)) => {
            let sixels = (0..6)
                .filter(|bit| bits & (1 << bit) != 0)
                .map(|bit| view! { <i class=SIXEL[bit]></i> })
                .collect_view();
            view! { <span class=class style=grid aria-hidden="true">{sixels}</span> }.into_any()
        }
        None => view! { <span class=class style=grid>{cell.glyph.to_string()}</span> }.into_any(),
    }
}

/// Render the grid. `label` is read by screen readers instead of the cells.
#[must_use]
pub fn teletext_screen(view: &ScreenView, label: String, compact: bool) -> AnyView {
    let cells = view
        .rows
        .iter()
        .enumerate()
        .flat_map(|(row, cells)| {
            let tall = view.tall.get(row).copied().unwrap_or(false);
            cells
                .iter()
                .enumerate()
                .map(move |(column, cell)| cell_view(cell, row, column, tall))
                .collect::<Vec<_>>()
        })
        .collect_view();
    let class = if compact {
        format!("{} {}", style::grid, style::compact)
    } else {
        style::grid.to_owned()
    };
    view! {
        <div class=class role="img" aria-label=label data-teletext-screen>
            {cells}
        </div>
    }
    .into_any()
}

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;

#[cfg(not(target_arch = "wasm32"))]
mod native {
    use super::{ScreenCell, ScreenView};
    use ersatzrs_app::teletext_elements::{level1::decode_level1_page, model::ElementSubpage};
    use ersatzrs_domain::{teletext_element_page_label, TeletextElementBinding};

    /// Decode a subpage into a renderable view. The first eight header cells
    /// show the page number the way a receiver would.
    #[must_use]
    pub fn screen_view(
        subpage: &ElementSubpage,
        page_number: u16,
        subpage_index: u16,
        bindings: &[TeletextElementBinding],
    ) -> ScreenView {
        let rows: Vec<[u8; 40]> = subpage.rows[..25].to_vec();
        let screen = decode_level1_page(&rows);
        let designation = subpage.charset_designation();
        let header = format!("P{:<7}", teletext_element_page_label(page_number));
        let mut view = ScreenView::default();
        for (row, cells) in screen.rows.iter().enumerate() {
            if screen.covered[row] {
                view.rows.push(Vec::new());
                view.tall.push(false);
                continue;
            }
            let tall = (1..=22).contains(&row) && cells.iter().any(|cell| cell.double_height);
            let mut out = Vec::with_capacity(40);
            for (column, cell) in cells.iter().enumerate() {
                let glyph = if row == 0 && column < 8 {
                    header.chars().nth(column).unwrap_or(' ')
                } else {
                    ersatzrs_ffmpeg::teletext::teletext_level1_preview_glyph(designation, cell.code)
                };
                let mosaic = cell.mosaic.then(|| {
                    let code = cell.code;
                    let bits = (code & 0x1f) | ((code & 0x40) >> 1);
                    (bits, cell.separated)
                });
                let field = row > 0
                    && bindings.iter().any(|binding| {
                        binding.subpage == subpage_index
                            && usize::from(binding.row) == row
                            && (usize::from(binding.column)
                                ..usize::from(binding.column) + usize::from(binding.length))
                                .contains(&column)
                    });
                out.push(ScreenCell {
                    glyph,
                    foreground: if row == 0 && column < 8 {
                        7
                    } else {
                        cell.foreground
                    },
                    background: cell.background,
                    mosaic,
                    double_height: cell.double_height && tall,
                    flash: cell.flash,
                    conceal: cell.conceal,
                    field,
                });
            }
            view.rows.push(out);
            view.tall.push(tall);
        }
        view
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn header_shows_page_number_and_mosaics_map_sixels() {
            let mut subpage = ElementSubpage::default();
            let mut row = [0x20; 40];
            row[0] = 0x11;
            row[1] = 0x7f;
            row[2] = 0x0d;
            row[3] = 0x01;
            row[4] = b'#';
            subpage.set_row(2, &row);
            let view = screen_view(&subpage, 0x123, 0, &[]);
            let header: String = view.rows[0][..4].iter().map(|cell| cell.glyph).collect();
            assert_eq!(header, "P123");
            assert_eq!(view.rows[2][1].mosaic, Some((0x3f, false)));
            assert_eq!(view.rows[2][4].glyph, '£');
            assert!(view.tall[2]);
            assert!(
                view.rows[3].is_empty(),
                "row below double height is covered"
            );
        }
    }
}
