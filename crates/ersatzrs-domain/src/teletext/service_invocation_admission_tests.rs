//! Constructor-level A.8(1) evidence: one effective program per exact display.

use super::display_geometry_tests::{definition, glyph, invoke, local, owner, set};
use super::*;

fn budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 32,
        max_objects: 16,
        max_structure_items: 4096,
        max_base_content_bytes: 1024,
        max_commands: 4096,
        max_collected_commands: 4096,
        max_collected_drcs_references: 0,
    }
}

fn draft(
    pages: Vec<TeletextResolvedPageEnhancementDraft>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextResolvedServiceDraft {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    let base_pages = pages
        .iter()
        .map(|page| TeletextPageDraft {
            address: page.address,
            kind: TeletextPageKind::FullPage,
            language: language.clone(),
            national_option: TeletextNationalOption::new(0).unwrap(),
            rows: vec![],
            navigation: vec![],
        })
        .collect();
    TeletextResolvedServiceDraft {
        base: TeletextPageSetDraft {
            language,
            initial_page: owner().page,
            pages: base_pages,
        },
        pages,
        magazines: vec![],
        objects,
        drcs: vec![],
    }
}

fn page(
    address: TeletextPageAddress,
    main: Option<Vec<TeletextEnhancementCommand>>,
) -> TeletextResolvedPageEnhancementDraft {
    TeletextResolvedPageEnhancementDraft {
        address,
        presentation: None,
        compositional_links: None,
        local_program: main.map(|commands| TeletextEnhancementProgram::new(commands).unwrap()),
        resources: TeletextResolvedPageResourceSelection {
            objects: TeletextResolvedObjectPageLinks {
                public: None,
                global: None,
            },
            defaults: [None; 4],
            drcs: TeletextDrcsPageLinks::unlinked(),
        },
    }
}

fn resource_target(kind: TeletextResourceKind, number: u8, id: u16) -> TeletextObjectIdentity {
    let resource = TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, number).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    match kind {
        TeletextResourceKind::Pop => {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap()
        }
        TeletextResourceKind::Gpop => {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(id)).unwrap()
        }
        _ => unreachable!(),
    }
}

fn children(target: TeletextObjectIdentity, count: u16) -> Vec<TeletextEnhancementCommand> {
    (0..count)
        .flat_map(|index| {
            [
                set(
                    u8::try_from(index / 40).unwrap(),
                    u8::try_from(index % 40).unwrap(),
                ),
                invoke(target, TeletextObjectType::Passive, None),
            ]
        })
        .collect()
}

fn assert_rejection(
    draft: &TeletextResolvedServiceDraft,
    address: TeletextPageAddress,
    program_source: TeletextEffectiveProgramSource,
    source_location: TeletextEnhancementSource,
) {
    let original = draft.clone();
    assert_eq!(
        TeletextSemanticService::new(draft.clone(), budget()).unwrap_err(),
        TeletextServiceError::Collection {
            program_source,
            error: TeletextEnhancementCollectionError {
                display: address,
                source_location,
                reason: TeletextEnhancementCollectionFailure::InvocationCount {
                    object_type: TeletextObjectType::Passive,
                },
            },
        }
    );
    assert_eq!(*draft, original);
}

#[test]
fn sparse_defaults_share_one_counter_and_explicit_empty_suppresses_execution() {
    let active = resource_target(TeletextResourceKind::Gpop, 0xa0, 0xffff);
    let child = resource_target(TeletextResourceKind::Gpop, 0xa0, 0);
    let last = resource_target(TeletextResourceKind::Pop, 0xa1, 0x0100);
    let mut enhancement = page(owner(), None);
    enhancement.resources.objects.global = Some(TeletextRawDataPageNumber::new(2, 0xa0).unwrap());
    enhancement.resources.defaults[0] = Some(TeletextDefaultObjectSelection {
        target: active,
        object_type: TeletextObjectType::Active,
    });
    let objects = vec![
        definition(active, TeletextObjectType::Active, children(child, 127)),
        definition(child, TeletextObjectType::Passive, vec![glyph(0)]),
        definition(last, TeletextObjectType::Passive, vec![glyph(8)]),
    ];
    let accepted =
        TeletextSemanticService::new(draft(vec![enhancement.clone()], objects.clone()), budget())
            .unwrap();
    let (source, references) = accepted.collected(owner()).unwrap();
    assert_eq!(
        *source,
        TeletextEffectiveProgramSource::Defaults { slots: vec![0] }
    );
    assert_eq!(references.invocation_count(TeletextObjectType::Active), 1);
    assert_eq!(
        references.invocation_count(TeletextObjectType::Passive),
        127
    );
    enhancement.resources.defaults[3] = Some(TeletextDefaultObjectSelection {
        target: last,
        object_type: TeletextObjectType::Passive,
    });
    let mut supplied = draft(vec![enhancement], objects);
    assert_rejection(
        &supplied,
        owner(),
        TeletextEffectiveProgramSource::Defaults { slots: vec![0, 3] },
        TeletextEnhancementSource {
            object: None,
            command: 1,
        },
    );
    // Declared defaults remain present, but an explicitly supplied empty local
    // program suppresses them; their latent 128 calls are not display execution.
    supplied.pages[0].local_program = Some(TeletextEnhancementProgram::new(vec![]).unwrap());
    let empty = TeletextSemanticService::new(supplied.clone(), budget()).unwrap();
    let (source, references) = empty.collected(owner()).unwrap();
    assert_eq!(*source, TeletextEffectiveProgramSource::Local);
    assert_eq!(references.invocation_count(TeletextObjectType::Active), 0);
    assert_eq!(references.invocation_count(TeletextObjectType::Passive), 0);
    assert_eq!(
        empty
            .page_enhancements()
            .get(&owner())
            .unwrap()
            .resources
            .defaults,
        supplied.pages[0].resources.defaults
    );
}

