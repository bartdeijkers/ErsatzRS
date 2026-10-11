//! Checked synthetic contributions; cold composition is not replacement evidence.
use crate::teletext::*;

fn address(page: u16, subcode: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(subcode).unwrap(),
    }
}
fn resource(kind: TeletextResourceKind, byte: u8, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, byte).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}
fn language(code: &str) -> TeletextLanguage {
    TeletextLanguage::new(code.into()).unwrap()
}
fn page(page: u16, subcode: u16) -> TeletextPageDraft {
    TeletextPageDraft {
        address: address(page, subcode),
        kind: TeletextPageKind::FullPage,
        language: language("nld"),
        national_option: TeletextNationalOption::new(1).unwrap(),
        rows: vec![],
        navigation: vec![],
    }
}
fn draft(pages: Vec<TeletextPageDraft>) -> TeletextResolvedServiceDraft {
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language: language("nld"),
            initial_page: pages[0].address.page,
            pages,
        },
        pages: vec![],
        magazines: vec![],
        objects: vec![],
        drcs: vec![],
    }
}
fn metadata(owner: TeletextPageAddress) -> TeletextResolvedPageEnhancementDraft {
    TeletextResolvedPageEnhancementDraft {
        address: owner,
        presentation: None,
        compositional_links: None,
        local_program: None,
        resources: TeletextResolvedPageResourceSelection {
            objects: TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            },
            drcs: TeletextDrcsPageLinks::unlinked(),
            defaults: [None; 4],
        },
    }
}
fn service_budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 128,
        max_objects: 64,
        max_structure_items: 4096,
        max_base_content_bytes: 4096,
        max_commands: 4096,
        max_collected_commands: 4096,
        max_collected_drcs_references: 128,
    }
}
fn declarations_budget() -> TeletextDeclaredServiceBudget {
    TeletextDeclaredServiceBudget {
        max_mots: 8,
        max_displays: 128,
        max_resources: 128,
        max_defaults: 128,
        max_objects: 64,
    }
}
fn producer(
    input: TeletextResolvedServiceDraft,
    mots: Vec<TeletextMotLevel25Declaration>,
) -> TeletextProducerPresentation {
    let service = TeletextSemanticService::new(input, service_budget()).unwrap();
    let selections = service
        .base()
        .pages
        .iter()
        .map(|page| TeletextDisplayMotSelection {
            display: page.address,
            mot: mots
                .iter()
                .find(|mot| mot.identity().page().magazine() == page.address.page.magazine())
                .map(TeletextMotLevel25Declaration::identity),
        })
        .collect();
    let declared =
        TeletextDeclaredService::new(service, mots, selections, vec![], declarations_budget())
            .unwrap();
    TeletextProducerPresentation::new(
        declared,
        TeletextProducerPresentationBudget { max_pages: 128 },
    )
    .unwrap()
}
fn theme(primary: u8, rgb: [u8; 3], width: u8) -> TeletextLevel25PresentationDefaults {
    TeletextLevel25PresentationDefaults {
        primary: TeletextPageCharsetDesignation::new(primary).unwrap(),
        secondary: Some(TeletextPageCharsetDesignation::new(1).unwrap()),
        palette: TeletextLevel25Palette {
            clut2: [TeletextRgb::new(rgb).unwrap(); 8],
            clut3: [TeletextRgb::new([3, 5, 7]).unwrap(); 8],
        },
        screen_colour: TeletextColourReference::new(18).unwrap(),
        row_colour: TeletextColourReference::new(27).unwrap(),
        black_background_substitution: true,
        remapping: TeletextColourRemapping::FgClut2BgClut3,
        panels: TeletextSidePanels {
            left: true,
            right: true,
            status: TeletextPanelStatus::Level25And35,
            width: TeletextPanelWidth::new(width).unwrap(),
        },
    }
}
fn themed(
    pages: Vec<TeletextPageDraft>,
    defaults: TeletextLevel25PresentationDefaults,
) -> TeletextResolvedServiceDraft {
    let mut input = draft(pages);
    input.magazines.push(TeletextMagazinePresentationDraft {
        magazine: 2,
        presentation: defaults,
    });
    input
}
fn budget() -> TeletextProducerCompositionBudget {
    TeletextProducerCompositionBudget {
        max_producers: 16,
        max_input_items: 4096,
        max_input_content_bytes: 4096,
        max_input_commands: 4096,
        max_storage_bytes: 4 * 1024 * 1024,
        max_work: 1024 * 1024,
        service: service_budget(),
        declarations: declarations_budget(),
    }
}
fn compose(
    producers: Vec<TeletextProducerPresentation>,
    entry: TeletextPageAddress,
) -> Result<TeletextColdProducerComposition, TeletextProducerCompositionError> {
    TeletextColdProducerComposition::new(
        producers,
        TeletextProducerCompositionMetadata {
            language: language("eng"),
            entry,
        },
        budget(),
    )
}
fn unused() -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    }
}
fn empty_link() -> TeletextMotObjectLink {
    TeletextMotObjectLink::new(
        unused(),
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 213 }; 2],
    )
}
fn mot(flags: Option<u8>) -> TeletextMotLevel25Declaration {
    let empty = empty_link();
    let mut public = [empty; 7];
    let associations = if let Some(flags) = flags {
        public[0] = TeletextMotObjectLink::new(
            TeletextX27LinkTarget::Page {
                page: resource(TeletextResourceKind::Pop, 0xa0, 0).page(),
            },
            TeletextMotSubpageExtent::new(0).unwrap(),
            true,
            TeletextMotFallback::new(flags).unwrap(),
            [TeletextMotDefaultObject::NoObject { raw_number: 77 }; 2],
        );
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                false,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )]
    } else {
        vec![]
    };
    let drcs = TeletextMotDrcsLink::new(unused(), TeletextMotSubpageExtent::new(0).unwrap(), true);
    TeletextMotLevel25Declaration::new(
        resource(TeletextResourceKind::Mot, 0xfe, 9),
        associations,
        TeletextMotLevel25Links::new(
            empty,
            public,
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}
fn object(kind: TeletextResourceKind, s1: u8, colour: u8) -> TeletextEnhancementObjectDefinition {
    let owner = resource(kind, 0xa0, s1);
    let id = TeletextObjectId::new(0xffff);
    TeletextEnhancementObjectDefinition {
        identity: if kind == TeletextResourceKind::Pop {
            TeletextObjectIdentity::public(owner, id).unwrap()
        } else {
            TeletextObjectIdentity::global(owner, id).unwrap()
        },
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetScreenColour {
                colour: TeletextColourReference::new(colour).unwrap(),
            },
        ])
        .unwrap(),
    }
}
fn pop(page: TeletextPageDraft, flags: u8) -> TeletextProducerPresentation {
    let owner = page.address;
    let mut input = draft(vec![page]);
    let mut meta = metadata(owner);
    meta.resources.objects.public = Some(resource(TeletextResourceKind::Pop, 0xa0, 0).page());
    input.pages.push(meta);
    input.objects.push(object(TeletextResourceKind::Pop, 0, 0));
    let original = mot(Some(flags));
    let declaration = TeletextMotLevel25Declaration::new(
        original.identity(),
        vec![TeletextMotPageAssociation::new(
            u8::try_from(owner.page.get() - 200).unwrap(),
            TeletextMotAssociationSelection::new(
                false,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )],
        original.links(),
    )
    .unwrap();
    producer(input, vec![declaration])
}

#[test]
fn different_complete_magazine_themes_derive_full_pages_and_retain_originals() {
    let day = theme(0x20, [15, 0, 0], 4);
    let night = theme(0x37, [0, 0, 15], 9);
    let first = producer(themed(vec![page(200, 0)], day), vec![]);
    let mut second_page = page(201, 16);
    second_page.language = language("deu");
    let second = producer(themed(vec![second_page.clone()], night), vec![]);
    let originals = [first.original().clone(), second.original().clone()];
    let storage = first.original().semantic().base().pages.as_ptr();
    let result = compose(vec![first, second], second_page.address).unwrap();
    assert_eq!(result.entry(), address(201, 16));
    assert_eq!(result.composed().semantic().base().language.as_str(), "eng");
    assert_eq!(result.composed().semantic().base().initial_page.get(), 201);
    assert!(result
        .composed()
        .semantic()
        .magazine_presentations()
        .is_empty());
    for (index, owner, defaults) in [(0, address(200, 0), day), (1, address(201, 16), night)] {
        let composed = &result.composed().semantic().page_enhancements()[&owner];
        assert_eq!(composed.presentation, Some(defaults));
        assert!(composed.local_program.is_none());
        let record = &result.pages()[index];
        assert_eq!(record.address(), owner);
        assert_eq!(record.producer_ordinal(), index);
        assert_eq!(record.page_ordinal(), 0);
        assert_eq!(
            record.derivation(),
            TeletextProducerPageDerivation::MagazineToPage { magazine: 2 }
        );
        assert_eq!(result.producers()[index].original(), &originals[index]);
    }
    assert_eq!(
        result.producers()[0]
            .original()
            .semantic()
            .base()
            .pages
            .as_ptr(),
        storage
    );
    assert_eq!(result.composed().semantic().base().pages[1], second_page);
    let (producers, pages, metadata, composed) = result.into_parts();
    assert_eq!(producers.len(), 2);
    assert_eq!(pages.len(), 2);
    assert_eq!(metadata.entry, address(201, 16));
    assert_eq!(composed.semantic().base().language.as_str(), "eng");
}

#[test]
fn identical_full_magazine_keeps_native_m29_and_actual_page_precedence() {
    let common = theme(0x20, [15, 0, 0], 4);
    let explicit = theme(0x37, [0, 0, 15], 9);
    let first = producer(themed(vec![page(200, 0)], common), vec![]);
    let mut input = themed(vec![page(201, 0)], common);
    let mut selected = metadata(address(201, 0));
    selected.presentation = Some(explicit);
    input.pages.push(selected.clone());
    let second = producer(input, vec![]);
    let result = compose(vec![first, second], address(200, 0)).unwrap();
    assert_eq!(
        result.composed().semantic().magazine_presentations()[&2],
        common
    );
    assert!(!result
        .composed()
        .semantic()
        .page_enhancements()
        .contains_key(&address(200, 0)));
    assert_eq!(
        result.composed().semantic().page_enhancements()[&address(201, 0)],
        selected
    );
    assert!(result
        .pages()
        .iter()
        .all(|page| page.derivation() == TeletextProducerPageDerivation::Unchanged));
    assert!(
        matches!(result.composed().semantic().effective_presentation(address(201,0)).unwrap(),
        TeletextEffectivePresentation::Extension {source:TeletextPresentationSource::Page,defaults} if *defaults==explicit)
    );
}

#[test]
fn header_and_original_pop_context_keep_absence_and_decoder_dependent_secondary() {
    let defaults = theme(0x37, [0, 0, 15], 9);
    for use_pop in [false, true] {
        let original = if use_pop {
            pop(page(200, 0), 14)
        } else {
            producer(draft(vec![page(200, 0)]), vec![])
        };
        let before = original.original().clone();
        let result = compose(
            vec![
                original,
                producer(themed(vec![page(201, 0)], defaults), vec![]),
            ],
            address(200, 0),
        )
        .unwrap();
        assert_eq!(result.producers()[0].original(), &before);
        assert_eq!(
            result.producers()[0].pages()[0].secondary_selection(),
            TeletextProducerSecondarySelection::DecoderDependent
        );
        let service = result.composed().semantic();
        assert!(service.magazine_presentations().is_empty());
        assert_eq!(
            service.page_enhancements()[&address(201, 0)].presentation,
            Some(defaults)
        );
        assert!(service
            .page_enhancements()
            .get(&address(200, 0))
            .is_none_or(|page| page.presentation.is_none()));
        assert_eq!(
            result.pages()[0].derivation(),
            TeletextProducerPageDerivation::Unchanged
        );
        if use_pop {
            assert_eq!(result.composed().mots(), before.mots());
            assert_eq!(
                result.composed().resolutions()[0].conditional_pop_fallback(),
                before.resolutions()[0].conditional_pop_fallback()
            );
            assert!(matches!(
                service.effective_presentation(address(200, 0)).unwrap(),
                TeletextEffectivePresentation::MotFallback { .. }
            ));
        } else {
            assert!(matches!(
                service.effective_presentation(address(200, 0)).unwrap(),
                TeletextEffectivePresentation::Header { .. }
            ));
        }
    }
}

#[test]
fn exact_subcodes_local_presence_languages_and_navigation_survive_derived_pages() {
    let day = theme(0x20, [15, 0, 0], 4);
    let night = theme(0x37, [0, 0, 15], 9);
    let mut linked = page(200, 0);
    linked.navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Green,
        target: TeletextNavigationTarget {
            page: address(200, 16).page,
            subcode: Some(address(200, 16).subcode),
        },
    });
    let mut translated = page(200, 16);
    translated.language = language("eng");
    let mut input = themed(vec![linked.clone(), translated.clone(), page(200, 32)], day);
    let mut empty = metadata(address(200, 16));
    empty.local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
    let mut nonempty = metadata(address(200, 32));
    nonempty.local_program = Some(
        TeletextEnhancementProgram::new(vec![TeletextEnhancementCommand::SetScreenColour {
            colour: TeletextColourReference::BLACK,
        }])
        .unwrap(),
    );
    input.pages.extend([empty, nonempty]);
    let original = producer(input, vec![]);
    let before = original.original().clone();
    let result = compose(
        vec![
            original,
            producer(themed(vec![page(201, 0)], night), vec![]),
        ],
        address(200, 16),
    )
    .unwrap();
    assert_eq!(
        result.composed().semantic().base().pages[..3],
        before.semantic().base().pages
    );
    assert_eq!(result.composed().semantic().base().pages[0], linked);
    assert_eq!(result.composed().semantic().base().pages[1], translated);
    for (index, subcode, presence) in [
        (0, 0, TeletextProducerLocalProgramPresence::Absent),
        (1, 16, TeletextProducerLocalProgramPresence::PresentEmpty),
        (2, 32, TeletextProducerLocalProgramPresence::PresentNonEmpty),
    ] {
        let owner = address(200, subcode);
        assert_eq!(result.pages()[index].page_ordinal(), index);
        assert_eq!(result.pages()[index].producer_ordinal(), 0);
        assert_eq!(
            result.producers()[0]
                .page(owner)
                .unwrap()
                .local_program_presence(),
            presence
        );
        let actual = &result.composed().semantic().page_enhancements()[&owner];
        assert_eq!(actual.presentation, Some(day));
        let original = before
            .semantic()
            .page_enhancements()
            .get(&owner)
            .and_then(|page| page.local_program.as_ref());
        assert_eq!(actual.local_program.as_ref(), original);
    }
}

