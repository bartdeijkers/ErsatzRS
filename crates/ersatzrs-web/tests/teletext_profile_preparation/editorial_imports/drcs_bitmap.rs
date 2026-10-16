//! Native DRCS resource imports; actual external-editor proof is separate.

use super::*;
use ersatzrs_app::{
    teletext_design_documents::decode_teletext_design_documents,
    teletext_export_project::{
        parse_teletext_export_receipt_body, write_teletext_export_receipt_body,
        TeletextExportAssetScope, TeletextExportReceiptBody, TeletextExportSidecar,
    },
};
use ersatzrs_core::repositories::{TeletextDesignExportSave, TeletextDesignRepository as _};
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::{project_encoded_teletext_drcs, EncodedTeletextAuthoredService};
use ersatzrs_infra::repositories::SqliteTeletextDesignRepository;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = drcs_companion::response(theme, page, subcode);
    // Both pages share slot zero. A resource edit is allowed only if both
    // rendered effects fall inside independently declared free regions.
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        page["regions"].as_array_mut().unwrap().push(json!({
            "id":"free-glyph", "kind":"free", "area":"main",
            "row":1, "column":8, "width":1, "height":1
        }));
    }
    value
}

fn protected_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    value["authoring"]["pages"][1]["regions"]
        .as_array_mut()
        .unwrap()
        .pop();
    value
}

fn unrelated_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    for field in ["pages", "page_enhancements", "mot_selections"] {
        let mut extra = value["service"][field][1].clone();
        let owner_field = if field == "mot_selections" {
            "display"
        } else {
            "address"
        };
        extra[owner_field] = json!({"page":126,"subcode":32});
        if field == "page_enhancements" {
            for command in extra["local_program"]["commands"].as_array_mut().unwrap() {
                if command["operation"]["kind"] == "drcs" {
                    command["operation"] = json!({"kind":"g0","code":65});
                }
            }
        }
        value["service"][field].as_array_mut().unwrap().push(extra);
    }
    let mut authoring = value["authoring"]["pages"][1].clone();
    authoring["page"] = json!({"page":126,"subcode":32});
    value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(authoring);
    let mut association = value["service"]["mots"][0]["associations"][1].clone();
    association["page"] = json!(0x26);
    value["service"]["mots"][0]["associations"]
        .as_array_mut()
        .unwrap()
        .push(association);
    value
}

pub(super) fn hash(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut result = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(result, "{byte:02x}").unwrap();
    }
    result
}

fn companion(body: &TeletextExportReceiptBody) -> (&str, &str) {
    body.project
        .assets
        .iter()
        .find_map(|asset| match &asset.scope {
            TeletextExportAssetScope::DrcsTti {
                resource_name,
                editor_ordinal: 0,
            } => Some((asset.name.as_str(), resource_name.as_str())),
            TeletextExportAssetScope::DrcsTtiSet { resource_names }
                if resource_names.len() == 1 =>
            {
                Some((asset.name.as_str(), resource_names[0].as_deref().unwrap()))
            }
            _ => None,
        })
        .unwrap()
}

pub(super) fn from_receipt(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
) -> NativeTeletextEditorialImportRequest {
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    NativeTeletextEditorialImportRequest {
        name: "Synthetic shared DRCS bitmap concept".to_owned(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: head,
        sidecar: write_teletext_export_sidecar(
            &TeletextExportSidecar {
                schema_version: 1,
                receipt_id: receipt.metadata.id,
                receipt_digest: receipt.metadata.digest.clone(),
                project: body.project,
            },
            policy().project,
        )
        .unwrap(),
        files: body.files,
    }
}

pub(super) fn bitmap_request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    low: u8,
) -> NativeTeletextEditorialImportRequest {
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    let mut request = from_receipt(receipt, head);
    let name = companion(&body).0;
    edit_cell(&mut request.files, name, 1, 1, low);
    // Simulate the observed Save As defaults without claiming a GUI run here.
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let parsed = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let mut records = parsed.records().to_vec();
    records.push(TeletextTtiRecord::Region(0));
    records.push(TeletextTtiRecord::Cycle {
        value: 20,
        unit: ersatzrs_app::teletext_tti::TeletextTtiCycleUnit::Time,
    });
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    let mut text = String::from_utf8(file.bytes.clone()).unwrap();
    for prefix in ["PN,", "PS,"] {
        let start = text.find(prefix).unwrap() + prefix.len();
        let end = text[start..].find('\n').unwrap() + start;
        text[start..end].make_ascii_lowercase();
    }
    file.bytes = text.into_bytes();
    request
}

