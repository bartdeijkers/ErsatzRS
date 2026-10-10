//! Literal standard-code fixtures, not rendering or whole-page acceptance.

use std::collections::BTreeMap;

use serde_json::json;

use super::{
    TeletextCharsetPair, TeletextCharsetValueError, TeletextDiacriticIndex,
    TeletextEnhancementCharsetDesignation, TeletextPageCharsetDesignation,
};

// EN 300 706 tables 32/33: all entries with an assigned repertoire/national option.
const PAGE_CODES: [u8; 36] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0e, 0x10, 0x11, 0x12,
    0x13, 0x14, 0x15, 0x16, 0x1d, 0x1f, 0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x36, 0x37, 0x40,
    0x44, 0x47, 0x55, 0x57,
];

// EN 300 706 table 34: X26 includes ten additional Latin/Latin aliases.
const ENHANCEMENT_CODES: [u8; 46] = [
    0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f,
    0x10, 0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1d, 0x1e, 0x1f,
    0x20, 0x21, 0x22, 0x23, 0x24, 0x25, 0x26, 0x36, 0x37, 0x40, 0x44, 0x47, 0x55, 0x57,
];

#[test]
fn page_designation_checks_all_bytes_and_preserves_assigned_aliases() {
    for code in 0..=u8::MAX {
        let result = TeletextPageCharsetDesignation::new(code);
        let parsed = serde_json::from_value::<TeletextPageCharsetDesignation>(json!(code));
        if PAGE_CODES.contains(&code) {
            let designation = result.unwrap();
            assert_eq!(designation.get(), code);
            assert_eq!(u8::from(designation), code);
            assert_eq!(
                TeletextPageCharsetDesignation::try_from(code).unwrap(),
                designation
            );
            assert_eq!(parsed.unwrap(), designation);
            assert_eq!(serde_json::to_value(designation).unwrap(), json!(code));
        } else {
            assert_eq!(
                result,
                Err(TeletextCharsetValueError::PageDesignation { code })
            );
            assert!(parsed.is_err(), "{code:02X}");
        }
    }
}

#[test]
fn enhancement_designation_checks_all_bytes_and_preserves_assigned_aliases() {
    for code in 0..=u8::MAX {
        let result = TeletextEnhancementCharsetDesignation::new(code);
        let parsed = serde_json::from_value::<TeletextEnhancementCharsetDesignation>(json!(code));
        if ENHANCEMENT_CODES.contains(&code) {
            let designation = result.unwrap();
            assert_eq!(designation.get(), code);
            assert_eq!(u8::from(designation), code);
            assert_eq!(
                TeletextEnhancementCharsetDesignation::try_from(code).unwrap(),
                designation
            );
            assert_eq!(parsed.unwrap(), designation);
            assert_eq!(serde_json::to_value(designation).unwrap(), json!(code));
        } else {
            assert_eq!(
                result,
                Err(TeletextCharsetValueError::EnhancementDesignation { code })
            );
            assert!(parsed.is_err(), "{code:02X}");
        }
    }
}

#[test]
fn page_and_enhancement_latin_aliases_have_different_assigned_code_sets() {
    for code in [0x07, 0x0d, 0x0f, 0x17, 0x18, 0x19, 0x1a, 0x1b, 0x1c, 0x1e] {
        assert!(TeletextPageCharsetDesignation::new(code).is_err());
        let enhancement = TeletextEnhancementCharsetDesignation::new(code).unwrap();
        assert_eq!(enhancement.pair(), TeletextCharsetPair::LatinLatin);
        assert_eq!(enhancement.get(), code);
    }
    let english = TeletextPageCharsetDesignation::new(0x00).unwrap();
    let english_alias = TeletextPageCharsetDesignation::new(0x10).unwrap();
    assert_ne!(english, english_alias);
    assert_eq!(english.pair(), english_alias.pair());
}

#[test]
fn repertoire_pair_mapping_preserves_all_non_latin_and_mixed_designations() {
    let mappings = [
        (0x20, TeletextCharsetPair::Cyrillic1Cyrillic),
        (0x24, TeletextCharsetPair::Cyrillic2Cyrillic),
        (0x25, TeletextCharsetPair::Cyrillic3Cyrillic),
        (0x37, TeletextCharsetPair::GreekGreek),
        (0x40, TeletextCharsetPair::LatinArabic),
        (0x44, TeletextCharsetPair::LatinArabic),
        (0x47, TeletextCharsetPair::ArabicArabic),
        (0x55, TeletextCharsetPair::HebrewArabic),
        (0x57, TeletextCharsetPair::ArabicArabic),
    ];
    for (code, pair) in mappings {
        assert_eq!(
            TeletextPageCharsetDesignation::new(code).unwrap().pair(),
            pair
        );
        assert_eq!(
            TeletextEnhancementCharsetDesignation::new(code)
                .unwrap()
                .pair(),
            pair
        );
    }
    for code in ENHANCEMENT_CODES {
        if !mappings.iter().any(|(mapped, _)| *mapped == code) {
            assert_eq!(
                TeletextEnhancementCharsetDesignation::new(code)
                    .unwrap()
                    .pair(),
                TeletextCharsetPair::LatinLatin
            );
            if let Ok(page) = TeletextPageCharsetDesignation::new(code) {
                assert_eq!(page.pair(), TeletextCharsetPair::LatinLatin);
            }
        }
    }
}

