//! Opt-in public native decoder proof, not complete client/transport acceptance.
//! The ctypes oracle deliberately supports Linux x64 only; other platforms skip
//! this diagnostic. Production encoding remains platform independent.

use ersatzrs_domain::*;

use super::packets::{data_unit, terminator};
use super::*;

fn resource(global: bool, s1: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(s1).unwrap(),
    )
    .unwrap()
}

fn target(resource: TeletextResourceIdentity, id: u16) -> TeletextObjectIdentity {
    if resource.kind() == TeletextResourceKind::Gpop {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(id)).unwrap()
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(id)).unwrap()
    }
}

fn position(row: u8, column: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::SetPosition {
        position: TeletextEnhancementPosition::new(row, column).unwrap(),
    }
}

fn at(operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column: 0,
        operation,
    }
}

fn definition(
    identity: TeletextObjectIdentity,
    kind: TeletextObjectType,
    character: u8,
    foreground: u8,
    background: u8,
    child: Option<(TeletextObjectIdentity, TeletextObjectType)>,
) -> TeletextEnhancementObjectDefinition {
    let mut commands = vec![
        position(1, 0),
        at(TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(foreground).unwrap(),
        }),
        at(TeletextColumnOperation::Background {
            colour: TeletextColourReference::new(background).unwrap(),
        }),
        at(TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(character).unwrap(),
        }),
    ];
    if let Some((target, object_type)) = child {
        commands.push(TeletextEnhancementCommand::InvokeObject {
            target,
            object_type,
            offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
        });
    }
    TeletextEnhancementObjectDefinition {
        identity,
        object_type: kind,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}

fn unused() -> TeletextX27LinkTarget {
    TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(8).unwrap(),
    }
}