fn with_shared_resources(owner: TeletextPageAddress) -> TeletextProducerPresentation {
    let mut input = draft(vec![page(owner.page.get(), owner.subcode.get())]);
    input.objects.push(object(TeletextResourceKind::Pop, 0, 0));
    input.drcs.push(
        TeletextDrcsSubtable::new(
            resource(TeletextResourceKind::Drcs, 0xc0, 3),
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(7).unwrap(),
                bitmap: TeletextDrcsBitmap::new([0xaaa; 10]).unwrap(),
            }],
        )
        .unwrap(),
    );
    producer(input, vec![mot(None)])
}

#[test]
fn identical_full_resources_and_mot_coalesce_and_safe_bank_adoption_keeps_roles() {
    let first = with_shared_resources(address(200, 0));
    let second = with_shared_resources(address(201, 0));
    let originals = [first.original().clone(), second.original().clone()];
    let result = compose(vec![first, second], address(200, 0)).unwrap();
    assert_eq!(result.composed().mots().len(), 1);
    assert_eq!(
        result.composed().semantic().objects().definitions().len(),
        1
    );
    assert_eq!(result.composed().semantic().drcs().subtables().len(), 1);
    assert_eq!(
        result.composed().semantic().drcs().subtables()[0].glyphs()[0]
            .bitmap
            .get(),
        [0xaaa; 10]
    );
    assert_eq!(result.composed().resource_inventory().len(), 3);
    for (index, original) in originals.iter().enumerate() {
        assert_eq!(result.producers()[index].original(), original);
    }
    let result = compose(
        vec![
            with_shared_resources(address(200, 0)),
            producer(draft(vec![page(201, 0)]), vec![]),
        ],
        address(200, 0),
    )
    .unwrap();
    assert_eq!(result.composed().mots().len(), 1);
    assert_eq!(result.composed().resolutions()[1].defaults(), &[None; 4]);
    assert!(result.composed().resolutions()[1]
        .conditional_pop_fallback()
        .is_none());
    assert_eq!(result.producers()[1].original().mots().len(), 0);
}

