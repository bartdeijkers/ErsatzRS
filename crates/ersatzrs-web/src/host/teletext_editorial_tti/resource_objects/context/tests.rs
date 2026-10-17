//! Actual native services and sealed catalogue projections, not uploaded maps.
use super::super::{mapping, project::reconcile_project, project::validate_applied_project};
use super::*;
use ersatzrs_app::{
    teletext_export_project::{
        TeletextExportAssetScope, TeletextExportFile, TeletextExportProject,
    },
    teletext_tti::{write_teletext_tti, TeletextTtiDocument, TeletextTtiRecord},
};
use ersatzrs_domain::*;
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, lift_teletext_v1_page_set,
    EncodedTeletextAuthoredService, TeletextEncodingRegion, TeletextResourceFullUpdate,
    TeletextServiceResourceTransmission,
};
use serde_json::json;

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(30)
}
fn policy() -> crate::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    crate::host::teletext_runtime_policy::parse_native_teletext_runtime_policy(include_str!(
        "../../../../../tests/fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap()
    .encoding
}
fn limits() -> project::Limits {
    (
        TeletextTtiBudget {
            max_bytes: 65536,
            max_records: 128,
        },
        1024,
        policy().validation.service,
    )
}
fn target(id: u16) -> TeletextObjectIdentity {
    let global = id == 21;
    let identity = TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(
            1,
            if global {
                0xa1
            } else if id == 24 {
                0xa2
            } else {
                0xa0
            },
        )
        .unwrap(),
        TeletextResourceSubpageIndex::new(if global { 2 } else { 15 }).unwrap(),
    )
    .unwrap();
    if global {
        TeletextObjectIdentity::global(identity, TeletextObjectId::new(id))
    } else {
        TeletextObjectIdentity::public(identity, TeletextObjectId::new(id))
    }
    .unwrap()
}
fn program(commands: Vec<TeletextEnhancementCommand>) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands).unwrap()
}
fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}
fn root(offset: u8, selected: u16, extra: bool) -> TeletextEnhancementProgram {
    let mut commands = vec![glyph(0, b'A')];
    if extra {
        commands.push(TeletextEnhancementCommand::AtColumn {
            column: 1,
            operation: TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(2).unwrap(),
            },
        });
        commands.push(glyph(1, b'B'));
    }
    commands.push(TeletextEnhancementCommand::InvokeObject {
        target: target(selected),
        object_type: if selected == 23 {
            TeletextObjectType::Adaptive
        } else {
            TeletextObjectType::Passive
        },
        offset: Some(TeletextObjectOffset::new(0, offset).unwrap()),
    });
    program(commands)
}
fn address() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(123).unwrap(),
        subcode: TeletextSubcode::new(0).unwrap(),
    }
}
fn links() -> TeletextX27Level25Links {
    let absent = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(1).unwrap(),
        },
        TeletextX27LinkValidity::Level25And35,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    let selected = |id| {
        TeletextX27LinkDeclaration::new(
            TeletextX27LinkTarget::Page {
                page: target(id).resource().unwrap().page(),
            },
            TeletextX27LinkValidity::Level25And35,
            TeletextSubpageRequirementMask::new(
                1 << target(id).resource().unwrap().logical_subpage().get(),
            ),
        )
        .unwrap()
    };
    TeletextX27Level25Links::new(selected(21), selected(22), absent, absent).unwrap()
}
fn native(replacement: TeletextEnhancementProgram) -> EncodedTeletextAuthoredService {
    native_fixture(replacement, false)
}
fn definitions(
    replacement: TeletextEnhancementProgram,
    colliding: bool,
) -> Vec<TeletextEnhancementObjectDefinition> {
    let mut objects = [
        (21, TeletextObjectType::Active, replacement),
        (
            22,
            TeletextObjectType::Passive,
            program(vec![glyph(0, b'P')]),
        ),
        (
            23,
            TeletextObjectType::Adaptive,
            program(vec![glyph(0, b'X')]),
        ),
    ]
    .into_iter()
    .map(
        |(id, object_type, program)| TeletextEnhancementObjectDefinition {
            identity: target(id),
            object_type,
            program,
        },
    )
    .collect::<Vec<_>>();
    if colliding {
        objects.push(TeletextEnhancementObjectDefinition {
            identity: target(24),
            object_type: TeletextObjectType::Passive,
            program: program(vec![glyph(0, b'R')]),
        });
    }
    objects
}
fn native_fixture(
    replacement: TeletextEnhancementProgram,
    colliding: bool,
) -> EncodedTeletextAuthoredService {
    let policy = policy();
    let language = TeletextLanguage::new("eng".into()).unwrap();
    let base = TeletextPageSetDraft {
        language: language.clone(),
        initial_page: address().page,
        pages: vec![TeletextPageDraft {
            address: address(),
            kind: TeletextPageKind::FullPage,
            language,
            national_option: TeletextNationalOption::new(0).unwrap(),
            rows: vec![],
            navigation: vec![],
        }],
    };
    let lifted = lift_teletext_v1_page_set(
        base,
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let mut pages = lifted
        .page_enhancements()
        .values()
        .cloned()
        .collect::<Vec<_>>();
    pages[0].compositional_links = Some(links());
    pages[0].resources.objects.public = Some(target(22).resource().unwrap().page());
    pages[0].resources.objects.global = Some(target(21).resource().unwrap().page());
    pages[0].local_program = Some(program(vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(1, 0).unwrap(),
        },
        TeletextEnhancementCommand::InvokeObject {
            target: target(21),
            object_type: TeletextObjectType::Active,
            offset: None,
        },
    ]));
    let objects = definitions(replacement, colliding);
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        policy.validation.service,
    )
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: address(),
            mot: None,
        }],
        vec![],
        policy.validation.declarations,
    )
    .unwrap();
    let transmissions = declared
        .resource_inventory()
        .iter()
        .map(|resource| TeletextServiceResourceTransmission {
            resource: *resource,
            counter: TeletextResourceUpdateCounter::new(7).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect::<Vec<_>>();
    encode_teletext_authored_service_before(declared, &transmissions, &policy.encoding, deadline())
        .unwrap()
}
fn receipt(service: &EncodedTeletextAuthoredService) -> TeletextExportProject {
    let hash = "a".repeat(64);
    let mut value = json!({"schema_version":1,"editor_profile":"qteletextmaker-1.0-carrier-v8",
        "original":{"design_id":1,"revision":1,"digest":hash,"expected_head":1,"document_version":1},
        "channel_id":1,"entry":{"page":123,"subcode":0},"language":"eng","v1_region":null,
        "producers":[{"ordinal":0,"addon_id":"org.example.context","contract":2,"binding_revision":1,
            "profile_id":1,"profile_revision":1,"installation":{"state_revision":"1","version":"1.0.0",
            "package_sha256":hash,"content_sha256":hash,"manifest_sha256":hash},"authoring":null}],
        "pages":[{"name":"display.tti","page":{"page":123,"subcode":0},"producer_ordinal":0,
            "editor_page":291,"editor_ordinal":0,"language":"eng","kind":"full_page","header_controls":0,
            "navigation":[],"materialized_magazine":null,"tti_bytes":1,"tti_sha256":hash}],"assets":[]});
    for page in service
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        let header = page.resource_header().unwrap();
        let identity = header.identity();
        let name = if identity.kind() == TeletextResourceKind::Gpop {
            "gpop.dvb".to_owned()
        } else {
            format!("pop-{:02x}.dvb", identity.page().page())
        };
        value["assets"].as_array_mut().unwrap().push(json!({"name":name,"scope":{"kind":"resource",
            "identity":{"kind":identity.kind(),"page":{"magazine":1,"page":identity.page().page()},
                "subpage":identity.logical_subpage().get()},"physical_subcode":header.physical_subcode()},
            "unit_count":page.data_units().len(),"bytes":page.data_units().len()*46,"sha256":hash}));
    }
    value["assets"].as_array_mut().unwrap().push(json!({"name":"context.tti","scope":{
        "kind":"object_context_tti_v1","resource_name":"gpop.dvb","object_id":21,"editor_page":256,"editor_ordinal":0},
        "unit_count":0,"bytes":1,"sha256":hash}));
    serde_json::from_value(value).unwrap()
}
fn files(
    service: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
    document: &TeletextTtiDocument,
) -> Vec<TeletextExportFile> {
    let mut result = vec![TeletextExportFile {
        name: "display.tti".into(),
        bytes: vec![0],
    }];
    for asset in &project.assets {
        let bytes = match &asset.scope {
            TeletextExportAssetScope::ObjectContextTtiV1 { .. } => {
                write_teletext_tti(document, limits().0).unwrap()
            }
            TeletextExportAssetScope::Resource { identity, .. } => service
                .pages()
                .iter()
                .find(|page| {
                    page.resource_header()
                        .is_some_and(|header| header.identity().page().page() == identity.page.page)
                })
                .unwrap()
                .data_units()
                .iter()
                .flatten()
                .copied()
                .collect(),
            _ => unreachable!(),
        };
        result.push(TeletextExportFile {
            name: asset.name.clone(),
            bytes,
        });
    }
    result
}

