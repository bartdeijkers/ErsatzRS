//! Foreign MOT ownership remains original when Base compatibility requires X26.
use super::*;

fn selected() -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        resource(TeletextResourceKind::Pop, 0xa1, 0),
        TeletextObjectId::new(20),
    )
    .unwrap()
}

fn links() -> TeletextX27Level25Links {
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    let pop = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: selected().resource().unwrap().page(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(1),
    )
    .unwrap();
    TeletextX27Level25Links::new(unused, pop, unused, unused).unwrap()
}

fn fixture(compatibility: bool) -> TeletextDeclaredService {
    let mut draft = input();
    let mut contrast = object(20, b'Z');
    contrast.identity = selected();
    draft.objects.push(contrast);
    draft.pages[1].local_program = None;
    draft.pages[1].resources.objects.public = Some(selected().resource().unwrap().page());
    draft.pages[1].compositional_links = Some(links());
    draft.base.pages[1].rows.push(TeletextRow {
        row: 2,
        runs: vec![TeletextRun {
            column: 1,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text {
                text: if compatibility { "£Z" } else { "ZZ" }.into(),
            },
        }],
    });
    // Full declaration resolution must admit the original MOT2A0 plus the
    // superseding explicit POP2A1. A supplied detached selection is insufficient.
    declared(draft)
}

fn check_original(declared: &TeletextDeclaredService) {
    let page = &declared.semantic().page_enhancements()[&owner(1)];
    assert!(page.local_program.is_none());
    assert_eq!(page.compositional_links, Some(links()));
    assert_eq!(
        page.resources.objects.public,
        Some(selected().resource().unwrap().page())
    );
    let resolution = declared
        .resolutions()
        .iter()
        .find(|value| value.display() == owner(1))
        .unwrap();
    assert!(matches!(resolution.role(TeletextX27LinkRole::Pop),
        TeletextMotRoleResolution::Selected { target: actual, provenance: TeletextMotLinkProvenance::X27 { .. }, original_mot: Some(mot), .. }
        if *actual == selected().resource().unwrap().page() && mot.target == target(20).resource().unwrap().page() && mot.supersedable));
    assert_eq!(
        resolution.defaults()[2],
        Some(TeletextDefaultObjectSelection {
            target: target(20),
            object_type: TeletextObjectType::Passive,
        })
    );
    let (source, effective) = declared.semantic().effective_program(owner(1)).unwrap();
    assert_eq!(
        source,
        TeletextEffectiveProgramSource::Defaults { slots: vec![2] }
    );
    assert_eq!(
        effective.commands(),
        &[TeletextEnhancementCommand::InvokeObject {
            target: target(20),
            object_type: TeletextObjectType::Passive,
            offset: None,
        }]
    );
    assert!(declared
        .resource_inventory()
        .contains(&target(20).resource().unwrap()));
    assert!(declared
        .resource_inventory()
        .contains(&selected().resource().unwrap()));
    for (identity, value) in [(target(20), b'A'), (selected(), b'Z')] {
        assert!(
            matches!(declared.semantic().objects().definition(identity).unwrap().program.commands()[1],
            TeletextEnhancementCommand::AtColumn { operation: TeletextColumnOperation::G0 { code }, .. } if code.get() == value)
        );
    }
}

