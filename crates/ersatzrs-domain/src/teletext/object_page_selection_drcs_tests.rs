//! Raw normal/global targets and their shared glyph consumers form one final graph.
use super::*;

fn reference(table: TeletextDrcsTableScope, changed: bool) -> TeletextDrcsGlyphReference {
    TeletextDrcsGlyphReference {
        table,
        subtable: TeletextDrcsSubtableIndex::new(if changed { 15 } else { 0 }).unwrap(),
        slot: TeletextDrcsGlyphSlot::new(if changed { 47 } else { 0 }).unwrap(),
    }
}
fn consumer(changed: bool) -> TeletextEnhancementProgram {
    let mut commands = program(global(0xa2, 0)).commands().to_vec();
    for (column, table) in [
        (4, TeletextDrcsTableScope::Normal),
        (5, TeletextDrcsTableScope::Global),
    ] {
        commands.push(TeletextEnhancementCommand::AtColumn {
            column,
            operation: TeletextColumnOperation::Drcs {
                reference: reference(table, changed),
            },
        });
    }
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn selection_input(changed: bool) -> TeletextResolvedServiceDraft {
    let mut source = input(true, false);
    source.objects[0].program = consumer(changed);
    for (kind, byte, s1, slot, bitmap) in [
        (TeletextResourceKind::Drcs, 0xb0, 0, 0, 0x801),
        (TeletextResourceKind::Gdrcs, 0xb1, 0, 0, 0x803),
        (TeletextResourceKind::Drcs, 0xb2, 15, 47, 0x805),
        (TeletextResourceKind::Gdrcs, 0xb3, 15, 47, 0x807),
    ] {
        source.drcs.push(
            TeletextDrcsSubtable::new(
                identity(kind, byte, s1),
                vec![TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(slot).unwrap(),
                    bitmap: TeletextDrcsBitmap::new([bitmap; 10]).unwrap(),
                }],
            )
            .unwrap(),
        );
    }
    for page in &mut source.pages {
        page.resources.drcs = TeletextDrcsPageLinks::new(
            Some(raw(if changed { 0xb2 } else { 0xb0 })),
            Some(raw(if changed { 0xb3 } else { 0xb1 })),
        )
        .unwrap();
        let original = page.compositional_links.unwrap();
        page.compositional_links = Some(
            TeletextX27Level25Links::new(
                original.link(TeletextX27LinkRole::Gpop),
                original.link(TeletextX27LinkRole::Pop),
                link(
                    if changed { 0xb3 } else { 0xb1 },
                    if changed { 0x8000 } else { 1 },
                ),
                link(
                    if changed { 0xb2 } else { 0xb0 },
                    if changed { 0x8000 } else { 1 },
                ),
            )
            .unwrap(),
        );
    }
    source
}
fn selections(original: &TeletextDeclaredService) -> Vec<TeletextObjectPageSelectionChange> {
    [owner(0), owner(32)]
        .into_iter()
        .flat_map(|display| {
            [
                (TeletextX27LinkRole::Drcs, 0xb2),
                (TeletextX27LinkRole::Gdrcs, 0xb3),
            ]
            .map(|(role, page)| change(original, display, role, link(page, 0x8000)))
        })
        .collect()
}
fn consumer_overlay() -> TeletextResourceObjectProgramOverlay {
    TeletextResourceObjectProgramOverlay::new(
        target(0xa0),
        consumer(false),
        consumer(true),
        vec![Some(0), Some(1), Some(2)],
        limits(),
    )
    .unwrap()
}