#[test]
fn adopted_bank_cannot_contaminate_roles_or_pop_context_and_conflicting_banks_are_located() {
    let result = compose(
        vec![
            pop(page(200, 0), 14),
            producer(draft(vec![page(200, 16)]), vec![]),
        ],
        address(200, 0),
    );
    let error = result.unwrap_err();
    assert_eq!(error.producer, Some(1));
    assert_eq!(error.page, Some(0));
    assert_eq!(error.display, Some(address(200, 16)));
    assert!(matches!(
        error.reason,
        TeletextProducerCompositionFailure::Context { .. }
            | TeletextProducerCompositionFailure::Declarations(_)
    ));
    let error = compose(
        vec![pop(page(200, 0), 1), pop(page(201, 0), 14)],
        address(200, 0),
    )
    .unwrap_err();
    assert_eq!(error.producer, Some(1));
    assert_eq!(
        error.resource,
        Some(resource(TeletextResourceKind::Mot, 0xfe, 9))
    );
    assert!(
        matches!(error.reason,TeletextProducerCompositionFailure::MotConflict {first_producer:0,first_mot}
        if first_mot==resource(TeletextResourceKind::Mot,0xfe,9))
    );
}

#[test]
fn resource_collision_uses_raw_page_and_s1_independently_of_kind_but_not_other_s1() {
    for (kind, colour) in [
        (TeletextResourceKind::Pop, 1),
        (TeletextResourceKind::Gpop, 0),
    ] {
        let mut first = draft(vec![page(200, 0)]);
        first.objects.push(object(TeletextResourceKind::Pop, 0, 0));
        let mut second = draft(vec![page(201, 0)]);
        second.objects.push(object(kind, 0, colour));
        let error = compose(
            vec![producer(first, vec![]), producer(second, vec![])],
            address(200, 0),
        )
        .unwrap_err();
        assert_eq!(error.producer, Some(1));
        assert_eq!(error.resource, Some(resource(kind, 0xa0, 0)));
        assert!(matches!(
            error.reason,
            TeletextProducerCompositionFailure::ResourceConflict { first_producer: 0 }
        ));
    }
    let mut first = draft(vec![page(200, 0)]);
    first.objects.push(object(TeletextResourceKind::Pop, 0, 0));
    let mut second = draft(vec![page(201, 0)]);
    second
        .objects
        .push(object(TeletextResourceKind::Gpop, 1, 0));
    let result = compose(
        vec![producer(first, vec![]), producer(second, vec![])],
        address(200, 0),
    )
    .unwrap();
    assert_eq!(
        result.composed().semantic().objects().definitions().len(),
        2
    );
    assert_eq!(result.composed().resource_inventory().len(), 2);
    // This is logical cold composition; S1-space physical allocation remains downstream.

    let mut producers = Vec::new();
    for (number, bitmap) in [(200, [0xaaa; 10]), (201, [0x555; 10])] {
        let mut input = draft(vec![page(number, 0)]);
        input.drcs.push(
            TeletextDrcsSubtable::new(
                resource(TeletextResourceKind::Drcs, 0xc0, 3),
                vec![TeletextDrcsGlyphDefinition {
                    slot: TeletextDrcsGlyphSlot::new(7).unwrap(),
                    bitmap: TeletextDrcsBitmap::new(bitmap).unwrap(),
                }],
            )
            .unwrap(),
        );
        producers.push(producer(input, vec![]));
    }
    let error = compose(producers, address(200, 0)).unwrap_err();
    assert_eq!(error.producer, Some(1));
    assert_eq!(
        error.resource,
        Some(resource(TeletextResourceKind::Drcs, 0xc0, 3))
    );
    assert!(matches!(
        error.reason,
        TeletextProducerCompositionFailure::ResourceConflict { first_producer: 0 }
    ));
}

