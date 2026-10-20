//! Native replay only: no editor, receipt, installation or client claim.
use super::*;
use ersatzrs_app::teletext_design_documents::{
    TeletextSavedObjectResourceAddition, TeletextSavedResourceTransmission,
    TeletextSavedResourceUpdate,
};
use ersatzrs_domain::*;
use ersatzrs_ffmpeg::teletext::TeletextResourceFullUpdate;
use serde_json::json;
use std::time::Duration;

fn address(number: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(number).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    }
}
fn resource(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(2, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}
fn target(resource: TeletextResourceIdentity, id: u16) -> TeletextObjectIdentity {
    match resource.kind() {
        TeletextResourceKind::Pop => {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(id))
        }
        TeletextResourceKind::Gpop => {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(id))
        }
        _ => unreachable!(),
    }
    .unwrap()
}
fn definition(
    resource: TeletextResourceIdentity,
    id: u16,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: target(resource, id),
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}
fn policy() -> TeletextChannelDocumentEncodingPolicy {
    use crate::host::teletext_runtime_policy::{
        parse_native_teletext_authoring_input, parse_native_teletext_authoring_policy,
    };
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    let root = value.as_object_mut().unwrap();
    root.remove("source");
    root.remove("recovery_timeout_ms");
    root.insert("operation_timeout_ms".to_owned(), json!(1000));
    root.insert(
        "preview".to_owned(),
        json!({"max_pages":16,"max_model_records":32768,
        "max_pixels":168_000,"max_diagnostics":1400,"max_work":50_000_000}),
    );
    let policy = parse_native_teletext_authoring_policy(&value.to_string()).unwrap();
    let input = parse_native_teletext_authoring_input(
        &json!({"input_version":1,"channel_id":1,"entry_addon_id":"org.synthetic.bank",
            "entry":{"page":200,"subcode":0},"v1_region":"western_latin",
            "resource_transmissions":[]})
        .to_string(),
        &policy,
    )
    .unwrap();
    policy.encoding_policy(&input).unwrap()
}
fn source() -> TeletextDeclaredService {
    let policy = policy();
    let public = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let mot_id = resource(TeletextResourceKind::Mot, 0xfe, 9);
    let selection = TeletextDefaultObjectSelection {
        target: target(public, 19),
        object_type: TeletextObjectType::Passive,
    };
    let language = TeletextLanguage::new("eng".into()).unwrap();
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: language.clone(),
                initial_page: address(200).page,
                pages: vec![TeletextPageDraft {
                    address: address(200),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(0).unwrap(),
                    rows: vec![TeletextRow {
                        row: 1,
                        runs: vec![TeletextRun {
                            column: 8,
                            style: TeletextStyle::default(),
                            content: TeletextRunContent::Text {
                                text: "£ ORIGINAL".into(),
                            },
                        }],
                    }],
                    navigation: vec![TeletextNavigationLink {
                        role: TeletextNavigationRole::Red,
                        target: TeletextNavigationTarget {
                            page: address(200).page,
                            subcode: Some(address(200).subcode),
                        },
                    }],
                }],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: address(200),
                presentation: None,
                compositional_links: None,
                local_program: None,
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: Some(public.page()),
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [None, None, Some(selection), None],
                },
            }],
            magazines: vec![],
            objects: vec![definition(public, 19, TeletextObjectType::Passive, vec![])],
            drcs: vec![],
        },
        policy.validation.service,
    )
    .unwrap()
    .with_base_text_spellings(
        vec![TeletextBaseTextSpelling::new(
            address(200),
            TeletextEnhancementPosition::new(1, 8).unwrap(),
            '£',
            TeletextGlyphCode::new(b' ').unwrap(),
            TeletextEnhancementCharsetDesignation::new(0).unwrap(),
            TeletextColumnOperation::G2 {
                code: TeletextGlyphCode::new(0x23).unwrap(),
            },
        )
        .unwrap()],
        policy.validation.service,
    )
    .unwrap();
    let mot = source_mot(public, selection, mot_id);
    TeletextDeclaredService::new(
        semantic,
        vec![mot],
        vec![TeletextDisplayMotSelection {
            display: address(200),
            mot: Some(mot_id),
        }],
        vec![],
        policy.validation.declarations,
    )
    .unwrap()
}
fn source_mot(
    public: TeletextResourceIdentity,
    selection: TeletextDefaultObjectSelection,
    mot_id: TeletextResourceIdentity,
) -> TeletextMotLevel25Declaration {
    let unused = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    };
    let empty = TeletextMotObjectLink::new(
        unused,
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 213 }; 2],
    );
    let mut public_links = [empty; 7];
    public_links[0] = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page {
            page: public.page(),
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
    let drcs = TeletextMotDrcsLink::new(unused, TeletextMotSubpageExtent::new(0).unwrap(), true);
    TeletextMotLevel25Declaration::new(
        mot_id,
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                false,
                Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )],
        TeletextMotLevel25Links::new(
            empty,
            public_links,
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}
fn banks() -> Vec<TeletextObjectResourceAddition> {
    let public = resource(TeletextResourceKind::Pop, 0xa1, 15);
    let global = resource(TeletextResourceKind::Gpop, 0xa2, 0);
    vec![
        TeletextObjectResourceAddition::new(
            global,
            vec![definition(
                global,
                33,
                TeletextObjectType::Active,
                vec![TeletextEnhancementCommand::InvokeObject {
                    target: target(public, 32),
                    object_type: TeletextObjectType::Passive,
                    offset: None,
                }],
            )],
            policy().validation.service,
        )
        .unwrap(),
        TeletextObjectResourceAddition::new(
            public,
            vec![definition(
                public,
                32,
                TeletextObjectType::Passive,
                vec![TeletextEnhancementCommand::AtColumn {
                    column: 8,
                    operation: TeletextColumnOperation::G0 {
                        code: TeletextGlyphCode::new(b'B').unwrap(),
                    },
                }],
            )],
            policy().validation.service,
        )
        .unwrap(),
        TeletextObjectResourceAddition::new(
            resource(TeletextResourceKind::Pop, 0xa1, 0),
            vec![],
            policy().validation.service,
        )
        .unwrap(),
    ]
}
fn saved(
    index: usize,
    banks: Vec<TeletextObjectResourceAddition>,
) -> Vec<TeletextSavedObjectResourceAddition> {
    banks
        .into_iter()
        .map(|bank| {
            let owner = bank.resource();
            TeletextSavedObjectResourceAddition::new(
                index,
                bank,
                TeletextSavedResourceTransmission {
                    resource: owner,
                    counter: TeletextResourceUpdateCounter::new(7).unwrap(),
                    update: TeletextSavedResourceUpdate::Changed,
                    last_subpage: !(owner.page()
                        == resource(TeletextResourceKind::Pop, 0xa1, 0).page()
                        && owner.logical_subpage().get() == 0),
                },
            )
            .unwrap()
        })
        .collect()
}
fn with_transmissions(
    mut policy: TeletextChannelDocumentEncodingPolicy,
    original: &TeletextDeclaredService,
    added: &[TeletextSavedObjectResourceAddition],
) -> TeletextChannelDocumentEncodingPolicy {
    policy.transmissions = original
        .resource_inventory()
        .iter()
        .map(|resource| TeletextServiceResourceTransmission {
            resource: *resource,
            counter: TeletextResourceUpdateCounter::new(3).unwrap(),
            update: TeletextResourceFullUpdate::Unchanged,
            last_subpage: true,
        })
        .chain(added.iter().map(|saved| {
            let record = saved.transmission();
            TeletextServiceResourceTransmission {
                resource: record.resource,
                counter: record.counter,
                update: TeletextResourceFullUpdate::Changed,
                last_subpage: record.last_subpage,
            }
        }))
        .collect();
    policy
}
fn edits<'a>(
    banks: &'a [TeletextSavedObjectResourceAddition],
    definitions: &'a [TeletextResourceObjectDefinitionAddition],
) -> EditorialResourceDocuments<'a> {
    EditorialResourceDocuments {
        base_text_edits: &[],
        main_programs: &[],
        main_presence: &[],
        programs: (vec![], &[], &[], &[], &[]),
        resource_operands: &[],
        resource_programs: &[],
        resource_additions: definitions,
        object_resource_additions: banks,
        object_subpage_announcements: &[],
        object_page_selections: &[],
        main_resource_invocations: &[],
    }
}
fn replay(
    documents: Vec<TeletextProfileDocument>,
    banks: &[TeletextSavedObjectResourceAddition],
    definitions: &[TeletextResourceObjectDefinitionAddition],
    policy: TeletextChannelDocumentEncodingPolicy,
    region: Option<TeletextEncodingRegion>,
    entry: usize,
) -> Result<EncodedChannelTeletextOriginals, ChannelTeletextDocumentEncodingError> {
    encode_original_producer_documents_with_editorial_resources(
        Some(entry),
        documents,
        edits(banks, definitions),
        Instant::now() + Duration::from_secs(30),
        region,
        policy,
    )
}
fn authored(source: &TeletextDeclaredService) -> TeletextProfileDocument {
    TeletextProfileDocument::Authored(Box::new(source.clone()))
}
fn rebuilt(
    original: &TeletextDeclaredService,
    banks: &[TeletextObjectResourceAddition],
    definitions: &[TeletextResourceObjectDefinitionAddition],
) -> TeletextDeclaredService {
    let before = original.semantic();
    let mut objects = before.objects().definitions().to_vec();
    objects.extend(
        banks
            .iter()
            .flat_map(|bank| bank.definitions().iter().cloned()),
    );
    objects.extend(
        definitions
            .iter()
            .map(|addition| addition.definition().clone()),
    );
    let mut empty = original.empty_object_pages().to_vec();
    empty.extend(
        banks
            .iter()
            .filter(|bank| bank.definitions().is_empty())
            .map(TeletextObjectResourceAddition::resource),
    );
    let semantic = TeletextSemanticService::new_with_object_companions(
        TeletextResolvedServiceDraft {
            base: before.base().clone(),
            pages: before.page_enhancements().values().cloned().collect(),
            magazines: before
                .magazine_presentations()
                .iter()
                .map(
                    |(magazine, presentation)| TeletextMagazinePresentationDraft {
                        magazine: *magazine,
                        presentation: *presentation,
                    },
                )
                .collect(),
            objects,
            drcs: before.drcs().subtables().to_vec(),
        },
        empty.clone(),
        policy().composition.service,
    )
    .unwrap()
    .with_base_text_spellings(
        before.base_text_spellings().values().copied().collect(),
        policy().composition.service,
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        original.mots().to_vec(),
        original.selections().to_vec(),
        empty,
        policy().composition.declarations,
    )
    .unwrap()
}
fn units(value: &EncodedChannelTeletextOriginals) -> Vec<Vec<[u8; 46]>> {
    let encoded = value.encoded.as_ref().unwrap();
    std::iter::once(encoded.magazine_units().to_vec())
        .chain(
            encoded
                .pages()
                .iter()
                .map(|page| page.data_units().to_vec()),
        )
        .collect()
}

