//! EN 300 706 Table 25/26 and clause 15.3: Base G0 is row-local; G2 is not switched.

use super::*;

fn retain(name: &str, encoded: &EncodedTeletextAuthoredService, points: &Value) {
    for overlay in encoded
        .declared()
        .semantic()
        .physical_base_overlays()
        .values()
    {
        for row in overlay.rows() {
            let unit = encoded.pages()[0]
                .data_units()
                .iter()
                .find(|unit| packet(unit) == row.row())
                .unwrap();
            let actual = body(unit).map(|code| code & 0x7f);
            let start = if row.row() == 0 { 8 } else { 0 };
            assert_eq!(&actual[start..], &row.cells()[start..]);
        }
    }
    if let Some(root) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_BASE_G0_PROOF_DIR"])
    {
        let mut units = encoded.pages()[0].data_units().to_vec();
        units.push(super::super::super::packets::terminator(2));
        std::fs::write(
            std::path::Path::new(&root).join(format!("{name}.json")),
            serde_json::to_vec(
                &json!({"name":name,"page":0x200,"subcode":0,"points":points,
                "units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>()}),
            )
            .unwrap(),
        )
        .unwrap();
    }
}

fn composition(original: &TeletextDeclaredService) -> TeletextMainComposition {
    compose_teletext_main_program(
        original.semantic(),
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
    .unwrap()
}

fn glyph_at(composition: &TeletextMainComposition, row: u8, column: u8) -> TeletextG0Glyph {
    composition
        .glyph_intent()
        .records()
        .iter()
        .find_map(|intent| {
            let base = intent.context().candidate().base()?;
            if (base.position().row(), base.position().column()) != (row, column) {
                return None;
            }
            match intent.glyph().unwrap() {
                TeletextGlyphIdentity::G0 { glyph } => Some(glyph),
                other => panic!("expected Base G0 at {row}/{column}, got {other:?}"),
            }
        })
        .unwrap()
}

#[test]
fn header_g0_does_not_apply_body_national_substitution() {
    let declared = declare(draft(0));
    let mut cells = [b' '; 40];
    cells[8] = 0x23;
    let original = declared
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(0),
                vec![
                    TeletextPhysicalBaseRow::new(0, cells).unwrap(),
                    TeletextPhysicalBaseRow::new(1, cells).unwrap(),
                ],
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap();
    let composed = composition(&original);
    assert_eq!(glyph_at(&composed, 0, 8).national_subset(), None);
    assert_eq!(
        glyph_at(&composed, 1, 8).national_subset(),
        Some(TeletextNationalSubset::English)
    );
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), &original);
    retain(
        "header",
        &encoded,
        &json!([
            {"row":0,"column":8,"unicode":0x23}, {"row":1,"column":8,"unicode":0xa3},
        ]),
    );
}

#[test]
fn base_escape_switches_g0_then_restores_primary_at_the_next_row() {
    let mut input = draft(0);
    input.pages[0].presentation.as_mut().unwrap().secondary =
        Some(TeletextPageCharsetDesignation::new(0x37).unwrap());
    let mut cells = [b' '; 40];
    cells[8..13].copy_from_slice(&[b'A', 0x1b, b'A', 0x1b, b'A']);
    let original = declare(input)
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(0),
                vec![
                    TeletextPhysicalBaseRow::new(1, cells).unwrap(),
                    // An odd number of switches must not leak into the next row.
                    TeletextPhysicalBaseRow::new(2, {
                        cells[11] = b' ';
                        cells[12] = b'A';
                        cells
                    })
                    .unwrap(),
                    TeletextPhysicalBaseRow::new(3, [b'A'; 40]).unwrap(),
                ],
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap();
    let composed = composition(&original);
    {
        let row = 1;
        for (column, repertoire) in [
            (8, TeletextG0Repertoire::Latin),
            (10, TeletextG0Repertoire::Greek),
            (12, TeletextG0Repertoire::Latin),
        ] {
            let glyph = glyph_at(&composed, row, column);
            assert_eq!((glyph.code(), glyph.repertoire()), (b'A', repertoire));
        }
    }
    assert_eq!(
        glyph_at(&composed, 2, 12).repertoire(),
        TeletextG0Repertoire::Greek
    );
    assert_eq!(
        glyph_at(&composed, 3, 8).repertoire(),
        TeletextG0Repertoire::Latin
    );
    let secondary =
        composed
            .glyph_intent()
            .records()
            .iter()
            .find(|intent| {
                intent.context().candidate().base().is_some_and(|base| {
                    (base.position().row(), base.position().column()) == (1, 10)
                })
            })
            .unwrap();
    let selected = secondary.context().base_g0().unwrap();
    assert_eq!(selected.designation().get(), 0x37);
    assert_eq!(
        selected.origin(),
        TeletextBaseG0Origin::EscapeSecondary {
            position: TeletextEnhancementPosition::new(1, 9).unwrap(),
        }
    );
    assert_eq!(secondary.selected_pair(), None);
    assert_eq!(
        secondary.context().selection().unwrap().value(),
        Some(TeletextCharsetSelection::PagePrimary(
            TeletextPageCharsetDesignation::new(0).unwrap()
        ))
    );
    assert_secondary_origin_carrier(&composed, &original);
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), &original);
    assert!(encoded.compiler_drcs_resources().is_empty());
    retain(
        "escape",
        &encoded,
        &json!([
            {"row":1,"column":8,"unicode":0x41},
            {"row":1,"column":10,"unicode":0x391,"level25_only":true},
            {"row":1,"column":12,"unicode":0x41},
            {"row":2,"column":12,"unicode":0x391,"level25_only":true},
            {"row":3,"column":8,"unicode":0x41},
        ]),
    );
}

