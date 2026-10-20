//! Literal raw-to-visual geometry; no footprint/raster/publication oracle.

use super::*;

pub(super) fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}
pub(super) fn panels(
    left: bool,
    right: bool,
    width: u8,
    status: TeletextPanelStatus,
) -> TeletextSidePanels {
    TeletextSidePanels {
        left,
        right,
        width: TeletextPanelWidth::new(width).unwrap(),
        status,
    }
}
pub(super) fn presentation(panels: TeletextSidePanels) -> TeletextLevel25PresentationDefaults {
    TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(0).unwrap(),
        secondary: None,
        palette: TeletextLevel25Palette::standard(),
        screen_colour: TeletextColourReference::BLACK,
        row_colour: TeletextColourReference::BLACK,
        black_background_substitution: false,
        remapping: TeletextColourRemapping::FgClut0BgClut0,
        panels,
    }
}
fn base(
    owner: TeletextPageAddress,
    presentation: Option<TeletextLevel25PresentationDefaults>,
) -> TeletextBaseDisplay {
    let rows: Vec<[TeletextBaseDisplayCell; 40]> = (0..25)
        .map(|row| {
            let source = TeletextBaseAttributeSource::RowDefault {
                row,
                start_column: if row == 0 { 8 } else { 0 },
            };
            let attributes = TeletextBaseDisplayAttributes::new(
                TeletextBaseAttributeValues {
                    raw_foreground: 7,
                    raw_background: 0,
                    foreground: TeletextColourReference::new(7).unwrap(),
                    background: TeletextColourReference::BLACK,
                    graphics: false,
                    separated: false,
                    double_height: false,
                    double_width: false,
                    boxed: false,
                    flash: false,
                    conceal: false,
                },
                TeletextBaseAttributeSources {
                    foreground: source,
                    background: source,
                    graphics: source,
                    separated: source,
                    height: source,
                    width: source,
                    boxed: source,
                    flash: source,
                    conceal: source,
                },
            )
            .unwrap();
            let cells: Vec<_> = (0..40)
                .map(|column| {
                    let position = TeletextEnhancementPosition::new(row, column).unwrap();
                    let authored = (row == 1 && column == 8).then(|| {
                        TeletextBaseAuthoredGlyph::new(
                            position,
                            8,
                            TeletextStyle::default(),
                            TeletextBaseAuthoredContent::Text { character: 'é' },
                        )
                        .unwrap()
                    });
                    let kind = if row == 0 && column < 8 {
                        TeletextBaseDisplayCellKind::HeaderOwned
                    } else if authored.is_some() {
                        TeletextBaseDisplayCellKind::Glyph
                    } else {
                        TeletextBaseDisplayCellKind::Blank
                    };
                    TeletextBaseDisplayCell::new(
                        position,
                        if authored.is_some() { b'e' } else { b' ' },
                        kind,
                        authored,
                        attributes,
                    )
                    .unwrap()
                })
                .collect();
            cells.try_into().unwrap()
        })
        .collect();
    TeletextBaseDisplay::new(
        owner,
        TeletextPageCharsetDesignation::new(0).unwrap(),
        TeletextNationalOption::new(0).unwrap(),
        presentation,
        rows.into_boxed_slice().try_into().unwrap(),
    )
    .unwrap()
}
pub(super) fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}
pub(super) fn set(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
pub(super) fn glyph(column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(b'A').unwrap(),
        },
    }
}
pub(super) fn invoke(
    target: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    offset: Option<(u8, u8)>,
) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type,
        offset: offset.map(|(row, column)| TeletextObjectOffset::new(row, column).unwrap()),
    }
}
pub(super) fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}
pub(super) struct Fixture {
    pub(super) base: TeletextBaseDisplay,
    pub(super) spatial: TeletextSpatialTrace,
    pub(super) glyphs: TeletextGlyphLayers,
}
pub(super) fn fixture(
    presentation: Option<TeletextLevel25PresentationDefaults>,
    commands: Vec<TeletextEnhancementCommand>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
) -> Fixture {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    let service = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: language.clone(),
                initial_page: owner().page,
                pages: vec![TeletextPageDraft {
                    address: owner(),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(0).unwrap(),
                    rows: vec![],
                    navigation: vec![],
                }],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation,
                compositional_links: None,
                local_program: Some(TeletextEnhancementProgram::new(commands).unwrap()),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    defaults: [None; 4],
                    drcs: TeletextDrcsPageLinks::unlinked(),
                },
            }],
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 16,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 4096,
            max_collected_commands: 4096,
            max_collected_drcs_references: 128,
        },
    )
    .unwrap();
    let base = base(owner(), presentation);
    let spatial = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 128,
            max_invocations: 32,
        },
    )
    .unwrap();
    let glyphs = build_teletext_glyph_layers(&base, &spatial, 128).unwrap();
    Fixture {
        base,
        spatial,
        glyphs,
    }
}
fn samples(panels: TeletextSidePanels, row: u8, columns: &[u8]) -> Fixture {
    let target = local(0xffff);
    let beyond = local(0x0100);
    let commands = columns
        .iter()
        .map(|&column| {
            invoke(
                if column <= 71 { target } else { beyond },
                TeletextObjectType::Passive,
                Some((row, column.min(71))),
            )
        })
        .collect();
    fixture(
        Some(presentation(panels)),
        commands,
        vec![
            definition(target, TeletextObjectType::Passive, vec![glyph(0)]),
            definition(beyond, TeletextObjectType::Passive, vec![glyph(1)]),
        ],
    )
}
pub(super) fn resolve(fixture: &Fixture) -> TeletextDisplayGeometry {
    resolve_teletext_display_geometry(
        &fixture.base,
        &fixture.spatial,
        &fixture.glyphs,
        TeletextDisplayGeometryBudget {
            max_candidates: fixture.glyphs.candidates().len(),
            max_events: fixture.spatial.events().len(),
            max_invocations: fixture.spatial.invocations().len(),
        },
    )
    .unwrap()
}
fn enhanced_at(
    geometry: &TeletextDisplayGeometry,
    row: u16,
    column: u16,
) -> &TeletextPlacedGlyphCandidate {
    geometry
        .records()
        .iter()
        .find(|record| {
            record.candidate().position() == (row, column)
                && record.candidate().operation().is_some()
        })
        .unwrap()
}
fn unplaced(reason: TeletextCandidateNonplacement) -> TeletextCandidatePlacement {
    TeletextCandidatePlacement::Unplaced(reason)
}