#[test]
fn identical_display_is_not_deduplicated_entry_is_exact_and_debug_is_safe() {
    let error = compose(
        vec![
            producer(draft(vec![page(200, 16)]), vec![]),
            producer(draft(vec![page(200, 16)]), vec![]),
        ],
        address(200, 16),
    )
    .unwrap_err();
    assert_eq!(error.producer, Some(1));
    assert_eq!(error.page, Some(0));
    assert_eq!(error.display, Some(address(200, 16)));
    assert!(matches!(
        error.reason,
        TeletextProducerCompositionFailure::DuplicateDisplay {
            first_producer: 0,
            first_page: 0
        }
    ));
    let error = compose(
        vec![producer(draft(vec![page(200, 16)]), vec![])],
        address(200, 0),
    )
    .unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextProducerCompositionFailure::MissingEntry
    ));
    assert!(matches!(
        compose(vec![], address(200, 0)).unwrap_err().reason,
        TeletextProducerCompositionFailure::Empty
    ));
    let result = compose(
        vec![with_shared_resources(address(200, 16))],
        address(200, 16),
    )
    .unwrap();
    let debug = format!("{result:?}");
    assert!(!debug.contains("raw_number"));
    assert!(!debug.contains("2730"));
    assert!(!debug.contains("SetScreenColour"));
}