#[test]
fn atomic_drcs_page_selection_rebuilds_normal_global_shared_consumers_and_exact_undo() {
    let original = bind_input(selection_input(false));
    let before = original.clone();
    let selected = selections(&original);
    assert!(original
        .clone()
        .with_object_page_selections_and_program_overlays(&selected, &[], limits(), budget(),)
        .is_err());
    assert!(original
        .clone()
        .with_resource_object_program_overlays(&[consumer_overlay()], limits(),)
        .is_err());
    let edited = original
        .clone()
        .with_object_page_selections_and_program_overlays(
            &selected,
            &[consumer_overlay()],
            limits(),
            budget(),
        )
        .unwrap();
    assert_eq!(edited, bind_input(selection_input(true)));
    assert_eq!(original, before);
    assert_eq!(edited.mots(), original.mots());
    assert_eq!(edited.defaults(), original.defaults());
    assert_eq!(edited.selections(), original.selections());
    assert_eq!(edited.resource_inventory(), original.resource_inventory());
    assert_eq!(edited.empty_object_pages(), original.empty_object_pages());
    assert_eq!(edited.semantic().drcs(), original.semantic().drcs());
    for display in [owner(0), owner(32)] {
        assert_eq!(
            edited.semantic().collected(display).unwrap().1.references(),
            &[
                reference(TeletextDrcsTableScope::Normal, true),
                reference(TeletextDrcsTableScope::Global, true),
            ]
        );
        for role in [TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop] {
            assert_eq!(
                edited
                    .resolutions()
                    .iter()
                    .find(|r| r.display() == display)
                    .unwrap()
                    .role(role),
                original
                    .resolutions()
                    .iter()
                    .find(|r| r.display() == display)
                    .unwrap()
                    .role(role)
            );
        }
    }
    let reverse: Vec<_> = selected
        .iter()
        .map(|intent| {
            change(
                &edited,
                intent.display(),
                intent.role(),
                link(
                    if intent.role() == TeletextX27LinkRole::Drcs {
                        0xb0
                    } else {
                        0xb1
                    },
                    1,
                ),
            )
        })
        .collect();
    let overlay = TeletextResourceObjectProgramOverlay::new(
        target(0xa0),
        consumer(true),
        consumer(false),
        vec![Some(0), Some(1), Some(2)],
        limits(),
    )
    .unwrap();
    assert_eq!(
        edited
            .with_object_page_selections_and_program_overlays(
                &reverse,
                &[overlay],
                limits(),
                budget(),
            )
            .unwrap(),
        original
    );
}

#[test]
fn atomic_drcs_page_selection_rejects_incomplete_wrong_kind_stale_and_duplicate_intent() {
    use TeletextObjectPageSelectionError as Error;
    let original = bind_input(selection_input(false));
    let selected = selections(&original);
    let apply = |changes: &[TeletextObjectPageSelectionChange]| {
        original
            .clone()
            .with_object_page_selections_and_program_overlays(
                changes,
                &[consumer_overlay()],
                limits(),
                budget(),
            )
    };
    assert!(apply(&selected[..3]).is_err());
    assert!(matches!(
        apply(&[selected[0], selected[0]]),
        Err(Error::Duplicate { .. })
    ));
    for (role, page) in [
        (TeletextX27LinkRole::Drcs, 0xb3),
        (TeletextX27LinkRole::Gdrcs, 0xb2),
        (TeletextX27LinkRole::Drcs, 0xba),
    ] {
        let missing = change(&original, owner(0), role, link(page, 0x8000));
        assert!(matches!(
            apply(&[missing]),
            Err(Error::MissingResource { .. })
        ));
    }
    let stale = TeletextObjectPageSelectionChange::new(
        owner(0),
        TeletextX27LinkRole::Drcs,
        selected[0].expected_links(),
        selected[1].expected_role(),
        selected[0].replacement(),
    )
    .unwrap();
    assert!(matches!(
        apply(&[stale]),
        Err(Error::OriginalMismatch { .. })
    ));
    let mut wrong_mask = selected.clone();
    wrong_mask[0] = change(
        &original,
        owner(0),
        TeletextX27LinkRole::Drcs,
        link(0xb2, 0),
    );
    assert!(matches!(
        apply(&wrong_mask),
        Err(Error::Service(TeletextServiceError::CompositionalUsage {
            role: TeletextX27LinkRole::Drcs,
            ..
        }))
    ));
}

