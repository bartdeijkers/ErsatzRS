//! Canonical growth changes nn and following pointers, never receipt identity.
use super::*;
use ersatzrs_ffmpeg::teletext::EncodedTeletextObjectPage;

fn target(subpage: u8, object: u16) -> TeletextObjectIdentity {
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        identity(false).resource().unwrap().page(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap();
    TeletextObjectIdentity::public(resource, TeletextObjectId::new(object)).unwrap()
}

fn service(
    growth: bool,
    subpage: u8,
    counter: u8,
    update: TeletextResourceFullUpdate,
) -> EncodedTeletextAuthoredService {
    let policy = crate::host::teletext_runtime_policy::parse_native_teletext_runtime_policy(
        include_str!("../../../../../../tests/fixtures/teletext-output-policy-v1.json"),
    )
    .unwrap()
    .encoding;
    let base = compile(false, 2, b'A', TeletextObjectType::Passive);
    let mut commands = original_commands(2, b'A');
    if growth {
        commands.extend((0..20).map(|_| TeletextEnhancementCommand::AtColumn {
            column: 9,
            operation: glyph(b'B'),
        }));
    }
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: base.declared().semantic().base().clone(),
            pages: base
                .declared()
                .semantic()
                .page_enhancements()
                .values()
                .cloned()
                .collect(),
            magazines: vec![],
            objects: vec![
                TeletextEnhancementObjectDefinition {
                    identity: target(subpage, 21),
                    object_type: TeletextObjectType::Passive,
                    program: TeletextEnhancementProgram::new(commands).unwrap(),
                },
                TeletextEnhancementObjectDefinition {
                    identity: target(subpage, 33),
                    object_type: TeletextObjectType::Passive,
                    program: TeletextEnhancementProgram::new(vec![]).unwrap(),
                },
            ],
            drcs: vec![],
        },
        policy.validation.service,
    )
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![],
        base.declared().selections().to_vec(),
        vec![],
        policy.validation.declarations,
    )
    .unwrap();
    encode_teletext_authored_service_before(
        declared,
        &[TeletextServiceResourceTransmission {
            resource: target(subpage, 21).resource().unwrap(),
            counter: TeletextResourceUpdateCounter::new(counter).unwrap(),
            update,
            last_subpage: true,
        }],
        &policy.encoding,
        deadline(),
    )
    .unwrap()
}

fn object(service: &EncodedTeletextAuthoredService) -> &EncodedTeletextObjectPage {
    service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::Object(object) => Some(object),
            _ => None,
        })
        .unwrap()
}

fn receipt(service: &EncodedTeletextAuthoredService) -> TeletextExportProject {
    let mut project = project(service, false);
    let TeletextExportAssetScope::ObjectTtiV1 { object_id, .. } = &mut project.assets[1].scope
    else {
        unreachable!()
    };
    *object_id = 21;
    let mut following = project.assets[1].clone();
    following.name = "following.tti".into();
    let TeletextExportAssetScope::ObjectTtiV1 { object_id, .. } = &mut following.scope else {
        unreachable!()
    };
    *object_id = 33;
    project.assets.push(following);
    project
}

fn documents(
    service: &EncodedTeletextAuthoredService,
    project: &TeletextExportProject,
) -> Vec<TeletextExportFile> {
    let page = service
        .pages()
        .iter()
        .find(|page| page.resource_header().is_some())
        .unwrap();
    let mut files = vec![TeletextExportFile {
        name: project.pages[0].name.clone(),
        bytes: vec![0],
    }];
    for asset in &project.assets {
        let bytes = if let TeletextExportAssetScope::ObjectTtiV1 { object_id, .. } = asset.scope {
            let frame = build_frame(
                page,
                target(15, object_id),
                limits().1,
                limits().0,
                deadline(),
            )
            .unwrap();
            write_teletext_tti(&frame.document, limits().0).unwrap()
        } else {
            page.data_units().iter().flatten().copied().collect()
        };
        files.push(TeletextExportFile {
            name: asset.name.clone(),
            bytes,
        });
    }
    files
}

#[test]
fn canonical_extent_growth_keeps_original_receipt_and_empty_following_object_authority() {
    let original = service(false, 15, 7, TeletextResourceFullUpdate::Changed);
    let candidate = service(true, 15, 7, TeletextResourceFullUpdate::Changed);
    let project = receipt(&original);
    let unchanged_project = project.clone();
    let before = documents(&original, &project);
    let unchanged_before = before.clone();
    let mut after = before.clone();
    // Only the edited companion is uploaded; native resources and the empty
    // following definition's companion remain the exact original files.
    after[2].bytes = documents(&candidate, &project)[2].bytes.clone();
    let old = object(&original);
    let new = object(&candidate);
    assert_ne!(old.header().last_packet(), new.header().last_packet());
    assert_ne!(
        old.header().physical_subcode(),
        new.header().physical_subcode()
    );
    let pointer = |page: &EncodedTeletextObjectPage| {
        page.definition_positions()
            .iter()
            .find(|position| position.target() == target(15, 33))
            .unwrap()
            .absolute_index()
    };
    assert!(pointer(new) > pointer(old));
    assert!(mapping(&project.assets[1], &project, &original).is_ok());
    assert!(mapping(&project.assets[1], &project, &candidate).is_err());
    let edits =
        reconcile_project(&project, &original, &before, &after, bounds(), deadline()).unwrap();
    assert!(edits.operands.is_empty());
    assert_eq!(edits.programs.len(), 1);
    assert_eq!(edits.programs[0].identity(), target(15, 21));
    authorize_programs(
        &project,
        &original,
        &edits.operands,
        &edits.programs,
        bounds(),
        deadline(),
    )
    .unwrap();
    validate_applied_project(
        &project,
        &original,
        &candidate,
        &after,
        bounds(),
        deadline(),
    )
    .unwrap();
    assert_eq!(project, unchanged_project);
    assert_eq!(before, unchanged_before);
    assert_eq!(after[1], before[1]);
    assert_eq!(after[3], before[3]);
}

#[test]
fn canonical_extent_normalization_rejects_counter_subpage_and_transmission_drift() {
    let original = service(false, 15, 7, TeletextResourceFullUpdate::Changed);
    let candidate = service(true, 15, 7, TeletextResourceFullUpdate::Changed);
    let project = receipt(&original);
    let mut after = documents(&original, &project);
    after[2].bytes = documents(&candidate, &project)[2].bytes.clone();
    for (subpage, counter, update) in [
        (15, 6, TeletextResourceFullUpdate::Changed),
        (14, 7, TeletextResourceFullUpdate::Changed),
        (15, 7, TeletextResourceFullUpdate::Unchanged),
    ] {
        let invalid = service(true, subpage, counter, update);
        let error =
            validate_applied_project(&project, &original, &invalid, &after, bounds(), deadline())
                .unwrap_err();
        assert_eq!(error.failure, Failure::Mapping);
    }
    let mut wrong_count = project.clone();
    wrong_count.assets[0].unit_count += 1;
    assert!(mapping(&wrong_count.assets[1], &wrong_count, &original).is_err());
    let mut wrong_extent = project;
    let TeletextExportAssetScope::Resource {
        physical_subcode, ..
    } = &mut wrong_extent.assets[0].scope
    else {
        unreachable!()
    };
    *physical_subcode = object(&candidate).header().physical_subcode();
    assert!(mapping(&wrong_extent.assets[1], &wrong_extent, &original).is_err());
}
