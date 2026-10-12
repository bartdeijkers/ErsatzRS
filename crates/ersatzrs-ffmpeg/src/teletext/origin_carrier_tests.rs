//! Literal per-origin recipes, not normalization or receiver acceptance.

use super::*;
use crate::teletext::{
    build_teletext_origin_carrier, build_teletext_origin_carrier_with_usage,
    TeletextOriginCarrierBudget, TeletextOriginCarrierCommandRole as Role,
    TeletextOriginCarrierFailure as Failure,
};

fn limits() -> TeletextOriginCarrierBudget {
    TeletextOriginCarrierBudget {
        max_candidates: 4096,
        max_origins: 8192,
        catalog: TeletextObjectCatalogBudget {
            max_objects: 64,
            max_commands: 8192,
        },
        max_id_checks: 65_600,
        max_output_commands: 6,
    }
}
fn origin(composition: &TeletextMainComposition, row: u8, column: u8) -> usize {
    composition
        .display_resolution()
        .cell(row, column)
        .unwrap()
        .origin()
        .unwrap()
}
fn operations(recipe: &crate::teletext::TeletextOriginCarrier<'_>) -> Vec<TeletextColumnOperation> {
    recipe
        .definition()
        .program
        .commands()
        .iter()
        .map(|command| {
            let TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation,
            } = command
            else {
                panic!("carrier changed relative origin")
            };
            *operation
        })
        .collect()
}
fn attributes(column: u8, size: TeletextDisplaySize, boxed: bool) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::Attributes {
            double_width: matches!(
                size,
                TeletextDisplaySize::DoubleWidth | TeletextDisplaySize::DoubleSize
            ),
            double_height: matches!(
                size,
                TeletextDisplaySize::DoubleHeight | TeletextDisplaySize::DoubleSize
            ),
            boxed,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    )
}
fn active_fixture() -> Fixture {
    let target = local(0xffff);
    let mut fixture = Fixture::new(
        vec![text(1, 8, "£")],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture
        .objects
        .push(definition(target, TeletextObjectType::Active, vec![g0(0)]));
    fixture
}

#[test]
fn active_a_over_base_pound_has_exact_passive_body_and_original_origin_lineage() {
    let service = active_fixture().build();
    let composition = compose(&service);
    let retained = (service.clone(), composition.clone());
    let index = origin(&composition, 1, 8);
    let recipe =
        build_teletext_origin_carrier(&composition, index, service.objects(), limits()).unwrap();
    assert_eq!(recipe.original_origin_index(), index);
    assert_eq!(
        recipe.original_origin(),
        &composition.display_resolution().origins()[index]
    );
    assert_eq!(
        recipe.candidate_index(),
        recipe.original_origin().candidate_index()
    );
    assert_eq!(recipe.candidate().priority(), TeletextGlyphPriority::Active);
    assert_eq!(
        composition.glyph_intent().records()[recipe.candidate_index()]
            .context()
            .selection()
            .unwrap()
            .value(),
        Some(TeletextCharsetSelection::PagePrimary(
            TeletextPageCharsetDesignation::new(1).unwrap()
        ))
    );
    assert_eq!(
        recipe.location(),
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        })
    );
    assert_eq!(
        recipe.root_source(),
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(recipe.invocation().unwrap().get(), 0);
    assert!(std::ptr::eq(
        recipe.composition(),
        std::ptr::from_ref(&composition)
    ));
    assert!(std::ptr::eq(recipe.catalog(), service.objects()));
    assert_eq!(recipe.definition().identity, local(0));
    assert_eq!(recipe.definition().object_type, TeletextObjectType::Passive);
    assert_eq!(
        recipe.command_roles(),
        &[
            Role::Foreground,
            Role::Background,
            Role::Flash,
            Role::Display,
            Role::Charset,
            Role::Glyph
        ]
    );
    assert_eq!(
        operations(&recipe),
        vec![
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(7).unwrap()
            },
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::BLACK
            },
            TeletextColumnOperation::Flash {
                mode: TeletextFlashMode::Steady,
                rate: TeletextFlashRate::Slow1Hz
            },
            TeletextColumnOperation::Attributes {
                double_width: false,
                double_height: false,
                boxed: false,
                conceal: false,
                invert: false,
                underline_or_separated_mosaics: false
            },
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(1).unwrap()
            },
            TeletextColumnOperation::G0 { code: code(b'A') },
        ]
    );
    assert_eq!(
        recipe.command_roles().len(),
        recipe.definition().program.commands().len()
    );
    assert_eq!((service, composition), retained);
}

