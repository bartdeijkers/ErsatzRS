//! Assigned Table32 non-Latin header7 is distinct from reserved region-zero7.

#[path = "non_latin/greek_unicode.rs"]
mod greek_unicode;

#[path = "non_latin/cyrillic_hebrew.rs"]
mod cyrillic_hebrew;

#[path = "non_latin/native_g2.rs"]
mod native_g2;

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

#[test]
fn complete_arabic_g0_coded_tables_keep_assignment_pixels_and_wire() {
    let mut cases = Vec::new();
    for primary in [0x47, 0x57] {
        let codes: Vec<u8> = (0x20..=0x7f).collect();
        let mut input = draft(primary);
        input.base.pages[0].rows.clear();
        let declared = declare(input)
            .with_physical_base_overlays(
                vec![TeletextPhysicalBaseOverlay::new(
                    owner(0),
                    codes
                        .chunks(32)
                        .enumerate()
                        .map(|(index, chunk)| {
                            let mut cells = [b' '; 40];
                            cells[1..33].copy_from_slice(chunk);
                            TeletextPhysicalBaseRow::new(u8::try_from(index + 1).unwrap(), cells)
                                .unwrap()
                        })
                        .collect(),
                )
                .unwrap()],
                semantic_limits(),
            )
            .unwrap();
        // No logical Unicode alias, optional bank or higher-level resource is
        // involved: Table44 assigns every coded G0 slot, including7/E.
        let composition = greek_unicode::preview(&declared);
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
        for (index, code) in codes.iter().enumerate() {
            let row = u8::try_from(index / 32 + 1).unwrap();
            let column = u8::try_from(index % 32 + 1).unwrap();
            assert_eq!(
                composition.base_display().cells()[usize::from(row)][usize::from(column)].code(),
                *code
            );
            if *code == 0x20 {
                continue; // SPACE is classified as blank rather than a glyph.
            }
            let candidate = composition
                .glyph_layers()
                .candidates()
                .iter()
                .position(|candidate| {
                    candidate.base().is_some_and(|cell| {
                        cell.position() == TeletextEnhancementPosition::new(row, column).unwrap()
                    })
                })
                .unwrap();
            let intent = composition.glyph_intent().records()[candidate];
            assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
            assert_eq!(intent.reason(), None);
            assert!(
                matches!(intent.glyph(), Some(TeletextGlyphIdentity::G0 { glyph })
                if glyph.repertoire() == TeletextG0Repertoire::Arabic
                && glyph.code() == *code && glyph.national_subset().is_none())
            );
        }
        assert_arabic_7e_pixels(&raster);
        let encoded = encode_teletext_authored_service(declared.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &declared);
        assert!(declared.resource_inventory().is_empty());
        assert!(encoded.compiler_drcs_resources().is_empty());
        assert!(encoded.magazine_units().is_empty());
        assert!(matches!(
            encoded.pages()[0],
            EncodedTeletextAuthoredPage::NativeDisplay(_)
        ));
        let mut units = encoded.pages()[0].data_units().to_vec();
        for (index, chunk) in codes.chunks(32).enumerate() {
            let row = units
                .iter()
                .find(|unit| usize::from(packet(unit)) == index + 1)
                .unwrap();
            assert_eq!(&body(row).map(|code| code & 0x7f)[1..33], chunk);
        }
        units.push(super::super::super::packets::terminator(2));
        cases.push(json!({"primary":primary,"page":0x200,"subcode":0,
            "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()}));
    }
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_ARABIC_G0_FIXTURE"])
    {
        std::fs::write(path, serde_json::to_vec(&json!({"cases":cases})).unwrap()).unwrap();
    }
}

fn assert_arabic_7e_pixels(raster: &TeletextRaster) {
    let expected_rows = [0_u16, 390, 198, 102, 54, 28, 102, 254, 0, 0];
    for (y, mask) in expected_rows.iter().enumerate() {
        for x in 0..12 {
            let Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))) =
                raster.pixel(31 * 12 + x, 3 * 10 + y)
            else {
                panic!("Arabic7/E must resolve to colour pixels")
            };
            assert_eq!(
                rgb.get(),
                if mask & (1 << (11 - x)) == 0 {
                    [0; 3]
                } else {
                    [15; 3]
                }
            );
        }
    }
}
