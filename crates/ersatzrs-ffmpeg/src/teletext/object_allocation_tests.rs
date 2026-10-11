//! Checked semantic ownership and independent pointer-number fixtures.

use ersatzrs_domain::*;

use super::{allocate_teletext_resource_objects, TeletextObjectAllocationError};

fn resource(kind: TeletextResourceKind, page: u8, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(8, page).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}

fn target(resource: TeletextResourceIdentity, id: u16) -> TeletextObjectIdentity {
    match resource.kind() {
        TeletextResourceKind::Pop => {
            TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap()
        }
        TeletextResourceKind::Gpop => {
            TeletextObjectIdentity::global(resource, TeletextObjectId::new(id)).unwrap()
        }
        _ => unreachable!(),
    }
}

fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    }
}

fn catalog(
    definitions: Vec<TeletextEnhancementObjectDefinition>,
) -> TeletextEnhancementObjectCatalog {
    TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 1024,
            max_commands: 0,
        },
    )
    .unwrap()
}

#[test]
fn all_pointer_slots_types_and_s1_preserve_full_ids_and_independent_banks() {
    let types = [
        TeletextObjectType::Active,
        TeletextObjectType::Adaptive,
        TeletextObjectType::Passive,
    ];
    for s1 in 0_u8..16 {
        let resource = resource(TeletextResourceKind::Pop, 0xa0, s1);
        let definitions: Vec<_> = types
            .into_iter()
            .enumerate()
            .flat_map(|(type_index, kind)| {
                (0_u16..32).map(move |ordinal| {
                    definition(
                        target(
                            resource,
                            0x8000 + u16::try_from(type_index).unwrap() * 256 + ordinal,
                        ),
                        kind,
                    )
                })
            })
            .collect();
        let input = catalog(definitions);
        let before = input.clone();
        let output = allocate_teletext_resource_objects(&input, &[], 96).unwrap();
        assert_eq!(input, before);
        assert_eq!(output.entries().len(), 96);
        assert_eq!(output.resources().collect::<Vec<_>>(), vec![resource]);
        assert_eq!(output.resource_entries(resource).count(), 96);
        for (index, entry) in output.entries().iter().enumerate() {
            let ordinal = index % 32;
            // Independent normative address: four pointer packets, eight slots each.
            let packet = ordinal / 8;
            let triplet_group = (ordinal % 8) / 2;
            let half = ordinal % 2;
            let number = entry.object_number();
            assert_eq!(usize::from(number >> 7), packet);
            assert_eq!(usize::from((number >> 5) & 3), triplet_group);
            assert_eq!(usize::from((number >> 4) & 1), half);
            assert_eq!(number & 15, u16::from(s1));
            assert_eq!(entry.target(), input.definitions()[index].identity);
            assert_eq!(entry.object_type(), input.definitions()[index].object_type);
            assert_eq!(
                output.lookup(entry.target(), entry.object_type()),
                Some(entry)
            );
            assert!(output
                .lookup(entry.target(), types[(index / 32 + 1) % 3])
                .is_none());
            if ordinal < 16 {
                let default = entry.mot_default_allocation().unwrap();
                assert_eq!(default.target, entry.target());
                assert_eq!(default.object_type, entry.object_type());
                assert_eq!(u16::from(default.selector), number);
            } else {
                assert!(entry.mot_default_allocation().is_none());
            }
        }
    }
}

#[test]
fn default_priority_deduplication_and_shared_full_target_reuse_are_deterministic() {
    let resource = resource(TeletextResourceKind::Gpop, 0xa0, 15);
    let definitions: Vec<_> = (0_u16..32)
        .map(|id| definition(target(resource, 0xff00 + id), TeletextObjectType::Passive))
        .collect();
    let input = catalog(definitions);
    let defaults = [31_u16, 31, 30, 29].map(|id| TeletextDefaultObjectSelection {
        target: target(resource, 0xff00 + id),
        object_type: TeletextObjectType::Passive,
    });
    let output = allocate_teletext_resource_objects(&input, &defaults, 32).unwrap();
    assert_eq!(
        output,
        allocate_teletext_resource_objects(&input, &defaults, 32).unwrap()
    );
    assert_eq!(output.entries().len(), 32);
    assert_eq!(
        &output.entries()[..3]
            .iter()
            .map(|entry| entry.target().object_id().get())
            .collect::<Vec<_>>(),
        &[0xff1f, 0xff1e, 0xff1d]
    );
    for (index, entry) in output.entries()[..3].iter().enumerate() {
        assert_eq!(
            entry.object_number(),
            u16::try_from(index).unwrap() * 16 + 15
        );
        assert!(entry.mot_default_allocation().is_some());
    }
    assert_eq!(output.entries()[3].target().object_id().get(), 0xff00);
}