#[test]
fn lowest_free_id_uses_full_exact_owner_namespace_and_charges_scan_plus_probes() {
    let mut fixture = active_fixture();
    fixture.objects.extend([
        definition(local(0), TeletextObjectType::Passive, vec![]),
        definition(local(1), TeletextObjectType::Passive, vec![]),
    ]);
    let foreign = TeletextObjectIdentity::local(
        TeletextPageAddress {
            subcode: TeletextSubcode::new(0x1235).unwrap(),
            ..owner()
        },
        TeletextObjectId::new(2),
    );
    let service = fixture.build();
    let composition = compose(&service);
    // Foreign owner entries are legal in the standalone allocation catalogue,
    // but cannot be added to a service lacking that owner's display page.
    let mut definitions = service.objects().definitions().to_vec();
    definitions.push(definition(foreign, TeletextObjectType::Passive, vec![]));
    let catalog = TeletextEnhancementObjectCatalog::new(definitions, limits().catalog).unwrap();
    let index = origin(&composition, 1, 8);
    let mut budget = limits();
    budget.max_id_checks = 7; // Four actual catalogue entries plus probes0/1/2.
    let recipe = build_teletext_origin_carrier(&composition, index, &catalog, budget).unwrap();
    assert_eq!(recipe.definition().identity, local(2));
    let (result, admitted) =
        build_teletext_origin_carrier_with_usage(&composition, index, &catalog, budget);
    let metered = result.unwrap();
    assert_eq!(admitted, 7);
    assert_eq!(metered.definition(), recipe.definition());
    assert_eq!(metered.command_roles(), recipe.command_roles());
    assert_eq!(metered.candidate(), recipe.candidate());
    assert_eq!(metered.original_origin(), recipe.original_origin());
    assert_eq!(metered.candidate_index(), recipe.candidate_index());
    assert_eq!(metered.location(), recipe.location());
    assert_eq!(metered.root_source(), recipe.root_source());
    assert_eq!(metered.invocation(), recipe.invocation());
    budget.max_id_checks = 6;
    let error = build_teletext_origin_carrier(&composition, index, &catalog, budget).unwrap_err();
    let (result, admitted) =
        build_teletext_origin_carrier_with_usage(&composition, index, &catalog, budget);
    assert_eq!(result.unwrap_err(), error);
    assert_eq!(
        admitted, 6,
        "denied probe2 keeps admitted scan and probes0/1"
    );
    assert_eq!(
        error.reason,
        Failure::InputBudget {
            kind: "identity checks",
            count: 7,
            maximum: 6
        }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        })
    );
    for (maximum, admitted) in [(3, 0), (4, 4)] {
        budget.max_id_checks = maximum;
        let ordinary =
            build_teletext_origin_carrier(&composition, index, &catalog, budget).unwrap_err();
        let (result, used) =
            build_teletext_origin_carrier_with_usage(&composition, index, &catalog, budget);
        assert_eq!(used, admitted);
        assert_eq!(result.unwrap_err(), ordinary);
    }
}