fn default_producer() -> TeletextProducerPresentation {
    let definition = object(TeletextResourceKind::Pop, 0, 0);
    let selection = TeletextDefaultObjectSelection {
        target: definition.identity,
        object_type: TeletextObjectType::Passive,
    };
    let mut input = draft(vec![page(200, 0), page(200, 16)]);
    for owner in [address(200, 0), address(200, 16)] {
        let mut meta = metadata(owner);
        meta.resources.objects.public = Some(definition.identity.resource().unwrap().page());
        meta.resources.defaults[2] = Some(selection);
        if owner.subcode.get() == 16 {
            meta.local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
        }
        input.pages.push(meta);
    }
    input.objects.push(definition);
    let mut public = [empty_link(); 7];
    public[0] = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page {
            page: selection.target.resource().unwrap().page(),
        },
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(1).unwrap(),
        [
            TeletextMotDefaultObject::Object {
                target: selection.target,
                object_type: selection.object_type,
            },
            TeletextMotDefaultObject::NoObject { raw_number: 77 },
        ],
    );
    let empty = mot(None);
    let links = TeletextMotLevel25Links::new(
        empty_link(),
        public,
        empty.links().drcs_link(None),
        [empty.links().drcs_link(None); 7],
        TeletextMotEnhancementCounts::new(0, 0, 0),
    )
    .unwrap();
    let bank = TeletextMotLevel25Declaration::new(
        empty.identity(),
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                false,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )],
        links,
    )
    .unwrap();
    producer(input, vec![bank])
}