#[test]
fn both_panels_split_four_and_twelve_map_raw_boundaries_without_visual_sorting() {
    let raw = panels(true, true, 4, TeletextPanelStatus::Level25And35);
    let fixture = samples(raw, 1, &[0, 39, 40, 51, 52, 55, 56, 67, 68, 71, 72]);
    let geometry = resolve(&fixture);
    assert_eq!(
        (
            geometry.left_width(),
            geometry.right_width(),
            geometry.total_width()
        ),
        (4, 12, 56)
    );
    assert_eq!(geometry.panels(), Some(raw));
    for (column, area, visual) in [
        (0, TeletextRawDisplayArea::Main, 4),
        (39, TeletextRawDisplayArea::Main, 43),
        (40, TeletextRawDisplayArea::RightPanel, 44),
        (51, TeletextRawDisplayArea::RightPanel, 55),
        (68, TeletextRawDisplayArea::LeftPanel, 0),
        (71, TeletextRawDisplayArea::LeftPanel, 3),
    ] {
        let record = enhanced_at(&geometry, 1, column);
        assert_eq!(record.area(), area);
        assert_eq!(
            record.placement(),
            TeletextCandidatePlacement::Placed {
                row: 1,
                column: visual
            }
        );
    }
    for column in [52, 55, 56, 67] {
        assert_eq!(
            enhanced_at(&geometry, 1, column).placement(),
            unplaced(TeletextCandidateNonplacement::OutsideActiveWidth)
        );
    }
    assert_eq!(
        enhanced_at(&geometry, 1, 72).area(),
        TeletextRawDisplayArea::Outside
    );
    assert_eq!(
        enhanced_at(&geometry, 1, 72).placement(),
        unplaced(TeletextCandidateNonplacement::OutsideDisplay)
    );
    // Output is candidate-aligned RAW order: rightpanel40 precedes leftpanel68.
    for (record, candidate) in geometry.records().iter().zip(fixture.glyphs.candidates()) {
        assert_eq!(record.candidate(), candidate);
    }
}

#[test]
fn left_only_zero_means_sixteen_and_keeps_disabled_right_area_distinct() {
    let geometry = resolve(&samples(
        panels(true, false, 0, TeletextPanelStatus::Level25And35),
        1,
        &[0, 39, 40, 55, 56, 71],
    ));
    assert_eq!(
        (
            geometry.left_width(),
            geometry.right_width(),
            geometry.total_width()
        ),
        (16, 0, 56)
    );
    for (raw, visual) in [(0, 16), (39, 55), (56, 0), (71, 15)] {
        assert_eq!(
            enhanced_at(&geometry, 1, raw).placement(),
            TeletextCandidatePlacement::Placed {
                row: 1,
                column: visual
            }
        );
    }
    for raw in [40, 55] {
        assert_eq!(
            enhanced_at(&geometry, 1, raw).placement(),
            unplaced(TeletextCandidateNonplacement::DisabledPanel)
        );
    }
}

