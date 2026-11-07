//! Field markers in uploaded pages and filling bound fields with data.
//!
//! A marker is typed in the editor as `%NAME%%%%`: a percent sign, a name of
//! letters, digits or underscores, then one or more percent signs. The whole
//! run is the field width. `%` is identical in every Latin G0 national
//! option, unlike `{`, `}` or `#`, so markers survive any character set.

use ersatzrs_domain::{TeletextElementBinding, TeletextElementBindingSource};

use super::model::ElementPage;

/// A marker found in an uploaded page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldMarker {
    pub name: String,
    pub subpage: u16,
    pub row: u8,
    pub column: u8,
    pub length: u8,
}

/// Find all `%NAME%%%` markers on rows 1..=24 of every subpage.
#[must_use]
pub fn find_field_markers(page: &ElementPage) -> Vec<FieldMarker> {
    let mut markers = Vec::new();
    for (subpage_index, subpage) in page.subpages.iter().enumerate() {
        for row in 1..=24_usize {
            let cells = &subpage.rows[row];
            let mut column = 0;
            while column < 40 {
                if cells[column] != b'%' {
                    column += 1;
                    continue;
                }
                let name_start = column + 1;
                let mut end = name_start;
                while end < 40 && (cells[end].is_ascii_alphanumeric() || cells[end] == b'_') {
                    end += 1;
                }
                let name_end = end;
                while end < 40 && cells[end] == b'%' {
                    end += 1;
                }
                if name_end > name_start && end > name_end {
                    markers.push(FieldMarker {
                        name: String::from_utf8_lossy(&cells[name_start..name_end])
                            .to_ascii_uppercase(),
                        subpage: u16::try_from(subpage_index).unwrap_or(u16::MAX),
                        row: row as u8,
                        column: column as u8,
                        length: (end - column) as u8,
                    });
                    column = end;
                } else {
                    column += 1;
                }
            }
        }
    }
    markers
}

/// Bindings for freshly found markers, keeping earlier choices for markers
/// that still exist at the same place (re-uploads of a new version).
#[must_use]
pub fn bindings_for_markers(
    markers: &[FieldMarker],
    previous: &[TeletextElementBinding],
) -> Vec<TeletextElementBinding> {
    markers
        .iter()
        .map(|marker| {
            let kept = previous.iter().find(|binding| {
                binding.field == marker.name
                    && binding.subpage == marker.subpage
                    && binding.row == marker.row
                    && binding.column == marker.column
            });
            TeletextElementBinding {
                field: marker.name.clone(),
                subpage: marker.subpage,
                row: marker.row,
                column: marker.column,
                length: marker.length,
                source: kept.map_or_else(
                    || TeletextElementBindingSource::suggest(&marker.name),
                    |binding| binding.source,
                ),
                static_text: kept
                    .map(|binding| binding.static_text.clone())
                    .unwrap_or_default(),
            }
        })
        .collect()
}

/// Data available to fill fields at one moment on one channel.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FieldValues {
    pub channel_name: String,
    pub channel_number: String,
    pub date: String,
    pub time: String,
    pub now_title: String,
    pub now_start: String,
    pub next_title: String,
    pub next_start: String,
}

impl FieldValues {
    /// The text for one source, or `None` to keep the page as designed.
    #[must_use]
    pub fn text_for<'a>(&'a self, binding: &'a TeletextElementBinding) -> Option<&'a str> {
        Some(match binding.source {
            TeletextElementBindingSource::Unset => return None,
            TeletextElementBindingSource::Static => &binding.static_text,
            TeletextElementBindingSource::ChannelName => &self.channel_name,
            TeletextElementBindingSource::ChannelNumber => &self.channel_number,
            TeletextElementBindingSource::Date => &self.date,
            TeletextElementBindingSource::Time => &self.time,
            TeletextElementBindingSource::NowTitle => &self.now_title,
            TeletextElementBindingSource::NowStart => &self.now_start,
            TeletextElementBindingSource::NextTitle => &self.next_title,
            TeletextElementBindingSource::NextStart => &self.next_start,
        })
    }
}

/// Write bound values into a copy of the page. Unset bindings keep the
/// original cells; text is reduced to safe Level 1 characters, cut to the
/// field width and padded with spaces so the layout never moves.
#[must_use]
pub fn apply_field_values(
    page: &ElementPage,
    bindings: &[TeletextElementBinding],
    values: &FieldValues,
) -> ElementPage {
    let mut filled = page.clone();
    for binding in bindings {
        let Some(text) = values.text_for(binding) else {
            continue;
        };
        let Some(subpage) = filled.subpages.get_mut(usize::from(binding.subpage)) else {
            continue;
        };
        let row = usize::from(binding.row);
        if !(1..=24).contains(&row) {
            continue;
        }
        let start = usize::from(binding.column);
        let end = (start + usize::from(binding.length)).min(40);
        let mut encoded = level1_safe_text(text).into_iter();
        for cell in &mut subpage.rows[row][start..end] {
            *cell = encoded.next().unwrap_or(b' ');
        }
        subpage.present[row] = true;
    }
    filled
}

