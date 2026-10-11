//! Exact producer pairing and same-instance resource word evidence.

use super::*;
use crate::teletext::{
    allocate_teletext_resource_objects, encode_teletext_object_page,
    TeletextObjectPageTransmission, TeletextResourceFullUpdate, TeletextResourceTransmissionMode,
};
use ersatzrs_domain::*;

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    }
}
fn target(s1: u8) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(s1).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(0xffff),
    )
    .unwrap()
}
fn set(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn foreground(value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 5,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    }
}
fn catalog(s1: u8, value: u8) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        vec![TeletextEnhancementObjectDefinition {
            identity: target(s1),
            object_type: TeletextObjectType::Passive,
            program: TeletextEnhancementProgram::new(vec![
                set(0, 0),
                set(1, 5),
                foreground(4),
                foreground(value),
            ])
            .unwrap(),
        }],
        TeletextObjectCatalogBudget {
            max_objects: 8,
            max_commands: 32,
        },
    )
    .unwrap()
}
fn encode(
    catalog: &TeletextEnhancementObjectCatalog,
    allocations: &TeletextResourceObjectAllocations,
) -> Vec<EncodedTeletextAuthoredPage> {
    vec![EncodedTeletextAuthoredPage::Object(
        encode_teletext_object_page(
            catalog.definitions()[0].identity.resource().unwrap(),
            catalog,
            allocations,
            TeletextObjectPageTransmission {
                counter: TeletextResourceUpdateCounter::new(7).unwrap(),
                update: TeletextResourceFullUpdate::Changed,
                mode: TeletextResourceTransmissionMode::Serial,
                last_subpage: true,
            },
            32,
        )
        .unwrap(),
    )]
}
fn trace(catalog: &TeletextEnhancementObjectCatalog) -> TeletextSpatialTrace {
    let call = TeletextEnhancementCommand::InvokeObject {
        target: catalog.definitions()[0].identity,
        object_type: TeletextObjectType::Passive,
        offset: None,
    };
    resolve_teletext_spatial_program(
        owner(),
        &TeletextEnhancementProgram::new(vec![call, call]).unwrap(),
        catalog,
        TeletextSpatialTraceBudget {
            max_events: 16,
            max_invocations: 2,
        },
    )
    .unwrap()
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
fn actual_resource_words_settle_each_repeat_but_never_cross_instances() {
    let catalog = catalog(5, 13);
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 8).unwrap();
    let pages = encode(&catalog, &allocations);
    let context =
        ResourceTransmissionContext::new(&catalog, &allocations, &pages, 4, 1, 1).unwrap();
    let trace = trace(&catalog);
    let mut positions = vec![];
    for (ordinal, &event) in trace.events().iter().enumerate() {
        if event.source().object.is_some() {
            if let Some(word) = context.word(&trace, event, ordinal).unwrap() {
                positions.push(TeletextAttributeTransmissionPosition {
                    event_ordinal: ordinal,
                    source: event.source(),
                    invocation: event.invocation(),
                    word,
                });
            }
        }
    }
    let evidence = TeletextAttributeTransmissionEvidence::new(&trace, &positions, 16).unwrap();
    // Marker0; omitted identity; row selector1; first FG2; second FG3.
    assert_eq!(context.word(&trace, trace.events()[1], 1).unwrap(), None);
    assert_eq!(context.word(&trace, trace.events()[3], 3).unwrap(), Some(2));
    assert_eq!(context.word(&trace, trace.events()[4], 4).unwrap(), Some(3));
    assert_eq!(
        evidence.compare(attribute(&trace, 4), attribute(&trace, 3)),
        Some(std::cmp::Ordering::Greater)
    );
    assert_eq!(
        evidence.compare(attribute(&trace, 9), attribute(&trace, 8)),
        Some(std::cmp::Ordering::Greater)
    );
    assert_eq!(
        evidence.compare(attribute(&trace, 9), attribute(&trace, 4)),
        None
    );
    assert_eq!(trace.events()[4].source().object, Some(target(5)));
}

