//! Explicit Arabic glyph forms in authored cell order, not string shaping.
//!
//! EN300706 Tables44/45 specify glyphs, not a Unicode mapping. These aliases
//! identify their visible contextual forms using Unicode Presentation Forms A/B
//! names (Unicode chapter9.2 and charts UFB50/UFE70). Retain the original scalar;
//! never normalize it into a base letter or infer joining/bidi/cell allocation.
//! The separate Seen-family tail is FE73 (Unicode proposal L2/01-095), not a
//! license to collapse an isolated/final whole letter into its one-cell body.
use super::Character;

// Columns2..7, in printed row order. The final rectangle is still raw-coded:
// neither an arbitrary black-square scalar nor stock decoder PUA is its alias.
const G0: [&str; 6] = [
    " !\"£$%\u{fe73}\u{fef1})(*+،-./",
    "0123456789:؛>=<؟",
    "\u{fe94}\u{fe80}\u{fe92}\u{fe8f}\u{fe98}\u{fe95}\u{fe8e}\u{fe8d}\u{fe91}\u{fe93}\u{fe97}\u{fe9b}\u{fe9f}\u{fea3}\u{fea7}\u{fea9}",
    "\u{feab}\u{fead}\u{feaf}\u{feb3}\u{feb7}\u{febb}\u{febf}\u{fec3}\u{fec7}\u{fecb}\u{fecf}\u{fe9c}\u{fea0}\u{fea4}\u{fea8}#",
    "ـ\u{fed3}\u{fed7}\u{fedb}\u{fedf}\u{fee3}\u{fee7}\u{feeb}\u{feed}\u{fef0}\u{fef3}\u{fe99}\u{fe9d}\u{fea1}\u{fea5}\u{fef4}",
    "\u{feef}\u{fecc}\u{fed0}\u{fed4}\u{fed1}\u{fed8}\u{fed5}\u{fed9}\u{fee0}\u{fedd}\u{fee4}\u{fee1}\u{fee8}\u{fee5}\u{fefb}",
];

// Table45 column4 contains standalone Latin letters, not diacritical marks.
// 7/F is unmarked and deliberately absent. 2/0 is explicitly SPACE (note1).
const G2: [&str; 6] = [
    " \u{fec9}\u{fe81}\u{fe83}\u{fe85}\u{fe87}\u{fe8b}\u{fe89}\u{fb7c}\u{fb7d}\u{fb7a}\u{fb58}\u{fb59}\u{fb56}\u{fb8a}\u{fb94}",
    "٠١٢٣٤٥٦٧٨٩\u{fece}\u{fecd}\u{fefc}\u{feec}\u{feea}\u{fee9}",
    "àABCDEFGHIJKLMNO",
    "PQRSTUVWXYZëêùî\u{feca}",
    "éabcdefghijklmno",
    "pqrstuvwxyzâôûç",
];

pub(super) fn encode_g0(character: char) -> Option<Character> {
    if character == '@' {
        // Table29/cl12.3.4: mode10000,code2A means @ in every G0 context;
        // the transmitted native G0/2A still means asterisk.
        return Some(Character {
            fallback: b' ',
            enhancement: Some((0x10, 0x2a)),
        });
    }
    Some(Character {
        fallback: encode(&G0, character)?,
        enhancement: None,
    })
}

pub(super) fn encode_g2(character: char) -> Option<Character> {
    Some(Character {
        fallback: b' ',
        enhancement: Some((0x0f, encode(&G2, character)?)),
    })
}

pub(super) fn decode_g2(code: u8) -> Option<char> {
    decode(&G2, code)
}

pub(super) fn decode_g0_replacement(code: u8) -> Option<char> {
    if code == 0x2a {
        Some('@')
    } else {
        decode(&G0, code)
    }
}

fn encode(columns: &[&str; 6], character: char) -> Option<u8> {
    columns.iter().enumerate().find_map(|(column, text)| {
        text.chars()
            .position(|value| value == character)
            .and_then(|row| u8::try_from(0x20 + column * 16 + row).ok())
    })
}

fn decode(columns: &[&str; 6], code: u8) -> Option<char> {
    let offset = usize::from(code.checked_sub(0x20)?);
    columns.get(offset / 16)?.chars().nth(offset % 16)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complete_explicit_columns_keep_unique_scalar_identity_and_inverse_bounds() {
        for columns in [&G0, &G2] {
            assert_eq!(
                columns
                    .iter()
                    .map(|text| text.chars().count())
                    .sum::<usize>(),
                95
            );
            let mut unique = std::collections::BTreeSet::new();
            for (column, text) in columns.iter().enumerate() {
                assert_eq!(text.chars().count(), if column == 5 { 15 } else { 16 });
                for (row, character) in text.chars().enumerate() {
                    let code = u8::try_from(0x20 + column * 16 + row).unwrap();
                    assert!(unique.insert(character));
                    assert_eq!(encode(columns, character), Some(code));
                    assert_eq!(decode(columns, code), Some(character));
                }
            }
            for code in (0..0x20).chain(0x7f..=0xff) {
                assert_eq!(decode(columns, code), None);
            }
        }
    }

    #[test]
    fn transmitted_asterisk_and_x26_at_keep_distinct_meaning_and_fallback() {
        let asterisk = encode_g0('*').unwrap();
        assert_eq!((asterisk.fallback, asterisk.enhancement), (0x2a, None));
        let at = encode_g0('@').unwrap();
        assert_eq!((at.fallback, at.enhancement), (0x20, Some((0x10, 0x2a))));
        assert_eq!(decode_g0_replacement(0x2a), Some('@'));
        assert_eq!(
            super::super::decode_generated_replacement(
                0x10,
                0x2a,
                ersatzrs_domain::TeletextCharsetPair::ArabicArabic,
            ),
            Some('@')
        );
        for character in [
            '\0', '\u{feff}', '🙂', '\u{e65e}', '\u{feb1}', '\u{feb2}', 'ب',
        ] {
            assert!(encode_g0(character).is_none());
            assert!(encode_g2(character).is_none());
        }
    }
}