fn assert_secondary_origin_carrier(
    composed: &TeletextMainComposition,
    original: &TeletextDeclaredService,
) {
    let origin = composed
        .display_resolution()
        .cell(1, 10)
        .unwrap()
        .origin()
        .unwrap();
    let carrier = build_teletext_origin_carrier(
        composed,
        origin,
        original.semantic().objects(),
        TeletextOriginCarrierBudget {
            max_candidates: 4096,
            max_origins: 8192,
            catalog: TeletextObjectCatalogBudget {
                max_objects: 64,
                max_commands: 8192,
            },
            max_id_checks: 65_600,
            max_output_commands: 6,
        },
    )
    .unwrap();
    assert!(carrier.definition().program.commands().iter().any(|command| matches!(command,
        TeletextEnhancementCommand::AtColumn { operation: TeletextColumnOperation::DesignateCharset { designation }, .. }
        if designation.get() == 0x37)));
}

#[test]
fn header_logical_text_preserves_literal_g0_and_explicit_supplementary_intent() {
    let mut input = draft(1);
    input.base.pages[0].rows = vec![TeletextRow {
        row: 0,
        runs: vec![TeletextRun {
            column: 8,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "#£".into() },
        }],
    }];
    let original = declare(input);
    let composed = composition(&original);
    assert_eq!(
        (
            glyph_at(&composed, 0, 8).code(),
            glyph_at(&composed, 0, 8).national_subset()
        ),
        (0x23, None)
    );
    assert_eq!(composed.base_display().cell(0, 9).unwrap().code(), b' ');
    assert!(composed.base_replacement(0, 9).is_some());
    let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
    assert_eq!(encoded.declared(), &original);
    retain(
        "logical-header",
        &encoded,
        &json!([
            {"row":0,"column":8,"unicode":0x23},
            {"row":0,"column":9,"unicode":0xa3,"level1_fallback":0x20},
        ]),
    );
}