#[test]
fn exhausted_exact_owner_namespace_retains_all_admitted_scans_and_probes() {
    let service = active_fixture().build();
    let composition = compose(&service);
    let saved = composition.clone();
    let mut definitions = (0..u16::MAX)
        .map(|id| definition(local(id), TeletextObjectType::Passive, vec![]))
        .collect::<Vec<_>>();
    definitions.extend_from_slice(service.objects().definitions());
    let mut budget = limits();
    budget.catalog.max_objects = 65_536;
    budget.max_id_checks = 131_072;
    let catalog = TeletextEnhancementObjectCatalog::new(definitions, budget.catalog).unwrap();
    let index = origin(&composition, 1, 8);
    let (result, admitted) =
        build_teletext_origin_carrier_with_usage(&composition, index, &catalog, budget);
    assert_eq!(admitted, 131_072);
    assert_eq!(result.unwrap_err().reason, Failure::IdentityExhausted);
    budget.max_id_checks -= 1;
    let ordinary =
        build_teletext_origin_carrier(&composition, index, &catalog, budget).unwrap_err();
    let (result, admitted) =
        build_teletext_origin_carrier_with_usage(&composition, index, &catalog, budget);
    assert_eq!(admitted, 131_071);
    assert_eq!(result.unwrap_err(), ordinary);
    assert_eq!(
        ordinary.reason,
        Failure::InputBudget {
            kind: "identity checks",
            count: 131_072,
            maximum: 131_071,
        }
    );
    assert_eq!(composition, saved);
}

#[test]
fn effective_axes_not_requested_double_size_control_the_recipe() {
    let service = Fixture::new(
        vec![],
        Some(vec![
            position(1, 9),
            attributes(9, TeletextDisplaySize::DoubleHeight, false),
            g0(9),
            position(2, 8),
            attributes(8, TeletextDisplaySize::DoubleSize, false),
            g0(8),
        ]),
    )
    .build();
    let composition = compose(&service);
    let index = origin(&composition, 2, 8);
    let original = &composition.display_resolution().origins()[index];
    assert_eq!(
        original.requested_size(),
        Some(TeletextDisplaySize::DoubleSize)
    );
    assert_eq!(
        original.effective_size(),
        Some(TeletextDisplaySize::DoubleHeight)
    );
    let recipe =
        build_teletext_origin_carrier(&composition, index, service.objects(), limits()).unwrap();
    assert!(matches!(
        operations(&recipe)[3],
        TeletextColumnOperation::Attributes {
            double_width: false,
            double_height: true,
            ..
        }
    ));
    assert_eq!(recipe.original_origin_index(), index);
}

#[test]
fn enhanced_families_retain_typed_operation_and_exact_charset_without_font_identity_guessing() {
    for (primary, operation, needs_charset) in [
        (0x37, TeletextColumnOperation::G0 { code: code(0x41) }, true),
        (
            0x24,
            TeletextColumnOperation::G1Alpha {
                code: TeletextG1AlphaCode::new(0x41).unwrap(),
            },
            true,
        ),
        (0x37, TeletextColumnOperation::G2 { code: code(0x54) }, true),
        (
            0x24,
            TeletextColumnOperation::G0Diacritic {
                code: code(0x41),
                diacritic: TeletextDiacriticIndex::new(2).unwrap(),
            },
            true,
        ),
        (
            0x55,
            TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(1).unwrap(),
            },
            false,
        ),
        (
            0x37,
            TeletextColumnOperation::G3 {
                code: code(0x21),
                compatibility: TeletextG3Compatibility::Level25,
            },
            false,
        ),
    ] {
        let mut fixture = Fixture::new(vec![], Some(vec![position(1, 8), at(8, operation)]));
        fixture.primary = Some(primary);
        let service = fixture.build();
        let composition = compose(&service);
        let recipe = build_teletext_origin_carrier(
            &composition,
            origin(&composition, 1, 8),
            service.objects(),
            limits(),
        )
        .unwrap();
        let actual = operations(&recipe);
        assert_eq!(actual.last(), Some(&operation));
        assert_eq!(
            recipe.command_roles().contains(&Role::Charset),
            needs_charset
        );
        if needs_charset {
            assert_eq!(
                actual[4],
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(primary).unwrap()
                }
            );
        }
    }
}

