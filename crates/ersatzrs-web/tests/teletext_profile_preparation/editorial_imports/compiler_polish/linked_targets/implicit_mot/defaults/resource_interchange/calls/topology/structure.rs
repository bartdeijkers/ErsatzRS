//! Resource call topology composes with real glyph/style shape, not raw words.
use super::*;
use ersatzrs_ffmpeg::teletext::{project_encoded_teletext_display, TeletextEncodedDisplayRecord};

pub(in super::super) fn original(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = mixed::original(theme, page, subcode);
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = page["regions"].as_array_mut().unwrap();
        let inherited = regions
            .iter()
            .position(|region| region["id"] == "free-glyph-10")
            .unwrap();
        assert_eq!(
            regions[inherited],
            json!({
                "id":"free-glyph-10","kind":"free","area":"main",
                "row":1,"column":10,"width":1,"height":1
            })
        );
        // drcs_bank::free_glyphs already permits this exact cell. Merge it
        // into the new fixture's adjacent Zone instead of overlapping it.
        regions.remove(inherited);
        let zone = regions
            .iter_mut()
            .find(|region| region["id"] == "active-glyph-zone")
            .unwrap();
        assert_eq!(zone["column"], 8);
        assert_eq!(zone["width"], 2);
        // A moved enlarged A can cover Main9..10. Extend only this new
        // fixture's permission before its original service and receipt exist.
        zone["width"] = json!(3);
    }
    value
}
fn shaped(theme: &str, page: u16, subcode: u16, insert: bool) -> Value {
    let mut value = original(theme, page, subcode);
    let object = value["service"]["objects"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|object| object["identity"]["object_id"] == 20)
        .unwrap();
    let commands = object["program"]["commands"].as_array_mut().unwrap();
    assert_eq!(commands.len(), 5);
    assert_eq!(
        commands[2],
        json!({"kind":"at_column","column":8,
        "operation":{"kind":"g0","code":65}})
    );
    let before = commands.clone();
    if insert {
        let mut moved = before[2].clone();
        moved["column"] = json!(9);
        let mut call = before[4].clone();
        call["offset"]["column"] = json!(1);
        // Attributes10 resets every field this new writer owns. Foreground
        // would survive that reset and legitimately affect protected cells.
        let mut inserted_style = before[1].clone();
        assert_eq!(inserted_style["operation"]["boxed"], false);
        inserted_style["operation"]["boxed"] = json!(true);
        *commands = vec![
            before[0].clone(),
            before[1].clone(),
            inserted_style,
            json!({"kind":"at_column","column":8,
                "operation":{"kind":"g0","code":66}}),
            moved,
            before[3].clone(),
            before[4].clone(),
            call,
        ];
    } else {
        // Delete the original enlarged style, glyph and call. The original
        // position and normal-size reset retain their exact order and values.
        *commands = vec![before[0].clone(), before[3].clone()];
    }
    value
}
fn added(theme: &str, page: u16, subcode: u16) -> Value {
    shaped(theme, page, subcode, true)
}
fn removed(theme: &str, page: u16, subcode: u16) -> Value {
    shaped(theme, page, subcode, false)
}

