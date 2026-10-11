//! Original semantic colours, not synthetic selector or renderer acceptance.

use ersatzrs_domain::*;

use super::colour_context::{
    resolve_teletext_colour_context as resolve, TeletextColourContextFailure as Failure,
};

fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(200).unwrap(),
        subcode: TeletextSubcode::new(0x1234).unwrap(),
    }
}

fn colour(value: u8) -> TeletextColourReference {
    TeletextColourReference::new(value).unwrap()
}

fn screen(value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetScreenColour {
        colour: colour(value),
    }
}

fn row(number: u8, value: u8, following_rows: bool) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetRowColour {
        row: number,
        colour: colour(value),
        following_rows,
    }
}

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}

fn local(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(owner(), TeletextObjectId::new(id))
}

fn public(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Pop,
            TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
            TeletextResourceSubpageIndex::new(0).unwrap(),
        )
        .unwrap(),
        TeletextObjectId::new(id),
    )
    .unwrap()
}

fn invoke(target: TeletextObjectIdentity, kind: TeletextObjectType) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::InvokeObject {
        target,
        object_type: kind,
        offset: None,
    }
}

fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: program(commands),
    }
}

fn defaults(screen: u8, row: u8) -> TeletextLevel25PresentationDefaults {
    let mut defaults =
        super::presentation::western_latin_v1_presentation(TeletextNationalOption::new(1).unwrap())
            .unwrap();
    defaults.screen_colour = colour(screen);
    defaults.row_colour = colour(row);
    defaults
}

fn service(
    main: Option<Vec<TeletextEnhancementCommand>>,
    objects: Vec<TeletextEnhancementObjectDefinition>,
    selections: [Option<TeletextDefaultObjectSelection>; 4],
    presentation: Option<TeletextLevel25PresentationDefaults>,
    magazine: Option<TeletextLevel25PresentationDefaults>,
) -> TeletextSemanticService {
    let language = TeletextLanguage::new("nld".into()).unwrap();
    let public = objects.iter().find_map(|definition| {
        (definition.identity.scope() == TeletextObjectScope::Public)
            .then(|| definition.identity.resource().unwrap().page())
    });
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                language: language.clone(),
                initial_page: owner().page,
                pages: vec![TeletextPageDraft {
                    address: owner(),
                    kind: TeletextPageKind::FullPage,
                    language,
                    national_option: TeletextNationalOption::new(1).unwrap(),
                    rows: vec![],
                    navigation: vec![],
                }],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: owner(),
                presentation,
                compositional_links: None,
                local_program: main.map(program),
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public,
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::unlinked(),
                    defaults: selections,
                },
            }],
            magazines: magazine
                .into_iter()
                .map(|presentation| TeletextMagazinePresentationDraft {
                    magazine: 2,
                    presentation,
                })
                .collect(),
            objects,
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 32,
            max_objects: 16,
            max_structure_items: 1024,
            max_base_content_bytes: 0,
            max_commands: 1024,
            max_collected_commands: 4096,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}

fn simple(main: Vec<TeletextEnhancementCommand>) -> TeletextSemanticService {
    service(Some(main), vec![], [None; 4], None, None)
}

#[test]
fn page_then_magazine_then_black_defaults_do_not_use_default_screen_for_rows() {
    for (page, magazine, expected_screen, expected_row) in [
        (None, None, 0, 0),
        (None, Some(defaults(6, 4)), 6, 4),
        (Some(defaults(7, 2)), Some(defaults(6, 4)), 7, 2),
    ] {
        let service = service(Some(vec![]), vec![], [None; 4], page, magazine);
        let actual = resolve(&service, owner(), 0).unwrap();
        assert_eq!(actual.owner(), owner());
        assert_eq!(actual.screen_colour(), colour(expected_screen));
        assert_eq!(actual.row_colours(), &[colour(expected_row); 25]);
        assert_eq!(actual.row_colour(25), None);
    }
}

#[test]
fn explicit_screen_supplies_unassigned_rows_but_real_row_colour_wins() {
    let service = service(
        Some(vec![screen(12), row(7, 3, false)]),
        vec![],
        [None; 4],
        Some(defaults(6, 2)),
        None,
    );
    let actual = resolve(&service, owner(), 2).unwrap();
    assert_eq!(actual.screen_colour(), colour(12));
    for number in 0..25 {
        assert_eq!(
            actual.row_colour(number),
            Some(colour(if number == 7 { 3 } else { 12 }))
        );
    }
}

