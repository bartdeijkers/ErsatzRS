//! Actual supplied-program diagnostics, distinct from original target and wire proof.

use super::*;

fn limits() -> TeletextEnhancementCollectionBudget {
    TeletextEnhancementCollectionBudget {
        max_commands: 8192,
        max_drcs_references: 8,
    }
}
fn catalog(objects: Vec<TeletextEnhancementObjectDefinition>) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        objects,
        TeletextObjectCatalogBudget {
            max_objects: 16,
            max_commands: 2048,
        },
    )
    .unwrap()
}
fn selected(display: &TeletextDisplayResolution, row: u8, column: u8) -> usize {
    let origin = display.cell(row, column).unwrap().origin().unwrap();
    display.origins()[origin].candidate_index()
}

#[test]
fn current_unicode_local_replay_differs_from_active_target_without_second_merge() {
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
    let service = fixture.build();
    let original = compose(&service);
    let retained = (service.clone(), original.clone());
    let replay = replay_teletext_physical_display(
        &service,
        &original,
        original.program(),
        service.objects(),
        limits(),
    )
    .unwrap();
    let target_candidate = selected(original.display_resolution(), 1, 8);
    assert_eq!(
        original.glyph_layers().candidates()[target_candidate].priority(),
        TeletextGlyphPriority::Active
    );
    assert!(
        matches!(original.glyph_intent().records()[target_candidate].glyph(),
        Some(TeletextGlyphIdentity::G0 { glyph }) if glyph.code() == b'A')
    );
    let physical_candidate = selected(replay.display_resolution(), 1, 8);
    assert_eq!(
        replay.glyph_layers().candidates()[physical_candidate].priority(),
        TeletextGlyphPriority::Local
    );
    assert!(
        matches!(replay.glyph_intent().records()[physical_candidate].glyph(),
        Some(TeletextGlyphIdentity::G2 { glyph }) if glyph.code() == 0x23
            && glyph.repertoire() == TeletextG2Repertoire::Latin)
    );
    // Every physical root event is exactly the supplied command, in its supplied
    // ordinal. No second layout merge or additional fallback event was inserted.
    let root_commands: Vec<_> = replay
        .spatial_trace()
        .events()
        .iter()
        .filter(|event| event.source().object.is_none())
        .map(|event| (event.command(), event.source().command))
        .collect();
    let supplied: Vec<_> = original
        .program()
        .commands()
        .iter()
        .copied()
        .enumerate()
        .map(|(ordinal, command)| (command, ordinal))
        .collect();
    assert_eq!(root_commands, supplied);
    assert_eq!(replay.owner(), owner());
    assert_eq!((service, original), retained);
}

#[test]
fn shifted_main_and_added_carrier_resolve_original_service_drcs_pixels() {
    let bitmap = [0x800, 0x400, 0x200, 0x100, 0x80, 0x40, 0x20, 0x10, 0x8, 1];
    let service = drcs_fixture(bitmap).build();
    let original = compose_drcs(&service);
    let retained = original.clone();
    let reference = TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Normal,
        subtable: TeletextDrcsSubtableIndex::new(15).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(47).unwrap(),
    };
    let carrier = local(0x0100);
    let mut definitions = service.objects().definitions().to_vec();
    definitions.push(definition(
        carrier,
        TeletextObjectType::Passive,
        vec![at(0, TeletextColumnOperation::Drcs { reference })],
    ));
    let catalog = catalog(definitions);
    let mut commands = vec![TeletextEnhancementCommand::SetScreenColour {
        colour: TeletextColourReference::BLACK,
    }];
    commands.extend_from_slice(original.program().commands());
    commands.extend([position(2, 8), invoke(carrier, TeletextObjectType::Passive)]);
    let actual = program(commands);
    let replay =
        replay_teletext_physical_display(&service, &original, &actual, &catalog, limits()).unwrap();
    assert_eq!(replay.drcs_execution().records().len(), 4);
    let normal: Vec<_> = replay
        .drcs_execution()
        .records()
        .iter()
        .filter(|record| record.reference() == reference)
        .collect();
    assert_eq!(normal.len(), 2);
    for record in &normal {
        assert_eq!(record.bitmap().get(), bitmap);
        assert_eq!(record.resource().kind(), TeletextResourceKind::Drcs);
        assert_eq!(record.resource().logical_subpage().get(), 15);
    }
    assert_eq!(
        normal[0].candidate().source(),
        Some(TeletextEnhancementSource {
            object: None,
            command: 2
        })
    );
    assert_eq!(
        normal[1].candidate().source(),
        Some(TeletextEnhancementSource {
            object: Some(carrier),
            command: 0
        })
    );
    assert_eq!(normal[1].candidate().invocation().unwrap().get(), 2);
    assert_eq!(normal[1].candidate().position(), (2, 8));
    assert_eq!(original, retained);
}

