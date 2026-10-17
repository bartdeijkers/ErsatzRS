//! Receipt mapping and final-pass scalar boundaries; no editor/client acceptance.
use super::*;
use ersatzrs_app::teletext_tti::{write_teletext_tti, TeletextTtiDocument, TeletextTtiRecord};
use ersatzrs_domain::*;
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, lift_teletext_v1_page_set, TeletextEncodingRegion,
    TeletextResourceFullUpdate, TeletextServiceResourceTransmission,
};
use serde_json::json;

fn deadline() -> Instant {
    Instant::now() + std::time::Duration::from_secs(30)
}
fn limits() -> (TeletextTtiBudget, usize) {
    (
        TeletextTtiBudget {
            max_bytes: 65536,
            max_records: 128,
        },
        1024,
    )
}
fn owner() -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(123).unwrap(),
        subcode: TeletextSubcode::new(16).unwrap(),
    }
}
fn identity(global: bool) -> TeletextObjectIdentity {
    let resource = TeletextResourceIdentity::new(
        if global {
            TeletextResourceKind::Gpop
        } else {
            TeletextResourceKind::Pop
        },
        TeletextRawDataPageNumber::new(8, 0xa0).unwrap(),
        TeletextResourceSubpageIndex::new(15).unwrap(),
    )
    .unwrap();
    if global {
        TeletextObjectIdentity::global(resource, TeletextObjectId::new(65535))
    } else {
        TeletextObjectIdentity::public(resource, TeletextObjectId::new(65535))
    }
    .unwrap()
}
fn foreground(colour: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::Foreground {
        colour: TeletextColourReference::new(colour).unwrap(),
    }
}
fn glyph(code: u8) -> TeletextColumnOperation {
    TeletextColumnOperation::G0 {
        code: TeletextGlyphCode::new(code).unwrap(),
    }
}
fn compile(
    global: bool,
    colour: u8,
    code: u8,
    kind: TeletextObjectType,
) -> EncodedTeletextAuthoredService {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    let root = value.as_object_mut().unwrap();
    root.remove("source");
    root.remove("recovery_timeout_ms");
    root.insert("operation_timeout_ms".into(), json!(1000));
    root.insert(
        "preview".into(),
        json!({"max_pages":16,"max_model_records":32_768,
        "max_pixels":168_000,"max_diagnostics":1400,"max_work":50_000_000}),
    );
    let policy = crate::host::teletext_runtime_policy::parse_native_teletext_authoring_policy(
        &value.to_string(),
    )
    .unwrap();
    let input=crate::host::teletext_runtime_policy::parse_native_teletext_authoring_input(&json!({"input_version":1,"channel_id":1,"entry_addon_id":"org.example.editor","entry":{"page":123,"subcode":16},"v1_region":"western_latin","resource_transmissions":[]}).to_string(),&policy).unwrap();
    let policy = policy.encoding_policy(&input).unwrap();
    let language = TeletextLanguage::new("eng".into()).unwrap();
    let lifted = lift_teletext_v1_page_set(
        TeletextPageSetDraft {
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
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let commands = vec![
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 0).unwrap(),
        },
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(4, 8).unwrap(),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: foreground(colour),
        },
        TeletextEnhancementCommand::AtColumn {
            column: 8,
            operation: glyph(code),
        },
    ];
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages: lifted.page_enhancements().values().cloned().collect(),
            magazines: vec![],
            objects: vec![TeletextEnhancementObjectDefinition {
                identity: identity(global),
                object_type: kind,
                program: TeletextEnhancementProgram::new(commands).unwrap(),
            }],
            drcs: vec![],
        },
        policy.validation.service,
    )
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![],
        vec![TeletextDisplayMotSelection {
            display: owner(),
            mot: None,
        }],
        vec![],
        policy.validation.declarations,
    )
    .unwrap();
    let transmissions = [TeletextServiceResourceTransmission {
        resource: identity(global).resource().unwrap(),
        counter: TeletextResourceUpdateCounter::new(7).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    }];
    encode_teletext_authored_service_before(declared, &transmissions, &policy.encoding, deadline())
        .unwrap()
}
fn project(service: &EncodedTeletextAuthoredService, global: bool) -> TeletextExportProject {
    let resource = identity(global).resource().unwrap();
    let page = service
        .pages()
        .iter()
        .find(|page| page.resource_header().is_some())
        .unwrap();
    let mut value:serde_json::Value=serde_json::from_str(r#"{"schema_version":1,"editor_profile":"qteletextmaker-1.0-carrier-v6","original":{"design_id":1,"revision":1,"digest":"","expected_head":1,"document_version":1},"channel_id":1,"entry":{"page":123,"subcode":16},"language":"eng","v1_region":null,"producers":[{"ordinal":0,"addon_id":"org.example.editor","contract":2,"binding_revision":1,"profile_id":1,"profile_revision":1,"installation":{"state_revision":"1","version":"1.0.0","package_sha256":"","content_sha256":"","manifest_sha256":""},"authoring":null}],"pages":[{"name":"display.tti","page":{"page":123,"subcode":16},"producer_ordinal":0,"editor_page":291,"editor_ordinal":0,"language":"eng","kind":"full_page","header_controls":0,"navigation":[],"materialized_magazine":null,"tti_bytes":1,"tti_sha256":""}],"assets":[]}"#).unwrap();
    value["original"]["digest"] = json!("a".repeat(64));
    value["pages"][0]["tti_sha256"] = json!("a".repeat(64));
    for key in ["package_sha256", "content_sha256", "manifest_sha256"] {
        value["producers"][0]["installation"][key] = json!("a".repeat(64));
    }
    let wire = ersatzrs_addon_contract::TeletextResourceIdentityWire {
        kind: if global {
            TeletextResourceKindWire::Gpop
        } else {
            TeletextResourceKindWire::Pop
        },
        page: ersatzrs_addon_contract::TeletextRawDataPageNumberWire {
            magazine: 8,
            page: 0xa0,
        },
        subpage: 15,
    };
    value["assets"] = json!([
        {"name":"resource.bin","scope":{"kind":"resource","identity":wire,"physical_subcode":page.resource_header().unwrap().physical_subcode()},"unit_count":page.data_units().len(),"bytes":page.data_units().len()*46,"sha256":"a".repeat(64)},
        {"name":"object.tti","scope":{"kind":"object_tti_v1","resource_name":"resource.bin","object_id":65535,"editor_page":256,"editor_ordinal":0},"unit_count":0,"bytes":1,"sha256":"a".repeat(64)}
    ]);
    assert_eq!(resource.logical_subpage().get(), 15);
    serde_json::from_value(value).unwrap()
}
fn canonical(
    service: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
) -> ObjectEditorFrame {
    frame(
        &mapping(&project.assets[1], project, service).unwrap(),
        limits(),
        deadline(),
    )
    .unwrap()
}
fn bytes(records: Vec<TeletextTtiRecord>) -> Vec<u8> {
    write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, limits().0).unwrap(),
        limits().0,
    )
    .unwrap()
}
fn words(frame: &ObjectEditorFrame) -> Vec<u32> {
    frame
        .document
        .records()
        .iter()
        .filter_map(|record| match record {
            TeletextTtiRecord::Packet { row: 26, words, .. } => Some(words.as_slice()),
            _ => None,
        })
        .flatten()
        .copied()
        .collect()
}
fn mutate(frame: &ObjectEditorFrame, word: usize, value: u32) -> Vec<u8> {
    let mut records = frame.document.records().to_vec();
    let packet=records.iter_mut().find(|record|matches!(record,TeletextTtiRecord::Packet {row:26,designation,..} if usize::from(*designation)==word/13)).unwrap();
    let TeletextTtiRecord::Packet { words, .. } = packet else {
        unreachable!()
    };
    words[word % 13] = value;
    bytes(records)
}
fn scalar_word(frame: &ObjectEditorFrame, command: usize) -> usize {
    frame
        .command_positions
        .iter()
        .find(|position| position.command == command)
        .unwrap()
        .editor_word
}
fn check_error(error: &NativeTeletextEditorialTtiError, global: bool) {
    assert_eq!(error.resource, identity(global).resource());
    assert_eq!((error.page, error.subcode), (0x8a0, 15));
}