#[test]
fn authored_foreign_mot_default_with_separate_base_compatibility_preserves_original_owner() {
    let native_declared = fixture(false);
    check_original(&native_declared);
    let native = encode_teletext_authored_service(
        native_declared.clone(),
        &transmissions(&native_declared),
        &limits(),
    )
    .expect("declared foreign default without a compatibility glyph must retain native activation");
    let EncodedTeletextAuthoredPage::NativeDisplay(control) = &native.pages()[1] else {
        panic!("absent Local and ordinary Base must use native defaults");
    };
    assert_eq!(control.encoded().defaults()[2].unwrap().target, target(20));
    assert!(control
        .encoded()
        .page()
        .data_units()
        .iter()
        .all(|unit| packet(unit) != 26));

    let original = fixture(true);
    check_original(&original);
    let before = original.clone();
    assert_eq!(
        original.resource_inventory(),
        native_declared.resource_inventory()
    );
    assert_eq!(original.defaults(), native_declared.defaults());
    assert_eq!(
        original.semantic().objects(),
        native_declared.semantic().objects()
    );
    let base = &original.semantic().base().pages[1];
    let layout = layout::compile_for_composition(base).unwrap();
    assert_eq!(layout.base_glyphs().len(), 1);
    assert_eq!(
        layout.base_glyphs()[0].position(),
        TeletextEnhancementPosition::new(2, 1).unwrap()
    );
    let fallback = encode_teletext_page(base, TeletextEncodingRegion::WesternLatin).unwrap();
    let fallback_rows = fallback
        .data_units()
        .iter()
        .filter(|unit| packet(unit) <= 25)
        .collect::<Vec<_>>();
    let row2 = fallback
        .data_units()
        .iter()
        .find(|unit| packet(unit) == 2)
        .unwrap();
    assert_eq!((body(row2)[1] & 0x7f, body(row2)[2] & 0x7f), (b' ', b'Z'));

    // EN300706 13.8/E.11: emitted compatibility X26 suppresses MOT defaults.
    // The full compiler must therefore realize A from original POP2A0 without
    // illegally invoking selected POP2A1's contrasting Z or changing owners.
    let result =
        encode_teletext_authored_service(original.clone(), &transmissions(&original), &limits())
            .expect("legal authored foreign MOT root plus separate compatibility X26 must compile");
    assert!(
        result.declared() == &before,
        "authored declarations must remain immutable"
    );
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &result.pages()[1] else {
        panic!("the compatibility glyph must be emitted through the checked X26 path");
    };
    assert_eq!(
        page.page()
            .data_units()
            .iter()
            .filter(|unit| packet(unit) <= 25)
            .collect::<Vec<_>>(),
        fallback_rows
    );
    assert_eq!(
        result.pages()[0].data_units(),
        native.pages()[0].data_units()
    );
    for expected in native
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        let actual = result
            .pages()
            .iter()
            .find(|page| page.inventory_record() == expected.inventory_record())
            .unwrap();
        assert_eq!(actual.data_units(), expected.data_units());
    }
    check_model(page.normalized());
}

fn check_model(page: &EncodedTeletextNormalizedMainProgram) {
    assert_eq!(
        page.original().original_program_source(),
        &TeletextEffectiveProgramSource::Defaults { slots: vec![2] }
    );
    let call = &page.original().spatial_trace().invocations()[0];
    assert_eq!(call.target(), target(20));
    assert_eq!(
        call.root_source(),
        TeletextSpatialRootSource::DefaultSlot { slot: 2 }
    );
    let original_base = page.original().base_display().cell(2, 1).unwrap();
    assert_eq!(original_base.code(), b' ');
    assert!(matches!(
        original_base.authored().unwrap().content(),
        TeletextBaseAuthoredContent::Text { character: '£' }
    ));
    let physical = page.physical().replay();
    for (display, intent, original) in [
        (
            page.original().display_resolution(),
            page.original().glyph_intent(),
            true,
        ),
        (
            physical.display_resolution(),
            physical.glyph_intent(),
            false,
        ),
    ] {
        for (row, column) in [(1, 8), (2, 1)] {
            let index = display.origins()[display.cell(row, column).unwrap().origin().unwrap()]
                .candidate_index();
            let glyph = intent.records()[index].glyph();
            if row == 1 || original {
                let expected = if row == 1 { b'A' } else { b' ' };
                assert!(
                    matches!(glyph, Some(TeletextGlyphIdentity::G0 { glyph }) if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == expected),
                    "row={row} col={column} glyph={glyph:?}"
                );
            } else {
                assert!(
                    matches!(glyph, Some(TeletextGlyphIdentity::G2 { glyph }) if glyph.repertoire() == TeletextG2Repertoire::Latin && glyph.code() == 0x23),
                    "row={row} col={column} glyph={glyph:?}"
                );
            }
        }
    }
    // The original Base keeps its fallback; the authored replacement, rather
    // than that SPACE's raw identity, supplies the semantic pound-sign mask.
    let original_glyph = super::super::raster::comparison_original_glyph(
        page.original(),
        page.original().display_resolution().cell(2, 1).unwrap(),
    )
    .unwrap();
    let physical_glyph = super::super::raster::comparison_physical_glyph(
        physical,
        physical.display_resolution().cell(2, 1).unwrap(),
    )
    .unwrap();
    assert!(original_glyph.mask.is_some());
    assert_eq!(original_glyph, physical_glyph);
}