fn mot(owner: TeletextResourceIdentity, global: bool) -> TeletextMotLevel25Declaration {
    let no_defaults = [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2];
    let inactive = TeletextMotObjectLink::new(
        unused(),
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        no_defaults,
    );
    let selected = TeletextMotObjectLink::new(
        TeletextX27LinkTarget::Page { page: owner.page() },
        TeletextMotSubpageExtent::new(owner.logical_subpage().get()).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        no_defaults,
    );
    let mut public = [inactive; 7];
    if !global {
        public[0] = selected;
    }
    let no_drcs =
        TeletextMotDrcsLink::new(unused(), TeletextMotSubpageExtent::new(0).unwrap(), false);
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
                global,
                (!global).then(|| TeletextMotPublicLinkIndex::new(1).unwrap()),
            ),
            TeletextMotAssociationSelection::new(false, None),
        )],
        TeletextMotLevel25Links::new(
            if global { selected } else { inactive },
            public,
            no_drcs,
            [no_drcs; 7],
            TeletextMotEnhancementCounts::new(1, owner.logical_subpage().get() + 1, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

fn display(
    root: TeletextObjectIdentity,
    kind: TeletextObjectType,
    allocations: &TeletextResourceObjectAllocations,
) -> Vec<[u8; 46]> {
    let owner = root.resource().unwrap();
    let mut draft = super::transport_tests::draft(200, 0, false);
    draft.rows = [2_u8, 4, 5, 6]
        .into_iter()
        .map(|row| TeletextRow {
            row,
            runs: vec![TeletextRun {
                column: if row == 2 { 1 } else { 8 },
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text {
                    text: if row == 2 { "B" } else { "TTTT" }.into(),
                },
            }],
        })
        .collect();
    let base = encode_teletext_page(&draft, TeletextEncodingRegion::WesternLatin).unwrap();
    let program = TeletextEnhancementProgram::new(vec![
        position(3, 8),
        TeletextEnhancementCommand::InvokeObject {
            target: root,
            object_type: kind,
            offset: None,
        },
    ])
    .unwrap();
    let local = compile_teletext_local_program_with_resource_objects(
        draft.address,
        &program,
        2,
        allocations,
        TeletextResolvedObjectPageLinks {
            public: (owner.kind() == TeletextResourceKind::Pop).then_some(owner.page()),
            global: (owner.kind() == TeletextResourceKind::Gpop).then_some(owner.page()),
        },
    )
    .unwrap();
    let original = base.data_units();
    assert!(
        matches!(source_header::observe_dvb_teletext_header(&original[2]).unwrap(),
        source_header::TeletextHeaderObservation::NotHeader(address) if address.packet_number == 26)
    );
    let mut units = original[..2].to_vec();
    for body in local.packet_bodies() {
        units.push(data_unit(2, 26, body, false));
    }
    units.extend_from_slice(&original[3..]);
    units.push(terminator(2));
    units
}

fn cell(row: u8, column: u8, character: u8, foreground: u8, background: u8) -> serde_json::Value {
    serde_json::json!({"row":row,"column":column,"unicode":character,
        "foreground":foreground,"background":background})
}

fn case(
    name: &str,
    global: bool,
    kind: TeletextObjectType,
    s1: u8,
    nested: bool,
) -> serde_json::Value {
    let owner = resource(global, s1);
    let root = target(owner, 0xffff);
    let adaptive = target(owner, 0x100);
    let passive = target(owner, 0);
    let mut definitions = vec![definition(
        root,
        kind,
        b'A',
        1,
        4,
        nested.then_some((adaptive, TeletextObjectType::Adaptive)),
    )];
    if nested {
        definitions.push(definition(
            adaptive,
            TeletextObjectType::Adaptive,
            b'D',
            2,
            5,
            Some((passive, TeletextObjectType::Passive)),
        ));
        definitions.push(definition(
            passive,
            TeletextObjectType::Passive,
            b'P',
            3,
            6,
            None,
        ));
    }
    let catalog = TeletextEnhancementObjectCatalog::new(
        definitions,
        TeletextObjectCatalogBudget {
            max_objects: 3,
            max_commands: 16,
        },
    )
    .unwrap();
    let allocations = allocate_teletext_resource_objects(&catalog, &[], 3).unwrap();
    let counter = TeletextResourceUpdateCounter::new(0).unwrap();
    let mut units = encode_teletext_mot_page(
        &mot(owner, global),
        &[[None; 2]; 8],
        counter,
        TeletextResourceFullUpdate::Changed,
        TeletextResourceTransmissionMode::Serial,
    )
    .unwrap()
    .data_units()
    .to_vec();
    units.push(terminator(2));
    for logical in 0..=s1 {
        let page = encode_teletext_object_page(
            resource(global, logical),
            &catalog,
            &allocations,
            TeletextObjectPageTransmission {
                counter,
                update: TeletextResourceFullUpdate::Changed,
                mode: TeletextResourceTransmissionMode::Serial,
                last_subpage: logical == s1,
            },
            16,
        )
        .unwrap();
        units.extend_from_slice(page.data_units());
        // Preserve C4-set resource bytes; .42 finalizes with own-magazine FF.
        units.push(terminator(0));
    }
    units.extend(display(root, kind, &allocations));
    let mut cells = vec![cell(4, 8, b'A', 1, 4)];
    if nested {
        cells.push(cell(5, 9, b'D', 2, 5));
        cells.push(cell(6, 10, b'P', 3, 6));
    } else {
        let (foreground, background) = if kind == TeletextObjectType::Active {
            (1, 4)
        } else {
            (7, 0)
        };
        cells.push(cell(4, 9, b'T', foreground, background));
    }
    serde_json::json!({"name":name,"units":units.iter().map(|unit|unit.to_vec()).collect::<Vec<_>>(),
        "page":0x200,"subcode":0,"cells":cells})
}

#[tokio::test]
#[ignore = "Requires Python 3 and the explicitly calibrated native libzvbi decoder"]
async fn authored_object_pages_decode_with_pinned_independent_libzvbi() {
    let cases = vec![
        case("pop_active_s0", false, TeletextObjectType::Active, 0, false),
        case(
            "pop_adaptive_s15",
            false,
            TeletextObjectType::Adaptive,
            15,
            false,
        ),
        case(
            "pop_passive_s15",
            false,
            TeletextObjectType::Passive,
            15,
            false,
        ),
        case("gpop_active_s0", true, TeletextObjectType::Active, 0, false),
        case(
            "gpop_adaptive_s15",
            true,
            TeletextObjectType::Adaptive,
            15,
            false,
        ),
        case(
            "gpop_passive_s15",
            true,
            TeletextObjectType::Passive,
            15,
            false,
        ),
        case("pop_nested", false, TeletextObjectType::Active, 0, true),
        case("gpop_nested", true, TeletextObjectType::Active, 0, true),
    ];
    let request =
        serde_json::to_vec(&serde_json::json!({"expected_version":"0.2.42","cases":cases}))
            .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_OBJECT_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-object-decoder.py");
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(report["decoder_version"], "0.2.42");
    assert_eq!(report["cases_passed"], 8);
    println!("Independent object calibration: {report}");
}