#[test]
fn aliases_classify_into_eight_pairs_without_consuming_extra_repertoire_pairs() {
    let mut page_counts = BTreeMap::new();
    let mut enhancement_counts = BTreeMap::new();
    for code in PAGE_CODES {
        *page_counts
            .entry(TeletextPageCharsetDesignation::new(code).unwrap().pair())
            .or_insert(0) += 1;
    }
    for code in ENHANCEMENT_CODES {
        *enhancement_counts
            .entry(
                TeletextEnhancementCharsetDesignation::new(code)
                    .unwrap()
                    .pair(),
            )
            .or_insert(0) += 1;
    }
    let expected_other = [
        (TeletextCharsetPair::Cyrillic1Cyrillic, 1),
        (TeletextCharsetPair::Cyrillic2Cyrillic, 1),
        (TeletextCharsetPair::Cyrillic3Cyrillic, 1),
        (TeletextCharsetPair::GreekGreek, 1),
        (TeletextCharsetPair::LatinArabic, 2),
        (TeletextCharsetPair::ArabicArabic, 2),
        (TeletextCharsetPair::HebrewArabic, 1),
    ];
    let mut expected_page = BTreeMap::from(expected_other);
    expected_page.insert(TeletextCharsetPair::LatinLatin, 27);
    let mut expected_enhancement = BTreeMap::from(expected_other);
    expected_enhancement.insert(TeletextCharsetPair::LatinLatin, 37);
    assert_eq!(page_counts, expected_page);
    assert_eq!(enhancement_counts, expected_enhancement);
}

#[test]
fn secondary_sentinel_is_optional_only_and_other_codes_remain_checked() {
    for code in 0..=u8::MAX {
        let result = TeletextPageCharsetDesignation::secondary_from_code(code);
        if code == 0x7f {
            assert_eq!(result, Ok(None));
            assert!(TeletextPageCharsetDesignation::new(code).is_err());
            assert!(TeletextEnhancementCharsetDesignation::new(code).is_err());
        } else if PAGE_CODES.contains(&code) {
            assert_eq!(result.unwrap().unwrap().get(), code);
        } else {
            assert_eq!(
                result,
                Err(TeletextCharsetValueError::PageDesignation { code })
            );
        }
    }
}

#[test]
fn diacritic_indices_preserve_all_four_bit_values_and_reject_every_high_byte() {
    for index in 0..=u8::MAX {
        let result = TeletextDiacriticIndex::new(index);
        let parsed = serde_json::from_value::<TeletextDiacriticIndex>(json!(index));
        if index <= 15 {
            let diacritic = result.unwrap();
            assert_eq!(diacritic.get(), index);
            assert_eq!(u8::from(diacritic), index);
            assert_eq!(TeletextDiacriticIndex::try_from(index).unwrap(), diacritic);
            assert_eq!(parsed.unwrap(), diacritic);
            assert_eq!(serde_json::to_value(diacritic).unwrap(), json!(index));
        } else {
            assert_eq!(
                result,
                Err(TeletextCharsetValueError::DiacriticIndex { index })
            );
            assert!(parsed.is_err(), "{index}");
        }
    }
}

#[test]
fn scalar_json_requires_unsigned_integer_values_without_constructor_bypass() {
    for raw in [
        "-1",
        "256",
        "1.5",
        "1.0",
        "true",
        "null",
        "\"1\"",
        "[]",
        "[1]",
        "{}",
        "{\"value\":1}",
    ] {
        assert!(
            serde_json::from_str::<TeletextPageCharsetDesignation>(raw).is_err(),
            "{raw}"
        );
        assert!(
            serde_json::from_str::<TeletextEnhancementCharsetDesignation>(raw).is_err(),
            "{raw}"
        );
        assert!(
            serde_json::from_str::<TeletextDiacriticIndex>(raw).is_err(),
            "{raw}"
        );
    }
    assert_eq!(
        serde_json::to_string(&TeletextPageCharsetDesignation::new(0x10).unwrap()).unwrap(),
        "16"
    );
    assert_eq!(
        serde_json::to_string(&TeletextEnhancementCharsetDesignation::new(0x55).unwrap()).unwrap(),
        "85"
    );
    assert_eq!(
        serde_json::to_string(&TeletextDiacriticIndex::new(12).unwrap()).unwrap(),
        "12"
    );
}

#[test]
fn value_errors_identify_designation_context_and_offending_code() {
    assert_eq!(
        TeletextPageCharsetDesignation::new(0x07)
            .unwrap_err()
            .to_string(),
        "teletext page charset designation 07 hexadecimal is reserved"
    );
    assert_eq!(
        TeletextEnhancementCharsetDesignation::new(0x7f)
            .unwrap_err()
            .to_string(),
        "teletext enhancement charset designation 7F hexadecimal is reserved"
    );
    assert_eq!(
        TeletextDiacriticIndex::new(16).unwrap_err().to_string(),
        "teletext diacritic index 16 must be between zero and 15"
    );
}
