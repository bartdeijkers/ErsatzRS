//! Synthetic TTI structure through native concepts; no actual QTM/client claim.
use super::*;
use ersatzrs_app::teletext_design_documents::decode_teletext_design_documents;
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, EncodedTeletextLocalObjectProgram,
};
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, TeletextEditorialEffectsFailure,
};
use serde_json::{json, Value};

#[path = "editorial_all_glyph_structure.rs"]
mod all_glyph_structure;

#[path = "editorial_local_glyph_structure/forward_anchor.rs"]
mod forward_anchor;

#[path = "editorial_local_glyph_structure/style_topology.rs"]
mod style_topology;

#[derive(Clone, Copy)]
enum Permission {
    Free,
    Zone,
    Narrow,
    Generated,
}
fn identity(id: u16) -> TeletextObjectIdentity {
    TeletextObjectIdentity::local(
        channel_documents::address(124, 32),
        TeletextObjectId::new(id),
    )
}
fn column(column: u8, operation: TeletextColumnOperation) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn { column, operation }
}
fn g0(column_index: u8, code: u8) -> TeletextEnhancementCommand {
    column(
        column_index,
        TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    )
}
fn g1(column_index: u8, bits: u8) -> TeletextEnhancementCommand {
    column(
        column_index,
        TeletextColumnOperation::G1 {
            mosaic: TeletextMosaic::new(bits).unwrap(),
        },
    )
}
fn commands(stage: u8) -> Vec<TeletextEnhancementCommand> {
    let mut result = vec![
        column(
            0,
            TeletextColumnOperation::Foreground {
                colour: TeletextColourReference::new(18).unwrap(),
            },
        ),
        column(
            0,
            TeletextColumnOperation::Background {
                colour: TeletextColourReference::new(0).unwrap(),
            },
        ),
        column(
            0,
            TeletextColumnOperation::DesignateCharset {
                designation: TeletextEnhancementCharsetDesignation::new(0).unwrap(),
            },
        ),
    ];
    result.extend(match stage {
        0 => vec![g0(0, b'A'), g0(0, b'A'), g1(1, 1)],
        1 => vec![
            g0(0, b'A'),
            g1(1, 2),
            g1(1, 3),
            g1(1, 4),
            g1(1, 5),
            g1(1, 1),
            g1(1, 6),
            g1(1, 7),
        ],
        2 => vec![g1(1, 3)],
        _ => unreachable!(),
    });
    result.push(column(
        2,
        TeletextColumnOperation::Attributes {
            double_height: false,
            double_width: false,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: true,
        },
    ));
    result.extend(match stage {
        0 => vec![g0(2, b'B'), g0(3, b'C')],
        1 => vec![g0(2, b'B'), g0(2, b'Z'), g0(2, b'C')],
        2 => vec![g0(2, b'D')],
        _ => unreachable!(),
    });
    result
}
fn mapping(stage: u8) -> Vec<Option<usize>> {
    match stage {
        1 => vec![
            Some(0),
            Some(1),
            Some(2),
            Some(4),
            None,
            None,
            None,
            None,
            Some(5),
            None,
            None,
            Some(6),
            Some(7),
            None,
            Some(8),
        ],
        // The exact bits3 survivor was inserted at stage1 command5, so its
        // composed root correspondence stays None rather than claiming bits1.
        2 => vec![Some(0), Some(1), Some(2), None, Some(6), Some(8)],
        _ => unreachable!(),
    }
}
fn program(stage: u8) -> TeletextEnhancementProgram {
    TeletextEnhancementProgram::new(commands(stage)).unwrap()
}
fn response(theme: &str, page: u16, subcode: u16, permission: Permission) -> Value {
    let mut value = drcs_bitmap::panels::Case::MainAndRight.response()(theme, page, subcode);
    let mut protected = Vec::new();
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        object["program"] = serde_json::to_value(program(0)).unwrap();
        let mut following = object.clone();
        following["identity"]["object_id"] = json!(65521);
        following["program"] = json!({"commands":[
            {"kind":"at_column","column":0,"operation":{"kind":"designate_charset","designation":0}},
            {"kind":"at_column","column":0,"operation":{"kind":"attributes","double_height":false,"double_width":false,"boxed":false,"conceal":false,"invert":true,"underline_or_separated_mosaics":false}},
            {"kind":"at_column","column":0,"operation":{"kind":"g0","code":80}}
        ]});
        protected.push(following);
    }
    value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .extend(protected);
    for metadata in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let owner = metadata["address"].clone();
        let source = metadata["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        let mut second = source.last().unwrap().clone();
        assert_eq!(second["offset"]["column"], 32);
        second["offset"]["column"] = json!(36);
        source.push(second);
        source.extend(json!([
            {"kind":"set_position","position":{"row":5,"column":8}},
            {"kind":"invoke_object","target":{"scope":"local","owner":owner,"object_id":65521},"object_type":"passive","offset":null}
        ]).as_array().unwrap().iter().cloned());
    }
    for metadata in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = metadata["regions"].as_array_mut().unwrap();
        let region = regions
            .iter_mut()
            .find(|r| r["id"] == "free-right-glyph")
            .unwrap();
        region["width"] = json!(
            if matches!(permission, Permission::Free | Permission::Zone) {
                8
            } else {
                4
            }
        );
        region["kind"] = json!(if matches!(permission, Permission::Zone) {
            "zone"
        } else {
            "free"
        });
        if matches!(permission, Permission::Generated) {
            regions.push(json!({"id":"generated-consumer","kind":"generated","area":"right_panel","row":1,"column":4,"width":4,"height":1}));
        }
    }
    value
}
fn free(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Permission::Free)
}
fn zone(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Permission::Zone)
}
fn narrow(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Permission::Narrow)
}
fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Permission::Generated)
}

