//! Fixed AB2 assignments retain their native namespace and original scalar.
use super::unicode_eligibility::{check_preview, text};
use super::*;

const CASES: [(u8, char, u8, Option<u8>); 12] = [
    // Keep the established Latin G2 spelling, rather than changing its wire.
    (0, '■', 0x20, Some(0x7f)),
    (0x20, '■', 0x7f, None),
    (0x24, '■', 0x7f, None),
    (0x25, '■', 0x7f, None),
    (0x37, '■', 0x7f, None),
    (0x55, '■', 0x7f, None),
    (0x47, '■', 0x7f, None),
    (0x57, '■', 0x7f, None),
    (0x37, '\u{0374}', 0x52, None),
    (0x20, 'α', 0x20, Some(0x58)),
    (0x24, 'α', 0x20, Some(0x58)),
    (0x25, 'α', 0x20, Some(0x58)),
];

#[test]
fn fixed_supplementary_aliases_keep_native_context_fallback_and_original_text() {
    let mut cases = Vec::new();
    for (primary, scalar, fallback, g2) in CASES {
        let source = text(primary, &scalar.to_string());
        let service = source.semantic();
        let layout =
            layout::compile_for_service_composition(service, &service.base().pages[0]).unwrap();
        assert_eq!(layout.rows[1][8], fallback, "{primary:02x}/{scalar}");
        assert_eq!(layout.base_glyphs().len(), usize::from(g2.is_some()));
        check_preview(&source);
        let encoded = encode_teletext_authored_service(source.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &source);
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(encoded.magazine_units().is_empty());
        assert!(source.resource_inventory().is_empty());
        if let Some(code) = g2 {
            let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
                panic!("a supplementary glyph needs its original X26 operation")
            };
            let original = page.normalized().original();
            assert_eq!(
                original.base_replacement(1, 8),
                Some(TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(code).unwrap(),
                })
            );
            assert_eq!(
                original.base_replacement_charset(1, 8).unwrap().get(),
                primary
            );
            assert_eq!(
                original.base_display().cells()[1][8]
                    .authored()
                    .unwrap()
                    .content(),
                TeletextBaseAuthoredContent::Text { character: scalar }
            );
        } else {
            assert!(matches!(
                &encoded.pages()[0],
                EncodedTeletextAuthoredPage::NativeDisplay(_)
            ));
            assert!(!encoded.pages()[0]
                .data_units()
                .iter()
                .any(|unit| packet(unit) == 26));
        }
        let mut units = encoded.pages()[0].data_units().to_vec();
        let row = units.iter().find(|unit| packet(unit) == 1).unwrap();
        assert_eq!(body(row)[8] & 0x7f, fallback);
        units.push(crate::teletext::packets::terminator(2));
        cases.push(
            json!({"name":"fixed_ab2_alias", "primary":primary,"page":0x200,"subcode":0,
            "points":[{"row":1,"column":8,"unicode":u32::from(scalar),
                "fallback":if g2.is_some() {32} else {u32::from(scalar)}}],
            "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>() }),
        );
    }
    if let Some(path) = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_AB2_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&json!({"cases":cases})).unwrap()).unwrap();
    }
}

#[test]
fn fixed_supplementary_aliases_do_not_conflate_scripts_unmarked_slots_or_normalize_input() {
    use crate::teletext::charset;
    assert_eq!(
        charset::decode_base_text_spelling(0x0f, 0x58, TeletextCharsetPair::LatinLatin),
        Some('ɑ')
    );
    for pair in [
        TeletextCharsetPair::Cyrillic1Cyrillic,
        TeletextCharsetPair::Cyrillic2Cyrillic,
        TeletextCharsetPair::Cyrillic3Cyrillic,
        TeletextCharsetPair::GreekGreek,
    ] {
        assert_eq!(
            charset::decode_base_text_spelling(0x0f, 0x58, pair),
            Some('α')
        );
        assert_eq!(
            charset::decode_base_text_spelling(0x0f, 0x7f, pair),
            Some(if pair == TeletextCharsetPair::GreekGreek {
                '■'
            } else {
                'z'
            })
        );
        assert_eq!(charset::decode_base_text_spelling(0x0f, 0x40, pair), None);
    }
    for pair in [
        TeletextCharsetPair::LatinLatin,
        TeletextCharsetPair::Cyrillic1Cyrillic,
        TeletextCharsetPair::Cyrillic2Cyrillic,
        TeletextCharsetPair::Cyrillic3Cyrillic,
        TeletextCharsetPair::GreekGreek,
        TeletextCharsetPair::HebrewArabic,
        TeletextCharsetPair::ArabicArabic,
    ] {
        assert_eq!(
            charset::decode_base_text_spelling(0x10, 0x7f, pair),
            Some('■')
        );
    }
    assert_eq!(
        charset::decode_base_text_spelling(0x0f, 0x7f, TeletextCharsetPair::ArabicArabic),
        None
    );
    assert!(charset::encode_native_g2('■', TeletextCharsetPair::Cyrillic1Cyrillic).is_none());
    let alpha = charset::encode_greek_g0('α').unwrap();
    assert_eq!((alpha.fallback, alpha.enhancement), (0x61, None));
    for (primary, scalar) in [
        (0, 'α'),
        (0x20, 'ɑ'),
        (0x37, '\u{02b9}'),
        (0x37, '▬'),
        (0x37, '█'),
        (0x47, '█'),
    ] {
        let source = text(primary, &scalar.to_string());
        let before = source.clone();
        let failure = encode_teletext_authored_service(source.clone(), &[], &limits()).unwrap_err();
        assert!(matches!(&failure.reason,Failure::Layout(error)
            if matches!(error.as_ref(),TeletextEncodingError::Character {address,row:1,column:8,codepoint}
                if *address==owner(0) && *codepoint==u32::from(scalar))));
        assert_eq!(source, before);
    }
}
