//! Exact POP/GPOP scalars and complete consumer validation, not editor acceptance.
use super::*;
use serde_json::{json, Value};

fn op(value: Value) -> TeletextColumnOperation {
    serde_json::from_value(value).unwrap()
}
fn fg(colour: u8) -> TeletextColumnOperation {
    op(json!({"kind":"foreground","colour":colour}))
}
fn glyph(code: u8) -> TeletextColumnOperation {
    op(json!({"kind":"g0","code":code}))
}
fn target(global: bool, s1: u8) -> TeletextObjectIdentity {
    let resource = resource(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        if global { 0xa1 } else { 0xa0 },
        s1,
    );
    if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(19))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(19))
    }
    .unwrap()
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn input() -> TeletextResolvedServiceDraft {
    let mut input = draft(&[address(200, 0), address(200, 1)]);
    for page in &mut input.pages {
        page.presentation = Some(presentation(0, 1));
        page.resources.objects = TeletextResolvedObjectPageLinks {
            public: Some(raw(0xa0)),
            global: Some(raw(0xa1)),
        };
        page.local_program = Some(program(vec![
            invoke(target(false, 1), TeletextObjectType::Passive),
            invoke(target(true, 2), TeletextObjectType::Passive),
        ]));
        page.resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
    }
    for (global, s1) in [(false, 1), (true, 2)] {
        input.objects.push(definition(
            target(global, s1),
            TeletextObjectType::Passive,
            vec![
                at(0, fg(1)),
                at(0, glyph(b'A')),
                at(
                    1,
                    op(json!({"kind":"drcs",
                "reference":{"table":"normal","subtable":0,"slot":0}})),
                ),
            ],
        ));
    }
    input.drcs.push(table(0xb0, 0, 1));
    input
}
fn service(input: TeletextResolvedServiceDraft) -> TeletextSemanticService {
    TeletextSemanticService::new_with_object_companions(
        input,
        vec![resource(TeletextResourceKind::Pop, 0xa2, 0)],
        budget(),
    )
    .unwrap()
    .with_physical_base_overlays(
        vec![TeletextPhysicalBaseOverlay::new(
            address(200, 1),
            vec![TeletextPhysicalBaseRow::new(5, [b'P'; 40]).unwrap()],
        )
        .unwrap()],
        budget(),
    )
    .unwrap()
}
fn change(global: bool) -> TeletextResourceObjectOperandOverlay {
    TeletextResourceObjectOperandOverlay::new(
        target(global, if global { 2 } else { 1 }),
        0,
        0,
        fg(1),
        fg(2),
    )
    .unwrap()
}

#[test]
fn resource_object_scalar_identity_keeps_full_scope_page_s1_and_refuses_local_aliases() {
    for global in [false, true] {
        let change = change(global);
        assert_eq!(
            change.identity(),
            target(global, if global { 2 } else { 1 })
        );
        assert_eq!(change.identity().owner(), None);
        assert_eq!((change.command(), change.column()), (0, 0));
        assert_eq!((change.expected(), change.replacement()), (fg(1), fg(2)));
        assert_eq!(
            TeletextLocalObjectOperandOverlay::new(change.identity(), 0, 0, fg(1), fg(2)),
            Err(TeletextLocalObjectOperandOverlayError::Scope)
        );
    }
    assert_eq!(
        TeletextResourceObjectOperandOverlay::new(
            TeletextObjectIdentity::local(address(200, 0), TeletextObjectId::new(19)),
            0,
            0,
            fg(1),
            fg(2)
        ),
        Err(TeletextResourceObjectOperandOverlayError::Scope)
    );
}

