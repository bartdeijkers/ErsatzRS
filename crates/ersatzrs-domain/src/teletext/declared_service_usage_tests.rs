//! Used resources must fit the effective declaration, not just exist in inventory.
use super::*;

fn used_resource(
    role: TeletextX27LinkRole,
    used_s1: u8,
    extent: u8,
    x27: Option<TeletextX27LinkValidity>,
) -> (
    TeletextResolvedServiceDraft,
    TeletextMotLevel25Declaration,
    Vec<TeletextResourceIdentity>,
) {
    let (mut input, empty) = used_input(role, used_s1);
    let mot = used_mot(role, extent);
    if let Some(validity) = x27 {
        let null = TeletextX27LinkDeclaration::new(
            unused(),
            validity,
            TeletextSubpageRequirementMask::new(0xffff),
        )
        .unwrap();
        let mut declarations = [null; 4];
        declarations[role.wire_code() as usize] = TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page { page: raw(0xa0) },
            validity,
            TeletextSubpageRequirementMask::new(1 << used_s1),
        )
        .unwrap();
        input.pages[0].compositional_links = Some(
            TeletextX27Level25Links::new(
                declarations[0],
                declarations[1],
                declarations[2],
                declarations[3],
            )
            .unwrap(),
        );
    }
    (input, mot, empty)
}

fn used_input(
    role: TeletextX27LinkRole,
    used_s1: u8,
) -> (TeletextResolvedServiceDraft, Vec<TeletextResourceIdentity>) {
    let mut input = draft();
    input.objects.clear();
    input.pages[0].resources = TeletextResolvedPageResourceSelection {
        objects: TeletextResolvedObjectPageLinks {
            public: None,
            global: None,
        },
        drcs: TeletextDrcsPageLinks::unlinked(),
        defaults: [None; 4],
    };
    let resource = identity(role.kind(), 0xa0, used_s1);
    let mut empty = Vec::new();
    let command = match role {
        TeletextX27LinkRole::Pop | TeletextX27LinkRole::Gpop => {
            let target = if role == TeletextX27LinkRole::Pop {
                input.pages[0].resources.objects.public = Some(raw(0xa0));
                TeletextObjectIdentity::public(resource, TeletextObjectId::new(30)).unwrap()
            } else {
                input.pages[0].resources.objects.global = Some(raw(0xa0));
                TeletextObjectIdentity::global(resource, TeletextObjectId::new(30)).unwrap()
            };
            input.objects.push(TeletextEnhancementObjectDefinition {
                identity: target,
                object_type: TeletextObjectType::Passive,
                program: TeletextEnhancementProgram::new(vec![]).unwrap(),
            });
            empty.extend(
                (0..16)
                    .filter(|s1| *s1 != used_s1)
                    .map(|s1| identity(role.kind(), 0xa0, s1)),
            );
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Passive,
                offset: None,
            }
        }
        TeletextX27LinkRole::Drcs | TeletextX27LinkRole::Gdrcs => {
            let scope = if role == TeletextX27LinkRole::Drcs {
                input.pages[0].resources.drcs =
                    TeletextDrcsPageLinks::new(Some(raw(0xa0)), None).unwrap();
                TeletextDrcsTableScope::Normal
            } else {
                input.pages[0].resources.drcs =
                    TeletextDrcsPageLinks::new(None, Some(raw(0xa0))).unwrap();
                TeletextDrcsTableScope::Global
            };
            for s1 in 0..16 {
                input.drcs.push(
                    TeletextDrcsSubtable::new(
                        identity(role.kind(), 0xa0, s1),
                        vec![TeletextDrcsGlyphDefinition {
                            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                            bitmap: TeletextDrcsBitmap::new([1; 10]).unwrap(),
                        }],
                    )
                    .unwrap(),
                );
            }
            TeletextEnhancementCommand::AtColumn {
                column: 0,
                operation: TeletextColumnOperation::Drcs {
                    reference: TeletextDrcsGlyphReference {
                        table: scope,
                        subtable: TeletextDrcsSubtableIndex::new(used_s1).unwrap(),
                        slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                    },
                },
            }
        }
    };
    input.pages[0].local_program = Some(TeletextEnhancementProgram::new(vec![command]).unwrap());
    (input, empty)
}