#[test]
fn actual_whole_program_missing_dependency_is_not_checked_against_old_main() {
    let service = drcs_fixture([0x800; 10]).build();
    let original = compose_drcs(&service);
    let retained = (service.clone(), original.clone());
    // The original main uses defined slot47. Supplied command1 legitimately has
    // a different operation, and fresh collection must diagnose missing slot46.
    let reference = TeletextDrcsGlyphReference {
        table: TeletextDrcsTableScope::Normal,
        subtable: TeletextDrcsSubtableIndex::new(15).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(46).unwrap(),
    };
    let actual = program(vec![
        position(1, 8),
        at(8, TeletextColumnOperation::Drcs { reference }),
    ]);
    let error =
        replay_teletext_physical_display(&service, &original, &actual, service.objects(), limits())
            .unwrap_err();
    assert_eq!(error.owner, owner());
    assert_eq!(
        error.location,
        Some(TeletextPhysicalDisplaySource::Main { command: 1 })
    );
    assert!(matches!(error.reason,
        TeletextMainCompositionFailure::DrcsProgram(TeletextDrcsProgramExecutionError::Dependencies { owner: address, error })
            if address == owner() && matches!(error.error, TeletextDrcsResourceError::MissingGlyph { slot, .. } if slot == reference.slot)));
    assert_eq!((service, original), retained);
}

#[test]
fn replay_keeps_unsettled_same_emitter_c2_writers_pending() {
    let service = Fixture::new(vec![text(1, 8, "A")], Some(vec![])).build();
    let original = compose(&service);
    let retained = original.clone();
    let actual = program(vec![
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
    ]);
    let replay =
        replay_teletext_physical_display(&service, &original, &actual, service.objects(), limits())
            .unwrap();
    let foreground = replay
        .attribute_execution()
        .cell(1, 8)
        .unwrap()
        .foreground();
    assert_eq!(foreground.value(), None);
    let pending = &replay.attribute_execution().pending()[foreground.pending().unwrap()];
    assert_eq!(
        pending.reason(),
        TeletextAttributePendingReason::TransmissionOrder
    );
    assert!(
        matches!(pending.first_source(), Some(TeletextAttributeSource::Enhancement { location, .. })
        if location == TeletextEnhancementSource { object: None, command: 1 })
    );
    assert!(
        matches!(pending.second_source(), Some(TeletextAttributeSource::Enhancement { location, .. })
        if location == TeletextEnhancementSource { object: None, command: 2 })
    );
    assert!(replay
        .display_resolution()
        .cell(1, 8)
        .unwrap()
        .attributes()
        .foreground()
        .pending()
        .is_some());
    assert_eq!(
        original
            .attribute_execution()
            .cell(1, 8)
            .unwrap()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(7).unwrap())
    );
    assert_eq!(original, retained);
}

#[test]
fn raw_and_expanded_limits_reject_before_a_partial_replay_or_input_mutation() {
    let service = Fixture::new(vec![], Some(vec![])).build();
    let original = compose(&service);
    let retained = (service.clone(), original.clone());
    let target = local(0xffff);
    let catalog = catalog(vec![definition(
        target,
        TeletextObjectType::Passive,
        (0..12).map(g0).collect(),
    )]);
    let actual = program(vec![
        position(1, 8),
        invoke(target, TeletextObjectType::Passive),
    ]);
    let mut budget = limits();
    budget.max_commands = 1;
    let error = replay_teletext_physical_display(&service, &original, &actual, &catalog, budget)
        .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextMainCompositionFailure::InputBudget { maximum: 1 }
    );
    assert_eq!(error.location, None);
    budget.max_commands = 4;
    let error = replay_teletext_physical_display(&service, &original, &actual, &catalog, budget)
        .unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextPhysicalDisplaySource::Object {
            identity: target,
            command: 2
        })
    );
    assert!(
        matches!(error.reason, TeletextMainCompositionFailure::SpatialTrace(error)
        if error.reason == TeletextSpatialTraceFailure::EventBudget { maximum: 4 })
    );
    assert_eq!((service, original), retained);
}