#[test]
fn actual_catalog_may_add_local_carriers_but_cannot_change_resource_body_or_type() {
    let original = catalog(5, 13);
    let allocations = allocate_teletext_resource_objects(&original, &[], 8).unwrap();
    let pages = encode(&original, &allocations);
    let context =
        ResourceTransmissionContext::new(&original, &allocations, &pages, 32, 8, 1).unwrap();
    let mut definitions = original.definitions().to_vec();
    definitions.push(TeletextEnhancementObjectDefinition {
        identity: TeletextObjectIdentity::local(owner(), TeletextObjectId::new(1)),
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    });
    let augmented = TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 8,
            max_commands: 32,
        },
    )
    .unwrap();
    let augmented_allocations = allocate_teletext_resource_objects(&augmented, &[], 8).unwrap();
    context
        .validate_catalog(&augmented, &augmented_allocations)
        .unwrap();
    let changed = catalog(5, 2);
    let failure = context
        .validate_catalog(&changed, &allocations)
        .unwrap_err();
    assert_eq!(failure.target, Some(target(5)));
    assert_eq!(failure.reason, Failure::Correspondence);
    let mut definitions = original.definitions().to_vec();
    definitions[0].object_type = TeletextObjectType::Adaptive;
    let changed = TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 8,
            max_commands: 32,
        },
    )
    .unwrap();
    assert_eq!(
        context
            .validate_catalog(&changed, &allocations)
            .unwrap_err()
            .target,
        Some(target(5))
    );
}

#[test]
fn missing_resource_wrong_s1_and_changed_reservations_are_not_word_authority() {
    let mut definitions = catalog(5, 13).definitions().to_vec();
    let mut decoy = definitions[0].clone();
    decoy.identity = TeletextObjectIdentity::public(
        target(5).resource().unwrap(),
        TeletextObjectId::new(0xfffe),
    )
    .unwrap();
    decoy.program = TeletextEnhancementProgram::new(vec![]).unwrap();
    definitions.insert(0, decoy);
    let catalog = TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 8,
            max_commands: 32,
        },
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 8).unwrap();
    let missing = ResourceTransmissionContext::new(&catalog, &allocations, &[], 32, 8, 1)
        .err()
        .unwrap();
    assert_eq!(missing.resource, target(5).resource());
    let other = self::catalog(6, 13);
    let other_allocations = allocate_teletext_resource_objects(&other, &[], 8).unwrap();
    let other_pages = encode(&other, &other_allocations);
    let wrong = ResourceTransmissionContext::new(&catalog, &allocations, &other_pages, 32, 8, 1)
        .err()
        .unwrap();
    assert_eq!(wrong.target, Some(target(6)));
    let pages = encode(&catalog, &allocations);
    let context =
        ResourceTransmissionContext::new(&catalog, &allocations, &pages, 32, 8, 1).unwrap();
    // Reserving the second authored target moves it ahead of the first target
    // and changes its physical selector without changing either host identity.
    let reservations = [TeletextDefaultObjectSelection {
        target: target(5),
        object_type: TeletextObjectType::Passive,
    }];
    let reserved = allocate_teletext_resource_objects(&catalog, &reservations, 8).unwrap();
    assert_ne!(reserved.entries(), allocations.entries());
    assert_eq!(
        context
            .validate_catalog(&catalog, &reserved)
            .unwrap_err()
            .target,
        Some(catalog.definitions()[0].identity)
    );
}

#[test]
fn changed_trace_operation_is_rejected_at_the_actual_event() {
    let original = catalog(5, 13);
    let allocations = allocate_teletext_resource_objects(&original, &[], 8).unwrap();
    let pages = encode(&original, &allocations);
    let context =
        ResourceTransmissionContext::new(&original, &allocations, &pages, 32, 8, 1).unwrap();
    let changed = catalog(5, 2);
    let trace = trace(&changed);
    assert_eq!(
        context.word(&trace, trace.events()[4], 4),
        Err(TeletextAttributeTransmissionError::Correspondence { event: 4 })
    );
}

#[test]
fn raw_fences_precede_missing_resources_and_empty_context_needs_no_fake_owner() {
    let catalog = catalog(5, 13);
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 8).unwrap();
    let failure = ResourceTransmissionContext::new(&catalog, &allocations, &[], 3, 1, 1)
        .err()
        .unwrap();
    assert_eq!(failure.resource, None);
    assert_eq!(
        failure.reason,
        Failure::Budget {
            kind: "commands",
            count: 4,
            maximum: 3
        }
    );
    let empty = TeletextEnhancementObjectCatalog::new(
        vec![],
        TeletextObjectCatalogBudget {
            max_objects: 0,
            max_commands: 0,
        },
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&empty, &[], 0).unwrap();
    ResourceTransmissionContext::new(&empty, &allocations, &[], 0, 0, 0).unwrap();
}