fn pairs() -> Vec<(TeletextColumnOperation, TeletextColumnOperation)> {
    let value = |kind: &str, before: u8, after: u8| {
        (
            op(json!({"kind":kind,"code":before})),
            op(json!({"kind":kind,"code":after})),
        )
    };
    let attributes = |boxed| {
        op(
            json!({"kind":"attributes","double_height":true,"double_width":true,
        "boxed":boxed,"conceal":false,"invert":false,"underline_or_separated_mosaics":false}),
        )
    };
    let mut pairs = vec![
        (fg(1), fg(2)),
        (
            op(json!({"kind":"background","colour":0})),
            op(json!({"kind":"background","colour":2})),
        ),
        (attributes(false), attributes(true)),
        value("g0", 65, 66),
        value("g2", 35, 36),
        value("g1_alpha", 64, 65),
        (
            op(json!({"kind":"g1","mosaic":1})),
            op(json!({"kind":"g1","mosaic":63})),
        ),
        (
            op(json!({"kind":"g0_diacritic","code":65,"diacritic":1})),
            op(json!({"kind":"g0_diacritic","code":66,"diacritic":12})),
        ),
        (
            op(json!({"kind":"drcs","reference":{"table":"normal","subtable":0,"slot":0}})),
            op(json!({"kind":"drcs","reference":{"table":"global","subtable":15,"slot":47}})),
        ),
        (
            op(json!({"kind":"designate_charset","designation":0})),
            op(json!({"kind":"designate_charset","designation":32})),
        ),
        (
            TeletextColumnOperation::Flash {
                mode: TeletextFlashMode::Normal,
                rate: TeletextFlashRate::Slow1Hz,
            },
            TeletextColumnOperation::Flash {
                mode: TeletextFlashMode::Inverted,
                rate: TeletextFlashRate::Fast2HzPhase3,
            },
        ),
    ];
    for compatibility in ["level15", "level25"] {
        pairs.push((
            op(json!({"kind":"g3","code":47,"compatibility":compatibility})),
            op(json!({"kind":"g3","code":95,"compatibility":compatibility})),
        ));
    }
    pairs
}

#[test]
fn resource_object_scalars_reuse_all_existing_family_geometry_and_error_precedence_checks() {
    let owner = address(200, 0);
    for (before, after) in pairs() {
        for column in [0, 39, 40] {
            let expected =
                TeletextLocalProgramOperandOverlay::new(owner, usize::MAX, column, before, after)
                    .map(|_| ())
                    .map_err(TeletextResourceObjectOperandOverlayError::Operand);
            for global in [false, true] {
                assert_eq!(
                    TeletextResourceObjectOperandOverlay::new(
                        target(global, 1),
                        usize::MAX,
                        column,
                        before,
                        after
                    )
                    .map(|_| ()),
                    expected
                );
            }
        }
        assert_eq!(
            TeletextResourceObjectOperandOverlay::new(target(false, 1), 0, 0, before, before),
            Err(TeletextResourceObjectOperandOverlayError::Operand(
                TeletextLocalProgramOperandOverlayError::Unchanged
            ))
        );
    }
    let attrs = |wide| {
        op(
            json!({"kind":"attributes","double_height":false,"double_width":wide,
        "boxed":false,"conceal":false,"invert":false,"underline_or_separated_mosaics":false}),
        )
    };
    for (before, after, error) in [
        (
            fg(1),
            glyph(65),
            TeletextLocalProgramOperandOverlayError::Family,
        ),
        (
            attrs(false),
            attrs(true),
            TeletextLocalProgramOperandOverlayError::Geometry,
        ),
        (
            op(json!({"kind":"g3","code":47,"compatibility":"level15"})),
            op(json!({"kind":"g3","code":47,"compatibility":"level25"})),
            TeletextLocalProgramOperandOverlayError::Family,
        ),
    ] {
        assert_eq!(
            TeletextResourceObjectOperandOverlay::new(target(false, 1), 0, 0, before, after),
            Err(TeletextResourceObjectOperandOverlayError::Operand(error))
        );
    }
}

#[test]
fn resource_object_service_recollects_both_shared_consumers_and_preserves_all_other_inputs() {
    let input = input();
    let original = service(input.clone());
    for global in [false, true] {
        let edited = original
            .clone()
            .with_resource_object_operand_overlays(&[change(global)], budget())
            .unwrap();
        let mut expected = input.clone();
        let object = expected
            .objects
            .iter_mut()
            .find(|object| object.identity == change(global).identity())
            .unwrap();
        let mut commands = object.program.commands().to_vec();
        commands[0] = at(0, fg(2));
        object.program = program(commands);
        assert_eq!(edited, service(expected));
        assert_eq!(edited.base(), original.base());
        assert_eq!(
            edited.physical_base_overlays(),
            original.physical_base_overlays()
        );
        assert_eq!(edited.drcs(), original.drcs());
        assert_eq!(edited.object_companions(), original.object_companions());
        for owner in [address(200, 0), address(200, 1)] {
            assert_eq!(
                edited.effective_program(owner).unwrap(),
                original.effective_program(owner).unwrap()
            );
            assert_eq!(edited.collected(owner), original.collected(owner));
        }
        assert!(matches!(
            edited.with_resource_object_operand_overlays(&[change(global)], budget()),
            Err(TeletextServiceError::ResourceObjectOperandOverlay {
                reason: TeletextResourceObjectOperandFailure::CommandMismatch,
                ..
            })
        ));
    }
}