/// Map text to characters that look the same in every Latin national option.
#[must_use]
pub fn level1_safe_text(text: &str) -> Vec<u8> {
    let mut output = Vec::with_capacity(text.len());
    for character in text.chars() {
        let folded: &str = match character {
            'à' | 'á' | 'â' | 'ã' | 'ä' | 'å' | 'ā' | 'ą' => "a",
            'À' | 'Á' | 'Â' | 'Ã' | 'Ä' | 'Å' | 'Ā' | 'Ą' => "A",
            'æ' => "ae",
            'Æ' => "AE",
            'ç' | 'ć' | 'č' => "c",
            'Ç' | 'Ć' | 'Č' => "C",
            'ď' | 'đ' => "d",
            'Ď' | 'Đ' => "D",
            'è' | 'é' | 'ê' | 'ë' | 'ē' | 'ę' | 'ě' => "e",
            'È' | 'É' | 'Ê' | 'Ë' | 'Ē' | 'Ę' | 'Ě' => "E",
            'ğ' => "g",
            'Ğ' => "G",
            'ì' | 'í' | 'î' | 'ï' | 'ı' => "i",
            'Ì' | 'Í' | 'Î' | 'Ï' | 'İ' => "I",
            'ł' => "l",
            'Ł' => "L",
            'ñ' | 'ń' | 'ň' => "n",
            'Ñ' | 'Ń' | 'Ň' => "N",
            'ò' | 'ó' | 'ô' | 'õ' | 'ö' | 'ø' | 'ő' => "o",
            'Ò' | 'Ó' | 'Ô' | 'Õ' | 'Ö' | 'Ø' | 'Ő' => "O",
            'œ' => "oe",
            'Œ' => "OE",
            'ř' => "r",
            'Ř' => "R",
            'ś' | 'š' | 'ş' => "s",
            'Ś' | 'Š' | 'Ş' => "S",
            'ß' => "ss",
            'ť' | 'ţ' => "t",
            'Ť' | 'Ţ' => "T",
            'ù' | 'ú' | 'û' | 'ü' | 'ů' | 'ű' => "u",
            'Ù' | 'Ú' | 'Û' | 'Ü' | 'Ů' | 'Ű' => "U",
            'ý' | 'ÿ' => "y",
            'Ý' | 'Ÿ' => "Y",
            'ź' | 'ż' | 'ž' => "z",
            'Ź' | 'Ż' | 'Ž' => "Z",
            '‘' | '’' | '`' | '´' => "'",
            '“' | '”' | '„' => "\"",
            '–' | '—' | '_' | '~' => "-",
            '…' => "...",
            '[' | '{' => "(",
            ']' | '}' => ")",
            '\\' => "/",
            '|' => "!",
            '@' => "a",
            '$' => "S",
            '#' | '^' => " ",
            character if (' '..='~').contains(&character) => {
                output.push(character as u8);
                continue;
            }
            _ => "?",
        };
        output.extend_from_slice(folded.as_bytes());
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::teletext_elements::model::ElementSubpage;

    fn page_with(row: usize, text: &[u8]) -> ElementPage {
        let mut subpage = ElementSubpage::default();
        let mut cells = [0x20; 40];
        cells[..text.len()].copy_from_slice(text);
        subpage.set_row(row, &cells);
        ElementPage {
            description: String::new(),
            page: 0x100,
            subpages: vec![subpage],
        }
    }

    #[test]
    fn markers_are_found_with_their_full_width() {
        let page = page_with(4, b"\x06Kanaal: \x03%kanaal%%%%%% %NU%%");
        let markers = find_field_markers(&page);
        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].name, "KANAAL");
        assert_eq!(
            (markers[0].row, markers[0].column, markers[0].length),
            (4, 10, 13)
        );
        assert_eq!((markers[1].column, markers[1].length), (24, 5));
        // A lone or unterminated percent sign is not a marker.
        assert!(find_field_markers(&page_with(5, b"50% off %ABC")).is_empty());
    }

    #[test]
    fn rebinding_keeps_choices_and_suggests_known_names() {
        let page = page_with(8, b"%NOW%%%%%% %X%%");
        let markers = find_field_markers(&page);
        let first = bindings_for_markers(&markers, &[]);
        assert_eq!(first[0].source, TeletextElementBindingSource::NowTitle);
        assert_eq!(first[1].source, TeletextElementBindingSource::Unset);
        let mut chosen = first.clone();
        chosen[1].source = TeletextElementBindingSource::Static;
        chosen[1].static_text = "Hi".to_owned();
        let again = bindings_for_markers(&markers, &chosen);
        assert_eq!(again[1].static_text, "Hi");
    }

    #[test]
    fn values_are_cut_padded_and_made_safe() {
        let page = page_with(8, b"\x07%NOW%%%%%%end");
        let bindings = bindings_for_markers(&find_field_markers(&page), &[]);
        let values = FieldValues {
            now_title: "Één #1 [live] programma".to_owned(),
            ..FieldValues::default()
        };
        let filled = apply_field_values(&page, &bindings, &values);
        assert_eq!(&filled.subpages[0].rows[8][..14], b"\x07Een  1 (liend");
        let short = FieldValues {
            now_title: "Hi".to_owned(),
            ..FieldValues::default()
        };
        let filled = apply_field_values(&page, &bindings, &short);
        assert_eq!(&filled.subpages[0].rows[8][..14], b"\x07Hi        end");
    }
}