fn declare_with_mot(
    draft: TeletextResolvedServiceDraft,
    declaration: TeletextMotLevel25Declaration,
) -> TeletextDeclaredService {
    let companions = vec![resource(TeletextResourceKind::Pop, 0xa0, 1)];
    let semantic = TeletextSemanticService::new_with_object_companions(
        draft,
        companions.clone(),
        semantic_limits(),
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![declaration],
        [0, 1]
            .map(|subcode| TeletextDisplayMotSelection {
                display: owner(subcode),
                mot: Some(resource(TeletextResourceKind::Mot, 0xfe, 9)),
            })
            .to_vec(),
        companions,
        declaration_limits(),
    )
    .unwrap()
}

fn typed_fixture(kind: TeletextObjectType, attribute_only: bool) -> TeletextDeclaredService {
    let mut draft = input();
    let root = draft
        .objects
        .iter_mut()
        .find(|object| object.identity == target(20))
        .unwrap();
    root.object_type = kind;
    if attribute_only {
        root.program = TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetScreenColour {
                colour: TeletextColourReference::new(4).unwrap(),
            },
            TeletextEnhancementCommand::SetRowColour {
                row: 1,
                colour: TeletextColourReference::new(2).unwrap(),
                following_rows: false,
            },
        ])
        .unwrap();
    }
    let mut contrast = object(20, b'Z');
    contrast.identity = selected();
    contrast.object_type = kind;
    draft.objects.push(contrast);
    for page in &mut draft.pages {
        page.resources.defaults[2].as_mut().unwrap().object_type = kind;
    }
    draft.pages[1].resources.objects.public = Some(selected().resource().unwrap().page());
    draft.pages[1].compositional_links = Some(links());
    draft.base.pages[1].rows.push(TeletextRow {
        row: 2,
        runs: vec![TeletextRun {
            column: 1,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "£Z".into() },
        }],
    });
    declare_with_mot(draft, mot_with_type(kind))
}

fn compiled_owner(
    result: &EncodedTeletextAuthoredService,
) -> &EncodedTeletextNormalizedMainProgram {
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &result.pages()[1] else {
        panic!("foreign defaults plus compatibility must use checked X26");
    };
    page.normalized()
}

fn assert_latin_cell(page: &EncodedTeletextNormalizedMainProgram, row: u8, column: u8, code: u8) {
    let physical = page.physical().replay();
    for (display, intent) in [
        (
            page.original().display_resolution(),
            page.original().glyph_intent(),
        ),
        (physical.display_resolution(), physical.glyph_intent()),
    ] {
        let index = display.origins()[display.cell(row, column).unwrap().origin().unwrap()]
            .candidate_index();
        assert!(
            matches!(intent.records()[index].glyph(), Some(TeletextGlyphIdentity::G0 { glyph })
            if glyph.repertoire() == TeletextG0Repertoire::Latin && glyph.code() == code),
            "row={row}, column={column}, expected={code}, actual={:?}",
            intent.records()[index].glyph()
        );
    }
}

#[test]
fn foreign_mot_root_types_retain_original_body_type_and_full_identity() {
    for kind in [
        TeletextObjectType::Passive,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Active,
    ] {
        let declared = typed_fixture(kind, false);
        let original = declared.clone();
        assert_eq!(
            declared
                .semantic()
                .objects()
                .definition(target(20))
                .unwrap()
                .object_type,
            kind
        );
        let result = encode_teletext_authored_service(
            declared.clone(),
            &transmissions(&declared),
            &limits(),
        )
        .unwrap_or_else(|error| panic!("legal foreign root type {kind:?}: {error}"));
        assert!(
            result.declared() == &original,
            "root materialization changed authored declarations"
        );
        let page = compiled_owner(&result);
        let calls = page.original().spatial_trace().invocations();
        assert_eq!(calls.len(), 1);
        assert_eq!(
            (
                calls[0].target(),
                calls[0].object_type(),
                calls[0].root_source()
            ),
            (
                target(20),
                kind,
                TeletextSpatialRootSource::DefaultSlot { slot: 2 }
            )
        );
        assert_latin_cell(page, 1, 8, b'A');
    }
}