#[test]
fn changed_main_screen_or_row_plane_is_rejected_before_reusing_original_base() {
    let service = Fixture::new(vec![text(1, 8, "T")], Some(vec![])).build();
    let original = compose(&service);
    let retained = (service.clone(), original.clone());
    let red = TeletextColourReference::new(1).unwrap();
    for (command, row) in [
        (
            TeletextEnhancementCommand::SetScreenColour { colour: red },
            None,
        ),
        (
            TeletextEnhancementCommand::SetRowColour {
                row: 3,
                colour: red,
                following_rows: false,
            },
            Some(3),
        ),
    ] {
        let actual = program(vec![command]);
        let error = replay_teletext_physical_display(
            &service,
            &original,
            &actual,
            service.objects(),
            limits(),
        )
        .unwrap_err();
        assert_eq!(error.owner, owner());
        assert_eq!(error.location, None); // Context has values, not a writer sidecar.
        assert_eq!(
            error.reason,
            TeletextMainCompositionFailure::PlaneContextMismatch { row }
        );
    }
    // An explicit command is not itself prohibited: actual black planes equal
    // the retained black baseline even though the original main was empty.
    let unchanged = program(vec![TeletextEnhancementCommand::SetRowColour {
        row: 3,
        colour: TeletextColourReference::BLACK,
        following_rows: true,
    }]);
    assert!(replay_teletext_physical_display(
        &service,
        &original,
        &unchanged,
        service.objects(),
        limits()
    )
    .is_ok());
    assert_eq!((service, original), retained);
}

#[test]
fn actual_active_catalog_planes_are_checked_instead_of_original_definitions() {
    let target = local(0xffff);
    let mut fixture = Fixture::new(
        vec![text(1, 8, "T")],
        Some(vec![
            position(1, 8),
            invoke(target, TeletextObjectType::Active),
        ]),
    );
    fixture
        .objects
        .push(definition(target, TeletextObjectType::Active, vec![g0(0)]));
    let service = fixture.build();
    let original = compose(&service);
    let retained = (service.clone(), original.clone());
    let red = TeletextColourReference::new(1).unwrap();
    for (plane, row) in [
        (
            TeletextEnhancementCommand::SetScreenColour { colour: red },
            None,
        ),
        // Relative row2 + actual invocation origin row1 is absolute row3.
        (
            TeletextEnhancementCommand::SetRowColour {
                row: 2,
                colour: red,
                following_rows: false,
            },
            Some(3),
        ),
    ] {
        let changed = catalog(vec![definition(
            target,
            TeletextObjectType::Active,
            vec![plane, g0(0)],
        )]);
        let error = replay_teletext_physical_display(
            &service,
            &original,
            original.program(),
            &changed,
            limits(),
        )
        .unwrap_err();
        assert_eq!(error.location, None);
        assert_eq!(
            error.reason,
            TeletextMainCompositionFailure::PlaneContextMismatch { row }
        );
    }
    let unchanged = catalog(vec![definition(
        target,
        TeletextObjectType::Active,
        vec![
            TeletextEnhancementCommand::SetScreenColour {
                colour: TeletextColourReference::BLACK,
            },
            g0(0),
        ],
    )]);
    let replay = replay_teletext_physical_display(
        &service,
        &original,
        original.program(),
        &unchanged,
        limits(),
    )
    .unwrap();
    let candidate = selected(replay.display_resolution(), 1, 8);
    assert_eq!(
        replay.glyph_layers().candidates()[candidate].priority(),
        TeletextGlyphPriority::Active
    );
    assert_eq!((service, original), retained);
}

fn tied_main() -> TeletextEnhancementProgram {
    program(vec![
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
    ])
}