#[test]
fn resource_tti_scalars_roundtrip_both_scopes_and_all_types_without_local_native_aliases() {
    for global in [false, true] {
        for kind in [
            TeletextObjectType::Active,
            TeletextObjectType::Adaptive,
            TeletextObjectType::Passive,
        ] {
            let before = compile(global, 2, b'A', kind);
            let after = compile(global, 4, b'B', kind);
            let project = project(&before, global);
            let baseline = canonical(&before, &project);
            let candidate = canonical(&after, &project);
            let baseline_bytes = write_teletext_tti(&baseline.document, limits().0).unwrap();
            let edited = write_teletext_tti(&candidate.document, limits().0).unwrap();
            let changes = reconcile(
                &project.assets[1],
                &project,
                &before,
                &baseline_bytes,
                &edited,
                limits(),
                deadline(),
            )
            .unwrap();
            assert_eq!(changes.len(), 2);
            assert!(changes
                .iter()
                .all(|change| change.identity() == identity(global)
                    && change.identity().owner().is_none()));
            assert_eq!((changes[0].command(), changes[1].command()), (2, 3));
            authorize(&project, &before, &changes, limits(), deadline()).unwrap();
            validate_applied(
                &project.assets[1],
                &project,
                &after,
                &edited,
                limits(),
                deadline(),
            )
            .unwrap();
            assert!(validate_applied(
                &project.assets[1],
                &project,
                &before,
                &edited,
                limits(),
                deadline()
            )
            .is_err());
        }
    }
}