#[test]
fn foreign_attribute_only_active_default_retains_screen_and_row_planes() {
    let declared = typed_fixture(TeletextObjectType::Active, true);
    let original = declared.clone();
    assert!(declared.semantic().objects().definition(target(20)).unwrap().program.commands().iter()
        .all(|command| !matches!(command, TeletextEnhancementCommand::AtColumn { operation, .. } if operation.glyph_family().is_some())));
    let result =
        encode_teletext_authored_service(declared.clone(), &transmissions(&declared), &limits())
            .expect(
                "foreign attribute-only default must not disappear without a collision glyph seed",
            );
    assert!(
        result.declared() == &original,
        "attribute-only realization mutated original metadata"
    );
    let page = compiled_owner(&result);
    assert_eq!(page.original().colour_context().screen_colour().get(), 4);
    assert_eq!(page.original().colour_context().row_colours()[1].get(), 2);
    let colours = page.physical().replay().colour_composition();
    assert!(
        matches!(colours.screen_pixel(), TeletextColourPixel::Rgb(rgb) if rgb.get() == [0, 0, 15])
    );
    assert!(
        matches!(colours.row_pixel(1), Some(TeletextColourPixel::Rgb(rgb)) if rgb.get() == [0, 15, 0])
    );
    assert_latin_cell(page, 1, 8, b'Z');
    assert_eq!(
        page.original().spatial_trace().invocations()[0].target(),
        target(20)
    );
}

fn global_target(page: u8) -> TeletextObjectIdentity {
    TeletextObjectIdentity::global(
        resource(TeletextResourceKind::Gpop, page, 0),
        TeletextObjectId::new(20),
    )
    .unwrap()
}

fn sparse_fixture(nested: bool, compatibility: bool) -> TeletextDeclaredService {
    let mut draft = input();
    let mut selected_pop = object(20, b'Z');
    selected_pop.identity = selected();
    let mut second_pop = object(21, b'C');
    second_pop.object_type = TeletextObjectType::Adaptive;
    let mut global = object(20, b'G');
    global.identity = global_target(0xb0);
    global.object_type = TeletextObjectType::Active;
    let mut selected_global = object(20, b'Q');
    selected_global.identity = global_target(0xb1);
    selected_global.object_type = TeletextObjectType::Active;
    for (object, column) in [
        (&mut second_pop, 10),
        (&mut global, 12),
        (&mut selected_global, 12),
    ] {
        object.program = TeletextEnhancementProgram::new(
            object
                .program
                .commands()
                .iter()
                .map(|command| match *command {
                    TeletextEnhancementCommand::SetPosition { .. } => {
                        TeletextEnhancementCommand::SetPosition {
                            position: TeletextEnhancementPosition::new(1, column).unwrap(),
                        }
                    }
                    TeletextEnhancementCommand::AtColumn { operation, .. } => {
                        TeletextEnhancementCommand::AtColumn { column, operation }
                    }
                    command => command,
                })
                .collect(),
        )
        .unwrap();
    }
    if nested {
        selected_global.object_type = TeletextObjectType::Passive;
        nested::add_dependency(&mut global, selected());
        nested::add_dependency(&mut second_pop, global_target(0xb1));
    }
    draft
        .objects
        .extend([selected_pop, second_pop, global, selected_global]);
    let defaults = [
        Some(TeletextDefaultObjectSelection {
            target: global_target(0xb0),
            object_type: TeletextObjectType::Active,
        }),
        None,
        Some(TeletextDefaultObjectSelection {
            target: target(20),
            object_type: TeletextObjectType::Passive,
        }),
        Some(TeletextDefaultObjectSelection {
            target: target(21),
            object_type: TeletextObjectType::Adaptive,
        }),
    ];
    let old = links();
    let declaration = TeletextX27Level25Links::new(
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page {
                page: global_target(0xb1).resource().unwrap().page(),
            },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(1),
        )
        .unwrap(),
        old.link(TeletextX27LinkRole::Pop),
        old.link(TeletextX27LinkRole::Gdrcs),
        old.link(TeletextX27LinkRole::Drcs),
    )
    .unwrap();
    for page in &mut draft.pages {
        page.compositional_links = Some(declaration);
        page.resources.objects.public = Some(selected().resource().unwrap().page());
        page.resources.objects.global = Some(global_target(0xb1).resource().unwrap().page());
        page.resources.defaults = defaults;
    }
    draft.base.pages[1].rows.push(TeletextRow {
        row: 2,
        runs: vec![TeletextRun {
            column: 1,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text { text: "£Z".into() },
        }],
    });
    if !compatibility {
        draft.base.pages[1].rows.last_mut().unwrap().runs[0].content =
            TeletextRunContent::Text { text: "ZZ".into() };
    }
    declare_with_mot(draft, sparse_mot(defaults))
}