#[test]
fn native_object_resources_encode_complete_forward_graph_and_preserve_original_defaults() {
    let original = source();
    let banks = banks();
    let additions = saved(0, banks.clone());
    let policy = with_transmissions(policy(), &original, &additions);
    let actual = replay(
        vec![authored(&original)],
        &additions,
        &[],
        policy.clone(),
        None,
        0,
    )
    .unwrap();
    let expected = rebuilt(&original, &banks, &[]);
    let separately_encoded = replay(vec![authored(&expected)], &[], &[], policy, None, 0).unwrap();
    assert_eq!(actual.producers[0].original(), &original);
    assert_eq!(actual.encoded.as_ref().unwrap().declared(), &expected);
    assert_eq!(units(&actual), units(&separately_encoded));
    assert_eq!(actual.provenance, separately_encoded.provenance);
    assert_eq!(actual.transmissions, separately_encoded.transmissions);
    assert!(!original.defaults().is_empty());
    assert!(!original.semantic().base_text_spellings().is_empty());
    assert_eq!(expected.mots(), original.mots());
    assert_eq!(expected.defaults(), original.defaults());
    assert_eq!(expected.resolutions(), original.resolutions());
    assert_eq!(expected.semantic().base(), original.semantic().base());
    assert_eq!(
        expected.semantic().base_text_spellings(),
        original.semantic().base_text_spellings()
    );
}