#[test]
fn complete_drcs_reference_and_original_instance_are_retained_without_charset_substitution() {
    let service = drcs_fixture([0x800, 1, 0x401, 0x202, 0, 0x104, 0x88, 0x50, 0x20, 0xfff]).build();
    let composition = compose_drcs(&service);
    let recipe = build_teletext_origin_carrier(
        &composition,
        origin(&composition, 1, 8),
        service.objects(),
        limits(),
    )
    .unwrap();
    assert_eq!(
        operations(&recipe).last(),
        Some(&TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                table: TeletextDrcsTableScope::Global,
                subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                slot: TeletextDrcsGlyphSlot::new(3).unwrap()
            }
        })
    );
    assert!(!recipe.command_roles().contains(&Role::Charset));
    assert_eq!(recipe.invocation().unwrap().get(), 1);
    assert_eq!(
        recipe.location(),
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 1
        })
    );
}

#[test]
fn authored_replacements_and_national_base_use_explicit_latin_conversion() {
    let service = Fixture::new(
        vec![text(1, 8, "£"), text(2, 8, "é"), text(3, 8, "§")],
        Some(vec![]),
    )
    .build();
    let composition = compose(&service);
    for (row, operation) in [
        (1, TeletextColumnOperation::G2 { code: code(0x23) }),
        (3, TeletextColumnOperation::G2 { code: code(0x27) }),
        (
            2,
            TeletextColumnOperation::G0Diacritic {
                code: code(b'e'),
                diacritic: TeletextDiacriticIndex::new(2).unwrap(),
            },
        ),
    ] {
        let recipe = build_teletext_origin_carrier(
            &composition,
            origin(&composition, row, 8),
            service.objects(),
            limits(),
        )
        .unwrap();
        let actual = operations(&recipe);
        assert_eq!(
            actual[4],
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(0).unwrap()
            }
        );
        assert_eq!(actual[5], operation);
        assert_eq!(recipe.candidate().priority(), TeletextGlyphPriority::Base);
        assert_eq!(recipe.location(), None);
    }
    let original = composition.glyph_intent().records()
        [composition.display_resolution().origins()[origin(&composition, 3, 8)].candidate_index()];
    assert!(
        matches!(original.glyph(), Some(TeletextGlyphIdentity::G0 { glyph })
        if glyph.code() == 0x40 && glyph.national_subset() == Some(TeletextNationalSubset::German))
    );
    assert!(composition.base_replacement(3, 8).is_none());
}

#[test]
fn shared_alias_consensus_has_a_physical_recipe_without_a_raw_winner() {
    let target = local(0xffff);
    let mut fixture = Fixture::new(
        vec![],
        Some(vec![
            position(1, 5),
            charset(5, 1),
            invoke(target, TeletextObjectType::Active),
            g0(6),
        ]),
    );
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Active,
        vec![charset(0, 0)],
    ));
    let service = fixture.build();
    let composition = compose(&service);
    let retained = composition.clone();
    let index = origin(&composition, 1, 6);
    let candidate = composition.display_resolution().origins()[index].candidate_index();
    let intent = &composition.glyph_intent().records()[candidate];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert_eq!(
        intent.selected_pair(),
        Some(TeletextCharsetPair::LatinLatin)
    );
    let selection = intent.context().selection().unwrap();
    assert_eq!(selection.value(), None);
    assert!(selection.pending().is_some());
    assert_eq!(selection.declaration(), None);
    let recipe =
        build_teletext_origin_carrier(&composition, index, service.objects(), limits()).unwrap();
    assert_eq!(
        operations(&recipe)[4],
        TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(0).unwrap()
        }
    );
    assert_eq!(
        operations(&recipe)[5],
        TeletextColumnOperation::G0 { code: code(b'A') }
    );
    assert_eq!(composition, retained);
}