fn encoded(service: &EncodedTeletextAuthoredService) -> &EncodedTeletextLocalObjectProgram {
    service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.normalized().encoded())
            }
            _ => None,
        })
        .unwrap()
}
fn definitions(service: &EncodedTeletextAuthoredService) -> [usize; 2] {
    [65520, 65521].map(|id| {
        usize::from(
            encoded(service)
                .definition_positions()
                .iter()
                .find(|p| p.target() == identity(id))
                .unwrap()
                .absolute_index(),
        )
    })
}
fn word(column: u32, mode: u32, data: u32) -> u32 {
    column | (mode << 6) | (data << 11)
}
fn number(index: usize) -> u32 {
    u32::try_from(((index / 13) << 4) | (index % 13)).unwrap()
}
fn marker(index: usize) -> u32 {
    let n = number(index);
    word(0x28 | (n >> 7), 23, n & 0x7f)
}
fn invocation(index: usize) -> u32 {
    let n = number(index);
    word(0x28 | (n >> 7), 19, n & 0x7f)
}
fn glyph_words(stage: u8) -> Vec<u32> {
    // Table28 literal encoding, independent of the compiler under test.
    commands(stage)
        .into_iter()
        .map(|command| match command {
            TeletextEnhancementCommand::AtColumn { column, operation } => match operation {
                TeletextColumnOperation::Foreground { colour } => {
                    word(u32::from(column), 0, u32::from(colour.get()))
                }
                TeletextColumnOperation::Background { colour } => {
                    word(u32::from(column), 3, u32::from(colour.get()))
                }
                TeletextColumnOperation::DesignateCharset { designation } => {
                    word(u32::from(column), 8, u32::from(designation.get()))
                }
                TeletextColumnOperation::Attributes {
                    underline_or_separated_mosaics: true,
                    ..
                } => word(u32::from(column), 12, 32),
                TeletextColumnOperation::G0 { code } => {
                    word(u32::from(column), 9, u32::from(code.get()))
                }
                TeletextColumnOperation::G1 { mosaic } => {
                    let bits = u32::from(mosaic.blocks());
                    word(u32::from(column), 1, 32 | (bits & 31) | ((bits & 32) << 1))
                }
                _ => unreachable!(),
            },
            _ => unreachable!(),
        })
        .collect()
}
fn packet_words(records: &[TeletextTtiRecord]) -> Vec<u32> {
    let mut flat = Vec::new();
    for record in records {
        if let TeletextTtiRecord::Packet {
            row: 26,
            designation,
            words,
        } = record
        {
            assert_eq!(usize::from(*designation), flat.len() / 13);
            flat.extend_from_slice(words);
        }
    }
    flat
}
fn page_name(request: &NativeTeletextEditorialImportRequest) -> String {
    let sidecar: Value = serde_json::from_slice(&request.sidecar).unwrap();
    sidecar["project"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["page"]["page"] == 124)
        .unwrap()["name"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn replace_packets(request: &mut NativeTeletextEditorialImportRequest, mut words: Vec<u32>) {
    assert!(words.len() <= 208);
    while !words.len().is_multiple_of(13) {
        words.push(word(63, 31, 5));
    }
    let name = page_name(request);
    let file = request.files.iter_mut().find(|f| f.name == name).unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let first = records
        .iter()
        .position(|r| matches!(r, TeletextTtiRecord::Packet { row: 26, .. }))
        .unwrap();
    records.retain(|r| !matches!(r, TeletextTtiRecord::Packet { row: 26, .. }));
    let packets = words
        .chunks_exact(13)
        .enumerate()
        .map(|(designation, chunk)| TeletextTtiRecord::Packet {
            row: 26,
            designation: u8::try_from(designation).unwrap(),
            words: chunk.try_into().unwrap(),
        });
    records.splice(first..first, packets);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
}
fn request_words(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    service: &EncodedTeletextAuthoredService,
    from: u8,
    replacement: Vec<u32>,
) -> NativeTeletextEditorialImportRequest {
    request_body_words(receipt, head, service, &glyph_words(from), replacement)
}
fn request_body_words(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    service: &EncodedTeletextAuthoredService,
    original_body: &[u32],
    replacement: Vec<u32>,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Synthetic Local glyph structure".clone_into(&mut request.name);
    let name = page_name(&request);
    let file = request.files.iter().find(|f| f.name == name).unwrap();
    let records = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut words = packet_words(records.records());
    let [target, following] = definitions(service);
    assert_eq!(words[target], marker(target));
    assert_eq!(words[following], marker(following));
    assert_eq!(&words[target + 1..following], original_body);
    let end = words.iter().position(|w| *w == word(63, 31, 5)).unwrap();
    words.truncate(end + 1);
    let moved = target + 1 + replacement.len();
    words.splice(target + 1..following, replacement);
    words[moved] = marker(moved);
    let original = invocation(following);
    let pointers: Vec<_> = words[..target]
        .iter()
        .enumerate()
        .filter_map(|(i, w)| (*w == original).then_some(i))
        .collect();
    assert_eq!(
        pointers.len(),
        1,
        "one actual following-object pointer must relocate"
    );
    words[pointers[0]] = invocation(moved);
    replace_packets(&mut request, words);
    request
}

fn request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    service: &EncodedTeletextAuthoredService,
    from: u8,
    to: u8,
) -> NativeTeletextEditorialImportRequest {
    request_words(receipt, head, service, from, glyph_words(to))
}

fn non_x26_records(service: &EncodedTeletextAuthoredService) -> Vec<TeletextEncodedDisplayRecord> {
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page.page())
            }
            _ => None,
        })
        .unwrap();
    project_encoded_teletext_display(page, service.unit_count())
        .unwrap()
        .records
        .into_iter()
        .filter(|record| {
            !matches!(
                record,
                TeletextEncodedDisplayRecord::Triplets { row: 26, .. }
            )
        })
        .collect()
}

