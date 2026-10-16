//! Actually executed POP defaults must survive whole-service national lowering.

use super::*;
use crate::teletext::{
    TeletextObjectPageTransmission, TeletextResourceObjectAllocations,
    TeletextResourceTransmissionMode,
};

// Existing pinned Latin G0 P, table0/50; the font binary remains unchanged.
const PASSIVE_P_MASK: [u16; 10] = [0, 0x3fc, 0x306, 0x306, 0x3fc, 0x300, 0x300, 0x300, 0, 0];

fn pop_identity() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap()
}

fn gpop_identity() -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Gpop,
        TeletextRawDataPageNumber::new(2, 0xd0).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap()
}

fn object_target(id: u16) -> TeletextObjectIdentity {
    if id == 20 {
        TeletextObjectIdentity::global(gpop_identity(), TeletextObjectId::new(id)).unwrap()
    } else {
        TeletextObjectIdentity::public(pop_identity(), TeletextObjectId::new(id)).unwrap()
    }
}

fn selections() -> [TeletextDefaultObjectSelection; 3] {
    [
        TeletextDefaultObjectSelection {
            target: object_target(20),
            object_type: TeletextObjectType::Active,
        },
        TeletextDefaultObjectSelection {
            target: object_target(22),
            object_type: TeletextObjectType::Adaptive,
        },
        TeletextDefaultObjectSelection {
            target: object_target(21),
            object_type: TeletextObjectType::Passive,
        },
    ]
}

fn objects() -> Vec<TeletextEnhancementObjectDefinition> {
    vec![
        TeletextEnhancementObjectDefinition {
            identity: object_target(20),
            object_type: TeletextObjectType::Active,
            // The independent Global default has no backward position or call.
            // It paints the same old Active body at absolute row1,column8.
            program: program(vec![
                position(1, 8),
                display_attributes(8, true, true),
                at(8, TeletextColumnOperation::G0 { code: code(b'A') }),
                display_attributes(10, false, false),
            ]),
        },
        TeletextEnhancementObjectDefinition {
            identity: object_target(22),
            object_type: TeletextObjectType::Adaptive,
            program: program(vec![
                position(1, 7),
                display_attributes(7, true, false),
                display_attributes(9, false, false),
                at(9, TeletextColumnOperation::G0 { code: code(b'X') }),
            ]),
        },
        TeletextEnhancementObjectDefinition {
            identity: object_target(21),
            object_type: TeletextObjectType::Passive,
            program: program(vec![
                position(4, 1),
                display_attributes(1, false, false),
                at(
                    1,
                    TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(2).unwrap(),
                    },
                ),
                at(
                    1,
                    TeletextColumnOperation::Background {
                        colour: TeletextColourReference::new(0).unwrap(),
                    },
                ),
                at(1, TeletextColumnOperation::G0 { code: code(b'P') }),
            ]),
        },
    ]
}

