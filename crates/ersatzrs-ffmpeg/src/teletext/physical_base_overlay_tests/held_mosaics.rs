//! EN 300 706 Table 26 and the fixed Annex G.3.3 example; no optional resources.

use super::*;

fn pixels(plan: &TeletextMainComposition, column: u8) -> Vec<u16> {
    let raster = render_teletext_composition(
        plan,
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
    assert!(raster.diagnostics().is_empty());
    (0..10)
        .map(|y| {
            (0..12).fold(0, |bits, x| {
                let value = raster.pixel(usize::from(column) * 12 + x, 10 + y).unwrap();
                let on = match value {
                    TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) => match rgb.get() {
                        [15, 15, 15] => true,
                        [0, 0, 0] => false,
                        other => panic!("unexpected colour {other:?}"),
                    },
                    other => panic!("unresolved pixel {other:?}"),
                };
                (bits << 1) | u16::from(on)
            })
        })
        .collect()
}

#[test]
fn annex_g33_keeps_held_shapes_and_exact_original_control_bytes() {
    let original = service("ASCII");
    let mut cells = [b' '; 40];
    cells[8..15].copy_from_slice(&[0x17, 0x66, 0x1e, 0x39, 0x1a, 0x1f, 0x66]);
    let edited = overlay(original.clone(), address(0x1234), 1, cells);
    let plan = composition(&edited, address(0x1234));
    assert_eq!(base_row(&native(&edited, address(0x1234)), 1), cells);
    assert_eq!(
        base_row(compiled(&edited, address(0x1234)).page(), 1),
        cells
    );
    let contiguous66 = [
        0x03f, 0x03f, 0x03f, 0xfc0, 0xfc0, 0xfc0, 0xfc0, 0x03f, 0x03f, 0x03f,
    ];
    let contiguous39 = [
        0xfc0, 0xfc0, 0xfc0, 0x03f, 0x03f, 0x03f, 0x03f, 0xfc0, 0xfc0, 0xfc0,
    ];
    let separated66 = [0, 0x01e, 0x01e, 0, 0x780, 0x780, 0x780, 0, 0x01e, 0x01e];
    assert_eq!(pixels(&plan, 9), contiguous66);
    assert_eq!(pixels(&plan, 10), contiguous66, "Hold is SetAt");
    assert_eq!(pixels(&plan, 11), contiguous39);
    assert_eq!(
        pixels(&plan, 12),
        contiguous39,
        "Held form survives separation control"
    );
    assert_eq!(pixels(&plan, 13), contiguous39, "Release is SetAfter");
    assert_eq!(pixels(&plan, 14), separated66);
    assert_eq!(
        pixels(&plan, 15),
        [0; 10],
        "Literal SPACE is not substituted"
    );
    if let Some(root) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_HELD_PROOF_DIR"])
    {
        let mut units = native(&edited, address(0x1234)).data_units().to_vec();
        units.push(super::super::packets::terminator(2));
        std::fs::write(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../..")
                .join(root)
                .join("annex-g33.json"),
            serde_json::to_vec(&serde_json::json!({
                "page":0x200,"subcode":0x1234,
                "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
                "points":[
                    {"row":1,"column":9,"mask":contiguous66},
                    {"row":1,"column":10,"mask":contiguous66},
                    {"row":1,"column":11,"mask":contiguous39},
                    {"row":1,"column":12,"mask":contiguous39},
                    {"row":1,"column":13,"mask":contiguous39},
                    {"row":1,"column":14,"mask":separated66},
                    {"row":1,"column":15,"mask": ([0_u16;10])},
                ],
            }))
            .unwrap(),
        )
        .unwrap();
    }
    assert_eq!(edited.base(), original.base());
    assert_eq!(
        native(&edited, address(0x1235)),
        native(&original, address(0x1235))
    );
}

fn intent_at(plan: &TeletextMainComposition, row: u8, column: u8) -> &TeletextExecutedGlyphIntent {
    plan.glyph_intent()
        .records()
        .iter()
        .find(|intent| {
            intent.context().candidate().base().is_some_and(|base| {
                base.position() == TeletextEnhancementPosition::new(row, column).unwrap()
            })
        })
        .unwrap()
}