#[test]
fn right_only_raw_five_resolves_eleven_without_enabling_unused_left_gap() {
    let raw = panels(false, true, 5, TeletextPanelStatus::Level25And35);
    let geometry = resolve(&samples(raw, 1, &[0, 39, 40, 50, 51, 55, 56, 71]));
    assert_eq!(
        (
            geometry.left_width(),
            geometry.right_width(),
            geometry.total_width()
        ),
        (0, 11, 51)
    );
    assert_eq!(geometry.panels().unwrap().width.get(), 5);
    for column in [0, 39, 40, 50] {
        assert_eq!(
            enhanced_at(&geometry, 1, column).placement(),
            TeletextCandidatePlacement::Placed {
                row: 1,
                column: u8::try_from(column).unwrap()
            }
        );
    }
    for column in [51, 55] {
        assert_eq!(
            enhanced_at(&geometry, 1, column).placement(),
            unplaced(TeletextCandidateNonplacement::OutsideActiveWidth)
        );
    }
    for column in [56, 71] {
        assert_eq!(
            enhanced_at(&geometry, 1, column).placement(),
            unplaced(TeletextCandidateNonplacement::DisabledPanel)
        );
    }
}

#[test]
fn disabled_and_level35_only_metadata_survives_without_new_level35_support() {
    for raw in [
        panels(false, false, 13, TeletextPanelStatus::Level25And35),
        panels(true, true, 4, TeletextPanelStatus::Level35Only),
    ] {
        let geometry = resolve(&samples(raw, 1, &[0, 40, 68]));
        assert_eq!(geometry.panels(), Some(raw));
        assert_eq!(
            (
                geometry.left_width(),
                geometry.right_width(),
                geometry.total_width()
            ),
            (0, 0, 40)
        );
        assert_eq!(
            enhanced_at(&geometry, 1, 0).placement(),
            TeletextCandidatePlacement::Placed { row: 1, column: 0 }
        );
        for column in [40, 68] {
            assert_eq!(
                enhanced_at(&geometry, 1, column).placement(),
                unplaced(TeletextCandidateNonplacement::DisabledPanel)
            );
        }
    }
    let absent = resolve(&fixture(None, vec![], vec![]));
    assert_eq!(absent.panels(), None);
    assert_eq!(absent.total_width(), 40);
}

#[test]
fn header_owned_cells_and_header_panels_are_explicitly_unplaced() {
    let geometry = resolve(&samples(
        panels(true, true, 4, TeletextPanelStatus::Level25And35),
        0,
        &[0, 7, 8, 39, 40, 68, 71],
    ));
    for column in [0, 7] {
        assert_eq!(
            enhanced_at(&geometry, 0, column).placement(),
            unplaced(TeletextCandidateNonplacement::HeaderOwned)
        );
    }
    for (raw, visual) in [(8, 12), (39, 43)] {
        assert_eq!(
            enhanced_at(&geometry, 0, raw).placement(),
            TeletextCandidatePlacement::Placed {
                row: 0,
                column: visual
            }
        );
    }
    for column in [40, 68, 71] {
        assert_eq!(
            enhanced_at(&geometry, 0, column).placement(),
            unplaced(TeletextCandidateNonplacement::HeaderPanel)
        );
    }
    assert!(!geometry
        .records()
        .iter()
        .any(|record| record.candidate().base().is_some()
            && record.candidate().position().0 == 0
            && record.candidate().position().1 < 8));
    assert_eq!(
        geometry
            .records()
            .iter()
            .filter(|record| record.candidate().base().is_some())
            .count(),
        992
    );
}

fn repeated() -> Fixture {
    let target = local(0xffff);
    fixture(
        Some(presentation(panels(
            true,
            true,
            4,
            TeletextPanelStatus::Level25And35,
        ))),
        vec![
            set(1, 8),
            glyph(8),
            invoke(target, TeletextObjectType::Passive, None),
            invoke(target, TeletextObjectType::Passive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![glyph(0)],
        )],
    )
}

#[test]
fn losing_sources_repeated_instances_and_original_unicode_base_survive_placement() {
    let fixture = repeated();
    let geometry = resolve(&fixture);
    assert_eq!(geometry.records().len(), 995);
    let records: Vec<_> = geometry
        .records()
        .iter()
        .filter(|record| record.candidate().position() == (1, 8))
        .collect();
    assert_eq!(records.len(), 4);
    assert_eq!(
        records[0].candidate().priority(),
        TeletextGlyphPriority::Base
    );
    let base = records[0].candidate().base().unwrap();
    assert_eq!(base.code(), b'e');
    assert_eq!(
        base.authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text { character: 'é' }
    );
    assert_eq!(
        records[1].candidate().priority(),
        TeletextGlyphPriority::Local
    );
    for (record, instance, root) in [(&records[2], 0, 2), (&records[3], 1, 3)] {
        assert_eq!(
            record.candidate().source(),
            Some(TeletextEnhancementSource {
                object: Some(local(0xffff)),
                command: 0
            })
        );
        assert_eq!(
            record.candidate().root_source(),
            Some(TeletextSpatialRootSource::Local { command: root })
        );
        assert_eq!(record.candidate().invocation().unwrap().get(), instance);
    }
    for record in records {
        assert_eq!(
            record.placement(),
            TeletextCandidatePlacement::Placed { row: 1, column: 12 }
        );
    }
    for (index, record) in geometry.records().iter().enumerate() {
        assert_eq!(record.candidate_index(), index);
        assert_eq!(record.candidate(), &fixture.glyphs.candidates()[index]);
    }
}