fn assert_rendered(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    stage: u8,
) {
    assert_eq!(non_x26_records(after), non_x26_records(before));
    assert_eq!(after.magazine_units(), before.magazine_units());
    assert_eq!(after.inventory(), before.inventory());
    assert_eq!(after.pages().len(), before.pages().len());
    assert_eq!(definitions(after), [11, if stage == 1 { 27 } else { 18 }]);
    assert_eq!(encoded(before).packet_bodies().len(), 2);
    assert_eq!(
        encoded(after).packet_bodies().len(),
        if stage == 1 { 3 } else { 2 }
    );
    let protected = identity(65521);
    assert_eq!(
        after.declared().semantic().objects().definition(protected),
        before.declared().semantic().objects().definition(protected)
    );
    for (a, b) in before.pages().iter().zip(after.pages()) {
        assert_eq!(a.resource_header(), b.resource_header());
        let changed = matches!(a,EncodedTeletextAuthoredPage::CompiledDisplay(p) if p.page().address()==channel_documents::address(124,32));
        if !changed {
            assert_eq!(a.data_units(), b.data_units());
        }
    }
    let old = external_editor_probe::preview_page(before, 124);
    let new = external_editor_probe::preview_page(after, 124);
    assert_eq!((new.width(), new.height()), (56 * 12, 25 * 10));
    let mut consumers = [false; 2];
    for (position, (a, b)) in old.pixels().iter().zip(new.pixels()).enumerate() {
        if a == b {
            continue;
        }
        let (row, column) = (position / new.width(), position % new.width());
        assert!(
            (10..20).contains(&row) && (44 * 12..52 * 12).contains(&column),
            "all Main/protected definitions/panel surroundings stay exact"
        );
        consumers[(column / 12 - 44) / 4] = true;
    }
    assert_eq!(consumers, [true, true]);
}