fn active_default_declaration(service: &TeletextSemanticService) -> TeletextMotLevel25Declaration {
    let original = declaration(service);
    let mut public = [original.links().object_link(None); 7];
    let [active, adaptive, passive] = selections();
    let global = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page {
            page: gpop_identity().page(),
        },
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [
            TeletextMotDefaultObject::Object {
                target: active.target,
                object_type: active.object_type,
            },
            TeletextMotDefaultObject::NoObject { raw_number: 77 },
        ],
    );
    public[0] = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page {
            page: pop_identity().page(),
        },
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [adaptive, passive].map(|selection| TeletextMotDefaultObject::Object {
            target: selection.target,
            object_type: selection.object_type,
        }),
    );
    let normal =
        TeletextMotPublicLinkIndex::ALL.map(|index| original.links().drcs_link(Some(index)));
    TeletextMotLevel25Declaration::new(
        original.identity(),
        vec![TeletextMotPageAssociation::new(
            owner().page.wire_page(),
            TeletextMotAssociationSelection::new(
                true,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(
                true,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
        )],
        TeletextMotLevel25Links::new(
            global,
            public,
            original.links().drcs_link(None),
            normal,
            original.links().counts(),
        )
        .unwrap(),
    )
    .unwrap()
}

fn input() -> TeletextDeclaredService {
    let seed = occupied_seed(true);
    let mut pages = seed
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    let metadata = &mut pages[0];
    metadata.local_program = None;
    metadata.compositional_links = None;
    metadata.resources.objects.public = Some(pop_identity().page());
    metadata.resources.objects.global = Some(gpop_identity().page());
    let [active, adaptive, passive] = selections();
    metadata.resources.defaults = [Some(active), None, Some(adaptive), Some(passive)];
    let semantic = rebuild(&seed, pages, seed.drcs().subtables().to_vec(), objects());
    let mot = active_default_declaration(&semantic);
    TeletextDeclaredService::new(
        semantic,
        vec![mot],
        vec![TeletextDisplayMotSelection {
            display: owner(),
            mot: Some(mot_identity()),
        }],
        vec![],
        authored::declaration_limits(),
    )
    .unwrap()
}

fn allocations(input: &TeletextDeclaredService) -> TeletextResourceObjectAllocations {
    assert_eq!(input.defaults().len(), 3);
    let defaults = input
        .defaults()
        .iter()
        .map(TeletextDeclaredDefaultReservation::selection)
        .collect::<Vec<_>>();
    assert_eq!(defaults, selections());
    crate::teletext::allocate_teletext_resource_objects(
        input.semantic().objects(),
        &defaults,
        service_budget().normalization.max_resource_objects,
    )
    .unwrap()
}

fn original_resource_pages(
    input: &TeletextDeclaredService,
    allocations: &TeletextResourceObjectAllocations,
) -> Vec<EncodedTeletextAuthoredPage> {
    let mut defaults = [[None; 2]; 8];
    let [active, adaptive, passive] = selections();
    for (bank, slot, selection) in [(0, 0, active), (1, 0, adaptive), (1, 1, passive)] {
        defaults[bank][slot] = Some(
            allocations
                .lookup(selection.target, selection.object_type)
                .unwrap()
                .mot_default_allocation()
                .unwrap(),
        );
    }
    let transmissions = authored::transmissions(input);
    assert_eq!(transmissions.len(), 5);
    transmissions
        .into_iter()
        .map(|tx| match tx.resource.kind() {
            TeletextResourceKind::Mot => EncodedTeletextAuthoredPage::Resource(
                crate::teletext::encode_teletext_mot_page(
                    &input.mots()[0],
                    &defaults,
                    tx.counter,
                    tx.update,
                    TeletextResourceTransmissionMode::Serial,
                )
                .unwrap(),
            ),
            TeletextResourceKind::Pop | TeletextResourceKind::Gpop => {
                EncodedTeletextAuthoredPage::Object(
                    crate::teletext::encode_teletext_object_page(
                        tx.resource,
                        input.semantic().objects(),
                        allocations,
                        TeletextObjectPageTransmission {
                            counter: tx.counter,
                            update: tx.update,
                            mode: TeletextResourceTransmissionMode::Serial,
                            last_subpage: tx.last_subpage,
                        },
                        service_budget().max_commands,
                    )
                    .unwrap(),
                )
            }
            TeletextResourceKind::Drcs | TeletextResourceKind::Gdrcs => {
                let table = input
                    .semantic()
                    .drcs()
                    .subtables()
                    .iter()
                    .find(|table| table.identity() == tx.resource)
                    .unwrap();
                EncodedTeletextAuthoredPage::Resource(
                    crate::teletext::encode_teletext_drcs_page(
                        table,
                        tx.counter,
                        tx.update,
                        TeletextResourceTransmissionMode::Serial,
                    )
                    .unwrap(),
                )
            }
            kind @ TeletextResourceKind::Mip => {
                panic!("unexpected original resource kind: {kind:?}")
            }
        })
        .collect()
}

fn assert_active_original(original: &crate::teletext::TeletextMainComposition) -> usize {
    assert_eq!(
        original.original_program_source(),
        &TeletextEffectiveProgramSource::Defaults {
            slots: vec![0, 2, 3]
        }
    );
    let origin = assert_polish_original(original);
    assert_eq!(
        original.display_resolution().origins()[origin].effective_size(),
        Some(TeletextDisplaySize::DoubleWidth)
    );
    for (column, fragment) in [
        (7, TeletextDisplayFragment::DoubleWidthLeft),
        (8, TeletextDisplayFragment::DoubleWidthRight),
    ] {
        let cell = original.display_resolution().cell(1, column).unwrap();
        assert_eq!(cell.fragment(), fragment);
        assert_eq!(
            cell.attributes().underline_or_separated_mosaics().value(),
            Some(false)
        );
        assert_eq!(
            crate::teletext::project_teletext_comparison_glyph(original, cell)
                .unwrap()
                .mask
                .unwrap()
                .rows(),
            POLISH_5B_MASK
        );
    }
    let view = original.display_resolution();
    let passive = view.cell(4, 1).unwrap().origin().unwrap();
    let index = view.origins()[passive].candidate_index();
    let candidate = original.glyph_layers().candidates()[index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Passive);
    assert_eq!(
        candidate.root_source(),
        Some(TeletextSpatialRootSource::DefaultSlot { slot: 3 })
    );
    assert!(matches!(candidate.source(),Some(source) if source.object==Some(object_target(21))));
    assert!(candidate.invocation().is_some());
    let intent = original.glyph_intent().records()[index];
    assert_eq!(intent.disposition(), TeletextGlyphDisposition::Assigned);
    assert!(
        matches!(intent.glyph(),Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.repertoire()==TeletextG0Repertoire::Latin && glyph.code()==b'P' && glyph.national_subset().is_none())
    );
    let glyph =
        crate::teletext::project_teletext_comparison_glyph(original, view.cell(4, 1).unwrap())
            .unwrap();
    assert_eq!(glyph.mask.unwrap().rows(), PASSIVE_P_MASK);
    assert_eq!(
        view.cell(4, 1).unwrap().attributes().foreground().value(),
        Some(TeletextColourReference::new(2).unwrap())
    );
    origin
}

fn assert_passive_pixels(display: &crate::teletext::EncodedTeletextCompiledDisplayPage) {
    let image = crate::teletext::render_teletext_physical_display(
        display.normalized().physical().replay(),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        crate::teletext::TeletextRasterBudget {
            max_pixels: 120_000,
            max_diagnostics: 4096,
        },
    )
    .unwrap();
    assert!(image.diagnostics().is_empty());
    for (y, row) in PASSIVE_P_MASK.into_iter().enumerate() {
        for x in 0..12 {
            let rgb = if row & (1 << (11 - x)) == 0 {
                [0, 0, 0]
            } else {
                [0, 15, 0]
            };
            assert!(
                matches!(image.pixel(12+x,40+y),Some(crate::teletext::TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get()==rgb)
            );
        }
    }
}

#[test]
fn selected_passive_default_remains_active_through_occupied_link_national_lowering() {
    let input = input();
    let before = input.clone();
    let metadata = &input.semantic().page_enhancements()[&owner()];
    assert!(metadata.local_program.is_none());
    assert!(metadata.compositional_links.is_none());
    assert_eq!(input.resource_inventory().len(), 5);
    assert_eq!(input.semantic().objects().definitions(), objects());
    let [active, adaptive, passive] = selections();
    assert_eq!(
        metadata.resources.defaults,
        [Some(active), None, Some(adaptive), Some(passive)]
    );
    assert_eq!(
        input.resolutions()[0].defaults(),
        &metadata.resources.defaults
    );
    let allocations = allocations(&input);
    let resources = original_resource_pages(&input, &allocations);
    let wires: Vec<_> = resources
        .iter()
        .map(|page| {
            (
                page.resource_header().unwrap().identity(),
                page.data_units().to_vec(),
            )
        })
        .collect();
    let fallback = rows(&polish_fallback(input.semantic()));
    let limits = service_budget();
    let budget = limits.normalization;
    // The factory's original also knows the actual resource word positions.
    // Bind those same primitive-encoded pages before comparing the FULL model.
    let resource_context =
        crate::teletext::resource_transmission::ResourceTransmissionContext::new(
            input.semantic().objects(),
            &allocations,
            &resources,
            limits.max_commands,
            budget.max_resource_objects,
            limits.max_pages,
        )
        .unwrap();
    let mut context = crate::teletext::compile_context::TeletextCompileContext::for_service(
        Some(&resource_context),
        None,
        &limits,
    );
    let composed = crate::teletext::composed_main::compile_composed_with_context(
        input.semantic(),
        owner(),
        &allocations,
        budget.composition,
        budget.packing,
        &mut context,
    )
    .unwrap();
    let original = composed.composition();
    let origin = assert_active_original(original);
    let plan =
        crate::teletext::plan_teletext_base_compatibility_realization(original, budget.realization)
            .unwrap();
    assert!(plan
        .origins()
        .iter()
        .any(|candidate| candidate.origin_index == origin));
    let encoded = encode_teletext_authored_service(
        input.clone(),
        &authored::transmissions(&input),
        &service_budget(),
    );
    assert_eq!(input, before);
    let encoded=encoded.expect("selected Active and visible Passive defaults require exact national lowering without suppression or declaration rewrites");
    assert_generated(&input, &encoded);
    let display = display(&encoded);
    assert!(
        display.normalized().original() == original,
        "the entire original model, including authenticated resource word positions, must match"
    );
    assert_active_original(display.normalized().original());
    assert_eq!(rows(display.page()), fallback);
    assert_passive_pixels(display);
    for (identity, bytes) in wires {
        let actual = encoded
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == identity)
            })
            .unwrap();
        assert_eq!(actual.data_units(), bytes);
    }
    assert_eq!(input, before);
}