#[test]
fn secondary_national_subset_is_its_own_and_escape_does_not_select_supplementary_g2() {
    for secondary in [None, Some(1), Some(0x37)] {
        let mut input = draft(0);
        input.pages[0].presentation.as_mut().unwrap().secondary =
            secondary.map(|code| TeletextPageCharsetDesignation::new(code).unwrap());
        input.pages[0].local_program = Some(
            TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::SetPosition {
                    position: TeletextEnhancementPosition::new(1, 20).unwrap(),
                },
                TeletextEnhancementCommand::AtColumn {
                    column: 20,
                    operation: TeletextColumnOperation::G2 {
                        code: TeletextGlyphCode::new(0x23).unwrap(),
                    },
                },
            ])
            .unwrap(),
        );
        let mut cells = [b' '; 40];
        cells[8..12].copy_from_slice(&[0x1b, 0x5b, 0x17, 0x5b]);
        let original = declare(input)
            .with_physical_base_overlays(
                vec![TeletextPhysicalBaseOverlay::new(
                    owner(0),
                    vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
                )
                .unwrap()],
                semantic_limits(),
            )
            .unwrap();
        let composed = composition(&original);
        for column in [9, 11] {
            let glyph = glyph_at(&composed, 1, column);
            let (family, subset) = match secondary {
                Some(1) => (
                    TeletextG0Repertoire::Latin,
                    Some(TeletextNationalSubset::German),
                ),
                Some(0x37) => (TeletextG0Repertoire::Greek, None),
                _ => (
                    TeletextG0Repertoire::Latin,
                    Some(TeletextNationalSubset::English),
                ),
            };
            assert_eq!(
                (glyph.repertoire(), glyph.national_subset()),
                (family, subset)
            );
        }
        let replacement = composed
            .glyph_intent()
            .records()
            .iter()
            .find(|intent| {
                intent.context().candidate().operation()
                    == Some(TeletextColumnOperation::G2 {
                        code: TeletextGlyphCode::new(0x23).unwrap(),
                    })
            })
            .unwrap();
        assert!(
            matches!(replacement.glyph(), Some(TeletextGlyphIdentity::G2 { glyph })
            if glyph.repertoire() == TeletextG2Repertoire::Latin && glyph.code() == 0x23)
        );
        assert_eq!(replacement.context().base_g0(), None);
        let encoded = encode_teletext_authored_service(original.clone(), &[], &limits()).unwrap();
        assert_eq!(encoded.declared(), &original);
        assert!(encoded.compiler_drcs_resources().is_empty());
        let expected = match secondary {
            Some(1) => 0xc4,
            Some(0x37) => 0x3ab,
            _ => 0x2190,
        };
        retain(
            &format!("secondary-{:02x}", secondary.unwrap_or(0x7f)),
            &encoded,
            &json!([
                {"row":1,"column":9,"unicode":expected,"level25_only":true},
                {"row":1,"column":11,"unicode":expected,"level25_only":true},
                {"row":1,"column":20,"unicode":0xa3,"level1_fallback":0x20},
            ]),
        );
    }
}

#[test]
fn missing_presentation_does_not_become_an_explicit_no_second_g0_declaration() {
    let mut input = draft(0);
    input.pages[0].presentation = None;
    let mut cells = [b' '; 40];
    cells[8] = 0x1b;
    let original = declare(input)
        .with_physical_base_overlays(
            vec![TeletextPhysicalBaseOverlay::new(
                owner(0),
                vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
            )
            .unwrap()],
            semantic_limits(),
        )
        .unwrap();
    let error = compose_teletext_main_program(
        original.semantic(),
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
    .unwrap_err();
    assert!(
        matches!(error.reason, TeletextMainCompositionFailure::BaseDisplay(error)
        if error.position == Some(TeletextEnhancementPosition::new(1, 8).unwrap())
        && error.reason == TeletextBaseDisplayFailure::DecoderDependentSecondary)
    );
    assert!(original
        .semantic()
        .page_enhancements()
        .get(&owner(0))
        .unwrap()
        .presentation
        .is_none());
}
