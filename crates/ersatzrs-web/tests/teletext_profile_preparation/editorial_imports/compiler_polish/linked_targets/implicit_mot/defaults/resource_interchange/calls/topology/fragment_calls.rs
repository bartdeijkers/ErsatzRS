//! Complete native resource calls survive a nonfirst V7 editor fragment.
use super::super::super::fragments;
use super::*;
use ersatzrs_app::teletext_export_project::{TeletextExportAssetMapping, TeletextExportProject};
use ersatzrs_ffmpeg::teletext::project_encoded_teletext_object;

#[path = "fragment_calls/actual_editor.rs"]
mod actual_editor;

const GLYPHS: usize = 220;
const CALL: usize = GLYPHS + 3;
const TERMINATOR: u32 = 63 | (31 << 6) | (5 << 11);

#[derive(Clone, Copy, PartialEq, Eq)]
enum Calls {
    Original,
    Added,
    Removed,
}
impl Calls {
    fn offsets(self) -> &'static [u8] {
        match self {
            Self::Original => &[0],
            Self::Added => &[0, 1],
            Self::Removed => &[],
        }
    }
    fn occupied(self) -> [bool; 2] {
        match self {
            Self::Original => [true, false],
            Self::Added => [true, true],
            Self::Removed => [false, false],
        }
    }
}
fn shaped(theme: &str, page: u16, subcode: u16, state: Calls) -> Value {
    let mut value = super::response(theme, page, subcode);
    let object = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 20)
        .unwrap();
    let commands = object["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 5);
    let before = commands.clone();
    assert_eq!(
        before[2],
        json!({"kind":"at_column","column":8,"operation":{"kind":"g0","code":65}})
    );
    *commands = before[..2]
        .iter()
        .cloned()
        .chain((0..GLYPHS).map(|_| before[2].clone()))
        .chain(std::iter::once(before[3].clone()))
        .collect();
    for offset in state.offsets() {
        let mut call = before[4].clone();
        call["offset"]["column"] = json!(offset);
        commands.push(call);
    }
    assert_eq!(commands.len(), CALL + state.offsets().len());
    assert!(
        serde_json::to_vec(&value).unwrap().len() <= fragments::encoding().input.max_response_bytes
    );
    value
}
fn original(theme: &str, page: u16, subcode: u16) -> Value {
    shaped(theme, page, subcode, Calls::Original)
}
fn added(theme: &str, page: u16, subcode: u16) -> Value {
    shaped(theme, page, subcode, Calls::Added)
}
fn removed(theme: &str, page: u16, subcode: u16) -> Value {
    shaped(theme, page, subcode, Calls::Removed)
}
fn selected(project: &TeletextExportProject) -> &TeletextExportAssetMapping {
    assert_eq!(project.editor_profile, "qteletextmaker-1.0-carrier-v7");
    let resource = seed::target(20).resource().unwrap();
    let parent = project
        .assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope,
        TeletextExportAssetScope::Resource{identity,..}
        if identity.kind==TeletextResourceKindWire::Gpop
            && identity.page.magazine==resource.page().magazine()
            && identity.page.page==resource.page().page()
            && identity.subpage==resource.logical_subpage().get())
        })
        .unwrap();
    let members = project
        .assets
        .iter()
        .filter(|asset| {
            matches!(&asset.scope,
        TeletextExportAssetScope::ObjectTtiFragmentV1{resource_name,object_id:20,..}
            if resource_name==&parent.name)
        })
        .collect::<Vec<_>>();
    assert_eq!(members.len(), 2);
    members
        .into_iter()
        .find(|asset| {
            matches!(
                asset.scope,
                TeletextExportAssetScope::ObjectTtiFragmentV1 {
                    fragment_index: 1,
                    fragment_count: 2,
                    ..
                }
            )
        })
        .unwrap()
}
fn words(records: &[TeletextTtiRecord]) -> Vec<u32> {
    let mut result = Vec::new();
    let mut designation = 0;
    for record in records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation: actual,
            words,
        } = record
        {
            assert_eq!(usize::from(*actual), designation);
            designation += 1;
            result.extend_from_slice(words);
        }
    }
    assert!(result.len() <= 208 && !result.is_empty());
    assert_eq!(
        result[2],
        0x28 | (21 << 6) | (2 << 11),
        "genuine Active Local definition marker2"
    );
    let end = result.iter().position(|word| *word == TERMINATOR).unwrap();
    assert!(result[end..].iter().all(|word| *word == TERMINATOR));
    result.truncate(end + 1);
    result
}
struct Expected {
    state: Calls,
    fragment: Vec<TeletextTtiRecord>,
    program: TeletextEnhancementProgram,
    pixels: Vec<TeletextRaster>,
    units: Vec<Vec<[u8; 46]>>,
}
fn expected(flow: &Flow, state: Calls) -> Expected {
    let exported = &flow.exported[0];
    let asset = selected(&exported.sidecar().project);
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == asset.name)
        .unwrap();
    let fragment = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    assert_eq!(
        words(&fragment).len(),
        match state {
            Calls::Original => 25,
            Calls::Added => 27,
            Calls::Removed => 23,
        }
    );
    let service = flow.accepted[0].service();
    let program = program(service).clone();
    assert_eq!(
        &program.commands()[CALL..],
        state
            .offsets()
            .iter()
            .copied()
            .map(call)
            .collect::<Vec<_>>()
    );
    Expected {
        state,
        fragment,
        program,
        pixels: pixels(service, state.occupied()),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
    }
}
async fn qualify(state: Calls) -> Expected {
    let factory: fn(&str, u16, u16) -> Value = match state {
        Calls::Original => original,
        Calls::Added => added,
        Calls::Removed => removed,
    };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        fragments::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let target = expected(&flow, state);
    fixture.state.pool.close().await;
    target
}
fn selector(flow: &Flow) -> u32 {
    let service = flow.accepted[0].service();
    let page = service
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| Some(header.identity()) == seed::target(20).resource())
        })
        .unwrap();
    let EncodedTeletextAuthoredPage::Object(object) = page else {
        unreachable!()
    };
    let position = object
        .command_positions()
        .iter()
        .find(|position| {
            position.source().object == Some(seed::target(20)) && position.source().command == CALL
        })
        .unwrap();
    assert_eq!(program(service).commands()[CALL], call(0));
    assert_eq!(position.first_word() + 1, position.semantic_word());
    let projection = project_encoded_teletext_object(page, service.unit_count()).unwrap();
    let native_word = |index: usize| {
        projection
            .packets
            .iter()
            .find(|packet| usize::from(packet.logical_row) == 3 + index / 13)
            .unwrap()
            .words[index % 13]
    };
    assert_eq!(native_word(position.first_word()), 0x28 | (16 << 6));
    let selector = native_word(position.semantic_word());
    assert_eq!((selector >> 6) & 31, 19);
    assert_eq!(
        selector & 0x38,
        0x30,
        "native Public call, not an editor Local alias"
    );
    let asset = selected(&flow.exported[0].sidecar().project);
    let TeletextExportAssetScope::ObjectTtiFragmentV1 {
        command_start,
        command_end,
        native_word_start,
        native_word_end,
        ..
    } = asset.scope
    else {
        unreachable!()
    };
    assert!(
        command_start <= u64::try_from(CALL).unwrap() && u64::try_from(CALL).unwrap() < command_end
    );
    assert!(
        usize::from(native_word_start) <= position.first_word()
            && position.semantic_word() < usize::from(native_word_end)
    );
    let file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == asset.name)
        .unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let editor = words(document.records());
    let index = 3 + position.first_word() - usize::from(native_word_start);
    assert_eq!(&editor[index..index + 2], &[0x28 | (16 << 6), selector]);
    assert_eq!(
        index + 2,
        editor.len() - 1,
        "original call ends this nonfirst command fragment"
    );
    selector
}
fn rewrite(
    records: &[TeletextTtiRecord],
    selector: u32,
    from: Calls,
    to: Calls,
) -> Vec<TeletextTtiRecord> {
    let mut meaningful = words(records);
    let tail = meaningful.len() - 1 - 2 * from.offsets().len();
    for (index, offset) in from.offsets().iter().enumerate() {
        assert_eq!(
            &meaningful[tail + 2 * index..tail + 2 * index + 2],
            &[0x28 | (16 << 6) | (u32::from(*offset) << 11), selector]
        );
    }
    assert!(
        !meaningful[3..tail].contains(&selector),
        "unique original root call tail"
    );
    meaningful.truncate(tail);
    for offset in to.offsets() {
        meaningful.extend([0x28 | (16 << 6) | (u32::from(*offset) << 11), selector]);
    }
    meaningful.push(TERMINATOR);
    let padded = meaningful.len().div_ceil(13) * 13;
    assert!(padded <= 208);
    meaningful.resize(padded, TERMINATOR);
    let mut result = records
        .iter()
        .filter(|record| !matches!(record, TeletextTtiRecord::Packet { row: 26, .. }))
        .cloned()
        .collect::<Vec<_>>();
    result.extend(
        meaningful
            .chunks_exact(13)
            .enumerate()
            .map(|(designation, chunk)| TeletextTtiRecord::Packet {
                row: 26,
                designation: u8::try_from(designation).unwrap(),
                words: chunk.try_into().unwrap(),
            }),
    );
    result
}
fn request(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    from: &Expected,
    target: &Expected,
    selector: u32,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    let before = request.files.clone();
    let sidecar = request.sidecar.clone();
    let name = &selected(&exported.sidecar().project).name;
    let file = request
        .files
        .iter_mut()
        .find(|file| &file.name == name)
        .unwrap();
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    assert_eq!(document.records(), from.fragment.as_slice());
    let changed = rewrite(document.records(), selector, from.state, target.state);
    assert_eq!(
        changed, target.fragment,
        "only native call tail produces the independent target fragment"
    );
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(changed, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(request.sidecar, sidecar);
    for (original, uploaded) in before.iter().zip(&request.files) {
        assert_eq!(original.name, uploaded.name);
        if &original.name != name {
            assert_eq!(original.bytes, uploaded.bytes);
        }
    }
    request
}
async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Expected,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let root = flow.accepted[0].service();
    let changes = if target.state == Calls::Original {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
        vec![]
    } else {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 17);
        assert!(
            overlay.pages().is_empty()
                && overlay.local_program_operands().is_empty()
                && overlay.local_object_operands().is_empty()
                && overlay.local_object_glyph_program_overlays().is_empty()
                && overlay.resource_object_operands().is_empty()
                && overlay.drcs_bitmap_overlays().is_empty()
        );
        let [change] = overlay.resource_object_program_overlays() else {
            panic!("one original large root program")
        };
        assert_eq!(change.identity(), seed::target(20));
        assert_eq!(change.expected(), program(root));
        assert_eq!(change.replacement(), &target.program);
        let mut retained = (0..CALL).map(Some).collect::<Vec<_>>();
        if target.state == Calls::Added {
            retained.extend([Some(CALL), None]);
        }
        assert_eq!(change.retained_commands(), retained);
        assert!(change.has_invocation_structure_changes());
        vec![change.clone()]
    };
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let expected = if changes.is_empty() {
        root.declared().clone()
    } else {
        root.declared()
            .clone()
            .with_resource_object_program_overlays(
                &changes,
                fragments::encoding().validation.service,
            )
            .unwrap()
    };
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(pixels(service, target.state.occupied()), target.pixels);
    assert_eq!(service.magazine_units(), root.magazine_units());
    assert_eq!(service.inventory(), root.inventory());
    assert_units(service, target);
}
fn assert_units(service: &EncodedTeletextAuthoredService, target: &Expected) {
    assert_eq!(service.pages().len(), target.units.len());
    for (page, units) in service.pages().iter().zip(&target.units) {
        assert_eq!(page.data_units(), units.as_slice());
    }
}
async fn undo_repeat(
    mut fixture: Fixture,
    flow: &Flow,
    deleted: &SavedEditorialTeletextDesign,
    root: &Expected,
    added: &Expected,
    removed: &Expected,
    selector: u32,
) -> Fixture {
    let exported = drcs_bitmap::reexport(&fixture, deleted).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(
        &fixture,
        request(&exported, &latest, 3, removed, root, selector),
    )
    .await;
    assert_eq!(undone.head().revision, 4);
    Box::pin(assert_concept(&fixture, flow, &undone, root)).await;
    let exported = drcs_bitmap::reexport(&fixture, &undone).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(
        &fixture,
        request(&exported, &latest, 4, root, added, selector),
    )
    .await;
    assert_eq!(repeated.head().revision, 5);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, flow, &repeated, added)).await;
    fixture
}