#[test]
fn native_object_resources_require_immutable_authored_source_positions_and_distinct_owners() {
    let original = source();
    for (documents, additions, region) in [
        (vec![authored(&original)], saved(1, banks()), None),
        (
            vec![TeletextProfileDocument::Base(
                original.semantic().base().clone(),
            )],
            saved(0, banks()),
            Some(TeletextEncodingRegion::WesternLatin),
        ),
        (
            vec![authored(&original)],
            saved(0, vec![banks()[0].clone(), banks()[0].clone()]),
            None,
        ),
        (
            vec![authored(&original)],
            saved(
                0,
                vec![TeletextObjectResourceAddition::new(
                    resource(TeletextResourceKind::Pop, 0xa0, 0),
                    vec![],
                    policy().composition.service,
                )
                .unwrap()],
            ),
            None,
        ),
        (
            vec![authored(&original)],
            saved(
                0,
                vec![TeletextObjectResourceAddition::new(
                    resource(TeletextResourceKind::Gpop, 0xa0, 0),
                    vec![],
                    policy().composition.service,
                )
                .unwrap()],
            ),
            None,
        ),
        // The bank is claimed against document0, but document1 already owns its page/S1.
        (
            vec![authored(&original), authored(&original)],
            saved(
                0,
                vec![TeletextObjectResourceAddition::new(
                    resource(TeletextResourceKind::Gpop, 0xa0, 0),
                    vec![],
                    policy().composition.service,
                )
                .unwrap()],
            ),
            None,
        ),
    ] {
        let policy = with_transmissions(policy(), &original, &additions);
        assert!(matches!(
            replay(documents, &additions, &[], policy, region, 0),
            Err(ChannelTeletextDocumentEncodingError::Association)
        ));
    }
}