#[test]
fn special_at_symbol_needs_no_charset_even_when_shared_designation_order_is_pending() {
    let target = local(0xffff);
    let special = TeletextColumnOperation::G0Diacritic {
        code: code(0x2a),
        diacritic: TeletextDiacriticIndex::new(0).unwrap(),
    };
    let mut fixture = Fixture::new(
        vec![],
        Some(vec![
            position(1, 8),
            charset(8, 0x37),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Active,
        vec![
            at(
                0,
                TeletextColumnOperation::DesignateCharset {
                    designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
                },
            ),
            at(0, special),
        ],
    ));
    let service = fixture.build();
    let composition = compose(&service);
    let index = origin(&composition, 1, 8);
    let candidate = composition.display_resolution().origins()[index].candidate_index();
    assert_eq!(
        composition.glyph_intent().records()[candidate].glyph(),
        Some(TeletextGlyphIdentity::AtSymbol)
    );
    assert!(composition.glyph_intent().records()[candidate]
        .context()
        .selection()
        .unwrap()
        .pending()
        .is_some());
    let recipe =
        build_teletext_origin_carrier(&composition, index, service.objects(), limits()).unwrap();
    assert_eq!(operations(&recipe).last(), Some(&special));
    assert!(!recipe.command_roles().contains(&Role::Charset));
}

#[test]
fn temporal_flash_and_uncertain_geometry_remain_located_derivation_gaps() {
    let target = local(0xffff);
    let mut fixture = Fixture::new(
        vec![],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Passive),
        ]),
    );
    fixture.objects.push(definition(
        target,
        TeletextObjectType::Passive,
        vec![
            at(
                0,
                TeletextColumnOperation::Flash {
                    mode: TeletextFlashMode::Normal,
                    rate: TeletextFlashRate::Fast2HzIncremental,
                },
            ),
            g0(0),
        ],
    ));
    let service = fixture.build();
    let composition = compose(&service);
    let error = build_teletext_origin_carrier(
        &composition,
        origin(&composition, 1, 8),
        service.objects(),
        limits(),
    )
    .unwrap_err();
    assert_eq!(error.reason, Failure::TemporalFlash);
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 1
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(error.invocation.unwrap().get(), 0);
    let uncertain = compose(
        &Fixture::new(
            vec![],
            Some(vec![
                position(24, 39),
                attributes(39, TeletextDisplaySize::DoubleSize, false),
                g0(39),
            ]),
        )
        .build(),
    );
    let empty = TeletextEnhancementObjectCatalog::new(vec![], limits().catalog).unwrap();
    let uncertain_origin = uncertain
        .display_resolution()
        .origins()
        .iter()
        .position(|origin| {
            let candidate = &uncertain.glyph_layers().candidates()[origin.candidate_index()];
            candidate.position() == (24, 39)
                && candidate.source()
                    == Some(TeletextEnhancementSource {
                        object: None,
                        command: 2,
                    })
        })
        .unwrap();
    let error =
        build_teletext_origin_carrier(&uncertain, uncertain_origin, &empty, limits()).unwrap_err();
    assert!(matches!(
        error.reason,
        Failure::OriginalPending { .. } | Failure::Geometry
    ));
    assert_eq!(error.origin, uncertain_origin);
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 2
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 2 })
    );
}

#[test]
fn original_substituted_window_failure_is_not_laundered_by_explicit_carrier_background() {
    let mut fixture = Fixture::new(
        vec![text(1, 8, "A")],
        Some(vec![
            TeletextEnhancementCommand::SetRowColour {
                row: 1,
                colour: TeletextColourReference::new(8).unwrap(),
                following_rows: false,
            },
            position(1, 8),
            attributes(8, TeletextDisplaySize::Normal, true),
        ]),
    );
    let mut presentation = crate::teletext::presentation::western_latin_v1_presentation(
        TeletextNationalOption::new(1).unwrap(),
    )
    .unwrap();
    presentation.black_background_substitution = true;
    fixture.presentation = Some(presentation);
    let service = fixture.build();
    let composition = compose(&service);
    let before = composition.clone();
    let error = build_teletext_origin_carrier(
        &composition,
        origin(&composition, 1, 8),
        service.objects(),
        limits(),
    )
    .unwrap_err();
    let Failure::Colour(error) = error.reason else {
        panic!("original parameter failure was replaced")
    };
    assert_eq!(
        error.reason,
        TeletextColourCompositionFailure::BackgroundParameterFromSubstitution {
            usage: TeletextBackgroundParameterUse::WindowTransparency
        }
    );
    assert!(error.parameter.is_some());
    assert_eq!(composition, before);
}