fn sparse_mot(
    defaults: [Option<TeletextDefaultObjectSelection>; 4],
) -> TeletextMotLevel25Declaration {
    let default = |index: usize| {
        defaults[index].map_or(
            TeletextMotDefaultObject::NoObject { raw_number: 77 },
            |value| TeletextMotDefaultObject::Object {
                target: value.target,
                object_type: value.object_type,
            },
        )
    };
    let link = |target: TeletextObjectIdentity, extent, pair| {
        TeletextMotObjectLink::new(
            TeletextX27LinkTarget::Page {
                page: target.resource().unwrap().page(),
            },
            TeletextMotSubpageExtent::new(extent).unwrap(),
            true,
            TeletextMotFallback::new(0).unwrap(),
            pair,
        )
    };
    let mut public = [empty_link(); 7];
    public[0] = link(target(20), 1, [default(2), default(3)]);
    let drcs = mot().links().drcs_link(None);
    TeletextMotLevel25Declaration::new(
        resource(TeletextResourceKind::Mot, 0xfe, 9),
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                true,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )],
        TeletextMotLevel25Links::new(
            link(global_target(0xb0), 0, [default(0), default(1)]),
            public,
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn foreign_sparse_global_and_repeated_public_owner_keep_distinct_slots_and_selectors() {
    let declared = sparse_fixture(false, true);
    let original = declared.clone();
    let (source, _) = declared.semantic().effective_program(owner(1)).unwrap();
    assert_eq!(
        source,
        TeletextEffectiveProgramSource::Defaults {
            slots: vec![0, 2, 3]
        }
    );
    let defaults = declared.semantic().page_enhancements()[&owner(1)]
        .resources
        .defaults
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let allocated =
        allocate_teletext_resource_objects(declared.semantic().objects(), &defaults, 64).unwrap();
    // The selector is per resource and object type. These equal selector0 values
    // must never identify the selected POP/GPOP's contrasting definition.
    for value in &defaults {
        assert_eq!(
            allocated
                .lookup(value.target, value.object_type)
                .unwrap()
                .mot_default_allocation()
                .unwrap()
                .selector,
            0
        );
    }
    let result =
        encode_teletext_authored_service(declared.clone(), &transmissions(&declared), &limits())
            .expect(
            "foreign GPOP and repeated POP owners require independent root/slot materialization",
        );
    assert!(
        result.declared() == &original,
        "sparse defaults, selectors and inventory must stay original"
    );
    let page = compiled_owner(&result);
    let roots = page
        .original()
        .spatial_trace()
        .invocations()
        .iter()
        .map(|call| (call.target(), call.object_type(), call.root_source()))
        .collect::<Vec<_>>();
    assert_eq!(
        roots,
        [
            (
                global_target(0xb0),
                TeletextObjectType::Active,
                TeletextSpatialRootSource::DefaultSlot { slot: 0 }
            ),
            (
                target(20),
                TeletextObjectType::Passive,
                TeletextSpatialRootSource::DefaultSlot { slot: 2 }
            ),
            (
                target(21),
                TeletextObjectType::Adaptive,
                TeletextSpatialRootSource::DefaultSlot { slot: 3 }
            ),
        ]
    );
    for (column, code) in [(8, b'A'), (10, b'C'), (12, b'G')] {
        assert_latin_cell(page, 1, column, code);
    }
}

#[test]
fn foreign_default_materialization_refuses_original_work_and_catalog_limits_atomically() {
    let declared = fixture(true);
    let original = declared.clone();
    let mut no_work = limits();
    no_work.max_assembly_work = Some(0);
    let mut no_new_objects = limits();
    no_new_objects.normalization.packing.max_objects =
        declared.semantic().objects().definitions().len();
    for bound in [no_work, no_new_objects] {
        assert!(
            encode_teletext_authored_service(declared.clone(), &transmissions(&declared), &bound)
                .is_err(),
            "foreign physical wrappers must not escape the original work/catalog bounds"
        );
        assert!(
            declared == original,
            "rejection mutated the original declared service"
        );
    }
}

#[path = "foreign_defaults/decoder.rs"]
mod decoder;

#[path = "foreign_defaults/nested.rs"]
mod nested;