fn final_evidence(
    service: &TeletextSemanticService,
    original: &TeletextMainComposition,
    actual: &TeletextEnhancementProgram,
    replay: &TeletextPhysicalDisplayReplay,
) -> TeletextAttributeTransmissionEvidence {
    let allocations = super::super::object_allocation::allocate_teletext_resource_objects(
        service.objects(),
        &[],
        4,
    )
    .unwrap();
    let encoded = super::super::local_object_program::compile_with_context(
        owner(),
        actual,
        service.objects(),
        &allocations,
        TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 64,
        },
        Some(original.colour_context().row_colours()[0]),
    )
    .unwrap();
    // These foregrounds physically follow the one row selector: blue word1,
    // pink word2. No ordinal-only or detached hypothetical evidence is used.
    assert_eq!(encoded.command_positions()[1].semantic_word(), 1);
    assert_eq!(encoded.command_positions()[2].semantic_word(), 2);
    let positions: Vec<_> = replay
        .spatial_trace()
        .events()
        .iter()
        .enumerate()
        .map(
            |(event_ordinal, event)| TeletextAttributeTransmissionPosition {
                event_ordinal,
                source: event.source(),
                invocation: event.invocation(),
                word: encoded
                    .command_positions()
                    .iter()
                    .find(|position| position.source() == event.source())
                    .unwrap()
                    .semantic_word(),
            },
        )
        .collect();
    TeletextAttributeTransmissionEvidence::new(
        replay.spatial_trace(),
        &positions,
        limits().max_commands,
    )
    .unwrap()
}

#[test]
fn internal_final_packing_updates_only_actual_attribute_display_and_colour_projections() {
    let service = Fixture::new(vec![text(1, 8, "A")], Some(vec![])).build();
    let original = compose(&service);
    let retained = original.clone();
    let actual = tied_main();
    let replay =
        replay_teletext_physical_display(&service, &original, &actual, service.objects(), limits())
            .unwrap();
    assert_eq!(
        replay
            .attribute_execution()
            .cell(1, 8)
            .unwrap()
            .foreground()
            .value(),
        None
    );
    let no_positions = TeletextAttributeTransmissionEvidence::new(
        replay.spatial_trace(),
        &[],
        limits().max_commands,
    )
    .unwrap();
    let sparse = replay
        .clone()
        .with_transmission(&service, &original, &no_positions, limits().max_commands)
        .unwrap();
    assert_eq!(
        sparse
            .attribute_execution()
            .cell(1, 8)
            .unwrap()
            .foreground()
            .value(),
        None
    );
    let evidence = final_evidence(&service, &original, &actual, &replay);
    let settled = replay
        .clone()
        .with_transmission(&service, &original, &evidence, limits().max_commands)
        .unwrap();
    assert_eq!(
        settled
            .attribute_execution()
            .cell(1, 8)
            .unwrap()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    assert_eq!(
        settled
            .display_resolution()
            .cell(1, 8)
            .unwrap()
            .attributes()
            .foreground()
            .value(),
        Some(TeletextColourReference::new(13).unwrap())
    );
    let pixel = settled
        .colour_composition()
        .pixel(
            settled.display_resolution(),
            1,
            8,
            true,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
        )
        .unwrap();
    assert!(matches!(pixel, TeletextColourPixel::Rgb(rgb) if rgb.get() == [7, 0, 7]));
    assert_eq!(settled.spatial_trace(), replay.spatial_trace());
    assert_eq!(settled.glyph_layers(), replay.glyph_layers());
    assert_eq!(settled.charset_execution(), replay.charset_execution());
    assert_eq!(settled.glyph_intent(), replay.glyph_intent());
    assert_eq!(settled.drcs_execution(), replay.drcs_execution());
    assert_eq!(settled.display_geometry(), replay.display_geometry());
    assert_eq!(original, retained);
}

#[test]
fn internal_final_evidence_rejects_changed_commands_with_identical_owner_and_ordinals() {
    let service = Fixture::new(vec![text(1, 8, "A")], Some(vec![])).build();
    let original = compose(&service);
    let retained = original.clone();
    let actual = tied_main();
    let replay =
        replay_teletext_physical_display(&service, &original, &actual, service.objects(), limits())
            .unwrap();
    let evidence = final_evidence(&service, &original, &actual, &replay);
    let mut commands = actual.commands().to_vec();
    commands[2] = at(
        8,
        TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(2).unwrap(),
        },
    );
    let changed = replay_teletext_physical_display(
        &service,
        &original,
        &program(commands),
        service.objects(),
        limits(),
    )
    .unwrap();
    let error = changed
        .with_transmission(&service, &original, &evidence, limits().max_commands)
        .unwrap_err();
    assert_eq!(error.owner, owner());
    assert!(
        matches!(error.reason, TeletextMainCompositionFailure::AttributeExecution(error)
        if error.reason == TeletextAttributeExecutionFailure::TransmissionMismatch)
    );
    assert_eq!(original, retained);
}