#[test]
fn raw_input_output_and_id_budgets_fence_work_with_original_blame_and_no_mutation() {
    let service = active_fixture().build();
    let composition = compose(&service);
    let retained = (service.clone(), composition.clone());
    let index = origin(&composition, 1, 8);
    for (kind, mut budget) in [
        ("candidates", limits()),
        ("origins", limits()),
        ("catalogue objects", limits()),
        ("catalogue commands", limits()),
        ("output commands", limits()),
        ("identity checks", limits()),
    ] {
        match kind {
            "candidates" => {
                budget.max_candidates = composition.glyph_layers().candidates().len() - 1;
            }
            "origins" => budget.max_origins = composition.display_resolution().origins().len() - 1,
            "catalogue objects" => budget.catalog.max_objects = 0,
            "catalogue commands" => budget.catalog.max_commands = 0,
            "output commands" => budget.max_output_commands = 5,
            "identity checks" => budget.max_id_checks = 1,
            _ => unreachable!(),
        }
        let error = build_teletext_origin_carrier(&composition, index, service.objects(), budget)
            .unwrap_err();
        let (result, admitted) = build_teletext_origin_carrier_with_usage(
            &composition,
            index,
            service.objects(),
            budget,
        );
        assert_eq!(result.unwrap_err(), error);
        assert_eq!(admitted, usize::from(kind == "identity checks"));
        assert!(
            matches!(error.reason, Failure::InputBudget { kind: actual, .. } if actual == kind)
        );
        assert_eq!(error.origin, index);
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: Some(local(0xffff)),
                command: 0
            })
        );
        assert_eq!(error.invocation.unwrap().get(), 0);
    }
    let error =
        build_teletext_origin_carrier(&composition, usize::MAX, service.objects(), limits())
            .unwrap_err();
    assert_eq!(error.reason, Failure::Correspondence);
    assert_eq!(error.location, None);
    assert_eq!((service, composition), retained);
}

#[test]
fn mosaic_separation_materialization_distinguishes_base_from_enhanced_and_undefined_response() {
    let mosaic = TeletextMosaic::new(63).unwrap();
    for (operation, expected) in [
        (None, Ok(true)),
        (Some(TeletextColumnOperation::G1 { mosaic }), Ok(false)),
        (
            Some(TeletextColumnOperation::G3 {
                code: code(0x21),
                compatibility: TeletextG3Compatibility::Level25,
            }),
            Err(Failure::UndefinedSpacing),
        ),
    ] {
        let rows = vec![TeletextRow {
            row: 1,
            runs: vec![TeletextRun {
                column: 8,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Mosaics {
                    cells: vec![mosaic],
                    separated: true,
                },
            }],
        }];
        let main =
            operation.map_or_else(Vec::new, |operation| vec![position(1, 8), at(8, operation)]);
        let service = Fixture::new(rows, Some(main)).build();
        let composition = compose(&service);
        let index = origin(&composition, 1, 8);
        assert_eq!(
            composition
                .display_resolution()
                .origin_attributes(index)
                .unwrap()
                .spacing_separated()
                .value(),
            Some(true)
        );
        let result =
            build_teletext_origin_carrier(&composition, index, service.objects(), limits());
        match expected {
            Ok(d5) => assert!(
                matches!(operations(&result.unwrap())[3], TeletextColumnOperation::Attributes { underline_or_separated_mosaics, .. } if underline_or_separated_mosaics == d5)
            ),
            Err(reason) => assert_eq!(result.unwrap_err().reason, reason),
        }
    }
}

#[test]
fn unknown_original_foreground_is_not_resolved_by_fresh_carrier_source_order() {
    let service = Fixture::new(
        vec![],
        Some(vec![
            position(1, 8),
            at(
                8,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(4).unwrap(),
                },
            ),
            at(
                8,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(13).unwrap(),
                },
            ),
            g0(8),
        ]),
    )
    .build();
    let composition = compose(&service);
    let before = composition.clone();
    let error = build_teletext_origin_carrier(
        &composition,
        origin(&composition, 1, 8),
        service.objects(),
        limits(),
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        Failure::Attribute {
            field: "foreground"
        }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 3
        })
    );
    assert_eq!(
        error.root_source,
        Some(TeletextSpatialRootSource::Local { command: 3 })
    );
    assert_eq!(composition, before);
}