fn used_mot(role: TeletextX27LinkRole, extent: u8) -> TeletextMotLevel25Declaration {
    let mut object_links = [empty_object(); 8];
    let unused_drcs =
        TeletextMotDrcsLink::new(unused(), TeletextMotSubpageExtent::new(0).unwrap(), true);
    let mut drcs_links = [unused_drcs; 8];
    let target = TeletextX27LinkTarget::Page { page: raw(0xa0) };
    match role {
        TeletextX27LinkRole::Gpop => {
            object_links[0] = object(
                target,
                extent,
                [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
                1,
            );
        }
        TeletextX27LinkRole::Pop => {
            object_links[1] = object(
                target,
                extent,
                [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
                1,
            );
        }
        TeletextX27LinkRole::Gdrcs => {
            drcs_links[0] = TeletextMotDrcsLink::new(
                target,
                TeletextMotSubpageExtent::new(extent).unwrap(),
                true,
            );
        }
        TeletextX27LinkRole::Drcs => {
            drcs_links[1] = TeletextMotDrcsLink::new(
                target,
                TeletextMotSubpageExtent::new(extent).unwrap(),
                true,
            );
        }
    }
    let public = TeletextMotPublicLinkIndex::new(1).unwrap();
    TeletextMotLevel25Declaration::new(
        identity(TeletextResourceKind::Mot, 0xfe, 9),
        vec![TeletextMotPageAssociation::new(
            0,
            TeletextMotAssociationSelection::new(
                role == TeletextX27LinkRole::Gpop,
                (role == TeletextX27LinkRole::Pop).then_some(public),
            ),
            TeletextMotAssociationSelection::new(
                role == TeletextX27LinkRole::Gdrcs,
                (role == TeletextX27LinkRole::Drcs).then_some(public),
            ),
        )],
        TeletextMotLevel25Links::new(
            object_links[0],
            object_links[1..].try_into().unwrap(),
            drcs_links[0],
            drcs_links[1..].try_into().unwrap(),
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn implicit_mot_used_subpages_reject_unannounced_object_and_drcs_resources() {
    for role in TeletextX27LinkRole::ALL {
        for x27 in [None, Some(TeletextX27LinkValidity::Level35Only)] {
            let (input, mot, empty) = used_resource(role, 15, 0, x27);
            let mot_id = mot.identity();
            let error = bind(input, mot, empty).unwrap_err();
            assert_eq!(
                error,
                TeletextDeclaredServiceError {
                    display: Some(owner(0)),
                    mot: Some(mot_id),
                    resource: Some(identity(role.kind(), 0xa0, 15)),
                    reason: TeletextDeclaredServiceFailure::ResourceUsage { role },
                }
            );
        }
    }
}

#[test]
fn implicit_mot_used_boundaries_allow_zero_fifteen_and_unused_extra_resources() {
    for role in TeletextX27LinkRole::ALL {
        for s1 in [0, 15] {
            let (input, mot, empty) = used_resource(role, s1, s1, None);
            bind(input, mot, empty).unwrap();
        }
    }
}

#[test]
fn applicable_x27_mask_can_select_used_subpage_beyond_original_mot_extent() {
    for role in TeletextX27LinkRole::ALL {
        let (input, mot, empty) =
            used_resource(role, 15, 0, Some(TeletextX27LinkValidity::Level25Only));
        let checked = bind(input, mot, empty).unwrap();
        assert!(
            matches!(checked.resolutions()[0].role(role), TeletextMotRoleResolution::Selected { requirements: TeletextMotResolutionRequirements::X27Mask(mask), .. } if mask.requires(TeletextResourceSubpageIndex::new(15).unwrap()))
        );
    }
}

#[test]
fn nested_default_invocation_cannot_escape_implicit_mot_extent_after_program_overlay() {
    let role = TeletextX27LinkRole::Pop;
    let (mut input, original_mot, mut empty) = used_resource(role, 0, 0, None);
    let old_target = input.objects[0].identity;
    let later_resource = identity(TeletextResourceKind::Pop, 0xa0, 15);
    let later_target =
        TeletextObjectIdentity::public(later_resource, TeletextObjectId::new(31)).unwrap();
    input.objects.push(TeletextEnhancementObjectDefinition {
        identity: later_target,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![]).unwrap(),
    });
    empty.retain(|resource| *resource != later_resource);
    let root_resource = identity(TeletextResourceKind::Gpop, 0xb0, 0);
    let root = TeletextObjectIdentity::global(root_resource, TeletextObjectId::new(20)).unwrap();
    let invoke = |target| TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: TeletextObjectType::Passive,
        offset: None,
    };
    let before = TeletextEnhancementProgram::new(vec![invoke(old_target)]).unwrap();
    let after = TeletextEnhancementProgram::new(vec![invoke(later_target)]).unwrap();
    input.objects.push(TeletextEnhancementObjectDefinition {
        identity: root,
        object_type: TeletextObjectType::Active,
        program: before.clone(),
    });
    input.pages[0].local_program = None;
    input.pages[0].resources.objects.global = Some(raw(0xb0));
    let default = TeletextDefaultObjectSelection {
        target: root,
        object_type: TeletextObjectType::Active,
    };
    input.pages[0].resources.defaults[0] = Some(default);
    let global = object(
        TeletextX27LinkTarget::Page { page: raw(0xb0) },
        0,
        [
            TeletextMotDefaultObject::Object {
                target: root,
                object_type: TeletextObjectType::Active,
            },
            TeletextMotDefaultObject::NoObject { raw_number: 0 },
        ],
        1,
    );
    let public = [original_mot
        .links()
        .object_link(Some(TeletextMotPublicLinkIndex::new(1).unwrap())); 7];
    let declared = mot(public, global, true);
    let checked = bind(input.clone(), declared.clone(), empty.clone()).unwrap();
    let original = checked.clone();
    let overlay = TeletextResourceObjectProgramOverlay::new_with_invocation_changes(
        root,
        before,
        after.clone(),
        vec![Some(0)],
        usage_budget(),
    )
    .unwrap();
    let error = checked
        .clone()
        .with_resource_object_program_overlays(&[overlay], usage_budget())
        .unwrap_err();
    assert_eq!(
        error,
        TeletextServiceError::CompositionalUsage {
            display: owner(0),
            role,
            page: raw(0xa0),
            subpage: TeletextResourceSubpageIndex::new(15).unwrap(),
            source_location: TeletextEnhancementSource {
                object: Some(root),
                command: 0
            },
        }
    );
    assert_eq!(checked, original);
    input.objects.last_mut().unwrap().program = after;
    assert_eq!(
        bind(input, declared, empty).unwrap_err().reason,
        TeletextDeclaredServiceFailure::ResourceUsage { role }
    );
}

fn usage_budget() -> TeletextServiceBudget {
    TeletextServiceBudget {
        max_records: 64,
        max_objects: 64,
        max_structure_items: 1024,
        max_base_content_bytes: 1024,
        max_commands: 1024,
        max_collected_commands: 1024,
        max_collected_drcs_references: 64,
    }
}