pub(in super::super) struct Expected {
    target: Target,
    units: Vec<Vec<[u8; 46]>>,
    retained: Vec<Option<usize>>,
    x26: Vec<u8>,
}
impl Expected {
    pub(in super::super) fn records(&self) -> &[TeletextTtiRecord] {
        &self.target.records
    }
    pub(in super::super) fn program(&self) -> &TeletextEnhancementProgram {
        &self.target.program
    }
    pub(in super::super) fn pixels(&self) -> &[TeletextRaster] {
        &self.target.pixels
    }
}
fn expected(flow: &Flow, occupied: [bool; 2], retained: Vec<Option<usize>>) -> Expected {
    let service = flow.accepted[0].service();
    Expected {
        target: Target {
            records: records(&flow.exported[0]),
            program: program(service).clone(),
            pixels: pixels(service, occupied),
            occupied,
        },
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
        retained,
        x26: x26_designations(service),
    }
}
fn x26_designations(service: &EncodedTeletextAuthoredService) -> Vec<u8> {
    let owner = channel_documents::address(124, 32);
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == owner =>
            {
                Some(page.page())
            }
            EncodedTeletextAuthoredPage::NativeDisplay(page)
                if page.encoded().page().address() == owner =>
            {
                Some(page.encoded().page())
            }
            _ => None,
        })
        .unwrap();
    project_encoded_teletext_display(page, service.unit_count())
        .unwrap()
        .records
        .into_iter()
        .filter_map(|record| match record {
            TeletextEncodedDisplayRecord::Triplets {
                row: 26,
                designation,
                ..
            } => Some(designation),
            _ => None,
        })
        .collect()
}
pub(in super::super) async fn qualify(insert: bool) -> Expected {
    let factory: fn(&str, u16, u16) -> Value = if insert { added } else { removed };
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        factory,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let target = expected(
        &flow,
        [insert, insert],
        if insert {
            vec![
                Some(0),
                Some(1),
                None,
                None,
                Some(2),
                Some(3),
                Some(4),
                None,
            ]
        } else {
            vec![Some(0), Some(3)]
        },
    );
    if insert {
        mixed::assert_b(&target.target.pixels);
        assert_eq!(target.target.program.commands()[6..], [call(0), call(1)]);
    }
    fixture.state.pool.close().await;
    target
}
pub(in super::super) async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    target: &Expected,
) {
    let stored = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let root = flow.accepted[0].service();
    let original = program(root);
    let changes = if &target.target.program == original {
        assert!(decoded.editorial_overlay().is_none());
        assert_eq!(stored.document, flow.originals[0].document);
        vec![]
    } else {
        let overlay = decoded.editorial_overlay().unwrap();
        assert_eq!(overlay.schema_version(), 17);
        assert!(overlay.pages().is_empty());
        assert!(overlay.local_program_operands().is_empty());
        assert!(overlay.local_object_operands().is_empty());
        assert!(overlay.local_object_glyph_program_overlays().is_empty());
        assert!(overlay.resource_object_operands().is_empty());
        assert!(overlay.drcs_bitmap_overlays().is_empty());
        let [change] = overlay.resource_object_program_overlays() else {
            panic!("combined structure must remain one exact resource program")
        };
        assert_eq!(change.identity(), seed::target(20));
        assert_eq!(change.expected(), original);
        assert_eq!(change.replacement(), &target.target.program);
        assert_eq!(change.retained_commands(), target.retained);
        assert!(change.has_invocation_structure_changes());
        vec![change.clone()]
    };
    let restored = drcs_bitmap::restored(fixture, concept).await;
    let before = flow.saved[0].candidate();
    assert_eq!(restored.selection(), before.selection());
    assert_eq!(restored.metadata(), before.metadata());
    assert_eq!(restored.transmissions(), before.transmissions());
    assert_eq!(restored.sources().len(), before.sources().len());
    for (actual, original) in restored.sources().iter().zip(before.sources()) {
        assert_eq!(actual.bytes(), original.bytes());
        assert_eq!(actual.authoring(), original.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        before.producers()[0].original()
    );
    let declared = if changes.is_empty() {
        root.declared().clone()
    } else {
        root.declared()
            .clone()
            .with_resource_object_program_overlays(&changes, policy().encoding.validation.service)
            .unwrap()
    };
    let service = restored.encoded().unwrap();
    assert_eq!(service.declared(), &declared);
    assert_eq!(service.magazine_units(), root.magazine_units());
    assert_eq!(service.inventory(), root.inventory());
    assert_eq!(
        pixels(service, target.target.occupied),
        target.target.pixels
    );
    assert_units(service, target);
}

pub(in super::super) fn assert_units(service: &EncodedTeletextAuthoredService, target: &Expected) {
    assert_eq!(service.pages().len(), target.units.len());
    for (page, expected) in service.pages().iter().zip(&target.units) {
        assert_eq!(
            page.data_units(),
            expected,
            "complete independent native page target"
        );
    }
}

#[tokio::test]
async fn v8_resource_glyph_style_shape_and_calls_publish_reopen_delete_and_root_undo() {
    let added = Box::pin(qualify(true)).await;
    let removed = Box::pin(qualify(false)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        original,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let root = expected(&flow, [true, false], vec![]);
    assert!(
        added.x26 != root.x26 || removed.x26 != root.x26,
        "independent target must exercise changed compiler-owned X26 packet presence"
    );
    eprintln!(
        "native display124/0020 X26 designations: root={:?} added={:?} removed={:?}",
        root.x26, added.x26, removed.x26
    );
    assert_ne!(root.target.pixels, added.target.pixels);
    assert_ne!(root.target.pixels, removed.target.pixels);
    let inserted = import(
        &fixture,
        upload(&flow.exported[0], &flow.receipts[0], 1, &added.target),
    )
    .await;
    assert_eq!(inserted.head().revision, 2);
    Box::pin(assert_concept(&fixture, &flow, &inserted, &added)).await;
    assert_original_pointers(&fixture, &flow).await;
    let day = publish_and_accept(&fixture, &flow, &inserted).await;
    assert_eq!(pixels(day.service(), [true, true]), added.target.pixels);
    assert_units(day.service(), &added);
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &inserted, &added)).await;
    let exported = drcs_bitmap::reexport(&fixture, &inserted).await;
    let latest = receipt(&fixture, &exported).await;
    let deleted = import(&fixture, upload(&exported, &latest, 2, &removed.target)).await;
    assert_eq!(deleted.head().revision, 3);
    Box::pin(assert_concept(&fixture, &flow, &deleted, &removed)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &deleted, &removed)).await;
    let exported = drcs_bitmap::reexport(&fixture, &deleted).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(&fixture, upload(&exported, &latest, 3, &root.target)).await;
    assert_eq!(undone.head().revision, 4);
    Box::pin(assert_concept(&fixture, &flow, &undone, &root)).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &undone, &root)).await;
    assert_pointers(&fixture, &day, &flow.accepted[1]).await;
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
    let zone = protected["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "active-glyph-zone")
        .unwrap();
    assert_eq!(zone["width"], 3);
    zone["kind"] = json!("generated");
    value
}
#[tokio::test]
async fn v8_resource_glyph_style_shape_and_calls_reject_generated_shared_consumer_atomically() {
    let added = Box::pin(qualify(true)).await;
    let removed = Box::pin(qualify(false)).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        generated,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for target in [&added.target, &removed.target] {
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            upload(&flow.exported[0], &flow.receipts[0], 1, target),
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Effects(error) = error else {
            panic!("combined shape must reach all-consumer permissions: {error:?}")
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(125, 32)));
        let (row, column) = error.position.unwrap();
        assert_eq!(row, 1);
        assert!((12..=14).contains(&column));
        assert_unchanged(&fixture, &flow).await;
        retained.assert_unchanged(&fixture, &flow).await;
    }
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