#[test]
fn wide_nested_coordinates_are_nonplacement_not_clamped_or_dropped() {
    let active = local(0xffff);
    let adaptive = local(0x0100);
    let passive = local(0);
    let below = local(1);
    let fixture = fixture(
        Some(presentation(panels(
            true,
            true,
            4,
            TeletextPanelStatus::Level25And35,
        ))),
        vec![
            set(24, 39),
            invoke(active, TeletextObjectType::Active, Some((23, 71))),
            invoke(below, TeletextObjectType::Passive, Some((1, 0))),
        ],
        vec![
            definition(
                active,
                TeletextObjectType::Active,
                vec![
                    set(24, 39),
                    invoke(adaptive, TeletextObjectType::Adaptive, Some((23, 71))),
                ],
            ),
            definition(
                adaptive,
                TeletextObjectType::Adaptive,
                vec![
                    set(24, 39),
                    invoke(passive, TeletextObjectType::Passive, Some((23, 71))),
                ],
            ),
            definition(
                passive,
                TeletextObjectType::Passive,
                vec![set(24, 0), glyph(39)],
            ),
            definition(below, TeletextObjectType::Passive, vec![glyph(0)]),
        ],
    );
    let geometry = resolve(&fixture);
    let record = enhanced_at(&geometry, 165, 369);
    assert_eq!(record.area(), TeletextRawDisplayArea::Outside);
    assert_eq!(
        record.placement(),
        unplaced(TeletextCandidateNonplacement::OutsideDisplay)
    );
    assert_eq!(
        record.candidate().source(),
        Some(TeletextEnhancementSource {
            object: Some(passive),
            command: 1
        })
    );
    assert_eq!(
        record.candidate().root_source(),
        Some(TeletextSpatialRootSource::Local { command: 1 })
    );
    assert_eq!(record.candidate().invocation().unwrap().get(), 2);
    let region = &geometry.object_regions()[2];
    assert_eq!(
        region.area_pending(),
        Some(TeletextObjectAreaPending::UnplacedAddress)
    );
    assert_eq!(
        region.interval(),
        TeletextObjectAddressInterval::Pending {
            reason: TeletextObjectIntervalPending::OutsideDisplay,
            endpoint: Some((165, 369)),
        }
    );
    assert_eq!(geometry.direct_writes(region.instance().id()).len(), 1);
    // Valid main-area column39 must not conceal a separately invalid row25.
    let below = enhanced_at(&geometry, 25, 39);
    assert_eq!(below.area(), TeletextRawDisplayArea::Main);
    assert_eq!(
        below.placement(),
        unplaced(TeletextCandidateNonplacement::OutsideDisplay)
    );
}

#[test]
fn three_exact_budgets_blame_original_candidates_events_and_calls_without_mutation() {
    let fixture = repeated();
    let before = (
        fixture.base.clone(),
        fixture.spatial.clone(),
        fixture.glyphs.clone(),
    );
    let budget = TeletextDisplayGeometryBudget {
        max_candidates: 995,
        max_events: 6,
        max_invocations: 2,
    };
    assert_eq!(
        resolve_teletext_display_geometry(&fixture.base, &fixture.spatial, &fixture.glyphs, budget)
            .unwrap()
            .records()
            .len(),
        995
    );
    let failure = |budget| {
        resolve_teletext_display_geometry(&fixture.base, &fixture.spatial, &fixture.glyphs, budget)
            .unwrap_err()
    };
    let candidate = failure(TeletextDisplayGeometryBudget {
        max_candidates: 994,
        ..budget
    });
    assert_eq!(
        candidate.reason,
        TeletextDisplayGeometryFailure::CandidateBudget {
            count: 995,
            maximum: 994
        }
    );
    assert_eq!(candidate.position, Some((24, 39)));
    assert_eq!(candidate.location, None);
    let event = failure(TeletextDisplayGeometryBudget {
        max_events: 5,
        ..budget
    });
    assert_eq!(
        event.reason,
        TeletextDisplayGeometryFailure::EventBudget {
            count: 6,
            maximum: 5
        }
    );
    assert_eq!(event.position, Some((1, 8)));
    assert_eq!(
        event.location,
        Some(TeletextEnhancementSource {
            object: Some(local(0xffff)),
            command: 0
        })
    );
    assert_eq!(
        event.root_source,
        Some(TeletextSpatialRootSource::Local { command: 3 })
    );
    assert_eq!(event.invocation.unwrap().get(), 1);
    let call = failure(TeletextDisplayGeometryBudget {
        max_invocations: 1,
        ..budget
    });
    assert_eq!(
        call.reason,
        TeletextDisplayGeometryFailure::InvocationBudget {
            count: 2,
            maximum: 1
        }
    );
    assert_eq!(call.position, Some((1, 8)));
    assert_eq!(
        call.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 3
        })
    );
    assert_eq!(
        call.root_source,
        Some(TeletextSpatialRootSource::Local { command: 3 })
    );
    assert_eq!(call.invocation.unwrap().get(), 1);
    assert_eq!((fixture.base, fixture.spatial, fixture.glyphs), before);
}

