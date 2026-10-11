//! Literal Table 21/Annex D fallback values; no packet presence is invented.
use super::*;

#[test]
fn all_sixteen_pop_flags_have_exact_panels_and_black_substitution() {
    let expected = [
        (0, 0, false),
        (0, 0, false),
        (16, 0, false),
        (0, 0, false),
        (0, 16, false),
        (0, 0, false),
        (8, 8, false),
        (0, 0, false),
        (0, 0, true),
        (0, 0, false),
        (16, 0, true),
        (0, 0, false),
        (0, 16, true),
        (0, 0, false),
        (8, 8, true),
        (0, 0, false),
    ];
    for (raw, (left, right, substitution)) in (0..16).zip(expected) {
        let result = resolve_teletext_mot_presentation_fallback(
            TeletextNationalOption::new(1).unwrap(),
            TeletextMotFallback::new(raw).unwrap(),
        )
        .unwrap();
        assert_eq!(
            result.panels.level25_widths(),
            (left, right),
            "flags {raw:X}"
        );
        assert_eq!(
            result.black_background_substitution, substitution,
            "flags {raw:X}"
        );
        assert_eq!(result.primary.get(), 1);
        assert_eq!(result.secondary, None);
        assert_eq!(result.palette, TeletextLevel25Palette::standard());
        assert_eq!(result.screen_colour, TeletextColourReference::BLACK);
        assert_eq!(result.row_colour, TeletextColourReference::BLACK);
        assert_eq!(result.remapping, TeletextColourRemapping::FgClut0BgClut0);
    }
}

#[test]
fn fallback_retains_header_national_charset_and_rejects_reserved_seven() {
    for national in 0..7 {
        let result = resolve_teletext_mot_presentation_fallback(
            TeletextNationalOption::new(national).unwrap(),
            TeletextMotFallback::new(14).unwrap(),
        )
        .unwrap();
        assert_eq!(result.primary.get(), national);
        let effective = TeletextEffectivePresentation::MotFallback {
            national_option: TeletextNationalOption::new(national).unwrap(),
            defaults: &result,
        };
        assert_eq!(effective.primary_designation().unwrap().get(), national);
    }
    assert!(resolve_teletext_mot_presentation_fallback(
        TeletextNationalOption::new(7).unwrap(),
        TeletextMotFallback::new(14).unwrap(),
    )
    .is_err());
}