#[test]
fn original_main_colours_outrank_later_active_invocation_colours() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            screen(1),
            row(0, 2, false),
            invoke(target, TeletextObjectType::Active),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![screen(4), row(0, 5, true)],
        )],
        [None; 4],
        None,
        None,
    );
    let actual = resolve(&service, owner(), 5).unwrap();
    assert_eq!(actual.screen_colour(), colour(1));
    assert_eq!(actual.row_colour(0), Some(colour(2)));
    assert_eq!(actual.row_colour(1), Some(colour(5)));
}

#[test]
fn original_later_active_header_colour_is_resolved_before_any_synthetic_selector() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            position(0, 8),
            invoke(target, TeletextObjectType::Active),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![row(0, 4, false)],
        )],
        [None; 4],
        Some(defaults(6, 2)),
        None,
    );
    let before = service.base().clone();
    let actual = resolve(&service, owner(), 3).unwrap();
    assert_eq!(actual.row_colour(0), Some(colour(4)));
    assert_eq!(actual.row_colour(1), Some(colour(2)));
    assert_eq!(service.base(), &before);
    assert_eq!(
        service.page_enhancements()[&owner()]
            .local_program
            .as_ref()
            .unwrap()
            .commands()
            .len(),
        2
    );
}

#[test]
fn active_invocation_order_is_preserved_instead_of_catalogue_order() {
    let first = local(0xffff);
    let second = local(0x0000);
    let service = service(
        Some(vec![
            invoke(second, TeletextObjectType::Active),
            invoke(first, TeletextObjectType::Active),
        ]),
        vec![
            definition(
                first,
                TeletextObjectType::Active,
                vec![screen(1), row(0, 2, false)],
            ),
            definition(
                second,
                TeletextObjectType::Active,
                vec![screen(4), row(0, 5, false)],
            ),
        ],
        [None; 4],
        None,
        None,
    );
    let actual = resolve(&service, owner(), 6).unwrap();
    assert_eq!(actual.screen_colour(), colour(1));
    assert_eq!(actual.row_colour(0), Some(colour(2)));
    assert_eq!(actual.row_colour(1), Some(colour(1)));
}

#[test]
fn following_rows_carry_survives_later_this_only_and_repeated_row_zero_selectors() {
    let service = simple(vec![
        row(0, 2, true),
        row(0, 3, false),
        row(5, 4, false),
        row(10, 6, true),
    ]);
    let actual = resolve(&service, owner(), 4).unwrap();
    for number in 0..25 {
        let expected = match number {
            0 => 3,
            5 => 4,
            10..=24 => 6,
            _ => 2,
        };
        assert_eq!(actual.row_colour(number), Some(colour(expected)));
    }
}

#[test]
fn sparse_default_order_is_reused_and_present_empty_local_suppresses_defaults() {
    let first = public(0xffff);
    let second = public(0x0100);
    let objects = vec![
        definition(second, TeletextObjectType::Active, vec![row(0, 4, false)]),
        definition(first, TeletextObjectType::Active, vec![row(0, 2, false)]),
    ];
    let mut selections = [None; 4];
    selections[2] = Some(TeletextDefaultObjectSelection {
        target: first,
        object_type: TeletextObjectType::Active,
    });
    selections[3] = Some(TeletextDefaultObjectSelection {
        target: second,
        object_type: TeletextObjectType::Active,
    });
    let defaults = service(None, objects.clone(), selections, None, None);
    assert_eq!(
        resolve(&defaults, owner(), 4).unwrap().row_colour(0),
        Some(colour(4))
    );
    let empty = service(Some(vec![]), objects, selections, None, None);
    assert_eq!(
        resolve(&empty, owner(), 0).unwrap().row_colours(),
        &[colour(0); 25]
    );
}

#[test]
fn exact_invocation_row_offset_is_applied_without_changing_caller_position() {
    let target = local(0xffff);
    let mut offset = invoke(target, TeletextObjectType::Active);
    if let TeletextEnhancementCommand::InvokeObject { offset, .. } = &mut offset {
        *offset = Some(TeletextObjectOffset::new(3, 71).unwrap());
    }
    let service = service(
        Some(vec![position(2, 39), offset, row(3, 7, false)]),
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![row(4, 2, false)],
        )],
        [None; 4],
        None,
        None,
    );
    let actual = resolve(&service, owner(), 4).unwrap();
    assert_eq!(actual.row_colour(9), Some(colour(2)));
    assert_eq!(actual.row_colour(3), Some(colour(7)));
    // Absolute object column 110 is intentionally not a colour-row rejection.
    // Its eventual glyph/panel geometry is a separate compiler obligation.
}