#[test]
fn native_object_resources_do_not_fabricate_or_rewrite_supplied_lifecycle() {
    let original = source();
    let additions = saved(0, banks());
    let correct = with_transmissions(policy(), &original, &additions);
    for variant in 0..6 {
        let mut policy = correct.clone();
        let index = original.resource_inventory().len();
        match variant {
            0 => {
                policy.transmissions.remove(index);
            }
            1 => {
                policy.transmissions.push(policy.transmissions[index]);
            }
            2 => {
                policy.transmissions[index].counter =
                    TeletextResourceUpdateCounter::new(6).unwrap();
            }
            3 => {
                policy.transmissions[index].update = TeletextResourceFullUpdate::Unchanged;
            }
            4 => {
                policy.transmissions[index].last_subpage =
                    !policy.transmissions[index].last_subpage;
            }
            5 => {
                policy.transmissions.remove(0);
            }
            _ => unreachable!(),
        }
        assert!(
            replay(vec![authored(&original)], &additions, &[], policy, None, 0).is_err(),
            "variant {variant}"
        );
    }
    assert_eq!(correct.transmissions[2].counter.get(), 7);
    assert_eq!(original.resource_inventory().len(), 2);
}

#[test]
fn native_object_resources_and_original_bank_definitions_share_one_complete_graph() {
    let original = source();
    let public = resource(TeletextResourceKind::Pop, 0xa1, 15);
    let original_bank = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let global = resource(TeletextResourceKind::Gpop, 0xa2, 0);
    let mut banks = banks();
    banks[0] = TeletextObjectResourceAddition::new(
        global,
        vec![definition(
            global,
            33,
            TeletextObjectType::Active,
            vec![TeletextEnhancementCommand::InvokeObject {
                target: target(original_bank, 20),
                object_type: TeletextObjectType::Adaptive,
                offset: None,
            }],
        )],
        policy().composition.service,
    )
    .unwrap();
    let definitions = vec![TeletextResourceObjectDefinitionAddition::new(
        definition(
            original_bank,
            20,
            TeletextObjectType::Adaptive,
            vec![TeletextEnhancementCommand::InvokeObject {
                target: target(public, 32),
                object_type: TeletextObjectType::Passive,
                offset: None,
            }],
        ),
        policy().composition.service,
    )
    .unwrap()];
    assert!(original
        .clone()
        .with_object_resource_additions(
            &banks,
            policy().composition.service,
            policy().composition.declarations
        )
        .is_err());
    assert!(original
        .clone()
        .with_resource_object_definition_additions(&definitions, policy().composition.service)
        .is_err());
    let additions = saved(0, banks.clone());
    let policy = with_transmissions(policy(), &original, &additions);
    let actual = replay(
        vec![authored(&original)],
        &additions,
        &definitions,
        policy.clone(),
        None,
        0,
    )
    .unwrap();
    let expected = rebuilt(&original, &banks, &definitions);
    let separate = replay(vec![authored(&expected)], &[], &[], policy, None, 0).unwrap();
    assert_eq!(actual.producers[0].original(), &original);
    assert_eq!(actual.encoded.as_ref().unwrap().declared(), &expected);
    assert_eq!(units(&actual), units(&separate));
}