#[test]
fn resource_tti_rejects_wrapper_marker_position_padding_and_forged_baseline_changes() {
    let service = compile(false, 2, b'A', TeletextObjectType::Passive);
    let project = project(&service, false);
    let frame = canonical(&service, &project);
    let baseline = write_teletext_tti(&frame.document, limits().0).unwrap();
    let words = words(&frame);
    // The passive wrapper has its literal definition marker at word2; the
    // first termination follows the final emitted semantic command.
    assert_eq!((words[2] >> 6) & 31, 23);
    let terminator = frame
        .command_positions
        .iter()
        .map(|position| position.editor_word + 1)
        .max()
        .unwrap();
    for word in [0, 1, 2, scalar_word(&frame, 1), terminator, words.len() - 1] {
        let edited = mutate(&frame, word, words[word] ^ 0x800);
        let error = reconcile(
            &project.assets[1],
            &project,
            &service,
            &baseline,
            &edited,
            limits(),
            deadline(),
        )
        .unwrap_err();
        assert_eq!(error.failure, Failure::Packet);
        assert_eq!(
            (error.row, error.designation, error.column),
            (Some(26), u8::try_from(word / 13).ok(), Some(word % 13))
        );
        check_error(&error, false);
    }
    let word = scalar_word(&frame, 3);
    let forged = mutate(
        &frame,
        word,
        (words[word] & 0x7ff) | (u32::from(b'B') << 11),
    );
    let error = reconcile(
        &project.assets[1],
        &project,
        &service,
        &forged,
        &forged,
        limits(),
        deadline(),
    )
    .unwrap_err();
    assert_eq!(error.failure, Failure::Packet);
    check_error(&error, false);
    let changed_column = mutate(&frame, word, words[word] ^ 1);
    assert!(reconcile(
        &project.assets[1],
        &project,
        &service,
        &baseline,
        &changed_column,
        limits(),
        deadline()
    )
    .is_err());
}

#[test]
fn resource_tti_protects_every_metadata_base_and_packet_presence_boundary() {
    let service = compile(true, 2, b'A', TeletextObjectType::Passive);
    let project = project(&service, true);
    let frame = canonical(&service, &project);
    let baseline = write_teletext_tti(&frame.document, limits().0).unwrap();
    for record in [
        TeletextTtiRecord::Description("changed".into()),
        TeletextTtiRecord::Cycle {
            value: 21,
            unit: ersatzrs_app::teletext_tti::TeletextTtiCycleUnit::Time,
        },
        TeletextTtiRecord::FastLinks([0; 6]),
        TeletextTtiRecord::BaseRow {
            row: 1,
            cells: [b' '; 40],
        },
        TeletextTtiRecord::BaseRow {
            row: 25,
            cells: [b' '; 40],
        },
        TeletextTtiRecord::Packet {
            row: 27,
            designation: 0,
            words: [0; 13],
        },
    ] {
        let mut records = frame.document.records().to_vec();
        if matches!(record, TeletextTtiRecord::Cycle { .. }) {
            records.retain(|record| !matches!(record, TeletextTtiRecord::Cycle { .. }));
        }
        if matches!(record, TeletextTtiRecord::Description(_)) {
            records.insert(0, record);
        } else {
            records.push(record);
        }
        let edited = bytes(records);
        check_error(
            &reconcile(
                &project.assets[1],
                &project,
                &service,
                &baseline,
                &edited,
                limits(),
                deadline(),
            )
            .unwrap_err(),
            true,
        );
    }
    for prefix in ["PN,", "SC,", "PS,", "RE,", "CT,", "OL,0,", "OL,26,"] {
        let edited = baseline
            .split_inclusive(|byte| *byte == b'\n')
            .filter(|line| !line.starts_with(prefix.as_bytes()))
            .flatten()
            .copied()
            .collect::<Vec<_>>();
        assert!(reconcile(
            &project.assets[1],
            &project,
            &service,
            &baseline,
            &edited,
            limits(),
            deadline()
        )
        .is_err());
    }
}

