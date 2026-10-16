//! Shared resource changes retain every consumer's captured permissions.
use super::*;
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, lift_teletext_v1_page_set, TeletextResourceFullUpdate,
    TeletextServiceResourceTransmission,
};

fn identity(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(1, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}
fn resource() -> TeletextResourceIdentity {
    identity(TeletextResourceKind::Drcs, 0xc0, 1)
}
fn bitmap(value: u16) -> TeletextDrcsBitmap {
    TeletextDrcsBitmap::new([value; 10]).unwrap()
}
fn change(old: u16, new: u16) -> TeletextDrcsBitmapOverlay {
    TeletextDrcsBitmapOverlay::new(
        resource(),
        TeletextDrcsGlyphSlot::new(0).unwrap(),
        bitmap(old),
        bitmap(new),
    )
    .unwrap()
}
fn edits(bitmaps: &[TeletextDrcsBitmapOverlay]) -> TeletextEditorialEdits<'_> {
    TeletextEditorialEdits {
        rows: &[],
        operands: &[],
        bitmaps,
    }
}
fn mot() -> TeletextMotLevel25Declaration {
    let unused = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(1).unwrap(),
    };
    let object = TeletextMotObjectLink::new(
        unused,
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let inactive =
        TeletextMotDrcsLink::new(unused, TeletextMotSubpageExtent::new(0).unwrap(), false);
    let mut normal = [inactive; 7];
    normal[0] = TeletextMotDrcsLink::new(
        TeletextX27LinkTarget::Page {
            page: resource().page(),
        },
        TeletextMotSubpageExtent::new(1).unwrap(),
        false,
    );
    TeletextMotLevel25Declaration::new(
        identity(TeletextResourceKind::Mot, 0xfe, 0),
        [0x23, 0x24]
            .map(|page| {
                TeletextMotPageAssociation::new(
                    page,
                    TeletextMotAssociationSelection::new(false, None),
                    TeletextMotAssociationSelection::new(
                        false,
                        Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
                    ),
                )
            })
            .into(),
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            inactive,
            normal,
            TeletextMotEnhancementCounts::new(1, 0, 2),
        )
        .unwrap(),
    )
    .unwrap()
}
fn drcs_command() -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 8,
        operation: TeletextColumnOperation::Drcs {
            reference: TeletextDrcsGlyphReference {
                table: TeletextDrcsTableScope::Normal,
                subtable: TeletextDrcsSubtableIndex::new(1).unwrap(),
                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
            },
        },
    }
}
fn add_panel_consumer(
    page: &mut TeletextResolvedPageEnhancementDraft,
    commands: &mut Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    add_panel_consumer_at(page, commands, 32)
}
fn add_panel_consumer_at(
    page: &mut TeletextResolvedPageEnhancementDraft,
    commands: &mut Vec<TeletextEnhancementCommand>,
    offset: u8,
) -> TeletextEnhancementObjectDefinition {
    page.presentation.as_mut().unwrap().panels = TeletextSidePanels {
        left: true,
        right: true,
        status: TeletextPanelStatus::Level25And35,
        width: TeletextPanelWidth::new(4).unwrap(),
    };
    let target = TeletextObjectIdentity::local(page.address, TeletextObjectId::new(1));
    commands.push(TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        // Original active column 8 plus offset selects the raw panel address.
        offset: Some(TeletextObjectOffset::new(0, offset).unwrap()),
    });
    let TeletextEnhancementCommand::AtColumn { operation, .. } = drcs_command() else {
        unreachable!()
    };
    TeletextEnhancementObjectDefinition {
        identity: target,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::AtColumn {
            column: 0,
            operation,
        }])
        .unwrap(),
    }
}
fn declared(value: u16, other: u16, panel: bool) -> TeletextDeclaredService {
    declared_with_panel_offset(value, other, panel.then_some(32))
}
fn declared_with_panel_offset(
    value: u16,
    other: u16,
    panel_offset: Option<u8>,
) -> TeletextDeclaredService {
    let policy = policy();
    let lifted = lift_teletext_v1_page_set(
        draft("ASCII"),
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let mut pages: Vec<_> = lifted.page_enhancements().values().cloned().collect();
    let mut objects = vec![];
    for page in &mut pages {
        page.resources.drcs = TeletextDrcsPageLinks::new(Some(resource().page()), None).unwrap();
        let mut commands = vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            drcs_command(),
        ];
        if let Some(offset) = panel_offset.filter(|_| page.address == address(124)) {
            objects.push(if offset == 32 {
                add_panel_consumer(page, &mut commands)
            } else {
                add_panel_consumer_at(page, &mut commands, offset)
            });
        }
        page.local_program = Some(TeletextEnhancementProgram::new(commands).unwrap());
    }
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects,
            drcs: vec![
                // MOT extent 1 declares every bank subpage 0..=1. The empty
                // companion preserves that valid inventory while references
                // and edit intentions deliberately retain their native S1=1.
                TeletextDrcsSubtable::new(identity(TeletextResourceKind::Drcs, 0xc0, 0), vec![])
                    .unwrap(),
                TeletextDrcsSubtable::new(
                    resource(),
                    [value, other]
                        .into_iter()
                        .enumerate()
                        .map(|(slot, value)| TeletextDrcsGlyphDefinition {
                            slot: TeletextDrcsGlyphSlot::new(u8::try_from(slot).unwrap()).unwrap(),
                            bitmap: bitmap(value),
                        })
                        .collect(),
                )
                .unwrap(),
            ],
        },
        policy.validation.service,
    )
    .unwrap();
    let mot = mot();
    let selections = [123, 124]
        .map(|page| TeletextDisplayMotSelection {
            display: address(page),
            mot: Some(mot.identity()),
        })
        .into();
    TeletextDeclaredService::new(
        semantic,
        vec![mot],
        selections,
        vec![],
        policy.validation.declarations,
    )
    .unwrap()
}
fn compile(value: u16, other: u16, panel: bool, counter: u8) -> EncodedTeletextAuthoredService {
    compile_declared(declared(value, other, panel), counter)
}
fn compile_declared(input: TeletextDeclaredService, counter: u8) -> EncodedTeletextAuthoredService {
    let tx: Vec<_> = input
        .resource_inventory()
        .iter()
        .map(|resource| TeletextServiceResourceTransmission {
            resource: *resource,
            counter: TeletextResourceUpdateCounter::new(counter).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect();
    encode_teletext_authored_service_before(input, &tx, &policy().encoding, deadline()).unwrap()
}

fn unused_resource(value: u16) -> EncodedTeletextAuthoredService {
    let original = declared(value, 0x123, false);
    let semantic = original.semantic();
    let pages = semantic
        .page_enhancements()
        .values()
        .cloned()
        .map(|mut page| {
            page.local_program = None;
            page
        })
        .collect();
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: semantic.base().clone(),
            pages,
            magazines: vec![],
            objects: vec![],
            drcs: semantic.drcs().subtables().to_vec(),
        },
        policy().validation.service,
    )
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        original.mots().to_vec(),
        original.selections().to_vec(),
        vec![],
        policy().validation.declarations,
    )
    .unwrap();
    assert!(declared
        .semantic()
        .page_enhancements()
        .values()
        .all(|page| page.local_program.is_none()));
    assert!(declared.semantic().objects().definitions().is_empty());
    compile_declared(declared, 1)
}

