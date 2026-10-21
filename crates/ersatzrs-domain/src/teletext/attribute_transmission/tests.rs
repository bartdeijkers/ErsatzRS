//! Two coordinate spaces are explicit; only authenticated page entries cross emitters.
use super::*;
use crate::teletext::attribute_execution_tests::{definition, foreground, invoke, local, owner};
use crate::teletext::*;

fn catalog(target: TeletextObjectIdentity) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![foreground(0, 3), foreground(0, 4)],
        )],
        TeletextObjectCatalogBudget {
            max_objects: 4,
            max_commands: 32,
        },
    )
    .unwrap()
}
fn trace(target: TeletextObjectIdentity, repeat: bool) -> TeletextSpatialTrace {
    let mut main = vec![
        foreground(8, 1),
        invoke(target, TeletextObjectType::Active, None),
    ];
    if repeat {
        main.push(invoke(target, TeletextObjectType::Active, None));
    }
    resolve_teletext_spatial_program(
        owner(),
        &TeletextEnhancementProgram::new(main).unwrap(),
        &catalog(target),
        TeletextSpatialTraceBudget {
            max_events: 32,
            max_invocations: 4,
        },
    )
    .unwrap()
}
fn positions(trace: &TeletextSpatialTrace) -> Vec<TeletextAttributeTransmissionPosition> {
    trace
        .events()
        .iter()
        .enumerate()
        .filter(|(_, event)| matches!(event.command(), TeletextEnhancementCommand::AtColumn { .. }))
        .map(
            |(event_ordinal, event)| TeletextAttributeTransmissionPosition {
                event_ordinal,
                source: event.source(),
                invocation: event.invocation(),
                word: if event.source().object.is_some() {
                    event.source().command
                } else {
                    1
                },
            },
        )
        .collect()
}
fn page_positions(
    ordinary: &[TeletextAttributeTransmissionPosition],
) -> Vec<TeletextAttributeTransmissionPosition> {
    ordinary
        .iter()
        .map(|position| TeletextAttributeTransmissionPosition {
            word: position.word
                + if position.source.object.is_some() {
                    8
                } else {
                    0
                },
            ..*position
        })
        .collect()
}
fn attribute(trace: &TeletextSpatialTrace, ordinal: usize) -> TeletextAttributeSource {
    let event = trace.events()[ordinal];
    let position = event.absolute_after();
    TeletextAttributeSource::Enhancement {
        event_ordinal: ordinal,
        location: event.source(),
        root_source: event.root_source(),
        invocation: event.invocation(),
        effective_start: (position.row(), position.column()),
        placed_position: (position.row(), position.column()),
    }
}

#[test]
fn same_page_transmission_keeps_relative_order_and_requires_explicit_absolute_subset() {
    let trace = trace(local(65534), false);
    let ordinary = positions(&trace);
    let page = page_positions(&ordinary);
    let legacy = TeletextAttributeTransmissionEvidence::new(&trace, &ordinary, 32).unwrap();
    let local = attribute(&trace, 2);
    let main = attribute(&trace, 0);
    assert_eq!(legacy.compare(local, main), None);
    let evidence = TeletextAttributeTransmissionEvidence::new_with_same_page_order(
        &trace, &ordinary, &page, 32,
    )
    .unwrap();
    assert_eq!(evidence.compare(local, main), Some(Ordering::Greater));
    assert_eq!(
        evidence.compare(attribute(&trace, 3), local),
        Some(Ordering::Greater)
    );
    let incomplete = TeletextAttributeTransmissionEvidence::new_with_same_page_order(
        &trace,
        &ordinary,
        &page[..1],
        32,
    )
    .unwrap();
    assert_eq!(incomplete.compare(local, main), None);
    assert_eq!(
        incomplete.compare(attribute(&trace, 3), local),
        Some(Ordering::Greater)
    );
    assert_eq!(
        TeletextAttributeTransmissionEvidence::new_with_same_page_order(&trace, &ordinary, &[], 32)
            .unwrap(),
        legacy
    );
    assert!(evidence.matches(&trace));
}