#[test]
fn sparse_default_ownership_survives_absent_and_present_empty_local_activation() {
    let first = default_producer();
    let before = first.original().clone();
    let result = compose(
        vec![first, producer(draft(vec![page(201, 0)]), vec![])],
        address(200, 16),
    )
    .unwrap();
    for index in 0..2 {
        assert_eq!(
            result.composed().resolutions()[index].defaults(),
            before.resolutions()[index].defaults()
        );
        assert_eq!(
            result.composed().resolutions()[index].conditional_pop_fallback(),
            before.resolutions()[index].conditional_pop_fallback()
        );
    }
    let service = result.composed().semantic();
    let (source, program) = service.effective_program(address(200, 0)).unwrap();
    assert_eq!(
        source,
        TeletextEffectiveProgramSource::Defaults { slots: vec![2] }
    );
    assert_eq!(program.commands().len(), 1);
    let (source, program) = service.effective_program(address(200, 16)).unwrap();
    assert_eq!(source, TeletextEffectiveProgramSource::Local);
    assert!(program.commands().is_empty());
    assert_eq!(result.composed().defaults(), before.defaults());
    assert_eq!(result.producers()[0].original(), &before);
}

#[test]
fn raw_aggregate_budgets_and_final_output_budgets_are_independent() {
    for dimension in 0..8 {
        let mut policy = budget();
        match dimension {
            0 => policy.max_producers = 0,
            1 => policy.max_input_items = 0,
            2 => policy.max_input_content_bytes = 0,
            3 => policy.max_input_commands = 0,
            4 => policy.max_storage_bytes = 0,
            5 => policy.max_work = 0,
            6 => policy.service.max_records = 0,
            _ => policy.declarations.max_displays = 0,
        }
        let input = if dimension == 3 {
            with_shared_resources(address(200, 0))
        } else {
            producer(draft(vec![page(200, 0)]), vec![])
        };
        let error = TeletextColdProducerComposition::new(
            vec![input],
            TeletextProducerCompositionMetadata {
                language: language("eng"),
                entry: address(200, 0),
            },
            policy,
        )
        .unwrap_err();
        match dimension {
            0..6 => assert!(matches!(
                error.reason,
                TeletextProducerCompositionFailure::Budget { maximum: 0, .. }
                    | TeletextProducerCompositionFailure::StorageOverflow
            )),
            6 => assert!(matches!(
                error.reason,
                TeletextProducerCompositionFailure::Semantic(_)
            )),
            _ => assert!(matches!(
                error.reason,
                TeletextProducerCompositionFailure::Declarations(_)
            )),
        }
    }
    let mut input = draft(vec![page(200, 0)]);
    input.base.pages[0].rows.push(TeletextRow {
        row: 1,
        runs: vec![TeletextRun {
            column: 1,
            style: TeletextStyle::default(),
            content: TeletextRunContent::Text {
                text: "private synthetic content".into(),
            },
        }],
    });
    let mut policy = budget();
    policy.max_input_content_bytes = 1;
    let error = TeletextColdProducerComposition::new(
        vec![producer(input, vec![])],
        TeletextProducerCompositionMetadata {
            language: language("eng"),
            entry: address(200, 0),
        },
        policy,
    )
    .unwrap_err();
    assert!(matches!(
        error.reason,
        TeletextProducerCompositionFailure::Budget { maximum: 1, .. }
    ));
    assert!(!format!("{error:?}").contains("private synthetic content"));
}

