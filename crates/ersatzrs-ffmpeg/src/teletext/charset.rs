//! EN 300 706 G0/G2 Unicode conversions and X/26 column modes from table 29.

use unicode_normalization::char::{compose, decompose_canonical};

use ersatzrs_domain::{TeletextCharsetPair, TeletextPageCharsetDesignation};

const NATIONAL_CODES: [u8; 13] = [
    0x23, 0x24, 0x40, 0x5b, 0x5c, 0x5d, 0x5e, 0x5f, 0x60, 0x7b, 0x7c, 0x7d, 0x7e,
];
// Table36 scalar positions; first seven retain the region-zero index. NUL is
// an explicitly unqualified Unicode alias, never an encodable character. Exact
// coded rows can still carry those original glyphs without inventing an alias.
const NATIONAL: [&str; 13] = [
    "£$@←½→↑#—¼‖¾÷",
    "#$§ÄÖÜ^_°äöüß",
    "#¤ÉÄÖÅÜ_éäöåü",
    "£$é°ç→↑#ùàòèì",
    "éïàëêùî#èâôûç",
    "ç$¡áéíóú¿üñèà",
    "#ůčťžýířéáěúš",
    "#ńą\0ŚŁćóężśłź",
    "\0ğİŞÖÇÜĞışöçü",
    "#ËČĆŽ\0Šëčćž\0š",
    "#\0ŢÂŞ\0\0ıţâş\0î",
    "#õŠÄÖŽÜÕšäöžü",
    "#$Šė\0Žčūšąųžį",
];

/// Table32 aliases select the actual Table36 subset, never only header bits.
pub(super) const fn unicode_national_subset(primary: TeletextPageCharsetDesignation) -> Option<u8> {
    Some(match primary.get() {
        0x00 | 0x10 | 0x40 => 0,
        0x01 | 0x09 | 0x11 | 0x21 => 1,
        0x02 | 0x0a | 0x12 => 2,
        0x03 | 0x0b | 0x13 => 3,
        0x04 | 0x0c | 0x14 | 0x44 => 4,
        0x05 | 0x15 => 5,
        0x06 | 0x0e | 0x26 => 6,
        0x08 => 7,
        0x16 | 0x36 => 8,
        0x1d => 9,
        0x1f => 10,
        0x22 => 11,
        0x23 => 12,
        _ => return None,
    })
}

/// A Level 1 byte, optionally overwritten by an exact Level 1.5 character.
#[derive(Debug, Clone, Copy)]
pub(super) struct Character {
    pub fallback: u8,
    pub enhancement: Option<(u8, u8)>,
}

pub(super) fn encode(character: char, national: u8) -> Option<Character> {
    let subset = NATIONAL.get(usize::from(national))?;
    if let Some(index) = subset
        .chars()
        .position(|value| value != '\0' && value == character)
    {
        return Some(Character {
            fallback: NATIONAL_CODES[index],
            enhancement: None,
        });
    }
    encode_without_national_subset(character)
}

/// Latin invariants and explicit X26 replacements without borrowing a national
/// table. Exact regional national cells are supplied by checked physical rows.
pub(super) fn encode_without_national_subset(character: char) -> Option<Character> {
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

/// Assigned Greek G0 Unicode positions from EN300706 Table42. This preserves
/// literal scalars; it neither composes strings nor borrows a Latin namespace.
/// The numeral-sign alias and rectangle remain raw-coded, not guessed Unicode.
pub(super) fn encode_greek_g0(character: char) -> Option<Character> {
    let scalar = u32::from(character);
    let code = match character {
        ' '..=';' | '=' | '?' => scalar,
        '«' => 0x3c,
        '»' => 0x3e,
        'ΐ' => 0x40,
        'Α'..='Ρ' => 0x41 + scalar - u32::from('Α'),
        'Σ'..='Ω' => 0x53 + scalar - u32::from('Σ'),
        'Ϊ'..='Ϋ' => 0x5a + scalar - u32::from('Ϊ'),
        'ά'..='ί' => 0x5c + scalar - u32::from('ά'),
        'ΰ' => 0x60,
        'α'..='ω' => 0x61 + scalar - u32::from('α'),
        'ϊ'..='ώ' => 0x7a + scalar - u32::from('ϊ'),
        _ => return None,
    };
    Some(Character {
        fallback: u8::try_from(code).ok()?,
        enhancement: None,
    })
}

/// Native G0 scalars from Tables38–40/46, in authored cell order. No bidi,
/// normalization, approximate aliases or Latin G2 replacements are implied.
pub(super) fn encode_cyrillic_hebrew_g0(
    character: char,
    pair: TeletextCharsetPair,
) -> Option<Character> {
    let alphabet = match pair {
        TeletextCharsetPair::Cyrillic1Cyrillic => {
            "ЧАБЦДЕФГХИЈКЛМНОПЌРСТУВЃЉЊЗЋЖЂШЏчабцдефгхијклмнопќрстувѓљњзћжђш"
        }
        TeletextCharsetPair::Cyrillic2Cyrillic => {
            "ЮАБЦДЕФГХИЙКЛМНОПЯРСТУЖВЬЪЗШЭЩЧЫюабцдефгхийклмнопярстужвьъзшэщч"
        }
        TeletextCharsetPair::Cyrillic3Cyrillic => {
            "ЮАБЦДЕФГХИЙКЛМНОПЯРСТУЖВЬІЗШЄЩЧЇюабцдефгхийклмнопярстужвьізшєщч"
        }
        TeletextCharsetPair::HebrewArabic => {
            return encode_hebrew_g0(character);
        }
        _ => return None,
    };
    let code = if character == 'ы' && pair == TeletextCharsetPair::Cyrillic2Cyrillic
        || character == 'ї' && pair == TeletextCharsetPair::Cyrillic3Cyrillic
    {
        0x26
    } else if (' '..='?').contains(&character)
        && (character != '&' || pair == TeletextCharsetPair::Cyrillic1Cyrillic)
    {
        u8::try_from(u32::from(character)).ok()?
    } else {
        0x40 + u8::try_from(alphabet.chars().position(|value| value == character)?).ok()?
    };
    Some(Character {
        fallback: code,
        enhancement: None,
    })
}

fn encode_hebrew_g0(character: char) -> Option<Character> {
    let code = match character {
        '£' => 0x23,
        '#' => 0x5f,
        ' '..='Z' => u8::try_from(u32::from(character)).ok()?,
        '←' => 0x5b,
        '½' => 0x5c,
        '→' => 0x5d,
        '↑' => 0x5e,
        'א'..='ת' => 0x60 + u8::try_from(u32::from(character) - u32::from('א')).ok()?,
        '₪' => 0x7b,
        '‖' => 0x7c,
        '¾' => 0x7d,
        '÷' => 0x7e,
        _ => return None,
    };
    Some(Character {
        fallback: code,
        enhancement: None,
    })
}