#[test]
fn context_root_offset_structure_and_original_unused_target_reconstruct_native_programs() {
    let original = native(root(0, 22, false));
    let project = receipt(&original);
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    assert_eq!(baseline.aliases, [target(21), target(22), target(23)]);
    let before = files(&original, &project, &baseline.document);
    for replacement in [root(1, 22, false), root(1, 22, true), root(0, 23, false)] {
        let candidate = native(replacement.clone());
        let frame = build_context_with_root(
            &original,
            target(21),
            &replacement,
            limits().1,
            limits().0,
            deadline(),
        )
        .unwrap();
        assert_eq!(frame.aliases, baseline.aliases);
        if replacement.commands().len() > 2 {
            assert_ne!(
                frame.encoded.definition_positions()[1].absolute_index(),
                baseline.encoded.definition_positions()[1].absolute_index()
            );
        }
        let after = files(&original, &project, &frame.document);
        let edits =
            reconcile_project(&project, &original, &before, &after, limits(), deadline()).unwrap();
        assert!(edits.operands.is_empty());
        assert_eq!(edits.programs.len(), 1);
        assert_eq!(edits.programs[0].identity(), target(21));
        assert_eq!(edits.programs[0].replacement(), &replacement);
        super::super::project::authorize_programs(
            &project,
            &original,
            &[],
            &edits.programs,
            limits(),
            deadline(),
        )
        .unwrap();
        validate_applied_project(
            &project,
            &original,
            &candidate,
            &after,
            limits(),
            deadline(),
        )
        .unwrap();
        let native_names = project
            .assets
            .iter()
            .filter(|asset| {
                matches!(
                    asset.scope,
                    TeletextExportAssetScope::Resource { .. }
                        | TeletextExportAssetScope::Magazine { .. }
                )
            })
            .map(|asset| asset.name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(
            before
                .iter()
                .filter(|file| native_names.contains(file.name.as_str()))
                .collect::<Vec<_>>(),
            after
                .iter()
                .filter(|file| native_names.contains(file.name.as_str()))
                .collect::<Vec<_>>()
        );
    }
}

fn mutate(document: &TeletextTtiDocument, index: usize, value: u32) -> Vec<u8> {
    let mut records = document.records().to_vec();
    let packet = records
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 26,
                designation,
                words,
            } if usize::from(*designation) == index / 13 => Some(words),
            _ => None,
        })
        .unwrap();
    packet[index % 13] = value;
    write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, limits().0).unwrap(),
        limits().0,
    )
    .unwrap()
}
#[test]
fn context_rejects_readonly_callees_main_unknown_alias_and_profile_relabeling() {
    let original = native(root(0, 22, false));
    let project = receipt(&original);
    let frame = build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let canonical = Page::index_identity(
        &frame.document,
        256,
        0,
        Location {
            page: 0,
            subcode: 0,
            resource: None,
        },
        deadline(),
    )
    .unwrap();
    let image = stream::Image::read(
        &canonical,
        Location {
            page: 0,
            subcode: 0,
            resource: None,
        },
        deadline(),
    )
    .unwrap();
    let before = files(&original, &project, &frame.document);
    let root_alias = frame.encoded.definition_positions()[0].target();
    let call = frame
        .encoded
        .command_positions()
        .iter()
        .find(|position| {
            position.source().object == Some(root_alias) && position.source().command == 1
        })
        .unwrap()
        .semantic_word();
    let changed = [
        (
            image.markers[1] + 1,
            image.words[image.markers[1] + 1] ^ (1 << 11),
        ),
        (0, image.words[0] ^ (1 << 11)),
        (call, image.words[call] ^ (1 << 11)),
        (image.markers[2], image.words[image.markers[2]] ^ (1 << 6)),
    ];
    for (index, value) in changed {
        let mut after = before.clone();
        after.last_mut().unwrap().bytes = mutate(&frame.document, index, value);
        assert!(
            reconcile_project(&project, &original, &before, &after, limits(), deadline()).is_err()
        );
    }
    let mut forged = project;
    "qteletextmaker-1.0-carrier-v6".clone_into(&mut forged.editor_profile);
    assert!(mapping(forged.assets.last().unwrap(), &forged, &original).is_err());
}