#[test]
fn resource_object_exact_target_failures_do_not_alias_scope_s1_or_original_command() {
    let original = service(input());
    for identity in [
        target(false, 0),
        target(true, 1),
        public(0xa3, 1, 19),
        public(0xa0, 1, 20),
    ] {
        let change =
            TeletextResourceObjectOperandOverlay::new(identity, 0, 0, fg(1), fg(2)).unwrap();
        assert!(
            matches!(original.clone().with_resource_object_operand_overlays(&[change],budget()),
            Err(TeletextServiceError::ResourceObjectOperandOverlay { identity:actual,
                reason:TeletextResourceObjectOperandFailure::MissingObject,.. }) if actual==identity)
        );
    }
    for (command, column, before) in [(usize::MAX, 0, fg(1)), (0, 1, fg(1)), (0, 0, fg(3))] {
        let change = TeletextResourceObjectOperandOverlay::new(
            target(false, 1),
            command,
            column,
            before,
            fg(2),
        )
        .unwrap();
        assert!(matches!(
            original
                .clone()
                .with_resource_object_operand_overlays(&[change], budget()),
            Err(TeletextServiceError::ResourceObjectOperandOverlay {
                reason: TeletextResourceObjectOperandFailure::CommandMismatch,
                ..
            })
        ));
    }
    assert!(matches!(
        original
            .clone()
            .with_resource_object_operand_overlays(&[change(false), change(false)], budget()),
        Err(TeletextServiceError::ResourceObjectOperandOverlay {
            reason: TeletextResourceObjectOperandFailure::Duplicate,
            ..
        })
    ));
    assert_eq!(
        original.with_resource_object_operand_overlays(&[], budget()),
        Err(TeletextServiceError::EmptyResourceObjectOperandOverlays)
    );
}

#[test]
fn resource_object_reconstruction_preserves_dependency_limits_before_cloning_or_accepting() {
    let original = service(input());
    let mut limited = budget();
    limited.max_commands = 0;
    let absent =
        TeletextResourceObjectOperandOverlay::new(target(false, 0), 0, 0, fg(1), fg(2)).unwrap();
    assert_eq!(
        original
            .clone()
            .with_resource_object_operand_overlays(&[absent], limited),
        Err(TeletextServiceError::Budget {
            dimension: "authored command",
            maximum: 0
        })
    );
    limited = budget();
    limited.max_collected_commands = 0;
    assert!(matches!(
        original
            .clone()
            .with_resource_object_operand_overlays(&[change(false)], limited),
        Err(TeletextServiceError::Budget {
            dimension: "collected command",
            maximum: 0
        })
    ));
    let reference =
        |slot| op(json!({"kind":"drcs","reference":{"table":"normal","subtable":0,"slot":slot}}));
    let drcs = TeletextResourceObjectOperandOverlay::new(
        target(false, 1),
        2,
        1,
        reference(0),
        reference(1),
    )
    .unwrap();
    assert!(matches!(
        original.with_resource_object_operand_overlays(&[drcs], budget()),
        Err(TeletextServiceError::CollectedDrcs { .. })
    ));
}