#[test]
fn aggregate_raw_counts_have_literal_exact_fit_and_one_short_cutoffs_before_derivation() {
    // Two independent originals: each has producer/page/provenance/potential
    // metadata/snapshot/selection records; three language strings total fifteen bytes.
    for (items, content, accepted) in [(12, 15, true), (11, 15, false), (12, 14, false)] {
        let producers = vec![
            producer(draft(vec![page(200, 0)]), vec![]),
            producer(draft(vec![page(201, 0)]), vec![]),
        ];
        let mut policy = budget();
        policy.max_producers = 2;
        policy.max_input_items = items;
        policy.max_input_content_bytes = content;
        policy.max_input_commands = 0;
        let result = TeletextColdProducerComposition::new(
            producers,
            TeletextProducerCompositionMetadata {
                language: language("eng"),
                entry: address(201, 0),
            },
            policy,
        );
        if accepted {
            assert_eq!(result.unwrap().pages().len(), 2);
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.producer, Some(1));
            assert!(matches!(
                error.reason,
                TeletextProducerCompositionFailure::Budget {
                    category: "input item",
                    count: 12,
                    maximum: 11
                } | TeletextProducerCompositionFailure::Budget {
                    category: "input content byte",
                    count: 15,
                    maximum: 14
                }
            ));
        }
    }
    for maximum in [0, 1] {
        let mut input = draft(vec![page(200, 0)]);
        let mut meta = metadata(address(200, 0));
        meta.local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
        input.pages.push(meta);
        input.objects.push(TeletextEnhancementObjectDefinition {
            identity: TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(7)),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![
                TeletextEnhancementCommand::SetScreenColour {
                    colour: TeletextColourReference::BLACK,
                },
            ])
            .unwrap(),
        });
        let mut policy = budget();
        policy.max_producers = 1;
        policy.max_input_items = 8;
        policy.max_input_content_bytes = 9;
        policy.max_input_commands = maximum;
        let result = TeletextColdProducerComposition::new(
            vec![producer(input, vec![])],
            TeletextProducerCompositionMetadata {
                language: language("eng"),
                entry: address(200, 0),
            },
            policy,
        );
        if maximum == 1 {
            assert_eq!(
                result
                    .unwrap()
                    .composed()
                    .semantic()
                    .objects()
                    .definitions()
                    .len(),
                1
            );
        } else {
            let error = result.unwrap_err();
            assert_eq!(error.producer, Some(0));
            assert!(matches!(
                error.reason,
                TeletextProducerCompositionFailure::Budget {
                    category: "input command",
                    count: 1,
                    maximum: 0
                }
            ));
        }
    }
}