#[test]
fn context_reports_colliding_native_selector_alias_edit_instead_of_dropping_full_identity() {
    let original = native_fixture(root(0, 22, false), true);
    let project = receipt(&original);
    let baseline =
        build_context(&original, target(21), limits().1, limits().0, deadline()).unwrap();
    let edited = build_context_with_root(
        &original,
        target(21),
        &root(0, 24, false),
        limits().1,
        limits().0,
        deadline(),
    )
    .unwrap();
    assert_eq!(
        baseline.aliases,
        [target(21), target(22), target(23), target(24)]
    );
    assert_ne!(baseline.document, edited.document);
    let allocations = allocate_teletext_resource_objects(
        original.declared().semantic().objects(),
        &[],
        limits().2.max_objects,
    )
    .unwrap();
    assert_ne!(target(22), target(24));
    assert_eq!(
        allocations
            .lookup(target(22), TeletextObjectType::Passive)
            .unwrap()
            .object_number(),
        allocations
            .lookup(target(24), TeletextObjectType::Passive)
            .unwrap()
            .object_number()
    );
    let before = files(&original, &project, &baseline.document);
    let after = files(&original, &project, &edited.document);
    let failure = reconcile_project(&project, &original, &before, &after, limits(), deadline())
        .err()
        .unwrap();
    assert_eq!(failure.failure, Failure::Structure);
    assert!(
        validate_applied_project(&project, &original, &original, &after, limits(), deadline())
            .is_err()
    );
}