#[test]
fn atomic_drcs_page_selection_rechecks_final_glyphs_and_collection_bounds() {
    use TeletextObjectPageSelectionError as Error;
    let original = bind_input(selection_input(false));
    let selected = selections(&original);
    assert!(matches!(
        original
            .clone()
            .with_object_page_selections_and_program_overlays(
                &selected,
                &[consumer_overlay()],
                TeletextServiceBudget {
                    max_structure_items: 3,
                    ..limits()
                },
                budget(),
            ),
        Err(Error::Service(TeletextServiceError::Budget { .. }))
    ));
    let reference_limited = original
        .clone()
        .with_object_page_selections_and_program_overlays(
            &selected,
            &[consumer_overlay()],
            TeletextServiceBudget {
                max_collected_drcs_references: 1,
                ..limits()
            },
            budget(),
        );
    assert!(
        matches!(
            &reference_limited,
            Err(Error::Service(TeletextServiceError::Budget {
                dimension: "collected DRCS reference",
                maximum: 1,
            }))
        ),
        "actual reference admission: {reference_limited:?}"
    );
    assert!(matches!(
        original
            .clone()
            .with_object_page_selections_and_program_overlays(
                &selected,
                &[consumer_overlay()],
                limits(),
                TeletextDeclaredServiceBudget {
                    max_resources: 3,
                    ..budget()
                },
            ),
        Err(Error::Declaration(_))
    ));
    let mut bad_program = consumer(true).commands().to_vec();
    bad_program[1] = TeletextEnhancementCommand::AtColumn {
        column: 4,
        operation: TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                slot: TeletextDrcsGlyphSlot::new(46).unwrap(),
                ..reference(TeletextDrcsTableScope::Normal, true)
            },
        },
    };
    let invalid = TeletextResourceObjectProgramOverlay::new(
        target(0xa0),
        consumer(false),
        TeletextEnhancementProgram::new(bad_program).unwrap(),
        vec![Some(0), Some(1), Some(2)],
        limits(),
    )
    .unwrap();
    assert!(matches!(
        original.with_object_page_selections_and_program_overlays(
            &selected,
            &[invalid],
            limits(),
            budget(),
        ),
        Err(Error::Service(TeletextServiceError::CollectedDrcs {
            error: TeletextCollectedDrcsError {
                error: TeletextDrcsResourceError::MissingGlyph { .. },
                ..
            },
            ..
        }))
    ));
}

#[test]
fn atomic_drcs_page_selection_retains_non_supersedable_mot_page_authority() {
    let original = bind_input(selection_input(false));
    let old = &original.mots()[0];
    let original_links = old.links();
    let mut normal = std::array::from_fn(|index| {
        original_links.drcs_link(Some(
            TeletextMotPublicLinkIndex::new(u8::try_from(index + 1).unwrap()).unwrap(),
        ))
    });
    normal[0] = TeletextMotDrcsLink::new(
        TeletextX27LinkTarget::Page { page: raw(0xb0) },
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
    );
    let global = TeletextMotDrcsLink::new(
        TeletextX27LinkTarget::Page { page: raw(0xb1) },
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
    );
    let links = TeletextMotLevel25Links::new(
        original_links.object_link(None),
        std::array::from_fn(|index| {
            original_links.object_link(Some(
                TeletextMotPublicLinkIndex::new(u8::try_from(index + 1).unwrap()).unwrap(),
            ))
        }),
        global,
        normal,
        original_links.counts(),
    )
    .unwrap();
    let mot = TeletextMotLevel25Declaration::new(
        old.identity(),
        old.associations()
            .iter()
            .map(|association| {
                TeletextMotPageAssociation::new(
                    association.page(),
                    association.objects(),
                    TeletextMotAssociationSelection::new(
                        true,
                        Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
                    ),
                )
            })
            .collect(),
        links,
    )
    .unwrap();
    let original = TeletextDeclaredService::new(
        original.semantic().clone(),
        vec![mot],
        original.selections().to_vec(),
        original.empty_object_pages().to_vec(),
        budget(),
    )
    .unwrap();
    let selected = selections(&original);
    assert!(matches!(
        original.with_object_page_selections_and_program_overlays(
            &selected,
            &[consumer_overlay()],
            limits(),
            budget(),
        ),
        Err(TeletextObjectPageSelectionError::Unresolved {
            role: TeletextX27LinkRole::Drcs,
            ..
        })
    ));
}