#[test]
fn foreign_exact_subcode_and_changed_same_owner_events_cannot_reuse_geometry_candidates() {
    let original = repeated();
    let other = TeletextPageAddress {
        subcode: TeletextSubcode::new(0x1235).unwrap(),
        ..owner()
    };
    let foreign = base(other, original.base.presentation());
    let budget = TeletextDisplayGeometryBudget {
        max_candidates: 1024,
        max_events: 128,
        max_invocations: 32,
    };
    let error =
        resolve_teletext_display_geometry(&foreign, &original.spatial, &original.glyphs, budget)
            .unwrap_err();
    assert_eq!(error.owner, other);
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::OwnerMismatch { actual: owner() }
    );
    // Same owner/source ordinal is insufficient: changed original glyph position
    // must reject correspondence, rather than relocate the previous contender.
    let changed = fixture(
        original.base.presentation(),
        vec![set(1, 8), glyph(9)],
        vec![],
    );
    let error = resolve_teletext_display_geometry(
        &original.base,
        &changed.spatial,
        &original.glyphs,
        budget,
    )
    .unwrap_err();
    let index = original
        .glyphs
        .candidates()
        .iter()
        .position(|candidate| {
            candidate.source()
                == Some(TeletextEnhancementSource {
                    object: None,
                    command: 1,
                })
        })
        .unwrap();
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::CandidateMismatch { candidate: index }
    );
    assert_eq!(error.position, Some((1, 8)));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
}

#[test]
fn last_direct_non_glyph_address_sets_interval_and_child_panel_does_not_extend_parent() {
    let parent = local(0xffff);
    let child = local(0x0100);
    let fixture = fixture(
        Some(presentation(panels(
            true,
            true,
            4,
            TeletextPanelStatus::Level25And35,
        ))),
        vec![set(1, 8), invoke(parent, TeletextObjectType::Active, None)],
        vec![
            definition(
                parent,
                TeletextObjectType::Active,
                vec![
                    glyph(0),
                    invoke(child, TeletextObjectType::Adaptive, Some((0, 32))),
                    TeletextEnhancementCommand::AtColumn {
                        column: 1,
                        operation: TeletextColumnOperation::DesignateCharset {
                            designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
                        },
                    },
                ],
            ),
            definition(child, TeletextObjectType::Adaptive, vec![glyph(0)]),
        ],
    );
    let geometry = resolve(&fixture);
    assert_eq!(geometry.object_regions().len(), 2);
    let parent_region = &geometry.object_regions()[0];
    let child_region = &geometry.object_regions()[1];
    assert_eq!(parent_region.instance().target(), parent);
    assert_eq!(
        parent_region.written_area(),
        Some(TeletextRawDisplayArea::Main)
    );
    assert_eq!(parent_region.area_pending(), None);
    assert_eq!(
        parent_region.interval(),
        TeletextObjectAddressInterval::Known {
            start: (1, 8),
            finish: (1, 9)
        }
    );
    let addresses = geometry.direct_addresses(parent_region.instance().id());
    assert_eq!(addresses.len(), 2);
    assert_eq!(
        addresses
            .iter()
            .map(|fact| fact.event().source().command)
            .collect::<Vec<_>>(),
        vec![0, 2]
    );
    assert_eq!(
        geometry.direct_writes(parent_region.instance().id()).len(),
        1
    );
    for fact in addresses {
        assert_eq!(
            fact.event(),
            &fixture.spatial.events()[fact.event_ordinal()]
        );
        assert_eq!(
            fact.event().invocation(),
            Some(parent_region.instance().id())
        );
    }
    assert_eq!(
        child_region.instance().parent(),
        Some(parent_region.instance().id())
    );
    assert_eq!(child_region.instance().target(), child);
    assert_eq!(
        (
            child_region.instance().origin().row(),
            child_region.instance().origin().column()
        ),
        (1, 40)
    );
    assert_eq!(
        child_region.written_area(),
        Some(TeletextRawDisplayArea::RightPanel)
    );
    assert_eq!(
        child_region.interval(),
        TeletextObjectAddressInterval::Known {
            start: (1, 8),
            finish: (1, 40)
        }
    );
    assert_eq!(
        enhanced_at(&geometry, 1, 40).placement(),
        TeletextCandidatePlacement::Placed { row: 1, column: 44 }
    );
}