pub(super) async fn stored(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
) -> TeletextDesignRevision {
    fixture
        .state
        .application
        .teletext_design_revision(
            concept.head().id,
            concept.head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

pub(super) async fn restored(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
) -> ersatzrs_web::host::teletext_designs::RestoredOriginalTeletextDesign {
    reload_original_teletext_design_revision(
        &fixture.state.application,
        concept.head().id,
        concept.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap()
}

pub(super) fn assert_only_glyph_pixels(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
) {
    assert_eq!(original.magazine_units(), edited.magazine_units());
    assert_eq!(original.inventory(), edited.inventory());
    for page in [124, 125] {
        let before = external_editor_probe::preview_page(original, page);
        let after = external_editor_probe::preview_page(edited, page);
        assert_eq!(
            (before.width(), before.height()),
            (after.width(), after.height())
        );
        let changed: Vec<_> = before
            .pixels()
            .iter()
            .zip(after.pixels())
            .enumerate()
            .filter_map(|(index, (a, b))| (a != b).then_some(index))
            .collect();
        // Four left-panel cells, main column eight, then bitmap x=10. The
        // 0x801 -> 0x803 edit adds precisely this pixel in glyph row zero.
        assert_eq!(changed, [(4 + 8) * 12 + 10 + before.width() * 10]);
        assert!(matches!(after.pixels()[changed[0]],
            TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) if rgb.get() == [15,0,0]));
    }
    for (a, b) in original.pages().iter().zip(edited.pages()) {
        assert_eq!(a.resource_header(), b.resource_header());
        if a.resource_header()
            .is_none_or(|header| header.identity().kind() != TeletextResourceKind::Drcs)
        {
            assert_eq!(a.data_units(), b.data_units());
        } else {
            assert_ne!(a.data_units(), b.data_units());
        }
    }
}

async fn assert_overlay(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    low: u16,
) {
    let revision = stored(fixture, concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 3);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert_eq!(overlay.drcs_bitmap_overlays().len(), 1);
    let bitmap = overlay.drcs_bitmap_overlays()[0];
    assert_eq!(bitmap.identity().page().page(), 0xc0);
    assert_eq!(bitmap.identity().logical_subpage().get(), 0);
    assert_eq!(bitmap.slot().get(), 0);
    assert_eq!(bitmap.expected().get()[0], 0x801);
    assert_eq!(bitmap.replacement().get()[0], 0x800 | low);
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
}

pub(super) async fn reexport(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
) -> ExportedOriginalTeletextDesign {
    let result = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: concept.head().id,
            revision: concept.head().revision,
            design_digest: concept.digest().to_owned(),
            expected_design_head: concept.head().revision,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = result else {
        panic!("concept re-export must succeed")
    };
    exported
}

// Fixture-only historical V3 singleton recipe bound to a genuinely re-exported
// current revision. Production exports remain V4 and their indexed resource
// editing acceptance stays open; this never relabels a V4 sidecar at import.
async fn retained_latest(
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
    exported: &ExportedOriginalTeletextDesign,
) -> TeletextDesignExportReceipt {
    let mut body = drcs_companion::v3_body(flow);
    body.project.original = exported.sidecar().project.original.clone();
    body.project.producers = exported.sidecar().project.producers.clone();
    body.project.pages = exported.sidecar().project.pages.clone();
    for asset in &mut body.project.assets {
        if !matches!(asset.scope, TeletextExportAssetScope::DrcsTti { .. }) {
            *asset = exported
                .sidecar()
                .project
                .assets
                .iter()
                .find(|fresh| fresh.name == asset.name)
                .unwrap()
                .clone();
        }
    }
    for file in &mut body.files {
        if let Some(fresh) = exported
            .files()
            .iter()
            .find(|fresh| fresh.name == file.name)
        {
            *file = fresh.clone();
        }
    }
    let current = restored(fixture, concept).await;
    let resource = current
        .encoded()
        .unwrap()
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity().kind() == TeletextResourceKind::Drcs)
        })
        .unwrap();
    let projected = project_encoded_teletext_drcs(resource, policy().project.max_units).unwrap();
    let name = companion(&body).0.to_owned();
    for record in projected.records {
        if let TeletextEncodedDisplayRecord::BaseRow { row, cells } = record {
            for (column, code) in cells.into_iter().enumerate() {
                edit_cell(&mut body.files, &name, row, column, code);
            }
        }
    }
    let bytes = &body
        .files
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    let asset = body
        .project
        .assets
        .iter_mut()
        .find(|asset| asset.name == name)
        .unwrap();
    asset.bytes = bytes.len();
    asset.sha256 = hash(bytes);
    let bytes = write_teletext_export_receipt_body(&body, policy().project).unwrap();
    let result = SqliteTeletextDesignRepository(fixture.state.pool.clone())
        .create_export(
            &TeletextDesignExportSave {
                design_id: concept.head().id,
                revision: concept.head().revision,
                design_digest: concept.digest().to_owned(),
                expected_design_head: concept.head().revision,
                receipt_version: 1,
                receipt: bytes.clone(),
            },
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap();
    let TeletextDesignMutation::Saved(metadata) = result else {
        panic!("historical fixture receipt must save")
    };
    fixture
        .state
        .application
        .teletext_design_export_receipt(
            metadata.id,
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

pub(super) fn read_retained_editor_file(path: &Path) -> Vec<u8> {
    use std::io::Read as _;
    let maximum = policy().tti.max_bytes;
    let metadata = std::fs::symlink_metadata(path).expect("retained editor file must exist");
    assert!(
        metadata.file_type().is_file(),
        "retained editor input must be a regular file"
    );
    assert!(
        metadata.len() <= u64::try_from(maximum).unwrap(),
        "retained editor input exceeds TTI budget"
    );
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .unwrap()
        .take(u64::try_from(maximum).unwrap() + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(
        bytes.len() <= maximum,
        "retained editor input grew beyond TTI budget"
    );
    bytes
}

pub(super) fn actual_editor_request(
    receipt: &TeletextDesignExportReceipt,
    directory: &Path,
) -> NativeTeletextEditorialImportRequest {
    let original = read_retained_editor_file(&directory.join("native-input.tti"));
    let edited = read_retained_editor_file(&directory.join("edited.tti"));
    assert_eq!(
        hash(&original),
        "208ee7a257e7300d7de8f8b413d943ce2c4b71a36c3d565f42895bec3d1ab0cf"
    );
    assert_eq!(
        hash(&edited),
        "739c3b04d6b4afd38b4524c0da9a3a1c1bc26d6079e140321f2eda3ad6ff9880"
    );
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    assert!(
        matches!(&body.project.assets.iter().find(|asset| asset.name == companion(&body).0).unwrap().scope,
        TeletextExportAssetScope::DrcsTtiSet {resource_names} if resource_names.len() == 1 && resource_names[0].is_some())
    );
    let mut request = from_receipt(receipt, 1);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == companion(&body).0)
        .unwrap();
    // The independently retained GUI input must be exactly today's freshly
    // exported resource. Only its genuine saved output replaces the companion.
    assert_eq!(file.bytes, original);
    file.bytes = edited;
    request
}

async fn shared_flow(legacy: bool, actual_directory: Option<&Path>) {
    Box::pin(shared_case(
        legacy,
        actual_directory,
        panels::Case::MainOnly,
    ))
    .await;
}

async fn shared_case(legacy: bool, actual_directory: Option<&Path>, case: panels::Case) {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        case.response(),
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let retained = case.originals_and_retained(&fixture, &flow).await;
    let receipt = if legacy {
        drcs_companion::seed_receipt(&fixture, &flow, drcs_companion::v3_body(&flow)).await
    } else {
        flow.receipts[0].clone()
    };
    let first_request = if let Some(directory) = actual_directory {
        assert!(!legacy, "actual retained input is bound to fresh V4 export");
        actual_editor_request(&receipt, directory)
    } else {
        bitmap_request(&receipt, 1, b'C')
    };
    let concept = import(&fixture, first_request).await;
    assert_eq!(concept.head().revision, 2);
    assert_overlay(&fixture, &flow, &concept, 3).await;
    let current = restored(&fixture, &concept).await;
    case.assert_pixels(flow.accepted[0].service(), current.encoded().unwrap());
    assert_original_pointers(&fixture, &flow).await;
    panels::assert_retained(retained.as_ref(), &fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_overlay(&fixture, &flow, &concept, 3).await;
    case.assert_pixels(
        flow.accepted[0].service(),
        restored(&fixture, &concept).await.encoded().unwrap(),
    );
    panels::assert_retained(retained.as_ref(), &fixture, &flow).await;
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    case.assert_pixels(flow.accepted[0].service(), accepted.service());
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    let exported = reexport(&fixture, &concept).await;
    let latest = if legacy {
        retained_latest(&fixture, &flow, &concept, &exported).await
    } else {
        super::receipt(&fixture, &exported).await
    };
    let replaced = import(&fixture, bitmap_request(&latest, 2, b'G')).await;
    assert_overlay(&fixture, &flow, &replaced, 7).await;
    let exported = reexport(&fixture, &replaced).await;
    let latest = if legacy {
        retained_latest(&fixture, &flow, &replaced, &exported).await
    } else {
        super::receipt(&fixture, &exported).await
    };
    let undone = import(&fixture, bitmap_request(&latest, 3, b'A')).await;
    assert_eq!(undone.head().revision, 4);
    assert_eq!(
        stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    assert_eq!(
        designs::units(restored(&fixture, &undone).await.encoded().unwrap()),
        designs::units(flow.accepted[0].service())
    );
    // Re-imports and undo remain concepts. Published output is still revision 2.
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    panels::assert_retained(retained.as_ref(), &fixture, &flow).await;
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_export_receipt(
                receipt.metadata.id,
                policy().project.max_bytes,
                designs::budget().max_serialized_bytes
            )
            .await
            .unwrap()
            .unwrap(),
        receipt
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn shared_drcs_bitmap_import_preview_explicit_publication_reopen_and_repeat_undo() {
    Box::pin(shared_flow(true, None)).await;
}

#[tokio::test]
async fn fresh_v4_singleton_bitmap_import_preview_publication_reopen_repeat_and_undo() {
    Box::pin(shared_flow(false, None)).await;
}

#[tokio::test]
#[ignore = "requires retained actual QTeletextMaker DRCS input and Save As output"]
async fn retained_actual_qtm_drcs_save_as_import_preview_publication_reopen_repeat_and_undo() {
    let directory = super::super::client_capture::selected("ERSATZRS_QTM_DRCS_CALIBRATION_DIR");
    let directory = std::path::PathBuf::from(directory);
    Box::pin(shared_flow(false, Some(&directory))).await;
}

#[tokio::test]
async fn bitmap_root_receipt_omits_unrelated_display_without_widening_import_permissions() {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        unrelated_response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let mut body = drcs_companion::v3_body(&flow);
    let name = body
        .project
        .pages
        .iter()
        .find(|page| page.page.page == 126)
        .unwrap()
        .name
        .clone();
    body.project.pages.retain(|page| page.page.page != 126);
    body.files.retain(|file| file.name != name);
    let receipt = drcs_companion::seed_receipt(&fixture, &flow, body).await;
    let concept = import(&fixture, bitmap_request(&receipt, 1, b'C')).await;
    let revision = stored(&fixture, &concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    // Both actual consumers were already covered. An unrelated page must not
    // cause a new broader root receipt and consume another export file slot.
    assert_eq!(
        decoded
            .editorial_overlay()
            .unwrap()
            .provenance()
            .export_receipt_id,
        receipt.metadata.id
    );
    let restored = restored(&fixture, &concept).await;
    assert_eq!(
        external_editor_probe::preview_page(restored.encoded().unwrap(), 126),
        external_editor_probe::preview_page(flow.accepted[0].service(), 126)
    );
    assert_only_glyph_pixels(flow.accepted[0].service(), restored.encoded().unwrap());
    assert_original_pointers(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

async fn rejected(
    fixture: &Fixture,
    flow: &Flow,
    request: NativeTeletextEditorialImportRequest,
) -> ersatzrs_web::host::teletext_editorial_designs::NativeTeletextEditorialImportError {
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
    assert_original_pointers(fixture, flow).await;
    error
}

#[tokio::test]
async fn drcs_bitmap_rejects_protected_and_unexported_shared_consumers() {
    for (factory, subset) in [
        (protected_response as fn(&str, u16, u16) -> Value, false),
        (response, true),
    ] {
        let (fixture, flow) = Box::pin(setup_with_fixture_policy(
            factory,
            Fixture::new_persistent().await,
            drcs_companion::encoding_policy,
        ))
        .await;
        let mut body = drcs_companion::v3_body(&flow);
        if subset {
            let name = body
                .project
                .pages
                .iter()
                .find(|page| page.page.page == 125)
                .unwrap()
                .name
                .clone();
            body.project.pages.retain(|page| page.page.page != 125);
            body.files.retain(|file| file.name != name);
        }
        let receipt = drcs_companion::seed_receipt(&fixture, &flow, body).await;
        rejected(&fixture, &flow, bitmap_request(&receipt, 1, b'C')).await;
        fixture.state.pool.close().await;
    }
}

fn assert_located_metadata_error(
    error: ersatzrs_web::host::teletext_editorial_designs::NativeTeletextEditorialImportError,
    tamper: u8,
) {
    if tamper <= 1 {
        let ersatzrs_web::host::teletext_editorial_designs::NativeTeletextEditorialImportError::Tti(
            error,
        ) = error
        else {
            panic!("resource metadata error must retain TTI coordinates")
        };
        assert_eq!((error.page, error.subcode), (0x1c0, 0));
        assert_eq!(
            error.resource,
            Some(
                TeletextResourceIdentity::new(
                    TeletextResourceKind::Drcs,
                    TeletextRawDataPageNumber::new(1, 0xc0).unwrap(),
                    TeletextResourceSubpageIndex::ZERO,
                )
                .unwrap()
            )
        );
        if tamper == 1 {
            assert_eq!(
                (error.row, error.column, error.designation),
                (Some(28), Some(0), Some(3))
            );
            assert_eq!(error.failure,
                    ersatzrs_web::host::teletext_editorial_designs::NativeTeletextEditorialTtiFailure::Packet);
        } else {
            assert_eq!(error.failure,
                    ersatzrs_web::host::teletext_editorial_designs::NativeTeletextEditorialTtiFailure::Status);
        }
    }
}

#[tokio::test]
async fn drcs_bitmap_rejects_changed_status_packet_undefined_slot_sidecar_and_raw_asset() {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        drcs_companion::encoding_policy,
    ))
    .await;
    let receipt =
        drcs_companion::seed_receipt(&fixture, &flow, drcs_companion::v3_body(&flow)).await;
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    let (name, raw) = companion(&body);
    for tamper in 0..5 {
        let mut request = bitmap_request(&receipt, 1, b'C');
        match tamper {
            0..=2 => {
                let file = request
                    .files
                    .iter_mut()
                    .find(|file| file.name == name)
                    .unwrap();
                let mut records = parse_teletext_tti(&file.bytes, policy().tti)
                    .unwrap()
                    .records()
                    .to_vec();
                match tamper {
                    0 => {
                        let status = records
                            .iter_mut()
                            .find_map(|record| match record {
                                TeletextTtiRecord::PageStatus(status) => Some(status),
                                _ => None,
                            })
                            .unwrap();
                        *status ^= 1;
                    }
                    1 => {
                        let words = records
                            .iter_mut()
                            .find_map(|record| match record {
                                TeletextTtiRecord::Packet {
                                    row: 28,
                                    designation: 3,
                                    words,
                                } => Some(words),
                                _ => None,
                            })
                            .unwrap();
                        words[0] ^= 1;
                    }
                    2 => {
                        let cells = records
                            .iter_mut()
                            .find_map(|record| match record {
                                TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
                                _ => None,
                            })
                            .unwrap();
                        cells[20] = b'A';
                    }
                    _ => unreachable!(),
                }
                file.bytes = write_teletext_tti(
                    &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
                    policy().tti,
                )
                .unwrap();
            }
            3 => {
                let mut sidecar = body.project.clone();
                sidecar
                    .assets
                    .iter_mut()
                    .find(|asset| asset.name == name)
                    .unwrap()
                    .sha256 = "0".repeat(64);
                request.sidecar = write_teletext_export_sidecar(
                    &TeletextExportSidecar {
                        schema_version: 1,
                        receipt_id: receipt.metadata.id,
                        receipt_digest: receipt.metadata.digest.clone(),
                        project: sidecar,
                    },
                    policy().project,
                )
                .unwrap();
            }
            _ => {
                request
                    .files
                    .iter_mut()
                    .find(|file| file.name == raw)
                    .unwrap()
                    .bytes[0] ^= 1;
            }
        }
        let error = rejected(&fixture, &flow, request).await;
        assert_located_metadata_error(error, tamper);
    }
    fixture.state.pool.close().await;
}

#[path = "drcs_bitmap/panels.rs"]
mod panels;
