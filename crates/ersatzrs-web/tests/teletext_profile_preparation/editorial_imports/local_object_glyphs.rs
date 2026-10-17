//! Synthetic native editor bytes, not actual QTM, ordinary delivery or client proof.
use super::*;
use ersatzrs_app::teletext_design_documents::decode_teletext_design_documents;
use ersatzrs_domain::{
    TeletextColumnOperation, TeletextGlyphCode, TeletextLocalObjectOperandOverlay, TeletextMosaic,
};
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, TeletextEditorialEffectsFailure,
};
use serde_json::{json, Value};

#[derive(Clone, Copy)]
enum Family {
    G0,
    G1,
}

impl Family {
    fn data(self, index: u8) -> u8 {
        match self {
            Self::G0 => b'A' + index,
            Self::G1 => 0x21 + index,
        }
    }
    fn word(self, index: u8) -> u32 {
        let mode = match self {
            Self::G0 => 9,
            Self::G1 => 1,
        };
        (mode << 6) | (u32::from(self.data(index)) << 11)
    }
    fn operation(self, index: u8) -> TeletextColumnOperation {
        match self {
            Self::G0 => TeletextColumnOperation::G0 {
                code: TeletextGlyphCode::new(self.data(index)).unwrap(),
            },
            Self::G1 => TeletextColumnOperation::G1 {
                mosaic: TeletextMosaic::new(1 + index).unwrap(),
            },
        }
    }
}

#[derive(Clone, Copy)]
enum Permission {
    Free,
    Zone,
    Narrow,
    Generated,
    Absent,
}

fn response(theme: &str, page: u16, subcode: u16, family: Family, permission: Permission) -> Value {
    let mut value = drcs_bitmap::panels::Case::MainAndRight.response()(theme, page, subcode);
    for object in value["service"]["objects"].as_array_mut().unwrap() {
        assert_eq!(object["identity"]["scope"], "local");
        assert_eq!(object["identity"]["object_id"], 65520);
        let commands = object["program"]["commands"].as_array_mut().unwrap();
        assert_eq!(commands.len(), 3);
        assert_eq!(commands[2]["operation"]["kind"], "drcs");
        commands[2]["operation"] = serde_json::to_value(family.operation(0)).unwrap();
    }
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let commands = page["local_program"]["commands"].as_array_mut().unwrap();
        let mut second = commands.last().unwrap().clone();
        assert_eq!(second["offset"]["column"], 32);
        second["offset"]["column"] = json!(33);
        commands.push(second);
    }
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        let regions = page["regions"].as_array_mut().unwrap();
        let panel = regions
            .iter_mut()
            .find(|region| region["id"] == "free-right-glyph")
            .unwrap();
        panel["width"] = json!(
            if matches!(permission, Permission::Free | Permission::Zone) {
                2
            } else {
                1
            }
        );
        panel["kind"] = json!(if matches!(permission, Permission::Zone) {
            "zone"
        } else {
            "free"
        });
        if matches!(permission, Permission::Generated) {
            regions.push(json!({"id":"generated-second","kind":"generated","area":"right_panel", "row":1,"column":1,"width":1,"height":1}));
        }
        if matches!(permission, Permission::Absent) {
            regions.retain(|region| region["id"] != "free-right-glyph");
        }
    }
    value
}

fn free(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Family::G0, Permission::Free)
}
fn zone(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Family::G1, Permission::Zone)
}
fn narrow(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Family::G0, Permission::Narrow)
}
fn generated(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Family::G0, Permission::Generated)
}
fn absent(theme: &str, page: u16, subcode: u16) -> Value {
    response(theme, page, subcode, Family::G0, Permission::Absent)
}