#[test]
fn nested_calls_across_independent_default_roots_share_the_display_counter() {
    let global_active = resource_target(TeletextResourceKind::Gpop, 0xa0, 0xffff);
    let global_child = resource_target(TeletextResourceKind::Gpop, 0xa0, 0);
    let public_active = resource_target(TeletextResourceKind::Pop, 0xa1, 0xffff);
    let public_child = resource_target(TeletextResourceKind::Pop, 0xa1, 0);
    for second_count in [63, 64] {
        let mut enhancement = page(owner(), None);
        enhancement.resources.objects = TeletextResolvedObjectPageLinks {
            global: Some(TeletextRawDataPageNumber::new(2, 0xa0).unwrap()),
            public: Some(TeletextRawDataPageNumber::new(2, 0xa1).unwrap()),
        };
        enhancement.resources.defaults[0] = Some(TeletextDefaultObjectSelection {
            target: global_active,
            object_type: TeletextObjectType::Active,
        });
        enhancement.resources.defaults[3] = Some(TeletextDefaultObjectSelection {
            target: public_active,
            object_type: TeletextObjectType::Active,
        });
        let supplied = draft(
            vec![enhancement],
            vec![
                definition(
                    global_active,
                    TeletextObjectType::Active,
                    children(global_child, 64),
                ),
                definition(
                    public_active,
                    TeletextObjectType::Active,
                    children(public_child, second_count),
                ),
                definition(global_child, TeletextObjectType::Passive, vec![glyph(0)]),
                definition(public_child, TeletextObjectType::Passive, vec![glyph(0)]),
            ],
        );
        if second_count == 63 {
            let accepted = TeletextSemanticService::new(supplied, budget()).unwrap();
            let (source, references) = accepted.collected(owner()).unwrap();
            assert_eq!(
                *source,
                TeletextEffectiveProgramSource::Defaults { slots: vec![0, 3] }
            );
            assert_eq!(references.invocation_count(TeletextObjectType::Active), 2);
            assert_eq!(
                references.invocation_count(TeletextObjectType::Passive),
                127
            );
        } else {
            assert_rejection(
                &supplied,
                owner(),
                TeletextEffectiveProgramSource::Defaults { slots: vec![0, 3] },
                TeletextEnhancementSource {
                    object: Some(public_active),
                    command: 127,
                },
            );
        }
    }
}

#[test]
fn exact_subcode_owners_have_independent_counters_in_one_service() {
    let other = TeletextPageAddress {
        subcode: TeletextSubcode::new(0x1235).unwrap(),
        ..owner()
    };
    let first = local(0xffff);
    let second = TeletextObjectIdentity::local(other, TeletextObjectId::new(0xffff));
    let main = |target, count| {
        (0..count)
            .map(|_| invoke(target, TeletextObjectType::Passive, None))
            .collect()
    };
    let mut supplied = draft(
        vec![
            page(owner(), Some(main(first, 127))),
            page(other, Some(main(second, 127))),
        ],
        vec![
            definition(first, TeletextObjectType::Passive, vec![]),
            definition(second, TeletextObjectType::Passive, vec![]),
        ],
    );
    let accepted = TeletextSemanticService::new(supplied.clone(), budget()).unwrap();
    for address in [owner(), other] {
        let (source, references) = accepted.collected(address).unwrap();
        assert_eq!(*source, TeletextEffectiveProgramSource::Local);
        assert_eq!(
            references.invocation_count(TeletextObjectType::Passive),
            127
        );
    }
    // The invalid second display must be blamed as that exact owner; neither
    // constructor collection nor resource IDs may pool the first one's calls.
    supplied.pages[1].local_program =
        Some(TeletextEnhancementProgram::new(main(second, 128)).unwrap());
    assert_rejection(
        &supplied,
        other,
        TeletextEffectiveProgramSource::Local,
        TeletextEnhancementSource {
            object: None,
            command: 127,
        },
    );
}