async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    stage: u8,
) {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 11);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    let expected = TeletextLocalObjectGlyphProgramOverlay::new(
        identity(65520),
        program(0),
        program(stage),
        mapping(stage),
    )
    .unwrap();
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        std::slice::from_ref(&expected)
    );
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
    let mut body: Value = serde_json::from_slice(&revision.document).unwrap();
    body.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        body,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let current = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.transmissions(), original.transmissions());
    assert_eq!(current.sources().len(), original.sources().len());
    for (a, b) in current.sources().iter().zip(original.sources()) {
        assert_eq!(a.bytes(), b.bytes());
        assert_eq!(a.authoring(), b.authoring());
    }
    assert_eq!(
        current.producers()[0].original(),
        original.producers()[0].original()
    );
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_local_object_glyph_program_overlays(&[expected], policy().encoding.validation.service)
        .unwrap();
    assert_eq!(current.encoded().unwrap().declared(), &expected);
    assert_rendered(
        flow.accepted[0].service(),
        current.encoded().unwrap(),
        stage,
    );
}

async fn reject(
    fixture: &Fixture,
    flow: &Flow,
    request: NativeTeletextEditorialImportRequest,
) -> NativeTeletextEditorialImportError {
    let head = request.expected_design_head;
    let accepted = fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap();
    let pending = fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap();
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        head
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(
            flow.saved[0].head().id,
            head + 1,
            designs::budget().max_serialized_bytes
        )
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        accepted
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        pending
    );
    assert_retained_storage(fixture, flow).await;
    result
}

async fn malformed(fixture: &Fixture, flow: &Flow) {
    let source = flow.accepted[0].service();
    let mut deleted = glyph_words(1);
    deleted.remove(2); // Original charset anchor must not disappear.
    let mut resource = glyph_words(1);
    resource.splice(3..3, [word(40, 24, 80), word(0, 13, 112)]); // Normal slot48 is invalid even in an original bound bank.
    let mut call = glyph_words(1);
    call.insert(
        3,
        invocation(definitions(source)[0] + 1 + glyph_words(1).len() + 1),
    ); // Correct relocated pointer, but no inserted call anchor.
    for body in [deleted, resource, call] {
        assert!(matches!(
            reject(
                fixture,
                flow,
                request_words(&flow.receipts[0], 1, source, 0, body)
            )
            .await,
            NativeTeletextEditorialImportError::Tti(_)
        ));
    }
    let mut oversized = request(&flow.receipts[0], 1, source, 0, 1);
    let name = page_name(&oversized);
    let file = oversized.files.iter_mut().find(|f| f.name == name).unwrap();
    file.bytes
        .extend_from_slice(b"OL,26,P@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@@\r\n");
    assert!(matches!(
        reject(fixture, flow, oversized).await,
        NativeTeletextEditorialImportError::Tti(_)
    ));
}