#[test]
fn unused_resource_bitmap_edit_does_not_spend_command_quota() {
    let before = unused_resource(0x801);
    let after = unused_resource(0x803);
    let changes = [change(0x801, 0x803)];
    let mut budget = policy().encoding;
    budget.max_commands = 0;
    // No display or object command references this resource; changing a
    // published bitmap consumes no semantic command and has no visual effects.
    validate_teletext_editorial_transition_with_operands(
        &before,
        &after,
        &permissions(false),
        edits(&changes),
        edits(&changes),
        &budget,
        deadline(),
    )
    .unwrap();
}
fn permissions(include_second: bool) -> TeletextExportProject {
    let mut result = project("zone", 1, 8, 1, 1);
    if include_second {
        let mut mapping = result.pages[0].clone();
        mapping.page.page = 124;
        mapping.name = "p124.tti".into();
        mapping.editor_page = 292;
        mapping.editor_ordinal = 1;
        result.pages.push(mapping);
        let pages = &mut result.producers[0].authoring.as_mut().unwrap().pages;
        let mut page = pages[0].clone();
        page.page.page = 124;
        pages.push(page);
    }
    result
}
fn validate_change(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    incoming: &[TeletextDrcsBitmapOverlay],
    cumulative: &[TeletextDrcsBitmapOverlay],
) -> Result<()> {
    validate_teletext_editorial_transition_with_operands(
        before,
        after,
        project,
        edits(incoming),
        edits(cumulative),
        &policy().encoding,
        deadline(),
    )
}