#[tokio::test]
async fn v7_resource_call_topology_nonfirst_fragment_publish_reopen_delete_undo_and_repeat() {
    let added = Box::pin(qualify(Calls::Added)).await;
    let removed = Box::pin(qualify(Calls::Removed)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original,
        Fixture::new_persistent().await,
        fragments::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let root = expected(&flow, Calls::Original);
    let selector = selector(&flow);
    assert_ne!(root.pixels, added.pixels);
    assert_ne!(root.pixels, removed.pixels);
    let inserted = import(
        &fixture,
        request(
            &flow.exported[0],
            &flow.receipts[0],
            1,
            &root,
            &added,
            selector,
        ),
    )
    .await;
    assert_eq!(inserted.head().revision, 2);
    Box::pin(assert_concept(&fixture, &flow, &inserted, &added)).await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &inserted).await;
    assert_eq!(pixels(day.service(), Calls::Added.occupied()), added.pixels);
    assert_units(day.service(), &added);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &inserted, &added)).await;
    let exported = drcs_bitmap::reexport(&fixture, &inserted).await;
    let latest = receipt(&fixture, &exported).await;
    let deleted = import(
        &fixture,
        request(&exported, &latest, 2, &added, &removed, selector),
    )
    .await;
    assert_eq!(deleted.head().revision, 3);
    Box::pin(assert_concept(&fixture, &flow, &deleted, &removed)).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &deleted, &removed)).await;
    fixture = Box::pin(undo_repeat(
        fixture, &flow, &deleted, &root, &added, &removed, selector,
    ))
    .await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    generations::assert_same_output(&recover(&fixture, 1).await, &day);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = original(theme, page, subcode);
    let protected = value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|page| page["page"]["page"] == 125)
        .unwrap();
    protected["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "nested-call-zone")
        .unwrap()["kind"] = json!("generated");
    value
}
#[tokio::test]
async fn v7_resource_call_topology_shared_generated_consumer_refuses_add_and_delete_atomically() {
    let added = Box::pin(qualify(Calls::Added)).await;
    let removed = Box::pin(qualify(Calls::Removed)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated,
        Fixture::new_persistent().await,
        fragments::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let root = expected(&flow, Calls::Original);
    let selector = selector(&flow);
    for target in [&added, &removed] {
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            request(
                &flow.exported[0],
                &flow.receipts[0],
                1,
                &root,
                target,
                selector,
            ),
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Effects(error) = error else {
            panic!("nonfirst native call edit must reach shared-consumer effects: {error:?}");
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(125, 32)));
        let main_column = if target.state == Calls::Added { 12 } else { 11 };
        assert_eq!(
            error.position,
            Some((5, 4 + main_column)),
            "original left4 panel preserves Main coordinate"
        );
        assert_unchanged(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        assert!(fixture.playout.try_recv().is_err());
    }
    fixture.state.pool.close().await;
}