async fn positive(factory: fn(&str, u16, u16) -> Value) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        factory,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert_eq!(definitions(flow.accepted[0].service()), [11, 21]);
    Box::pin(malformed(&fixture, &flow)).await;
    let concept = import(
        &fixture,
        request(&flow.receipts[0], 1, flow.accepted[0].service(), 0, 1),
    )
    .await;
    assert_eq!(concept.head().revision, 2);
    Box::pin(assert_concept(&fixture, &flow, &concept, 1)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &concept, 1)).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    let current = drcs_bitmap::restored(&fixture, &concept).await;
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(current.encoded().unwrap())
    );
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let latest = receipt(&fixture, &exported).await;
    let repeated = import(
        &fixture,
        request(&latest, 2, current.encoded().unwrap(), 1, 2),
    )
    .await;
    assert_eq!(repeated.head().revision, 3);
    Box::pin(assert_concept(&fixture, &flow, &repeated, 2)).await;
    let exported = drcs_bitmap::reexport(&fixture, &repeated).await;
    let latest = receipt(&fixture, &exported).await;
    let current = drcs_bitmap::restored(&fixture, &repeated).await;
    let undone = import(
        &fixture,
        request(&latest, 3, current.encoded().unwrap(), 2, 0),
    )
    .await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    assert_eq!(
        designs::units(
            drcs_bitmap::restored(&fixture, &undone)
                .await
                .encoded()
                .unwrap()
        ),
        designs::units(flow.accepted[0].service())
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn local_glyph_structure_native_free_zone_publish_reopen_packet_shift_repeat_undo() {
    for factory in [free as fn(&str, u16, u16) -> Value, zone] {
        Box::pin(positive(factory)).await;
    }
}

#[tokio::test]
async fn local_glyph_structure_native_protects_every_shared_consumer() {
    for factory in [narrow as fn(&str, u16, u16) -> Value, generated] {
        let (fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let NativeTeletextEditorialImportError::Effects(error) = reject(
            &fixture,
            &flow,
            request(&flow.receipts[0], 1, flow.accepted[0].service(), 0, 1),
        )
        .await
        else {
            panic!("protected second invocation requires effects refusal")
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert!(matches!(error.position, Some((1, 48..=51))));
        retained.assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}

// This opt-in boundary reuses the scalar trials' immutable file handoff. Only
// this new structural metadata and exact saved-packet oracle are case-specific.
mod actual_editor_trial {
    use super::super::local_object_glyphs::actual_editor as editor;
    use super::*;
    use std::path::{Path, PathBuf};

    pub(super) fn metadata(flow: &Flow) -> Value {
        let service = flow.accepted[0].service();
        let packed = encoded(service);
        let [target, following] = definitions(service);
        assert_eq!([target, following], [11, 21]);
        assert_eq!(packed.packet_bodies().len(), 2);
        let semantic = service.declared().semantic();
        let definition = semantic.objects().definition(identity(65520)).unwrap();
        assert_eq!(definition.object_type, TeletextObjectType::Passive);
        assert_eq!(definition.program, program(0));
        let source_words: Vec<_> = (0..commands(0).len())
            .map(|command| {
                let mut positions = packed.command_positions().iter().filter(|position| {
                    position.source().object == Some(identity(65520))
                        && position.source().command == command
                });
                let position = positions.next().unwrap();
                assert!(positions.next().is_none());
                assert_eq!(position.first_word(), position.semantic_word());
                assert!(position.original_local_command().is_none());
                assert_eq!(position.semantic_word(), target + 1 + command);
                position.semantic_word()
            })
            .collect();
        let calls: Vec<_> = semantic.page_enhancements()[&channel_documents::address(124, 32)]
            .local_program
            .as_ref()
            .unwrap()
            .commands()
            .iter()
            .filter_map(|command| match command {
                TeletextEnhancementCommand::InvokeObject {
                    target,
                    object_type,
                    offset,
                } if *target == identity(65520) => Some((*object_type, *offset)),
                _ => None,
            })
            .collect();
        assert_eq!(
            calls,
            [32, 36].map(|column| (
                TeletextObjectType::Passive,
                Some(TeletextObjectOffset::new(0, column).unwrap())
            ))
        );
        let tuples = |stage| {
            glyph_words(stage)
                .into_iter()
                .map(|word| [word & 63, (word >> 6) & 31, word >> 11])
                .collect::<Vec<_>>()
        };
        json!({"kind":"local_glyph_structure","identity":identity(65520),
            "body_first_word":target+1,"original_semantic_words":source_words,
            "original_commands":tuples(0),"replacement_commands":tuples(1),
            "expected_retained_commands":mapping(1),"invocation_offsets":[32,36],
            "consumer_visual_columns":[44,45,46,47,48,49,50,51],
            "definition_marker":{"absolute_index":target,"object_number":number(target),"object_type":"passive"},
            "following_definition":{"identity":identity(65521),"original_marker":following,
                "replacement_marker":27,"original_object_number":number(following),
                "replacement_object_number":number(27),"body":[[0,8,0],[0,12,16],[0,9,80]]},
            "deleted_original_command":3,"repositioned_original_command":8,
            "original_x26_packets":2,"replacement_x26_packets":3})
    }

    fn assert_saved(flow: &Flow, saved: &[u8]) {
        let expected = request(&flow.receipts[0], 1, flow.accepted[0].service(), 0, 1);
        let name = page_name(&expected);
        let expected = &expected
            .files
            .iter()
            .find(|file| file.name == name)
            .unwrap()
            .bytes;
        let packets = |bytes: &[u8]| {
            let mut packets: Vec<_> = parse_teletext_tti(bytes, policy().tti)
                .unwrap()
                .records()
                .iter()
                .filter_map(|record| match record {
                    TeletextTtiRecord::Packet {
                        row,
                        designation,
                        words,
                    } => Some((*row, *designation, *words)),
                    _ => None,
                })
                .collect();
            packets.sort_unstable();
            packets
        };
        assert_eq!(
            packets(saved),
            packets(expected),
            "genuine Save As must contain the exact structural body and relocated original markers/pointers"
        );
    }

    async fn run(directory: &Path) {
        let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
            free,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let ready =
            editor::inputs_for_edit(directory, "glyph-structure-free", &flow, || metadata(&flow));
        let marker = editor::completion(directory).await;
        let saved =
            live_client::read_bounded(&directory.join("edited.tti"), policy().tti.max_bytes)
                .unwrap();
        assert_saved(&flow, &saved);
        let mut request = editor::actual_request(directory, &flow, &saved);
        "Actual pinned QTM Local glyph structural concept".clone_into(&mut request.name);
        let concept = import(&fixture, request).await;
        assert_eq!(
            (concept.head().id, concept.head().revision),
            (flow.saved[0].head().id, 2)
        );
        Box::pin(assert_concept(&fixture, &flow, &concept, 1)).await;
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        fixture = fixture.reopen().await;
        Box::pin(assert_concept(&fixture, &flow, &concept, 1)).await;
        assert_original_pointers(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
        assert!(fixture.playout.try_recv().is_err());
        let revision = drcs_bitmap::stored(&fixture, &concept).await;
        editor::json_new(
            directory,
            "success.json",
            &json!({"probe_version":1,
            "case":"glyph-structure-free","editor":marker,
            "receipt_id":flow.receipts[0].metadata.id,"receipt_digest":flow.receipts[0].metadata.digest,
            "original_digest":flow.originals[0].digest,"design_id":concept.head().id,
            "revision":2,"digest":concept.digest(),"schema_version":11,
            "concept_document_sha256":drcs_bitmap::hash(&revision.document),
            "saved_file":"edited.tti","saved_sha256":drcs_bitmap::hash(&saved),
            "edit":ready["edit"],"native_preview_geometry":[672,250],
            "both_shared_consumers_changed":true,"exact_native_structural_intent":true,
            "following_protected_definition_and_non_x26_unchanged":true,
            "reopen_verified":true,"original_sources_package_profiles_config_night_unchanged":true,
            "publication":"unchanged","actual_widget_font_equivalence":"not asserted",
            "ordinary_stream_client_and_full_acceptance":"not measured by this test"}),
        );
        fixture.state.pool.close().await;
    }

    #[tokio::test]
    #[ignore = "requires fresh pinned headed QTM1.0 structural Save As via ERSATZRS_TELETEXT_GLYPH_STRUCTURE_EDITOR_DIR"]
    async fn actual_qtm_local_glyph_structure_save_as_v11_concept_preview_reopen() {
        let root = PathBuf::from(super::super::super::client_capture::selected(
            "ERSATZRS_TELETEXT_GLYPH_STRUCTURE_EDITOR_DIR",
        ));
        assert!(
            root.is_absolute(),
            "editor proof path must be an explicit absolute input"
        );
        std::fs::create_dir(&root).expect("actual editor proof root must be create-new");
        Box::pin(run(&root.join("glyph-structure-free"))).await;
    }
}

// Fixture-only EN300706 9.6.1 Fibonacci oracle: stages7/9/12/16,
// zero seed and MSB-first odd-parity bytes. No final encoded CRC is consulted.
fn expected_page_crc(records: &[TeletextEncodedDisplayRecord]) -> [u8; 2] {
    let mut header = None;
    let mut rows = [[b' '; 40]; 25];
    let mut present = 0_u32;
    for record in records {
        match record {
            TeletextEncodedDisplayRecord::HeaderText(cells) => {
                assert!(header.replace(*cells).is_none());
            }
            TeletextEncodedDisplayRecord::BaseRow { row, cells } => {
                assert!((1..=24).contains(row));
                let bit = 1_u32 << u32::from(*row);
                assert_eq!(present & bit, 0);
                present |= bit;
                rows[usize::from(*row)] = *cells;
            }
            _ => {}
        }
    }
    assert_eq!(
        present, 0x01ff_fffe,
        "this full-page fixture transmits all24 body rows"
    );
    let header = header.unwrap();
    let blank = [b' '; 40];
    let source = header[..24]
        .iter()
        .chain(rows[1..].iter().flatten())
        .chain(&blank);
    let mut register = 0_u16;
    let mut count = 0;
    for &cell in source {
        assert!(cell < 128);
        let byte = cell
            | if cell.count_ones().is_multiple_of(2) {
                128
            } else {
                0
            };
        for bit in (0..8).rev() {
            let feedback = ((register & 0x8940).count_ones() ^ u32::from((byte >> bit) & 1)) & 1;
            register = (register << 1) | u16::try_from(feedback).unwrap();
        }
        count += 1;
    }
    assert_eq!(count, 1024);
    register.to_be_bytes()
}

fn expected_combined_fallback(
    before: &EncodedTeletextAuthoredService,
) -> Vec<TeletextEncodedDisplayRecord> {
    let mut fallback = non_x26_records(before);
    let original_crc = expected_page_crc(&fallback);
    assert!(fallback.iter().any(|record| matches!(record,
        TeletextEncodedDisplayRecord::EditorialLinks { crc, .. } if *crc == original_crc)));
    let changed = fallback
        .iter_mut()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 2, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(changed[8], b' ');
    changed[8] = b'Q';
    let final_crc = expected_page_crc(&fallback);
    assert_ne!(
        final_crc, original_crc,
        "the permitted Base Q edit requires a fresh page CRC"
    );
    let crc = fallback
        .iter_mut()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::EditorialLinks { crc, .. } => Some(crc),
            _ => None,
        })
        .unwrap();
    *crc = final_crc;
    fallback
}

async fn assert_combined_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    base: &TeletextPhysicalBaseOverlay,
) -> TeletextRaster {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    let structural = TeletextLocalObjectGlyphProgramOverlay::new(
        identity(65520),
        program(0),
        program(1),
        mapping(1),
    )
    .unwrap();
    assert_eq!(overlay.schema_version(), 11);
    assert_eq!(overlay.pages(), std::slice::from_ref(base));
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        std::slice::from_ref(&structural)
    );
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
    let mut body: Value = serde_json::from_slice(&revision.document).unwrap();
    body.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        body,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (current, saved) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(current.bytes(), saved.bytes());
        assert_eq!(current.authoring(), saved.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let before = flow.accepted[0].service();
    let expected = before
        .declared()
        .clone()
        .with_physical_base_overlays(vec![base.clone()], policy().encoding.validation.service)
        .unwrap()
        .with_local_object_glyph_program_overlays(
            &[structural],
            policy().encoding.validation.service,
        )
        .unwrap();
    let after = restored.encoded().unwrap();
    assert_eq!(after.declared(), &expected);
    assert_eq!(definitions(after), [11, 27]);
    assert_eq!(non_x26_records(after), expected_combined_fallback(before));
    let old = external_editor_probe::preview_page(before, 124);
    let new = external_editor_probe::preview_page(after, 124);
    let mut effects = [false; 3];
    for (position, (left, right)) in old.pixels().iter().zip(new.pixels()).enumerate() {
        if left == right {
            continue;
        }
        let (row, column) = (position / new.width(), position % new.width());
        if (10..20).contains(&row) && (44 * 12..52 * 12).contains(&column) {
            effects[(column / 12 - 44) / 4] = true;
        } else {
            assert!(
                (20..30).contains(&row) && (12 * 12..13 * 12).contains(&column),
                "combined edit must preserve every Generated/unowned/protected cell"
            );
            effects[2] = true;
        }
    }
    assert_eq!(effects, [true; 3]);
    new
}

#[tokio::test]
async fn local_glyph_structure_native_combines_free_base_row_and_structural_concept() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        free,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let make = || {
        let mut edited = request(&flow.receipts[0], 1, flow.accepted[0].service(), 0, 1);
        let name = page_name(&edited);
        edit_cell(&mut edited.files, &name, 2, 8, b'Q');
        edited
    };
    // The original Free Main zone covers row2/column8. Generated row4 and
    // undeclared row6 remain protected even alongside a permitted structure edit.
    for row in [4, 6] {
        let mut invalid = make();
        let name = page_name(&invalid);
        edit_cell(&mut invalid.files, &name, row, 8, b'Q');
        let NativeTeletextEditorialImportError::Effects(error) =
            reject(&fixture, &flow, invalid).await
        else {
            panic!("combined import must retain original physical-byte permissions")
        };
        assert_eq!(error.reason, TeletextEditorialEffectsFailure::ProtectedByte);
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert_eq!(error.position, Some((row, 8)));
    }
    let edited = make();
    let name = page_name(&edited);
    let file = edited.files.iter().find(|file| file.name == name).unwrap();
    let parsed = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let cells = parsed
        .records()
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 2, cells } => Some(*cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(cells[8], b'Q');
    let base = TeletextPhysicalBaseOverlay::new(
        channel_documents::address(124, 32),
        vec![TeletextPhysicalBaseRow::new(2, cells).unwrap()],
    )
    .unwrap();
    let concept = import(&fixture, edited).await;
    assert_eq!(concept.head().revision, 2);
    let preview = Box::pin(assert_combined_concept(&fixture, &flow, &concept, &base)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_combined_concept(&fixture, &flow, &concept, &base)).await,
        preview
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn hidden_zero_program(stage: u8) -> TeletextEnhancementProgram {
    let mut body = commands(stage);
    body.insert(
        if stage == 0 { 4 } else { 3 },
        TeletextEnhancementCommand::SetPosition {
            position: TeletextEnhancementPosition::new(0, 0).unwrap(),
        },
    );
    TeletextEnhancementProgram::new(body).unwrap()
}
fn hidden_zero_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = free(theme, page, subcode);
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        if object["identity"]["object_id"] == 65520 && object["identity"]["owner"]["page"] == 124 {
            object["program"] = serde_json::to_value(hidden_zero_program(0)).unwrap();
        }
    }
    value
}
async fn assert_hidden_zero_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    expected: &TeletextLocalObjectGlyphProgramOverlay,
) -> TeletextRaster {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 11);
    assert_eq!(
        overlay.local_object_glyph_program_overlays(),
        std::slice::from_ref(expected)
    );
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.local_object_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
    let mut body: Value = serde_json::from_slice(&revision.document).unwrap();
    body.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        body,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (current, saved) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(current.bytes(), saved.bytes());
        assert_eq!(current.authoring(), saved.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    let before = flow.accepted[0].service();
    let declared = before
        .declared()
        .clone()
        .with_local_object_glyph_program_overlays(
            std::slice::from_ref(expected),
            policy().encoding.validation.service,
        )
        .unwrap();
    let after = restored.encoded().unwrap();
    assert_eq!(after.declared(), &declared);
    assert_rendered(before, after, 1);
    external_editor_probe::preview_page(after, 124)
}

