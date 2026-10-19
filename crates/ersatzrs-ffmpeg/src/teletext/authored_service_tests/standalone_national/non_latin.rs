//! Assigned Table32 non-Latin header7 is distinct from reserved region-zero7.

#[path = "non_latin/greek_unicode.rs"]
mod greek_unicode;

use super::*;

fn coded(primary: u8) -> TeletextDeclaredService {
    let mut input = draft(primary);
    input.base.pages[0].rows = vec![TeletextRow {
        row: 2,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Mosaics {
                cells: vec![
                    TeletextMosaic::new(0).unwrap(),
                    TeletextMosaic::new(32).unwrap(),
                    TeletextMosaic::new(63).unwrap(),
                ],
                separated: false,
            },
        }],
    }];
    let mut cells = [b' '; 40];
    cells[8..11].copy_from_slice(b"ABC");
    declare(input)
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(0),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap()
}

#[test]
fn assigned_non_latin_header_seven_coded_pages_need_no_optional_banks() {
    let mut cases = Vec::new();
    for (primary, repertoire) in [
        (0x37, TeletextG0Repertoire::Greek),
        (0x47, TeletextG0Repertoire::Arabic),
        (0x57, TeletextG0Repertoire::Arabic),
    ] {
        let declared = coded(primary);
        let service = declared.semantic();
        let page = &service.base().pages[0];
        let layout = layout::compile_for_service_composition(service, page)
            .expect("assigned matching non-Latin context accepts exact coded rows");
        assert_eq!(&layout.rows[1][8..11], b"ABC");
        assert!(layout.base_glyphs().is_empty());
        assert_eq!(&layout.rows[2][7..11], &[0x17, 0x20, 0x60, 0x7f]);
        let composition = compose_teletext_main_program(
            service,
            owner(0),
            TeletextMainCompositionBudget {
                max_input_commands: 64,
                max_output_commands: 64,
                collection: TeletextEnhancementCollectionBudget {
                    max_commands: 128,
                    max_drcs_references: 0,
                },
            },
        )
        .unwrap();
        let raster = render_teletext_composition(
            &composition,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            TeletextRasterBudget {
                max_pixels: 120_000,
                max_diagnostics: 1400,
            },
        )
        .unwrap();
        assert!(raster.diagnostics().is_empty());
        assert_eq!(composition.base_display().primary().get(), primary);
        assert_eq!(composition.base_display().national_option().get(), 7);
        for code in b'A'..=b'C' {
            let index = composition
                .glyph_layers()
                .candidates()
                .iter()
                .position(|candidate| {
                    candidate.base().is_some_and(|base| {
                        base.position()
                            == TeletextEnhancementPosition::new(1, 8 + code - b'A').unwrap()
                    })
                })
                .unwrap();
            let record = composition.glyph_intent().records()[index];
            assert!(
                matches!(record.glyph(), Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.code()==code && glyph.repertoire()==repertoire && glyph.national_subset().is_none())
            );
        }
        let encoded = encode_teletext_authored_service(declared.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &declared);
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(encoded.magazine_units().is_empty());
        assert!(declared.resource_inventory().is_empty());
        let page = &encoded.pages()[0];
        let EncodedTeletextAuthoredPage::NativeDisplay(_) = page else {
            panic!("coded base must remain a native page")
        };
        let mut units = page.data_units().to_vec();
        let row = units.iter().find(|unit| packet(unit) == 1).unwrap();
        assert_eq!(&body(row).map(|code| code & 0x7f)[8..11], b"ABC");
        let mosaics = units.iter().find(|unit| packet(unit) == 2).unwrap();
        assert_eq!(
            &body(mosaics).map(|code| code & 0x7f)[7..11],
            &[0x17, 0x20, 0x60, 0x7f]
        );
        let source_header::TeletextHeaderObservation::Header(header) =
            source_header::observe_dvb_teletext_header(&units[0]).unwrap()
        else {
            panic!("header")
        };
        assert_eq!(header.controls.c12_c14_bits(), 7);
        units.push(super::super::super::packets::terminator(2));
        cases.push(json!({"primary":primary,"page":0x200,"subcode":0,"units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()}));
    }
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_NON_LATIN_BASE_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&json!({"cases":cases})).unwrap()).unwrap();
    }
}