#[test]
fn resource_tti_authority_requires_v6_exact_resource_header_and_original_semantic_source() {
    let service = compile(false, 2, b'A', TeletextObjectType::Passive);
    let project = project(&service, false);
    let valid =
        TeletextResourceObjectOperandOverlay::new(identity(false), 3, 8, glyph(b'A'), glyph(b'B'))
            .unwrap();
    authorize(&project, &service, &[valid], limits(), deadline()).unwrap();
    for changes in [
        vec![valid, valid],
        vec![TeletextResourceObjectOperandOverlay::new(
            identity(false),
            0,
            8,
            glyph(b'A'),
            glyph(b'B'),
        )
        .unwrap()],
        vec![TeletextResourceObjectOperandOverlay::new(
            identity(false),
            3,
            8,
            glyph(b'C'),
            glyph(b'B'),
        )
        .unwrap()],
        vec![TeletextResourceObjectOperandOverlay::new(
            identity(true),
            3,
            8,
            glyph(b'A'),
            glyph(b'B'),
        )
        .unwrap()],
    ] {
        assert!(authorize(&project, &service, &changes, limits(), deadline()).is_err());
    }
    for variant in [
        "profile",
        "missing",
        "resource",
        "header",
        "object",
        "duplicate",
    ] {
        let mut changed = project.clone();
        match variant {
            "profile" => changed.editor_profile = "qteletextmaker-1.0-carrier-v4".into(),
            "missing" => {
                changed.assets.pop();
            }
            "resource" => {
                let TeletextExportAssetScope::ObjectTtiV1 { resource_name, .. } =
                    &mut changed.assets[1].scope
                else {
                    unreachable!()
                };
                *resource_name = "missing.bin".into();
            }
            "header" => {
                let TeletextExportAssetScope::Resource {
                    physical_subcode, ..
                } = &mut changed.assets[0].scope
                else {
                    unreachable!()
                };
                *physical_subcode ^= 0x10;
            }
            "object" => {
                let TeletextExportAssetScope::ObjectTtiV1 { object_id, .. } =
                    &mut changed.assets[1].scope
                else {
                    unreachable!()
                };
                *object_id = 65534;
            }
            _ => changed.assets.push(changed.assets[1].clone()),
        }
        assert!(authorize(&changed, &service, &[valid], limits(), deadline()).is_err());
    }
}

#[test]
fn resource_tti_identity_limits_and_original_deadline_remain_enforced() {
    let service = compile(false, 2, b'A', TeletextObjectType::Passive);
    let project = project(&service, false);
    let frame = canonical(&service, &project);
    let baseline = write_teletext_tti(&frame.document, limits().0).unwrap();
    for limits in [
        (
            TeletextTtiBudget {
                max_bytes: baseline.len() - 1,
                ..limits().0
            },
            limits().1,
        ),
        (
            TeletextTtiBudget {
                max_records: 1,
                ..limits().0
            },
            limits().1,
        ),
        (limits().0, 1),
    ] {
        assert!(reconcile(
            &project.assets[1],
            &project,
            &service,
            &baseline,
            &baseline,
            limits,
            deadline()
        )
        .is_err());
    }
    assert_eq!(
        reconcile(
            &project.assets[1],
            &project,
            &service,
            &baseline,
            &baseline,
            limits(),
            Instant::now()
        )
        .unwrap_err()
        .failure,
        Failure::TimedOut
    );
    for identity_record in [
        TeletextTtiRecord::PageNumber {
            page: 0x200,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(1),
    ] {
        let mut records = frame.document.records().to_vec();
        let index = usize::from(!matches!(
            identity_record,
            TeletextTtiRecord::PageNumber { .. }
        ));
        records[index] = identity_record;
        let changed = bytes(records);
        assert_eq!(
            reconcile(
                &project.assets[1],
                &project,
                &service,
                &baseline,
                &changed,
                limits(),
                deadline()
            )
            .unwrap_err()
            .failure,
            Failure::Identity
        );
    }
}
