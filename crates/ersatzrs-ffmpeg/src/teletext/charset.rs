//! EN 300 706 tables 35–37 and X/26 column modes from table 29.

use unicode_normalization::char::{compose, decompose_canonical};

const NATIONAL_CODES: [u8; 13] = [
    0x23, 0x24, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e,
];
// Normative character repertoires, indexed by table 32 region-zero option.
const NATIONAL: [&str; 7] = [
    "£$@←½→↑#—¼‖¾÷",
    "#$§ÄÖÜ^_°äöüß",
    "#¤ÉÄÖÅÜ_éäöåü",
    "£$é°ç→↑#ùàòèì",
    "éïàëêùî#èâôûç",
    "ç$¡áéíóú¿üñèà",
    "#ůčťžýířéáěúš",
];

/// A Level 1 byte, optionally overwritten by an exact Level 1.5 character.
#[derive(Debug, Clone, Copy)]
pub(super) struct Character {
    pub fallback: u8,
    pub enhancement: Option<(u8, u8)>,
}

pub(super) fn encode(character: char, national: u8) -> Option<Character> {
    let subset = NATIONAL.get(usize::from(national))?;
    if let Some(index) = subset.chars().position(|value| value == character) {
        return Some(Character {
            fallback: NATIONAL_CODES[index],
            enhancement: None,
        });
    }
    if character.is_ascii() {
        let code = u8::try_from(u32::from(character)).ok()?;
        if (0x20..=0x7e).contains(&code) && !NATIONAL_CODES.contains(&code) {
            return Some(Character {
                fallback: code,
                enhancement: None,
            });
        }
        if NATIONAL_CODES.contains(&code) && !matches!(code, 0x24 | 0x7c) {
            // X/26 accesses the unmodified G0 positions; @ is its special * code.
            let enhanced = if character == '@' { b'*' } else { code };
            return Some(Character {
                fallback: b' ',
                enhancement: Some((0x10, enhanced)),
            });
        }
    }
    // Canonical decomposition applies to this scalar only. Stored Unicode and
    // column identity are untouched; multi-mark/decomposed input is not folded.
    let mut count = 0;
    let mut components = ['\0'; 2];
    decompose_canonical(character, |component| {
        if count < 2 {
            components[count] = component;
        }
        count += 1;
    });
    if count == 2
        && components[0].is_ascii_alphabetic()
        && compose(components[0], components[1]) == Some(character)
    {
        let mark = match components[1] {
            '\u{300}' => 1,
            '\u{301}' => 2,
            '\u{302}' => 3,
            '\u{303}' => 4,
            '\u{304}' => 5,
            '\u{306}' => 6,
            '\u{307}' => 7,
            '\u{308}' => 8,
            '\u{30a}' => 10,
            '\u{327}' => 11,
            '\u{30b}' => 13,
            '\u{328}' => 14,
            '\u{30c}' => 15,
            _ => return None,
        };
        let base = u8::try_from(u32::from(components[0])).ok()?;
        return Some(Character {
            fallback: base,
            enhancement: Some((0x10 | mark, base)),
        });
    }
    // Fixed G2 symbols/letters. Reserved G2 positions are deliberately absent.
    let groups = [
        (0x20, "\u{a0}¡¢£$¥#§¤‘“«←↑→↓"),
        (0x30, "°±²³×µ¶·÷’”»¼½¾¿"),
        (0x50, "—¹®©™♪₠‰ɑ"),
        (0x5c, "⅛⅜⅝⅞"),
        (0x60, "\u{2126}ÆÐªĦ"),
        (0x66, "ĲĿŁØŒºÞŦŊŉ"),
        (0x70, "ĸæđðħıĳŀłøœßþŧŋ■"),
    ];
    for (start, symbols) in groups {
        if let Some(index) = symbols.chars().position(|value| value == character) {
            let data = start + u8::try_from(index).ok()?;
            return Some(Character {
                fallback: b' ',
                enhancement: Some((0x0f, data)),
            });
        }
    }
    None
}