#[test]
fn resource_object_drcs_changes_revalidate_every_shared_consumer_with_exact_source() {
    let mut input = input();
    input.drcs.push(table(0xb0, 1, 2));
    let original = service(input.clone());
    let reference = |subtable| {
        op(json!({"kind":"drcs","reference":{"table":"normal","subtable":subtable,"slot":0}}))
    };
    let change = TeletextResourceObjectOperandOverlay::new(
        target(false, 1),
        2,
        1,
        reference(0),
        reference(1),
    )
    .unwrap();
    let edited = original
        .clone()
        .with_resource_object_operand_overlays(&[change], budget())
        .unwrap();
    for owner in [address(200, 0), address(200, 1)] {
        let refs = &edited.collected(owner).unwrap().1;
        assert_eq!(
            refs.references()
                .iter()
                .map(|reference| reference.subtable.get())
                .collect::<Vec<_>>(),
            vec![1, 0]
        );
        assert_eq!(
            refs.sources()[0],
            TeletextEnhancementSource {
                object: Some(target(false, 1)),
                command: 2
            }
        );
    }
    // Only the second executing consumer loses its required original link.
    input.pages[1].resources.drcs = TeletextDrcsPageLinks::new(Some(raw(0xb0)), None).unwrap();
    let global = TeletextResourceObjectOperandOverlay::new(
        target(false, 1),
        2,
        1,
        reference(0),
        op(json!({"kind":"drcs","reference":{"table":"global","subtable":0,"slot":0}})),
    )
    .unwrap();
    input.pages[0].resources.drcs =
        TeletextDrcsPageLinks::new(Some(raw(0xb0)), Some(raw(0xb1))).unwrap();
    input.drcs.push(
        TeletextDrcsSubtable::new(
            resource(TeletextResourceKind::Gdrcs, 0xb1, 0),
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                bitmap: TeletextDrcsBitmap::new([3; 10]).unwrap(),
            }],
        )
        .unwrap(),
    );
    assert!(
        matches!(service(input).with_resource_object_operand_overlays(&[global],budget()),
        Err(TeletextServiceError::CollectedDrcs {error:TeletextCollectedDrcsError {
            source_location:Some(TeletextEnhancementSource {object:Some(identity),command:2}),
            error:TeletextDrcsResourceError::MissingLink {display,table:TeletextDrcsTableScope::Global,..},
        },..}) if display==address(200,1) && identity==target(false,1))
    );
}

fn unused_mot() -> TeletextMotLevel25Declaration {
    let unused = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(2).unwrap(),
    };
    let object = TeletextMotObjectLink::new(
        unused,
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 77 }; 2],
    );
    let drcs = TeletextMotDrcsLink::new(unused, TeletextMotSubpageExtent::new(0).unwrap(), false);
    TeletextMotLevel25Declaration::new(
        resource(TeletextResourceKind::Mot, 0xfe, 9),
        vec![],
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            drcs,
            [drcs; 7],
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

#[test]
fn resource_object_declared_wrapper_preserves_exact_mot_inventory_and_display_resolutions() {
    let mut input = input();
    let link = |page, mask| {
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page { page: raw(page) },
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(mask),
        )
        .unwrap()
    };
    let absent = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(0xffff),
    )
    .unwrap();
    for page in &mut input.pages {
        page.compositional_links = Some(
            TeletextX27Level25Links::new(link(0xa1, 4), link(0xa0, 2), absent, link(0xb0, 1))
                .unwrap(),
        );
    }
    let mot = unused_mot();
    let original = TeletextDeclaredService::new(
        service(input),
        vec![mot.clone()],
        [address(200, 0), address(200, 1)]
            .map(|display| TeletextDisplayMotSelection {
                display,
                mot: Some(mot.identity()),
            })
            .to_vec(),
        vec![resource(TeletextResourceKind::Pop, 0xa2, 0)],
        TeletextDeclaredServiceBudget {
            max_mots: 8,
            max_displays: 8,
            max_resources: 64,
            max_defaults: 64,
            max_objects: 64,
        },
    )
    .unwrap();
    let edited = original
        .clone()
        .with_resource_object_operand_overlays(&[change(false), change(true)], budget())
        .unwrap();
    assert_eq!(edited.mots(), original.mots());
    assert_eq!(edited.selections(), original.selections());
    assert_eq!(edited.resource_inventory(), original.resource_inventory());
    assert_eq!(edited.resolutions(), original.resolutions());
    assert_eq!(edited.defaults(), original.defaults());
    assert_eq!(edited.semantic().base(), original.semantic().base());
    assert_eq!(
        edited.semantic().physical_base_overlays(),
        original.semantic().physical_base_overlays()
    );
    assert_eq!(edited.semantic().drcs(), original.semantic().drcs());
}