#[test]
fn native_object_resources_preflight_raw_clone_and_combined_limits() {
    let original = source();
    let additions = saved(0, banks());
    for variant in 0..7 {
        let mut policy = with_transmissions(policy(), &original, &additions);
        match variant {
            0 => policy.composition.service.max_records = 2,
            1 => policy.composition.declarations.max_resources = 2,
            2 => policy.composition.service.max_objects = 1,
            3 => policy.composition.service.max_commands = 1,
            4 => policy.composition.max_storage_bytes = 1,
            5 => policy.composition.max_input_items = 3,
            6 => policy.composition.max_input_commands = 1,
            _ => unreachable!(),
        }
        assert!(
            replay(vec![authored(&original)], &additions, &[], policy, None, 0).is_err(),
            "variant {variant}"
        );
    }
}

#[test]
fn native_object_resource_source_ordinal_is_not_the_entry_producer_ordinal() {
    let original = source();
    let mut base = original.semantic().base().clone();
    base.initial_page = address(201).page;
    base.pages[0].address = address(201);
    base.pages[0].rows.clear();
    base.pages[0].navigation.clear();
    let additions = saved(1, banks());
    let policy = with_transmissions(policy(), &original, &additions);
    let actual = replay(
        vec![
            TeletextProfileDocument::Base(base.clone()),
            authored(&original),
        ],
        &additions,
        &[],
        policy.clone(),
        Some(TeletextEncodingRegion::WesternLatin),
        1,
    )
    .unwrap();
    assert_eq!(actual.producers[1].original(), &original);
    assert!(actual
        .provenance
        .iter()
        .any(|page| page.producer_ordinal() == 1 && page.address() == address(200)));
    let wrong_source = saved(0, banks());
    assert!(matches!(
        replay(
            vec![TeletextProfileDocument::Base(base), authored(&original)],
            &wrong_source,
            &[],
            policy,
            Some(TeletextEncodingRegion::WesternLatin),
            1
        ),
        Err(ChannelTeletextDocumentEncodingError::Association)
    ));
}