#[test]
fn same_page_transmission_rejects_corrupt_missing_duplicate_and_inconsistent_coordinates() {
    let trace = trace(local(65534), false);
    let ordinary = positions(&trace);
    let page = page_positions(&ordinary);
    for case in 0..6 {
        let mut changed = page.clone();
        match case {
            0 => changed[1].source.command += 1,
            1 => changed[1].invocation = None,
            2 => changed[2].word = changed[1].word,
            3 => changed[2].word += 1,
            4 => changed[0].word += 1,
            5 => changed.push(changed[1]),
            _ => unreachable!(),
        }
        assert!(
            TeletextAttributeTransmissionEvidence::new_with_same_page_order(
                &trace, &ordinary, &changed, 32
            )
            .is_err(),
            "corrupt case {case}"
        );
    }
    assert!(
        TeletextAttributeTransmissionEvidence::new_with_same_page_order(
            &trace,
            &ordinary[..1],
            &page,
            32
        )
        .is_err()
    );
    assert!(matches!(
        TeletextAttributeTransmissionEvidence::new_with_same_page_order(
            &trace, &ordinary, &page, 2
        ),
        Err(TeletextAttributeTransmissionError::Budget { maximum: 2 })
    ));
}

#[test]
fn same_page_transmission_repeated_targets_and_resource_coordinates_remain_ineligible() {
    let repeated = trace(local(65534), true);
    let ordinary = positions(&repeated);
    assert!(
        TeletextAttributeTransmissionEvidence::new_with_same_page_order(
            &repeated,
            &ordinary,
            &page_positions(&ordinary),
            32
        )
        .is_err()
    );
    let legacy = TeletextAttributeTransmissionEvidence::new(&repeated, &ordinary, 32).unwrap();
    assert_eq!(
        legacy.compare(attribute(&repeated, 2), attribute(&repeated, 5)),
        None
    );
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    let public = TeletextObjectIdentity::public(resource, TeletextObjectId::new(65534)).unwrap();
    let resource_trace = trace(public, false);
    let ordinary = positions(&resource_trace);
    assert!(
        TeletextAttributeTransmissionEvidence::new_with_same_page_order(
            &resource_trace,
            &ordinary,
            &page_positions(&ordinary),
            32
        )
        .is_err()
    );
}

#[test]
fn same_page_transmission_full_event_capacity_accepts_two_views_without_double_charging() {
    let trace = trace(local(65534), false);
    let ordinary = positions(&trace);
    let page = page_positions(&ordinary);
    let maximum = trace.events().len();
    assert!(ordinary.len() + page.len() > maximum);
    assert!(
        TeletextAttributeTransmissionEvidence::new_with_same_page_order(
            &trace, &ordinary, &page, maximum
        )
        .is_ok()
    );
}

#[test]
fn same_page_transmission_default_roots_never_gain_page_coordinates() {
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    let target = TeletextObjectIdentity::public(resource, TeletextObjectId::new(65534)).unwrap();
    let language = TeletextLanguage::new("eng".into()).unwrap();
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
                presentation: None,
                compositional_links: None,
                local_program: None,
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: None,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: [
                        None,
                        None,
                        Some(TeletextDefaultObjectSelection {
                            target,
                            object_type: TeletextObjectType::Active,
                        }),
                        None,
                    ],
                },
            }],
            magazines: vec![],
            objects: catalog(target).definitions().to_vec(),
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 16,
            max_objects: 4,
            max_structure_items: 4096,
            max_base_content_bytes: 1024,
            max_commands: 32,
            max_collected_commands: 32,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap();
    let trace = resolve_teletext_spatial_trace(
        &service,
        owner(),
        TeletextSpatialTraceBudget {
            max_events: 32,
            max_invocations: 4,
        },
    )
    .unwrap();
    assert!(matches!(
        trace.original_program_source(),
        TeletextEffectiveProgramSource::Defaults { .. }
    ));
    let ordinary = positions(&trace);
    assert!(TeletextAttributeTransmissionEvidence::new(&trace, &ordinary, 32).is_ok());
    assert!(
        TeletextAttributeTransmissionEvidence::new_with_same_page_order(
            &trace,
            &ordinary,
            &page_positions(&ordinary),
            32
        )
        .is_err()
    );
}