#[test]
fn out_of_display_colour_rows_are_located_without_clamping_or_u8_wrap() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            position(24, 0),
            TeletextEnhancementCommand::InvokeObject {
                target,
                object_type: TeletextObjectType::Active,
                offset: Some(TeletextObjectOffset::new(23, 0).unwrap()),
            },
        ]),
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![row(24, 2, true)],
        )],
        [None; 4],
        None,
        None,
    );
    assert_eq!(
        resolve(&service, owner(), 2).unwrap_err().reason,
        Failure::CommandBudget { maximum: 2 }
    );
    let error = resolve(&service, owner(), 3).unwrap_err();
    assert_eq!(error.reason, Failure::RowGeometry { row: 71 });
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 0
        })
    );
    assert_eq!(error.invocation, Some(1));
}

#[test]
fn main_and_active_relative_backwards_positions_are_located_separately() {
    let main = simple(vec![position(1, 20), position(1, 19)]);
    let error = resolve(&main, owner(), 2).unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(error.invocation, None);
    assert_eq!(
        error.reason,
        Failure::ActivePositionOrder {
            previous: (1, 20),
            requested: (1, 19)
        }
    );
    let target = local(0xffff);
    let active = service(
        Some(vec![invoke(target, TeletextObjectType::Active)]),
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![position(2, 0), position(1, 0)],
        )],
        [None; 4],
        None,
        None,
    );
    let error = resolve(&active, owner(), 3).unwrap_err();
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: Some(target),
            command: 1
        })
    );
    assert_eq!(error.invocation, Some(0));
}

#[test]
fn root_length_and_repeated_active_body_budgets_precede_geometry_processing() {
    let target = local(0xffff);
    let service = service(
        Some(vec![
            invoke(target, TeletextObjectType::Active),
            invoke(target, TeletextObjectType::Active),
        ]),
        vec![definition(
            target,
            TeletextObjectType::Active,
            vec![row(0, 4, false), position(1, 0)],
        )],
        [None; 4],
        None,
        None,
    );
    let error = resolve(&service, owner(), 1).unwrap_err();
    assert_eq!(error.reason, Failure::CommandBudget { maximum: 1 });
    assert_eq!(error.location, None);
    let error = resolve(&service, owner(), 5).unwrap_err();
    assert_eq!(error.reason, Failure::CommandBudget { maximum: 5 });
    assert_eq!(
        error.location,
        Some(TeletextEnhancementSource {
            object: None,
            command: 1
        })
    );
    assert_eq!(
        resolve(&service, owner(), 6).unwrap().row_colour(0),
        Some(colour(4))
    );
    let overflow = simple(vec![position(3, 0), position(2, 0)]);
    assert_eq!(
        resolve(&overflow, owner(), 1).unwrap_err().reason,
        Failure::CommandBudget { maximum: 1 }
    );
}

#[test]
fn unknown_subcode_is_not_replaced_by_an_empty_same_page_context() {
    let service = simple(vec![]);
    let mut unknown = owner();
    unknown.subcode = TeletextSubcode::new(0x1235).unwrap();
    let error = resolve(&service, unknown, 0).unwrap_err();
    assert_eq!(error.owner, unknown);
    assert_eq!(error.reason, Failure::UnknownOwner);
    assert_eq!(error.location, None);
}

#[test]
fn non_active_colour_declarations_are_not_contributors_or_readiness_validation() {
    let target = local(0xffff);
    for kind in [TeletextObjectType::Adaptive, TeletextObjectType::Passive] {
        let service = service(
            Some(vec![invoke(target, kind)]),
            vec![definition(target, kind, vec![screen(4), row(0, 5, true)])],
            [None; 4],
            None,
            None,
        );
        // The physical compiler must reject these inapplicable declarations. This
        // resolver inspects only the root, so it neither applies nor certifies them.
        let actual = resolve(&service, owner(), 1).unwrap();
        assert_eq!(actual.screen_colour(), colour(0));
        assert_eq!(actual.row_colours(), &[colour(0); 25]);
    }
}