#[test]
fn empty_and_address_only_instances_preserve_caller_ap_before_offset_and_typed_no_write_state() {
    let empty = local(0xffff);
    let addressed = local(0x0100);
    let fixture = fixture(
        None,
        vec![
            set(1, 8),
            invoke(empty, TeletextObjectType::Passive, Some((0, 4))),
            invoke(addressed, TeletextObjectType::Passive, Some((0, 1))),
        ],
        vec![
            definition(empty, TeletextObjectType::Passive, vec![]),
            definition(addressed, TeletextObjectType::Passive, vec![set(1, 3)]),
        ],
    );
    let geometry = resolve(&fixture);
    let empty = &geometry.object_regions()[0];
    let addressed = &geometry.object_regions()[1];
    assert_eq!(
        empty.interval(),
        TeletextObjectAddressInterval::Pending {
            reason: TeletextObjectIntervalPending::NoDirectAddress,
            endpoint: None
        }
    );
    assert_eq!(
        addressed.interval(),
        TeletextObjectAddressInterval::Known {
            start: (1, 8),
            finish: (2, 12)
        }
    );
    assert_eq!(
        (
            addressed.instance().origin().row(),
            addressed.instance().origin().column()
        ),
        (1, 9)
    );
    for region in [empty, addressed] {
        assert_eq!(region.written_area(), None);
        assert_eq!(
            region.area_pending(),
            Some(TeletextObjectAreaPending::NoDefiniteWrites)
        );
        assert!(geometry.direct_writes(region.instance().id()).is_empty());
    }
    assert!(geometry.direct_addresses(empty.instance().id()).is_empty());
    assert_eq!(
        geometry.direct_addresses(addressed.instance().id()).len(),
        1
    );
}

#[test]
fn definite_own_main_to_right_panel_write_is_located_without_blame_on_child_area() {
    let target = local(0xffff);
    let fixture = fixture(
        Some(presentation(panels(
            true,
            true,
            4,
            TeletextPanelStatus::Level25And35,
        ))),
        vec![set(1, 39), invoke(target, TeletextObjectType::Active, None)],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![glyph(0), glyph(1)],
        )],
    );
    let error = resolve_teletext_display_geometry(
        &fixture.base,
        &fixture.spatial,
        &fixture.glyphs,
        TeletextDisplayGeometryBudget {
            max_candidates: 1024,
            max_events: 128,
            max_invocations: 32,
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::CrossAreaWrite {
            previous: TeletextRawDisplayArea::Main,
            requested: TeletextRawDisplayArea::RightPanel
        }
    );
    assert_eq!(error.position, Some((1, 40)));
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
}

#[test]
fn charset_only_cross_area_is_rejected_while_a_single_disabled_panel_address_is_retained() {
    let target = local(0xffff);
    let fixture = fixture(
        Some(presentation(panels(
            true,
            true,
            4,
            TeletextPanelStatus::Level25And35,
        ))),
        vec![
            set(1, 32),
            invoke(target, TeletextObjectType::Passive, None),
        ],
        vec![definition(
            target,
            TeletextObjectType::Passive,
            vec![
                glyph(0),
                TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::DesignateCharset {
                        designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
                    },
                },
            ],
        )],
    );
    let error = resolve_teletext_display_geometry(
        &fixture.base,
        &fixture.spatial,
        &fixture.glyphs,
        TeletextDisplayGeometryBudget {
            max_candidates: 4096,
            max_events: 128,
            max_invocations: 32,
        },
    )
    .unwrap_err();
    assert_eq!(error.position, Some((1, 40)));
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::CrossAreaWrite {
            previous: TeletextRawDisplayArea::Main,
            requested: TeletextRawDisplayArea::RightPanel,
        }
    );
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 1,
        })
    );
    assert_eq!(error.invocation.unwrap().get(), 0);
    let disabled = samples(
        panels(false, false, 9, TeletextPanelStatus::Level25And35),
        1,
        &[40],
    );
    let geometry = resolve(&disabled);
    let region = &geometry.object_regions()[0];
    assert_eq!(region.written_area(), None);
    assert_eq!(
        region.area_pending(),
        Some(TeletextObjectAreaPending::UnplacedAddress)
    );
    assert_eq!(
        region.interval(),
        TeletextObjectAddressInterval::Known {
            start: (0, 0),
            finish: (1, 40),
        }
    );
    assert_eq!(geometry.direct_writes(region.instance().id()).len(), 1);
    assert_eq!(
        enhanced_at(&geometry, 1, 40).placement(),
        unplaced(TeletextCandidateNonplacement::DisabledPanel)
    );
}

