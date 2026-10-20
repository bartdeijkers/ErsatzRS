//! All four Table 26 enhanced spacing controls through the existing factory.

use super::*;

fn export_case(name: &str, service: &TeletextSemanticService) {
    export_page(name, &native(service, address(0x1234)));
}

fn export_page(name: &str, page: &EncodedTeletextPage) {
    let Some(root) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_SPACING_PROOF_DIR"])
    else {
        return;
    };
    let mut units = page.data_units().to_vec();
    units.push(super::super::packets::terminator(2));
    std::fs::write(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join(root)
            .join(format!("{name}.json")),
        serde_json::to_vec(&serde_json::json!({
            "name":name,"page":0x200,"subcode":0x1234,
            "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        }))
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn level25_spacing_controls_keep_exact_native_compiled_and_preview_rows() {
    for code in [0x00, 0x10, 0x0e, 0x0f] {
        let original = service("ASCII");
        if code == 0x00 {
            export_case("reference-normal", &original);
        }
        let mut cells = original_row(&original, 1);
        cells[7] = code;
        let edited = overlay(original.clone(), address(0x1234), 1, cells);
        let plan = composition(&edited, address(0x1234));
        assert_eq!(base_row(&native(&edited, address(0x1234)), 1), cells);
        assert_eq!(
            base_row(compiled(&edited, address(0x1234)).page(), 1),
            cells
        );
        assert_eq!(plan.base_display().cell(1, 7).unwrap().code(), code);
        let attrs = plan.base_display().cell(1, 8).unwrap().attributes();
        assert_eq!(attrs.double_width(), matches!(code, 0x0e | 0x0f));
        assert_eq!(attrs.double_height(), code == 0x0f);
        if matches!(code, 0x00 | 0x10) {
            assert_eq!(attrs.values().raw_foreground, 0);
            assert_eq!(attrs.graphics(), code == 0x10);
            assert_eq!(attrs.sources().foreground.effective_start_column(), 8);
        } else {
            assert_eq!(attrs.sources().width.effective_start_column(), 8);
            assert_eq!(attrs.sources().height.effective_start_column(), 8);
        }
        assert_eq!(edited.base(), original.base());
        assert_eq!(
            native(&edited, address(0x1235)),
            native(&original, address(0x1235))
        );
        export_case(&format!("control-{code:02x}"), &edited);
    }
}

#[test]
fn physical_black_selects_its_real_glyph_mode_and_clears_conceal_set_after() {
    for (code, glyph) in [(0x00, b'A'), (0x10, 0x66)] {
        let original = service("ASCII");
        let mut cells = [b' '; 40];
        cells[..8].copy_from_slice(&[0x04, 0x1d, 0x07, 0x18, code, glyph, 0x07, b'B']);
        let edited = overlay(original, address(0x1234), 1, cells);
        let plan = composition(&edited, address(0x1234));
        let control = plan.base_display().cell(1, 4).unwrap().attributes();
        let next = plan.base_display().cell(1, 5).unwrap().attributes();
        assert!(control.conceal());
        assert!(!next.conceal());
        assert_eq!(next.values().raw_foreground, 0);
        assert_eq!(next.values().raw_background, 4);
        assert_eq!(next.graphics(), code == 0x10);
        let rendered = plan.display_resolution().cell(1, 5).unwrap().attributes();
        assert_eq!(
            rendered.foreground().value(),
            Some(TeletextColourReference::BLACK)
        );
        assert_eq!(rendered.conceal().value(), Some(false));
        assert_eq!(
            base_row(compiled(&edited, address(0x1234)).page(), 1),
            cells
        );
        export_case(&format!("black-{code:02x}"), &edited);
    }
}

#[test]
fn physical_size_resets_both_axes_and_defers_an_obscured_normal_control() {
    let mut cells = [b' '; 40];
    cells[7..17].copy_from_slice(&[0x0e, b'A', 0x0c, b'B', 0x0f, b'C', b'X', 0x0c, b'D', b'E']);
    let edited = overlay(service("ASCII"), address(0x1234), 1, cells);
    let plan = composition(&edited, address(0x1234));
    for (column, width, height) in [
        (8, true, false),
        (10, false, false),
        (12, true, true),
        (15, false, false),
    ] {
        let attrs = plan
            .display_resolution()
            .cell(1, column)
            .unwrap()
            .attributes();
        assert_eq!(attrs.double_width().value(), Some(width), "column {column}");
        assert_eq!(
            attrs.double_height().value(),
            Some(height),
            "column {column}"
        );
    }
    let source = plan
        .base_display()
        .cell(1, 10)
        .unwrap()
        .attributes()
        .sources();
    assert_eq!(source.width.effective_start_column(), 9);
    assert_eq!(source.height.effective_start_column(), 9);
    assert_eq!(
        plan.base_footprints()
            .row(2)
            .unwrap()
            .suppressed_by()
            .unwrap()
            .column(),
        11
    );
    assert_eq!(base_row(&native(&edited, address(0x1234)), 1), cells);
    assert_eq!(
        base_row(compiled(&edited, address(0x1234)).page(), 1),
        cells
    );
    export_case("size-reset", &edited);
}

#[test]
fn double_size_preserves_visible_basic_fallback_in_the_enhanced_suppressed_row() {
    let original = service("ASCII");
    let mut upper = original_row(&original, 1);
    upper[..3].copy_from_slice(&[0x04, 0x1d, 0x07]);
    upper[7] = 0x0f;
    let mut lower = [b' '; 40];
    lower[..3].copy_from_slice(&[0x02, 0x1d, 0x07]);
    lower[24..32].copy_from_slice(b"FALLBACK");
    let apply = |upper| {
        original
            .clone()
            .with_physical_base_overlays(
                vec![TeletextPhysicalBaseOverlay::new(
                    address(0x1234),
                    vec![
                        TeletextPhysicalBaseRow::new(1, upper).unwrap(),
                        TeletextPhysicalBaseRow::new(2, lower).unwrap(),
                    ],
                )
                .unwrap()],
                service_budget(),
            )
            .unwrap()
    };
    let edited = apply(upper);
    let plan = composition(&edited, address(0x1234));
    let packet = native(&edited, address(0x1234));
    let compiled = compiled(&edited, address(0x1234));
    for (row, expected) in [(1, upper), (2, lower)] {
        assert_eq!(base_row(&packet, row), expected);
        assert_eq!(base_row(compiled.page(), row), expected);
    }
    assert_eq!(plan.base_display().cell(2, 24).unwrap().code(), b'F');
    assert_eq!(
        plan.base_footprints()
            .row(2)
            .unwrap()
            .suppressed_by()
            .unwrap()
            .column(),
        7
    );
    let raster = render_teletext_composition(
        &plan,
        TeletextColourPixelOptions {
            reveal_concealed: true,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextRasterBudget {
            max_pixels: 216_000,
            max_diagnostics: 1800,
        },
    )
    .unwrap();
    for y in 20..30 {
        for x in 24 * 12..32 * 12 {
            assert!(matches!(raster.pixel(x, y).unwrap(),
                TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) if rgb.get()==[0,0,15]));
        }
    }
    export_case("size-fallback", &edited);
    // The existing basic Double Height hidden-content guard is unchanged.
    upper[7] = 0x0d;
    let rejected = apply(upper);
    assert!(matches!(
        super::super::layout::compile_for_service_composition(&rejected, &rejected.base().pages[0]),
        Err(TeletextEncodingError::Structure(
            TeletextStructureError::DoubleHeight { row: 2, .. }
        ))
    ));
}

#[test]
fn applicable_level15_g2_remains_independent_of_physical_double_width() {
    let original = super::held_mosaics::with_main(
        &service("ASCII"),
        vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 16).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 16,
                operation: TeletextColumnOperation::G2 {
                    code: TeletextGlyphCode::new(0x30).unwrap(),
                },
            },
        ],
    );
    let mut cells = original_row(&original, 1);
    cells[7] = 0x0e;
    let edited = overlay(original.clone(), address(0x1234), 1, cells);
    let plan = composition(&edited, address(0x1234));
    assert_eq!(edited.page_enhancements(), original.page_enhancements());
    assert!(plan
        .display_resolution()
        .cell(1, 16)
        .unwrap()
        .attributes()
        .double_width()
        .value()
        .unwrap());
    assert!(matches!(
        encode_teletext_native_default_page(
            &edited,
            address(0x1234),
            TeletextEncodingRegion::WesternLatin
        ),
        Err(TeletextNativeDefaultPageEncodingError {
            reason: TeletextNativeDefaultPageEncodingFailure::LocalProgramPresent,
            ..
        })
    ));
    let encoded = compiled(&edited, address(0x1234));
    assert_eq!(base_row(encoded.page(), 1), cells);
    export_page("width-with-level15-g2", encoded.page());
}