#[tokio::test]
async fn local_glyph_structure_native_preserves_unemitted_zero_position_anchor() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        hidden_zero_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let before = flow.accepted[0].service();
    assert_eq!(
        before
            .declared()
            .semantic()
            .objects()
            .definition(identity(65520))
            .unwrap()
            .program,
        hidden_zero_program(0)
    );
    assert_eq!(hidden_zero_program(0).commands().len(), 10);
    assert_eq!(definitions(before), [11, 21]);
    let spans = encoded(before).command_positions();
    assert!(
        !spans
            .iter()
            .any(|position| position.source().object == Some(identity(65520))
                && position.source().command == 4),
        "the legal identity move has no physical word span"
    );
    for (command, word) in [(3, 15), (5, 16)] {
        let position = spans
            .iter()
            .find(|position| {
                position.source().object == Some(identity(65520))
                    && position.source().command == command
            })
            .unwrap();
        assert_eq!(position.first_word(), word);
        assert_eq!(position.semantic_word(), word);
    }
    // Keep the zero-word anchor at AP00 before the surviving second A. The
    // first A is deleted; exact right-biased correspondence retains old command5.
    let expected = TeletextLocalObjectGlyphProgramOverlay::new(
        identity(65520),
        hidden_zero_program(0),
        hidden_zero_program(1),
        vec![
            Some(0),
            Some(1),
            Some(2),
            Some(4),
            Some(5),
            None,
            None,
            None,
            None,
            Some(6),
            None,
            None,
            Some(7),
            Some(8),
            None,
            Some(9),
        ],
    )
    .unwrap();
    let edited = request(&flow.receipts[0], 1, before, 0, 1);
    let concept = import(&fixture, edited).await;
    assert_eq!(concept.head().revision, 2);
    let preview = Box::pin(assert_hidden_zero_concept(
        &fixture, &flow, &concept, &expected,
    ))
    .await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        Box::pin(assert_hidden_zero_concept(
            &fixture, &flow, &concept, &expected
        ))
        .await,
        preview
    );
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