#[test]
fn resource_pages_s1_scope_and_local_owners_never_alias_or_spill() {
    let owner = TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(1).unwrap(),
    };
    let resources = [
        resource(TeletextResourceKind::Pop, 0xa0, 0),
        resource(TeletextResourceKind::Pop, 0xa0, 15),
        resource(TeletextResourceKind::Gpop, 0xa1, 0),
    ];
    let mut definitions: Vec<_> = resources
        .map(|r| definition(target(r, 0xffff), TeletextObjectType::Active))
        .into();
    let local = TeletextObjectIdentity::local(owner, TeletextObjectId::new(0xffff));
    definitions.push(definition(local, TeletextObjectType::Passive));
    let input = catalog(definitions);
    let output = allocate_teletext_resource_objects(&input, &[], 4).unwrap();
    assert_eq!(output.entries().len(), 3);
    assert!(output.lookup(local, TeletextObjectType::Passive).is_none());
    assert_eq!(output.resources().count(), 3);
    for r in resources {
        let entry = output
            .lookup(target(r, 0xffff), TeletextObjectType::Active)
            .unwrap();
        assert_eq!(entry.target().resource(), Some(r));
        assert_eq!(entry.object_number(), u16::from(r.logical_subpage().get()));
        assert_eq!(output.resource_entries(r).count(), 1);
    }
}

#[test]
fn thirty_third_object_and_seventeenth_default_have_exact_normative_failures() {
    let resource = resource(TeletextResourceKind::Pop, 0xa0, 8);
    let input = catalog(
        (0_u16..33)
            .map(|id| definition(target(resource, id), TeletextObjectType::Adaptive))
            .collect(),
    );
    assert_eq!(
        allocate_teletext_resource_objects(&input, &[], 33).unwrap_err(),
        TeletextObjectAllocationError::ObjectCapacity {
            resource,
            object_type: TeletextObjectType::Adaptive,
            target: target(resource, 32),
            count: 33,
            maximum: 32
        }
    );
    let input = catalog(
        (0_u16..17)
            .map(|id| definition(target(resource, id), TeletextObjectType::Adaptive))
            .collect(),
    );
    let defaults: Vec<_> = input
        .definitions()
        .iter()
        .map(|d| TeletextDefaultObjectSelection {
            target: d.identity,
            object_type: d.object_type,
        })
        .collect();
    let before = input.clone();
    assert_eq!(
        allocate_teletext_resource_objects(&input, &defaults, 17).unwrap_err(),
        TeletextObjectAllocationError::DefaultCapacity {
            resource,
            object_type: TeletextObjectType::Adaptive,
            target: target(resource, 16),
            count: 17,
            maximum: 16
        }
    );
    assert_eq!(input, before);
}

#[test]
fn every_default_is_checked_before_assignment_even_after_valid_duplicates() {
    let resource = resource(TeletextResourceKind::Gpop, 0xa0, 1);
    let good = TeletextDefaultObjectSelection {
        target: target(resource, 0x101),
        object_type: TeletextObjectType::Active,
    };
    let input = catalog(vec![definition(good.target, good.object_type)]);
    let bad = TeletextDefaultObjectSelection {
        target: target(resource, 0x201),
        object_type: good.object_type,
    };
    assert_eq!(
        allocate_teletext_resource_objects(&input, &[good, good, bad], 3).unwrap_err(),
        TeletextObjectAllocationError::MissingDefault {
            ordinal: 2,
            target: bad.target,
            object_type: bad.object_type
        }
    );
    let wrong = TeletextDefaultObjectSelection {
        object_type: TeletextObjectType::Passive,
        ..good
    };
    assert_eq!(
        allocate_teletext_resource_objects(&input, &[good, wrong], 2).unwrap_err(),
        TeletextObjectAllocationError::DefaultType {
            ordinal: 1,
            target: good.target,
            expected: TeletextObjectType::Passive,
            actual: TeletextObjectType::Active
        }
    );
    let local = TeletextObjectIdentity::local(
        TeletextPageAddress {
            page: TeletextPageNumber::new(200).unwrap(),
            subcode: TeletextSubcode::new(0).unwrap(),
        },
        TeletextObjectId::new(1),
    );
    let bad = TeletextDefaultObjectSelection {
        target: local,
        object_type: good.object_type,
    };
    assert_eq!(
        allocate_teletext_resource_objects(&input, &[good, bad], 2).unwrap_err(),
        TeletextObjectAllocationError::DefaultScope {
            ordinal: 1,
            target: local,
            object_type: bad.object_type
        }
    );
}

#[test]
fn budgets_precede_validation_and_empty_input_has_no_allocated_resources() {
    let input = catalog(vec![]);
    let empty = allocate_teletext_resource_objects(&input, &[], 0).unwrap();
    assert!(empty.entries().is_empty());
    assert_eq!(empty.resources().count(), 0);
    let resource = resource(TeletextResourceKind::Pop, 0xa0, 0);
    let missing = TeletextDefaultObjectSelection {
        target: target(resource, 1),
        object_type: TeletextObjectType::Active,
    };
    assert_eq!(
        allocate_teletext_resource_objects(&input, &[missing], 0).unwrap_err(),
        TeletextObjectAllocationError::DefaultBudget {
            count: 1,
            maximum: 0
        }
    );
    let input = catalog(vec![definition(missing.target, missing.object_type)]);
    assert_eq!(
        allocate_teletext_resource_objects(&input, &[missing], 0).unwrap_err(),
        TeletextObjectAllocationError::ObjectBudget {
            count: 1,
            maximum: 0
        }
    );
}