#[test]
fn exact_bitmap_change_is_allowed_only_when_both_shared_consumers_are_editable() {
    let before = compile(0x801, 0x123, false, 1);
    let after = compile(0x803, 0x123, false, 1);
    let changes = [change(0x801, 0x803)];
    assert_eq!(
        before.pages()[0].data_units(),
        after.pages()[0].data_units()
    );
    assert_eq!(
        before.pages()[1].data_units(),
        after.pages()[1].data_units()
    );
    validate_change(&before, &after, &permissions(true), &changes, &changes).unwrap();
    for mut project in [permissions(false), permissions(true)] {
        if project.pages.len() == 2 {
            project.producers[0].authoring.as_mut().unwrap().pages[1]
                .regions
                .clear();
        }
        let failure = validate_change(&before, &after, &project, &changes, &changes).unwrap_err();
        assert_eq!(
            failure.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(failure.page, Some(address(124)));
        assert_eq!(failure.position, Some((1, 8)));
    }
}

#[test]
fn shared_panel_consumer_is_protected_even_when_both_main_cells_are_editable() {
    let before = compile(0x801, 0x123, true, 1);
    let after = compile(0x803, 0x123, true, 1);
    let composition = display(&after.pages()[1]).unwrap().1;
    assert_eq!(composition.display_geometry().total_width(), 56);
    let cell = composition.display_resolution().cell(1, 44).unwrap();
    assert_eq!(
        project_teletext_comparison_glyph(composition, cell)
            .unwrap()
            .mask
            .unwrap()
            .rows(),
        [0x803; 10]
    );
    let changes = [change(0x801, 0x803)];
    let failure =
        validate_change(&before, &after, &permissions(true), &changes, &changes).unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(failure.page, Some(address(124)));
    assert_eq!(failure.position, Some((1, 44)));
}

fn assert_captured_panel_bitmap_zone(
    offset: u8,
    area: TeletextAuthoringRegionAreaWire,
    raw_column: u8,
    visual_column: u8,
) {
    let before = compile_declared(declared_with_panel_offset(0x801, 0x123, Some(offset)), 1);
    let after = compile_declared(declared_with_panel_offset(0x803, 0x123, Some(offset)), 1);
    assert_eq!(
        before.declared().semantic().base(),
        after.declared().semantic().base()
    );
    for (old, new) in before.pages().iter().zip(after.pages()) {
        if let (Some((old_page, old)), Some((new_page, new))) = (display(old), display(new)) {
            // The changed resource never rewrites either display's fallback,
            // header, native X26 or selected presentation packets.
            assert_eq!(old_page.data_units(), new_page.data_units());
            assert_eq!(old.display_geometry(), new.display_geometry());
        }
    }
    for (page, column) in [(0, 8), (1, 12), (1, visual_column)] {
        for (service, expected) in [(&before, 0x801), (&after, 0x803)] {
            let composition = display(&service.pages()[page]).unwrap().1;
            let cell = composition.display_resolution().cell(1, column).unwrap();
            assert_eq!(
                project_teletext_comparison_glyph(composition, cell)
                    .unwrap()
                    .mask
                    .unwrap()
                    .rows(),
                [expected; 10]
            );
        }
    }
    let composition = display(&before.pages()[1]).unwrap().1;
    let geometry = composition.display_geometry();
    assert_eq!(
        (
            geometry.left_width(),
            geometry.right_width(),
            geometry.total_width()
        ),
        (4, 12, 56)
    );
    assert!(geometry.records().iter().any(|record| {
        record.candidate().position() == (1, u16::from(raw_column))
            && record.area()
                == match area {
                    TeletextAuthoringRegionAreaWire::LeftPanel => TeletextRawDisplayArea::LeftPanel,
                    TeletextAuthoringRegionAreaWire::RightPanel => {
                        TeletextRawDisplayArea::RightPanel
                    }
                    TeletextAuthoringRegionAreaWire::Main => unreachable!(),
                }
            && record.placement()
                == TeletextCandidatePlacement::Placed {
                    row: 1,
                    column: visual_column,
                }
    }));
    let mut project = permissions(true);
    let page = &mut project.producers[0].authoring.as_mut().unwrap().pages[1];
    let mut zone = page.regions[0].clone();
    zone.id = "panel-edit".into();
    zone.area = area;
    zone.column = 0;
    page.regions.push(zone);
    let changes = [change(0x801, 0x803)];
    validate_change(&before, &after, &project, &changes, &changes).unwrap();

    assert_captured_panel_controls(&before, &after, &project, area, visual_column, &changes);
}

fn assert_captured_panel_controls(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    area: TeletextAuthoringRegionAreaWire,
    visual_column: u8,
    changes: &[TeletextDrcsBitmapOverlay],
) {
    // A permission in another panel cannot authorize this consumer.
    let other_area = match area {
        TeletextAuthoringRegionAreaWire::LeftPanel => TeletextAuthoringRegionAreaWire::RightPanel,
        TeletextAuthoringRegionAreaWire::RightPanel => TeletextAuthoringRegionAreaWire::LeftPanel,
        TeletextAuthoringRegionAreaWire::Main => unreachable!(),
    };
    for (kind, column, region_area) in [
        (TeletextAuthoringRegionKindWire::Generated, 0, area),
        (TeletextAuthoringRegionKindWire::Zone, 1, area),
        (TeletextAuthoringRegionKindWire::Zone, 0, other_area),
    ] {
        let mut denied = project.clone();
        let zone = &mut denied.producers[0].authoring.as_mut().unwrap().pages[1].regions[1];
        zone.kind = kind;
        zone.column = column;
        zone.area = region_area;
        let failure = validate_change(before, after, &denied, changes, changes).unwrap_err();
        assert_eq!(
            failure.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(failure.page, Some(address(124)));
        assert_eq!(failure.position, Some((1, visual_column)));
    }
    for page in 0..2 {
        let mut denied = project.clone();
        denied.producers[0].authoring.as_mut().unwrap().pages[page]
            .regions
            .remove(0);
        let failure = validate_change(before, after, &denied, changes, changes).unwrap_err();
        assert_eq!(
            failure.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(
            failure.page,
            Some(address(if page == 0 { 123 } else { 124 }))
        );
        assert_eq!(failure.position, Some((1, if page == 0 { 8 } else { 12 })));
    }

    let mut overflow = project.clone();
    overflow.producers[0].authoring.as_mut().unwrap().pages[1].regions[1].width =
        if area == TeletextAuthoringRegionAreaWire::RightPanel {
            13
        } else {
            5
        };
    let failure = validate_change(before, after, &overflow, changes, changes).unwrap_err();
    assert_eq!(failure.reason, TeletextEditorialEffectsFailure::Association);
    assert_eq!(failure.page, Some(address(124)));
}

#[test]
fn shared_panel_bitmap_change_uses_captured_right_panel_zone() {
    assert_captured_panel_bitmap_zone(32, TeletextAuthoringRegionAreaWire::RightPanel, 40, 44);
}

#[test]
fn shared_panel_bitmap_change_uses_captured_left_panel_zone() {
    assert_captured_panel_bitmap_zone(60, TeletextAuthoringRegionAreaWire::LeftPanel, 68, 0);
}

#[test]
fn latest_bitmap_expectation_repeated_edit_and_explicit_root_revert_are_distinct() {
    let latest = compile(0x803, 0x123, false, 1);
    let after = compile(0x807, 0x123, false, 1);
    validate_change(
        &latest,
        &after,
        &permissions(true),
        &[change(0x803, 0x807)],
        &[change(0x801, 0x807)],
    )
    .unwrap();
    validate_change(
        &latest,
        &compile(0x801, 0x123, false, 1),
        &permissions(true),
        &[change(0x803, 0x801)],
        &[],
    )
    .unwrap();
    assert_eq!(
        validate_change(
            &latest,
            &after,
            &permissions(true),
            &[change(0x801, 0x807)],
            &[change(0x801, 0x807)]
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::Association
    );
}

#[test]
fn stale_duplicate_wrong_identity_and_final_association_are_rejected() {
    let before = compile(0x801, 0x123, false, 1);
    let after = compile(0x803, 0x123, false, 1);
    let valid = change(0x801, 0x803);
    let wrong = TeletextDrcsBitmapOverlay::new(
        identity(TeletextResourceKind::Drcs, 0xc0, 0),
        valid.slot(),
        valid.expected(),
        valid.replacement(),
    )
    .unwrap();
    for (incoming, cumulative) in [
        (vec![valid, valid], vec![valid]),
        (vec![wrong], vec![valid]),
        (vec![change(0x802, 0x803)], vec![valid]),
        (vec![valid], vec![valid, valid]),
        (vec![valid], vec![change(0x801, 0x807)]),
    ] {
        assert_eq!(
            validate_change(&before, &after, &permissions(true), &incoming, &cumulative)
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::Association
        );
    }
}

#[test]
fn unauthorized_bitmap_resource_header_and_function_changes_are_rejected() {
    let before = compile(0x801, 0x123, false, 1);
    let changes = [change(0x801, 0x803)];
    for after in [
        compile(0x803, 0x125, false, 1),
        compile(0x803, 0x123, false, 2),
    ] {
        assert_eq!(
            validate_change(&before, &after, &permissions(true), &changes, &changes)
                .unwrap_err()
                .reason,
            TeletextEditorialEffectsFailure::ProtectedStructure
        );
    }
    // A different resource function is never an authorized bitmap target, even
    // if it uses the same raw page, S1, slot and bitmap values.
    let wrong = TeletextDrcsBitmapOverlay::new(
        identity(TeletextResourceKind::Gdrcs, 0xc0, 1),
        changes[0].slot(),
        changes[0].expected(),
        changes[0].replacement(),
    )
    .unwrap();
    assert_eq!(
        validate_change(
            &before,
            &compile(0x803, 0x123, false, 1),
            &permissions(true),
            &[wrong],
            &changes
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::Association
    );
    assert_eq!(
        validate_change(
            &before,
            &compile(0x803, 0x123, false, 1),
            &permissions(true),
            &[],
            &[]
        )
        .unwrap_err()
        .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
}