#[test]
fn position_and_row_colour_addresses_reject_own_area_changes_even_with_disabled_panels() {
    let target = local(0xffff);
    for (body, position, previous, requested) in [
        (
            vec![set(0, 0), set(0, 1)],
            (1, 40),
            TeletextRawDisplayArea::Main,
            TeletextRawDisplayArea::RightPanel,
        ),
        (
            vec![
                set(0, 1),
                TeletextEnhancementCommand::SetRowColour {
                    row: 1,
                    colour: TeletextColourReference::BLACK,
                    following_rows: false,
                },
            ],
            (2, 39),
            TeletextRawDisplayArea::RightPanel,
            TeletextRawDisplayArea::Main,
        ),
    ] {
        let fixture = fixture(
            Some(presentation(panels(
                false,
                false,
                4,
                TeletextPanelStatus::Level25And35,
            ))),
            vec![
                set(1, 39),
                invoke(target, TeletextObjectType::Passive, None),
            ],
            vec![definition(target, TeletextObjectType::Passive, body)],
        );
        let error = resolve_teletext_display_geometry(
            &fixture.base,
            &fixture.spatial,
            &fixture.glyphs,
            TeletextDisplayGeometryBudget {
                max_candidates: 4096,
                max_events: 128,
                max_invocations: 32,
            },
        )
        .unwrap_err();
        assert_eq!(error.position, Some(position));
        assert_eq!(
            error.reason,
            TeletextDisplayGeometryFailure::CrossAreaWrite {
                previous,
                requested
            }
        );
        assert_eq!(
            error.location,
            Some(TeletextEnhancementSource {
                object: Some(target),
                command: 1
            })
        );
    }
}

#[test]
fn screen_colour_and_child_calls_do_not_fabricate_parent_addressed_cells() {
    let parent = local(0xffff);
    let child = local(0x0100);
    let fixture = fixture(
        Some(presentation(panels(
            true,
            true,
            4,
            TeletextPanelStatus::Level25And35,
        ))),
        vec![set(1, 0), invoke(parent, TeletextObjectType::Active, None)],
        vec![
            definition(
                parent,
                TeletextObjectType::Active,
                vec![
                    TeletextEnhancementCommand::SetScreenColour {
                        colour: TeletextColourReference::BLACK,
                    },
                    invoke(child, TeletextObjectType::Passive, Some((0, 68))),
                    glyph(0),
                ],
            ),
            definition(child, TeletextObjectType::Passive, vec![glyph(0)]),
        ],
    );
    let geometry = resolve(&fixture);
    let regions = geometry.object_regions();
    assert_eq!(
        regions[0].written_area(),
        Some(TeletextRawDisplayArea::Main)
    );
    assert_eq!(
        regions[1].written_area(),
        Some(TeletextRawDisplayArea::LeftPanel)
    );
    assert_eq!(
        geometry.direct_addresses(regions[0].instance().id()).len(),
        1
    );
    assert_eq!(
        geometry.direct_addresses(regions[1].instance().id()).len(),
        1
    );
}

#[test]
fn panel_screen_and_row_selectors_reject_exact_original_instance_even_when_disabled() {
    let target = local(0xffff);
    for (offset, raw, area) in [
        (1, 40, TeletextRawDisplayArea::RightPanel),
        (29, 68, TeletextRawDisplayArea::LeftPanel),
    ] {
        for enabled in [false, true] {
            for selector in [
                TeletextEnhancementCommand::SetScreenColour {
                    colour: TeletextColourReference::new(1).unwrap(),
                },
                TeletextEnhancementCommand::SetRowColour {
                    row: 0,
                    colour: TeletextColourReference::new(2).unwrap(),
                    following_rows: false,
                },
                TeletextEnhancementCommand::SetRowColour {
                    row: 0,
                    colour: TeletextColourReference::new(2).unwrap(),
                    following_rows: true,
                },
            ] {
                let fixture = fixture(
                    Some(presentation(panels(
                        enabled,
                        enabled,
                        4,
                        TeletextPanelStatus::Level25And35,
                    ))),
                    vec![
                        set(1, 39),
                        invoke(target, TeletextObjectType::Active, Some((0, offset))),
                    ],
                    vec![definition(
                        target,
                        TeletextObjectType::Active,
                        vec![set(0, 0), selector, glyph(0)],
                    )],
                );
                let before = fixture.spatial.clone();
                let error = resolve_teletext_display_geometry(
                    &fixture.base,
                    &fixture.spatial,
                    &fixture.glyphs,
                    TeletextDisplayGeometryBudget {
                        max_candidates: fixture.glyphs.candidates().len(),
                        max_events: fixture.spatial.events().len(),
                        max_invocations: fixture.spatial.invocations().len(),
                    },
                )
                .unwrap_err();
                assert_eq!(
                    error.reason,
                    TeletextDisplayGeometryFailure::PanelColour { area }
                );
                assert_eq!(error.owner, owner());
                assert_eq!(error.position, Some((1, raw)));
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
                assert_eq!(fixture.spatial, before);
            }
        }
    }
}