#[test]
fn hold_register_obeys_real_mode_size_and_row_changes_not_reinforcement_or_alphabetics() {
    let mut cells = [b' '; 40];
    cells[8..40].copy_from_slice(&[
        0x17, 0x7f, 0x1e, 0x1a, 0x41, 0x09, 0x19, 0x20, 0x09, 0x7f, 0x01, 0x17, 0x1e, 0x7f, 0x17,
        0x09, 0x0d, 0x1e, 0x7f, 0x0d, 0x09, 0x0c, 0x1e, 0x7f, 0x1f, 0x1e, 0x1f, 0x09, 0x17, 0x1e,
        0x19, 0x1a,
    ]);
    let mut next = [b' '; 40];
    next[8..11].copy_from_slice(&[0x17, 0x1e, 0x09]);
    let edited = service("ASCII")
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                address(0x1234),
                vec![
                    TeletextPhysicalBaseRow::new(1, cells).unwrap(),
                    TeletextPhysicalBaseRow::new(3, next).unwrap(),
                ],
            )
            .unwrap()],
            service_budget(),
        )
        .unwrap();
    let plan = composition(&edited, address(0x1234));
    for column in [
        10, 11, 13, 14, 18, 22, 23, 24, 27, 28, 32, 33, 34, 37, 38, 39,
    ] {
        let held = intent_at(&plan, 1, column).held_mosaic().unwrap();
        assert_eq!(held.mosaic().blocks(), 63, "column{column}");
        assert!(
            !held.separated(),
            "register retains original contiguous form at{column}"
        );
        assert!(matches!(
            intent_at(&plan, 1, column)
                .context()
                .candidate()
                .base()
                .unwrap()
                .kind(),
            TeletextBaseDisplayCellKind::SpacingControl { .. }
        ));
    }
    for column in [16, 20, 25, 29, 30] {
        assert_eq!(
            intent_at(&plan, 1, column)
                .held_mosaic()
                .unwrap()
                .mosaic()
                .blocks(),
            0,
            "column{column}"
        );
    }
    for column in [12, 15, 17, 19, 21, 26, 31, 35, 36] {
        assert_eq!(
            intent_at(&plan, 1, column).held_mosaic(),
            None,
            "column{column}"
        );
    }
    let after_space = intent_at(&plan, 1, 16).held_mosaic().unwrap();
    assert_eq!(
        after_space.source(),
        TeletextHeldMosaicSource::Mosaic {
            position: TeletextEnhancementPosition::new(1, 15).unwrap(),
        }
    );
    let after_size = intent_at(&plan, 1, 25).held_mosaic().unwrap();
    assert!(
        matches!(after_size.source(),TeletextHeldMosaicSource::SpacingReset {
        source:TeletextBaseAttributeSource::Spacing { position, control:TeletextBaseSpacingControl::DoubleHeight,.. }
    } if position == TeletextEnhancementPosition::new(1,24).unwrap())
    );
    let reenabling = intent_at(&plan, 1, 33).held_mosaic().unwrap();
    assert_eq!(
        reenabling.hold(),
        TeletextEnhancementPosition::new(1, 33).unwrap()
    );
    assert_eq!(
        reenabling.source(),
        TeletextHeldMosaicSource::Mosaic {
            position: TeletextEnhancementPosition::new(1, 31).unwrap(),
        }
    );
    assert_eq!(
        intent_at(&plan, 3, 9)
            .held_mosaic()
            .unwrap()
            .mosaic()
            .blocks(),
        0
    );
    assert_eq!(base_row(&native(&edited, address(0x1234)), 1), cells);
    assert_eq!(base_row(compiled(&edited, address(0x1234)).page(), 3), next);
}

