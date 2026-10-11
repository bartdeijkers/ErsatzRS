//! Opt-in native MOT activation calibration, not full client/service acceptance.
//! The existing oracle is intentionally limited to Linux x64 libzvbi 0.2.42.

use ersatzrs_domain::*;

use super::packets::{data_unit, terminator};
use super::*;

#[derive(Clone, Copy)]
enum DefaultProgram {
    RootOnly,
    NestedSelected,
}

fn resource(page: u8, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(8, page).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}
fn target(page: u8, s1: u8, id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::public(resource(page, s1), TeletextObjectId::new(id)).unwrap()
}
fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}
fn at(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    at(
        column,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}
fn definition(
    identity: TeletextObjectIdentity,
    object_type: TeletextObjectType,
    row: u8,
    column: u8,
    code: u8,
    colours: (u8, u8),
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity,
        object_type,
        program: TeletextEnhancementProgram::new(vec![
            position(row, column),
            at(
                column,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(colours.0).unwrap(),
                },
            ),
            at(
                column,
                TeletextColumnOperation::Background {
                    colour: TeletextColourReference::new(colours.1).unwrap(),
                },
            ),
            glyph(column, code),
        ])
        .unwrap(),
    }
}
fn definitions(mode: DefaultProgram) -> Vec<TeletextEnhancementObjectDefinition> {
    let mut root = definition(
        target(0xa0, 1, 0xffff),
        TeletextObjectType::Active,
        1,
        8,
        b'A',
        (1, 4),
    );
    let mut commands = root.program.commands().to_vec();
    if matches!(mode, DefaultProgram::NestedSelected) {
        commands.push(TeletextEnhancementCommand::InvokeObject {
            target: target(0xa1, 0, 0xfffe),
            object_type: TeletextObjectType::Adaptive,
            offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
        });
    }
    commands.push(glyph(9, b'C'));
    root.program = TeletextEnhancementProgram::new(commands).unwrap();
    vec![
        root,
        definition(
            target(0xa1, 0, 0xfffe),
            TeletextObjectType::Adaptive,
            1,
            0,
            b'D',
            (2, 5),
        ),
        // Same Active physical selector/S1 as the default root, contrasting page.
        definition(
            target(0xa1, 1, 0x100),
            TeletextObjectType::Active,
            1,
            8,
            b'Z',
            (3, 6),
        ),
    ]
}
fn links() -> TeletextX27Level25Links {
    let declaration = |target, mask| {
        TeletextX27LinkDeclaration::new(
            target,
            TeletextX27LinkValidity::Level25Only,
            TeletextSubpageRequirementMask::new(mask),
        )
        .unwrap()
    };
    let unused = declaration(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(8).unwrap(),
        },
        0xffff,
    );
    TeletextX27Level25Links::new(
        unused,
        declaration(
            TeletextX27LinkTarget::Page {
                page: resource(0xa1, 0).page(),
            },
            3,
        ),
        unused,
        unused,
    )
    .unwrap()
}
fn base() -> TeletextPageDraft {
    let mut draft = super::transport_tests::draft(200, 0x10, false);
    draft.rows = [(1, 8, "TTTT"), (2, 1, "B")]
        .into_iter()
        .map(|(row, column, text)| TeletextRow {
            row,
            runs: vec![TeletextRun {
                column,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: text.into() },
            }],
        })
        .collect();
    draft.navigation.push(TeletextNavigationLink {
        role: TeletextNavigationRole::Index,
        target: TeletextNavigationTarget {
            page: draft.address.page,
            subcode: Some(draft.address.subcode),
        },
    });
    draft
}
fn defaults() -> [Option<TeletextDefaultObjectSelection>; 4] {
    let mut result = [None; 4];
    result[3] = Some(TeletextDefaultObjectSelection {
        target: target(0xa0, 1, 0xffff),
        object_type: TeletextObjectType::Active,
    });
    result
}
fn service(mode: DefaultProgram) -> TeletextSemanticService {
    let page = base();
    TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: TeletextPageSetDraft {
                initial_page: page.address.page,
                language: page.language.clone(),
                pages: vec![page.clone()],
            },
            pages: vec![TeletextResolvedPageEnhancementDraft {
                address: page.address,
                presentation: Some(
                    super::presentation::western_latin_v1_presentation(page.national_option)
                        .unwrap(),
                ),
                compositional_links: Some(links()),
                local_program: None,
                resources: TeletextResolvedPageResourceSelection {
                    objects: TeletextResolvedObjectPageLinks {
                        public: Some(resource(0xa1, 0).page()),
                        global: None,
                    },
                    drcs: TeletextDrcsPageLinks::new(None, None).unwrap(),
                    defaults: defaults(),
                },
            }],
            magazines: vec![],
            objects: definitions(mode),
            drcs: vec![],
        },
        TeletextServiceBudget {
            max_records: 16,
            max_objects: 3,
            max_structure_items: 128,
            max_base_content_bytes: 32,
            max_commands: 32,
            max_collected_commands: 32,
            max_collected_drcs_references: 0,
        },
    )
    .unwrap()
}
fn mot() -> TeletextMotLevel25Declaration {
    let unused = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(8).unwrap(),
    };
    let empty = TeletextMotObjectLink::new(
        unused,
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let mut public = [empty; 7];
    public[0] = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page {
            page: resource(0xa0, 1).page(),
        },
        TeletextMotSubpageExtent::new(1).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [
            TeletextMotDefaultObject::NoObject { raw_number: 0 },
            TeletextMotDefaultObject::Object {
                target: target(0xa0, 1, 0xffff),
                object_type: TeletextObjectType::Active,
            },
        ],
    );
    let no_drcs =
        TeletextMotDrcsLink::new(unused, TeletextMotSubpageExtent::new(0).unwrap(), false);
    TeletextMotLevel25Declaration::new(
        TeletextResourceIdentity::new(
            TeletextResourceKind::Mot,
            TeletextRawDataPageNumber::new(2, 0xfe).unwrap(),
            TeletextResourceSubpageIndex::ZERO,
        )
        .unwrap(),
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
            public,
            no_drcs,
            [no_drcs; 7],
            TeletextMotEnhancementCounts::new(1, 4, 0),
        )
        .unwrap(),
    )
    .unwrap()
}
fn packet(unit: &[u8; 46]) -> u8 {
    match source_header::observe_dvb_teletext_header(unit).unwrap() {
        source_header::TeletextHeaderObservation::Header(_)
        | source_header::TeletextHeaderObservation::Terminator(_) => 0,
        source_header::TeletextHeaderObservation::NotHeader(address) => address.packet_number,
        source_header::TeletextHeaderObservation::NotTeletext { .. } => {
            panic!("non-teletext production unit")
        }
    }
}
fn resource_units(service: &TeletextSemanticService) -> Vec<[u8; 46]> {
    let supplied_defaults: Vec<_> = defaults().into_iter().flatten().collect();
    let allocations =
        allocate_teletext_resource_objects(service.objects(), &supplied_defaults, 3).unwrap();
    let mut mot_allocations = [[None; 2]; 8];
    let allocation = allocations
        .lookup(target(0xa0, 1, 0xffff), TeletextObjectType::Active)
        .unwrap();
    mot_allocations[1][1] = Some(allocation.mot_default_allocation().unwrap());
    assert_eq!(mot_allocations[1][1].unwrap().selector, 1);
    assert_eq!(
        allocations
            .lookup(target(0xa1, 1, 0x100), TeletextObjectType::Active)
            .unwrap()
            .object_number(),
        1
    );
    let counter = TeletextResourceUpdateCounter::new(0).unwrap();
    let mut units = encode_teletext_mot_page(
        &mot(),
        &mot_allocations,
        counter,
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap()
    .data_units()
    .to_vec();
    units.push(terminator(2));
    for page in [0xa0, 0xa1] {
        for s1 in 0..=1 {
            let encoded = encode_teletext_object_page(
                resource(page, s1),
                service.objects(),
                &allocations,
                TeletextObjectPageTransmission {
                    counter,
                    update: TeletextResourceFullUpdate::Changed,
                    mode: TeletextResourceTransmissionMode::Serial,
                    last_subpage: s1 == 1,
                },
                32,
            )
            .unwrap();
            units.extend_from_slice(encoded.data_units());
            units.push(terminator(0)); // Resource magazine eight, public ABI calibration.
        }
    }
    units
}
fn cell(row: u8, column: u8, unicode: u8, foreground: u8, background: u8) -> serde_json::Value {
    serde_json::json!({"row":row,"column":column,"unicode":unicode,"foreground":foreground,"background":background})
}
fn case(
    name: &str,
    suppressed: bool,
    mode: DefaultProgram,
    resources: &[[u8; 46]],
    display: &[[u8; 46]],
) -> serde_json::Value {
    let mut units = resources.to_vec();
    let before_body = display.iter().position(|unit| packet(unit) == 1).unwrap();
    units.extend_from_slice(&display[..before_body]);
    if suppressed {
        let empty = TeletextEnhancementProgram::new(vec![]).unwrap();
        let local = compile_teletext_local_program(base().address, &empty, 0).unwrap();
        assert_eq!(local.packet_bodies().len(), 1);
        units.push(data_unit(2, 26, &local.packet_bodies()[0], false));
    }
    units.extend_from_slice(&display[before_body..]);
    units.push(terminator(2));
    let cells = if suppressed {
        vec![
            cell(1, 8, b'T', 7, 0),
            cell(1, 9, b'T', 7, 0),
            cell(2, 9, b' ', 7, 0),
        ]
    } else {
        vec![
            cell(1, 8, b'A', 1, 4),
            cell(1, 9, b'C', 1, 4),
            match mode {
                DefaultProgram::NestedSelected => cell(2, 9, b'D', 2, 5),
                DefaultProgram::RootOnly => cell(2, 9, b' ', 7, 0),
            },
        ]
    };
    serde_json::json!({"name":name,"units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),"page":0x200,"subcode":0x10,"cells":cells})
}
fn cases(mode: DefaultProgram) -> Vec<serde_json::Value> {
    let service = service(mode);
    let original = service.clone();
    let page = base();
    let resolution =
        TeletextMotLevel25Resolution::resolve(&mot(), page.address, Some(links())).unwrap();
    assert!(matches!(resolution.role(TeletextX27LinkRole::Pop),
        TeletextMotRoleResolution::Selected { target: selected, .. }
            if *selected == resource(0xa1, 0).page()));
    assert_eq!(
        resolution.default_sources()[1].unwrap().link.defaults()[1],
        TeletextMotDefaultObject::Object {
            target: target(0xa0, 1, 0xffff),
            object_type: TeletextObjectType::Active,
        }
    );
    let encoded = encode_teletext_native_default_page(
        &service,
        page.address,
        TeletextEncodingRegion::WesternLatin,
    )
    .unwrap();
    assert_eq!(encoded.defaults(), &defaults());
    assert!(encoded.magazine_units().is_empty());
    assert_eq!(encoded.page().address(), page.address);
    assert_eq!(encoded.page().language(), &page.language);
    assert!(encoded
        .page()
        .data_units()
        .iter()
        .all(|unit| packet(unit) != 26));
    let legacy = encode_teletext_page(&page, TeletextEncodingRegion::WesternLatin).unwrap();
    for unit in legacy.data_units().iter().filter(|unit| packet(unit) != 26) {
        assert!(
            encoded.page().data_units().contains(unit),
            "changed v1 header/navigation/presentation/fallback unit"
        );
    }
    let resources = resource_units(&service);
    let result = vec![
        case(
            match mode {
                DefaultProgram::NestedSelected => "native_foreign_mot_default_nested_selected_page",
                DefaultProgram::RootOnly => "native_foreign_mot_default_root_only",
            },
            false,
            mode,
            &resources,
            encoded.page().data_units(),
        ),
        case(
            match mode {
                DefaultProgram::NestedSelected => {
                    "local_termination_suppresses_foreign_mot_default"
                }
                DefaultProgram::RootOnly => "local_termination_suppresses_foreign_root_only",
            },
            true,
            mode,
            &resources,
            encoded.page().data_units(),
        ),
    ];
    assert_eq!(service, original);
    result
}

// libzvbi's shifted X27/4 fields decode the valid 8A1 link as 358; a failed
// nested lookup rolls back even the correctly activated sparse MOT root. Keep
// the full production request and A/C/D oracle unchanged as a conformance gate.
#[tokio::test]
#[ignore = "Known libzvbi 0.2.42 X27/4 parsing deficiency; nested conformance remains unproven"]
async fn native_foreign_mot_default_and_nested_selected_resource_decode() {
    run_cases(
        DefaultProgram::NestedSelected,
        "ERSATZRS_TELETEXT_TEST_NATIVE_DEFAULT_FIXTURE",
    )
    .await;
}

#[tokio::test]
#[ignore = "Requires Python 3 and explicitly calibrated Linux x64 libzvbi 0.2.42"]
async fn native_foreign_mot_root_only_and_local_suppression_decode() {
    run_cases(
        DefaultProgram::RootOnly,
        "ERSATZRS_TELETEXT_TEST_NATIVE_DEFAULT_ROOT_FIXTURE",
    )
    .await;
}

async fn run_cases(mode: DefaultProgram, capture_variable: &str) {
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases(mode)}))
            .unwrap();
    if let Some(path) = crate::locator::first_non_empty_env(&[capture_variable]) {
        std::fs::write(path, &request).unwrap();
    }
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-object-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(report["decoder_version"], "0.2.42");
    assert_eq!(report["cases_passed"], 2);
    println!("Independent native default activation: {report}");
}