#[test]
fn earliest_plane_selector_uses_whole_own_region_not_its_transient_ap() {
    let target = local(0xffff);
    let fixture = fixture(
        Some(presentation(panels(
            false,
            true,
            0,
            TeletextPanelStatus::Level25And35,
        ))),
        vec![set(1, 39), invoke(target, TeletextObjectType::Active, None)],
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![
                TeletextEnhancementCommand::SetScreenColour {
                    colour: TeletextColourReference::new(1).unwrap(),
                },
                TeletextEnhancementCommand::SetScreenColour {
                    colour: TeletextColourReference::new(2).unwrap(),
                },
                glyph(1),
            ],
        )],
    );
    let error = resolve_teletext_display_geometry(
        &fixture.base,
        &fixture.spatial,
        &fixture.glyphs,
        TeletextDisplayGeometryBudget {
            max_candidates: fixture.glyphs.candidates().len(),
            max_events: fixture.spatial.events().len(),
            max_invocations: fixture.spatial.invocations().len(),
        },
    )
    .unwrap_err();
    assert_eq!(
        error.reason,
        TeletextDisplayGeometryFailure::PanelColour {
            area: TeletextRawDisplayArea::RightPanel
        }
    );
    assert_eq!(error.position, Some((1, 39)));
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 0
        })
    );
    assert_eq!(error.invocation.unwrap().get(), 0);
}

#[test]
fn main_plane_colours_and_panel_cell_colours_keep_separate_parent_child_regions() {
    let parent = local(0xffff);
    let child = local(0x0100);
    let fg = TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(16).unwrap(),
        },
    };
    let bg = TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation: TeletextColumnOperation::Background {
            colour: TeletextColourReference::new(17).unwrap(),
        },
    };
    let fixture = fixture(
        Some(presentation(panels(
            true,
            true,
            4,
            TeletextPanelStatus::Level25And35,
        ))),
        vec![
            TeletextEnhancementCommand::SetRowColour {
                row: 1,
                colour: TeletextColourReference::new(3).unwrap(),
                following_rows: false,
            },
            TeletextEnhancementCommand::SetScreenColour {
                colour: TeletextColourReference::new(4).unwrap(),
            },
            invoke(parent, TeletextObjectType::Active, None),
        ],
        vec![
            definition(
                parent,
                TeletextObjectType::Active,
                vec![
                    TeletextEnhancementCommand::SetScreenColour {
                        colour: TeletextColourReference::new(5).unwrap(),
                    },
                    TeletextEnhancementCommand::SetRowColour {
                        row: 0,
                        colour: TeletextColourReference::new(6).unwrap(),
                        following_rows: false,
                    },
                    invoke(child, TeletextObjectType::Passive, Some((0, 68))),
                    glyph(0),
                ],
            ),
            definition(child, TeletextObjectType::Passive, vec![fg, bg, glyph(0)]),
        ],
    );
    let geometry = resolve(&fixture);
    assert_eq!(
        geometry.object_regions()[0].written_area(),
        Some(TeletextRawDisplayArea::Main)
    );
    assert_eq!(
        geometry.object_regions()[1].written_area(),
        Some(TeletextRawDisplayArea::LeftPanel)
    );
    let child_writes = geometry.direct_writes(geometry.object_regions()[1].instance().id());
    assert_eq!(child_writes.len(), 3);
    assert_eq!(child_writes[0].event().command(), fg);
    assert_eq!(child_writes[1].event().command(), bg);
    assert_eq!(
        enhanced_at(&geometry, 1, 68).placement(),
        TeletextCandidatePlacement::Placed { row: 1, column: 0 }
    );
}

#[test]
fn unknown_plane_object_regions_retain_pending_facts_without_guessing_panel_ownership() {
    let target = local(0xffff);
    for (commands, body, pending) in [
        (
            vec![
                set(1, 39),
                invoke(target, TeletextObjectType::Active, Some((0, 1))),
            ],
            vec![TeletextEnhancementCommand::SetScreenColour {
                colour: TeletextColourReference::new(1).unwrap(),
            }],
            TeletextObjectAreaPending::NoDefiniteWrites,
        ),
        (
            vec![
                set(24, 39),
                invoke(target, TeletextObjectType::Active, Some((1, 1))),
            ],
            vec![
                TeletextEnhancementCommand::SetScreenColour {
                    colour: TeletextColourReference::new(1).unwrap(),
                },
                glyph(0),
            ],
            TeletextObjectAreaPending::UnplacedAddress,
        ),
    ] {
        let fixture = fixture(
            Some(presentation(panels(
                false,
                true,
                0,
                TeletextPanelStatus::Level25And35,
            ))),
            commands,
            vec![definition(target, TeletextObjectType::Active, body)],
        );
        let geometry = resolve(&fixture);
        assert_eq!(geometry.object_regions()[0].written_area(), None);
        assert_eq!(geometry.object_regions()[0].area_pending(), Some(pending));
        assert_eq!(
            fixture.spatial.events()[2].source(),
            TeletextEnhancementSource {
                object: Some(target),
                command: 0
            }
        );
        assert!(matches!(
            fixture.spatial.events()[2].command(),
            TeletextEnhancementCommand::SetScreenColour { .. }
        ));
    }
}