fn with_main(
    original: &TeletextSemanticService,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextSemanticService {
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: original.base().clone(),
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: address(0x1234),
                presentation: None,
                compositional_links: None,
                local_program: Some(TeletextEnhancementProgram::new(commands).unwrap()),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [None; 4],
                },
            }],
            magazines: vec![],
            objects: vec![],
            drcs: vec![],
        },
        service_budget(),
    )
    .unwrap()
}

#[test]
fn enhancement_glyphs_do_not_populate_hold_and_explicit_d5_retains_its_existing_precedence() {
    let main = vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 12).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 12,
            operation: TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(1).unwrap(),
            },
        },
        TeletextEnhancementCommand::AtColumn {
            column: 13,
            operation: TeletextColumnOperation::Attributes {
                double_width: false,
                double_height: false,
                boxed: false,
                conceal: false,
                invert: false,
                underline_or_separated_mosaics: false,
            },
        },
    ];
    let original = with_main(&service("ASCII"), main);
    let mut cells = [b' '; 40];
    cells[8..15].copy_from_slice(&[0x17, 0x1a, 0x7f, 0x1e, 0x19, 0x09, 0x1f]);
    let edited = overlay(original.clone(), address(0x1234), 1, cells);
    let plan = composition(&edited, address(0x1234));
    assert_eq!(
        pixels(&plan, 11),
        [0, 0x79e, 0x79e, 0, 0x79e, 0x79e, 0x79e, 0, 0x79e, 0x79e]
    );
    assert_eq!(
        pixels(&plan, 12),
        [0xfc0, 0xfc0, 0xfc0, 0, 0, 0, 0, 0, 0, 0]
    );
    assert_eq!(pixels(&plan, 13), [0xfff; 10]);
    assert_eq!(pixels(&plan, 14), [0xfff; 10]);
    assert!(intent_at(&plan, 1, 13).held_mosaic().unwrap().separated());
    let origin = plan
        .display_resolution()
        .cell(1, 14)
        .unwrap()
        .origin()
        .unwrap();
    let mut budget = TeletextOriginCarrierBudget {
        max_candidates: 4096,
        max_origins: 8192,
        max_id_checks: 65_600,
        max_output_commands: 32,
        catalog: TeletextObjectCatalogBudget {
            max_objects: 32,
            max_commands: 8192,
        },
    };
    let carrier = build_teletext_origin_carrier(&plan, origin, edited.objects(), budget).unwrap();
    assert!(carrier.definition().program.commands().iter().any(|command| matches!(command,
        TeletextEnhancementCommand::AtColumn { operation:TeletextColumnOperation::G1 { mosaic },.. } if mosaic.blocks()==63)));
    budget.max_output_commands = 0;
    assert!(build_teletext_origin_carrier(&plan, origin, edited.objects(), budget).is_err());
    assert_eq!(edited.page_enhancements(), original.page_enhancements());
    assert_eq!(
        base_row(compiled(&edited, address(0x1234)).page(), 1),
        cells
    );
}

#[test]
fn held_caption_content_cannot_escape_existing_box_visibility_guards() {
    let mut page = service("ASCII").base().pages[0].clone();
    page.kind = TeletextPageKind::Subtitle;
    let mut cells = [b' '; 40];
    cells[8..16].copy_from_slice(&[0x0b, 0x0b, 0x17, 0x7f, 0x1e, 0x0a, 0x09, 0x1f]);
    assert_eq!(
        super::super::base_display::validate_physical_base_row(&page, 1, &cells).unwrap_err(),
        TeletextEncodingError::HiddenContent {
            address: page.address,
            row: 1,
            column: 14
        }
    );
    // Releasing while still in the box, or holding a literal empty mosaic,
    // remains valid. Rejecting every unboxed control would also be incorrect.
    cells[13] = 0x1f;
    cells[14] = 0x0a;
    cells[15] = 0x09;
    super::super::base_display::validate_physical_base_row(&page, 1, &cells).unwrap();
    cells[11] = 0x20;
    cells[13] = 0x0a;
    cells[14] = 0x09;
    cells[15] = 0x1f;
    super::super::base_display::validate_physical_base_row(&page, 1, &cells).unwrap();
}