fn edited_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    before: u32,
    after: u32,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Synthetic retained local object glyph concept".clone_into(&mut request.name);
    let sidecar: Value = serde_json::from_slice(&request.sidecar).unwrap();
    let name = sidecar["project"]["pages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|page| page["page"]["page"] == 124)
        .unwrap()["name"]
        .as_str()
        .unwrap();
    let mut edits = 0;
    for file in &mut request.files {
        if file.name != name {
            continue;
        }
        let original = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
        let mut records = original.records().to_vec();
        for record in &mut records {
            if let TeletextTtiRecord::Packet { row: 26, words, .. } = record {
                for word in words {
                    if *word == before {
                        *word = after;
                        edits += 1;
                    }
                }
            }
        }
        file.bytes = write_teletext_tti(
            &TeletextTtiDocument::checked_from_records(records.clone(), policy().tti).unwrap(),
            policy().tti,
        )
        .unwrap();
        let mut reverted = records;
        for record in &mut reverted {
            if let TeletextTtiRecord::Packet { row: 26, words, .. } = record {
                for word in words {
                    if *word == after {
                        *word = before;
                    }
                }
            }
        }
        assert_eq!(
            reverted,
            original.records(),
            "all marker/address/source and invocation words stay original"
        );
    }
    assert_eq!(
        edits, 1,
        "two invocations share exactly one retained object body word"
    );
    request
}

async fn assert_overlay(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    family: Family,
    index: u8,
) -> TeletextLocalObjectOperandOverlay {
    let revision = drcs_bitmap::stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 4);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert!(overlay.drcs_bitmap_overlays().is_empty());
    assert_eq!(overlay.local_object_operands().len(), 1);
    let change = overlay.local_object_operands()[0];
    assert_eq!(change.owner(), channel_documents::address(124, 32));
    assert_eq!(change.identity().object_id().get(), 65520);
    assert_eq!((change.command(), change.column()), (2, 0));
    assert_eq!(change.expected(), family.operation(0));
    assert_eq!(change.replacement(), family.operation(index));
    assert_eq!(
        overlay.provenance().original_digest,
        flow.originals[0].digest
    );
    let mut value: Value = serde_json::from_slice(&revision.document).unwrap();
    value.as_object_mut().unwrap().remove("editorial_overlay");
    assert_eq!(
        value,
        serde_json::from_slice::<Value>(&flow.originals[0].document).unwrap()
    );
    change
}

async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    family: Family,
    index: u8,
) {
    let change = assert_overlay(fixture, flow, concept, family, index).await;
    let current = drcs_bitmap::restored(fixture, concept).await;
    let original = flow.saved[0].candidate();
    assert_eq!(current.selection(), original.selection());
    assert_eq!(current.metadata(), original.metadata());
    assert_eq!(current.transmissions(), original.transmissions());
    assert_eq!(current.sources().len(), original.sources().len());
    for (source, expected) in current.sources().iter().zip(original.sources()) {
        assert_eq!(source.bytes(), expected.bytes());
        assert_eq!(source.authoring(), expected.authoring());
    }
    assert_eq!(
        current.producers()[0].original(),
        original.producers()[0].original()
    );
    let after = current.encoded().unwrap();
    let before = flow.accepted[0].service();
    let expected = before
        .declared()
        .clone()
        .with_local_object_operand_overlays(&[change], policy().encoding.validation.service)
        .unwrap();
    assert_eq!(after.declared(), &expected);
    assert_rendered(before, after, change);
}

fn assert_rendered(
    before: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    after: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    change: TeletextLocalObjectOperandOverlay,
) {
    assert_eq!(after.pages().len(), before.pages().len());
    assert_eq!(after.magazine_units(), before.magazine_units());
    assert_eq!(after.inventory(), before.inventory());
    for (before, after) in before.pages().iter().zip(after.pages()) {
        let changed_owner = match before {
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => {
                page.page().address() == change.owner()
            }
            EncodedTeletextAuthoredPage::NativeDisplay(page) => {
                page.encoded().page().address() == change.owner()
            }
            _ => false,
        };
        assert_eq!(before.resource_header(), after.resource_header());
        if changed_owner {
            assert_ne!(before.data_units(), after.data_units());
        } else {
            assert_eq!(before.data_units(), after.data_units());
        }
    }
    let before = external_editor_probe::preview_page(before, 124);
    let after = external_editor_probe::preview_page(after, 124);
    assert_eq!((before.width(), before.height()), (56 * 12, 25 * 10));
    let mut changed = [false; 2];
    for (position, (a, b)) in before.pixels().iter().zip(after.pixels()).enumerate() {
        if a == b {
            continue;
        }
        let (row, column) = (position / before.width(), position % before.width());
        assert!(
            (10..20).contains(&row) && (44 * 12..46 * 12).contains(&column),
            "Main, other panel cells and protected rows must stay exact"
        );
        changed[column / 12 - 44] = true;
    }
    assert_eq!(
        changed,
        [true, true],
        "both shared consumers must change through the same validated preview"
    );
}

async fn reject(
    fixture: &Fixture,
    flow: &Flow,
    request: NativeTeletextEditorialImportRequest,
) -> NativeTeletextEditorialImportError {
    let error = import_teletext_editorial_design(
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
        1
    );
    assert!(fixture
        .state
        .application
        .teletext_design_revision(
            flow.saved[0].head().id,
            2,
            designs::budget().max_serialized_bytes
        )
        .await
        .unwrap()
        .is_none());
    assert_original_pointers(fixture, flow).await;
    error
}

async fn positive(factory: fn(&str, u16, u16) -> Value, family: Family) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        factory,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    // Literal address and family mutations are not same-command glyph edits.
    for replacement in [family.word(1) | 1, family.word(1) ^ (1 << 6)] {
        assert!(matches!(
            reject(
                &fixture,
                &flow,
                edited_request(&flow.receipts[0], 1, family.word(0), replacement)
            )
            .await,
            NativeTeletextEditorialImportError::Tti(_)
        ));
    }
    let concept = import(
        &fixture,
        edited_request(&flow.receipts[0], 1, family.word(0), family.word(1)),
    )
    .await;
    assert_eq!(concept.head().revision, 2);
    assert_concept(&fixture, &flow, &concept, family, 1).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_concept(&fixture, &flow, &concept, family, 1).await;
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
        edited_request(&latest, 2, family.word(1), family.word(2)),
    )
    .await;
    assert_eq!(repeated.head().revision, 3);
    assert_concept(&fixture, &flow, &repeated, family, 2).await;
    let exported = drcs_bitmap::reexport(&fixture, &repeated).await;
    let latest = receipt(&fixture, &exported).await;
    let undone = import(
        &fixture,
        edited_request(&latest, 3, family.word(2), family.word(0)),
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
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn retained_local_object_g0_free_and_g1_zone_import_preview_publish_repeat_undo() {
    for (factory, family) in [
        (free as fn(&str, u16, u16) -> Value, Family::G0),
        (zone, Family::G1),
    ] {
        Box::pin(positive(factory, family)).await;
    }
}

#[tokio::test]
async fn retained_local_object_all_panel_consumers_require_captured_edit_permission() {
    for (factory, position) in [
        (narrow as fn(&str, u16, u16) -> Value, (1, 45)),
        (generated, (1, 45)),
        (absent, (1, 44)),
    ] {
        let (fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
        ))
        .await;
        let retained = resource_bank_admin::Retained::capture(&fixture).await;
        let request = edited_request(&flow.receipts[0], 1, Family::G0.word(0), Family::G0.word(1));
        let NativeTeletextEditorialImportError::Effects(error) =
            reject(&fixture, &flow, request).await
        else {
            panic!("shared protected panel consumer must reject at effects qualification")
        };
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedDisplay
        );
        assert_eq!(error.page, Some(channel_documents::address(124, 32)));
        assert_eq!(error.position, Some(position));
        retained.assert_unchanged(&fixture, &flow).await;
        fixture.state.pool.close().await;
    }
}

#[path = "local_object_glyphs/actual_editor.rs"]
pub(super) mod actual_editor;

#[path = "local_object_glyphs/attributes.rs"]
mod attributes;

#[path = "local_object_glyphs/actual_attribute_editor.rs"]
mod actual_attribute_editor;

#[path = "local_object_glyphs/extended_glyphs.rs"]
mod extended_glyphs;

#[path = "local_object_glyphs/actual_extended_editor.rs"]
mod actual_extended_editor;

#[path = "local_object_glyphs/remaining_glyphs.rs"]
mod remaining_glyphs;

#[path = "local_object_glyphs/actual_remaining_editor.rs"]
mod actual_remaining_editor;

#[path = "local_object_glyphs/drcs_references.rs"]
mod drcs_references;

#[path = "local_object_glyphs/actual_drcs_reference_editor.rs"]
mod actual_drcs_reference_editor;

#[path = "local_object_glyphs/flash_charset.rs"]
mod flash_charset;

#[path = "local_object_glyphs/actual_flash_charset_editor.rs"]
mod actual_flash_charset_editor;

#[path = "local_object_glyphs/drcs_selections.rs"]
mod drcs_selections;

#[path = "local_object_glyphs/actual_drcs_selection_editor.rs"]
mod actual_drcs_selection_editor;
