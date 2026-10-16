//! No authored bank: receipt-bound Polish Base edits retain literal originals.
//! The opt-in genuine case requires a fresh no-bank export and retained QTM
//! Save As; native proof does not grant ordinary/independent/client acceptance.

use super::*;
use ersatzrs_app::teletext_export_project::TeletextExportAssetScope;
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
    TeletextEditorialEffectsFailure,
};
use serde_json::{json, Value};

const MASK: [u16; 10] = [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0];

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = polish_carrier::response(theme, page, subcode);
    value["service"]["drcs"] = json!([]);
    value["service"]["mots"] = json!([]);
    for selection in value["service"]["mot_selections"].as_array_mut().unwrap() {
        selection["mot"] = Value::Null;
    }
    for enhancement in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["resources"]["drcs"] = json!({"normal":null,"global":null});
        enhancement["compositional_links"] = Value::Null;
        for command in enhancement["local_program"]["commands"]
            .as_array_mut()
            .unwrap()
        {
            if command["operation"]["kind"] == "drcs" {
                command["operation"] = json!({"kind":"g0","code":65});
            }
        }
    }
    value
}

fn protected_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    let region = value["authoring"]["pages"][0]["regions"]
        .as_array_mut()
        .unwrap()
        .iter_mut()
        .find(|region| region["id"] == "free-national-base")
        .unwrap();
    region["width"] = json!(1);
    value
}

async fn setup_case(response: fn(&str, u16, u16) -> Value) -> (Fixture, Flow) {
    Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        channel_documents::policy,
    ))
    .await
}

pub(super) fn assert_authored(service: &EncodedTeletextAuthoredService) {
    assert!(service.declared().resource_inventory().is_empty());
    assert!(service.declared().semantic().drcs().subtables().is_empty());
    for page in service.declared().semantic().page_enhancements().values() {
        assert_eq!(page.resources.drcs, TeletextDrcsPageLinks::unlinked());
        assert!(page.compositional_links.is_none());
    }
}

pub(super) fn assert_concept(flow: &Flow, service: &EncodedTeletextAuthoredService, code: u8) {
    assert_authored(service);
    let original = flow.accepted[0].service().declared();
    let expected = original
        .clone()
        .with_physical_base_overlays(
            vec![expected_overlay(flow, code)],
            channel_documents::policy().validation.service,
        )
        .unwrap();
    assert_eq!(service.declared(), &expected);
    polish_carrier::assert_base(polish_carrier::composition(service), code);
    assert_eq!(
        service.compiler_drcs_resources().len(),
        usize::from(code == 0x5b)
    );
    assert_eq!(
        service
            .pages()
            .iter()
            .filter(|page| page.resource_header().is_some())
            .count(),
        service.compiler_drcs_resources().len()
    );
    assert_eq!(
        service.magazine_units(),
        flow.accepted[0].service().magazine_units()
    );
    assert_eq!(
        external_editor_probe::preview_page(service, 125),
        external_editor_probe::preview_page(flow.accepted[0].service(), 125)
    );
    if code == 0x5b {
        assert_generated(service);
    }
}

pub(super) fn expected_overlay(flow: &Flow, code: u8) -> TeletextPhysicalBaseOverlay {
    let original = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == "page-124-0020.tti")
        .unwrap();
    let document = parse_teletext_tti(&original.bytes, policy().tti).unwrap();
    let mut cells = *document
        .records()
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(cells[7], b'A');
    cells[7] = code;
    TeletextPhysicalBaseOverlay::new(
        channel_documents::address(124, 32),
        vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
    )
    .unwrap()
}

fn assert_generated(service: &EncodedTeletextAuthoredService) {
    let resource = &service.compiler_drcs_resources()[0];
    assert_eq!(resource.consumers(), [channel_documents::address(124, 32)]);
    assert_eq!(
        resource.table().identity().kind(),
        TeletextResourceKind::Drcs
    );
    assert_eq!(resource.table().identity().logical_subpage().get(), 0);
    assert_eq!(resource.table().glyphs().len(), 1);
    assert_eq!(resource.table().glyphs()[0].slot.get(), 0);
    assert_eq!(resource.table().glyphs()[0].bitmap.get(), MASK);
    let page = service
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == channel_documents::address(124, 32) =>
            {
                Some(page)
            }
            _ => None,
        })
        .unwrap();
    assert!(page
        .normalized()
        .physical()
        .replay()
        .drcs_execution()
        .records()
        .iter()
        .any(|record| record.resource() == resource.table().identity()
            && record.reference().table == TeletextDrcsTableScope::Normal
            && record.reference().subtable.get() == 0
            && record.reference().slot.get() == 0
            && record.bitmap().get() == MASK));
    let composition = page.normalized().original();
    let left = usize::from(composition.display_geometry().left_width());
    let raster = external_editor_probe::preview_page(service, 124);
    for (row, mask) in MASK.into_iter().enumerate() {
        for x in 0..24 {
            let rgb = if mask & (1 << (11 - x / 2)) == 0 {
                [0, 0, 0]
            } else {
                [15, 15, 15]
            };
            assert!(
                matches!(raster.pixel((left+7)*12+x,10+row),Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get() == rgb),
                "Polish pixel{x},{row}"
            );
        }
    }
}

pub(super) async fn assert_pointers(
    fixture: &Fixture,
    day: &AcceptedTeletextOutputCandidate,
    night: &AcceptedTeletextOutputCandidate,
) {
    for (channel, expected) in [(1, day), (2, night)] {
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(expected.generation().request.clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(expected.generation().clone())
        );
        generations::assert_same_output(&recover(fixture, channel).await, expected);
    }
}

fn rewrite(
    request: &mut NativeTeletextEditorialImportRequest,
    name: &str,
    modify: impl FnOnce(&mut [TeletextTtiRecord]),
) {
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    modify(&mut records);
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
}

async fn reject_generated_edits(
    fixture: &Fixture,
    flow: &Flow,
    receipt: &TeletextDesignExportReceipt,
    concept: &SavedEditorialTeletextDesign,
    day: &AcceptedTeletextOutputCandidate,
) {
    let baseline = drcs_bitmap::from_receipt(receipt, 2);
    let name = baseline
        .files
        .iter()
        .find(|file| file.name.starts_with("drcs-set-"))
        .unwrap()
        .name
        .clone();
    for bitmap in [true, false] {
        let mut request = drcs_bitmap::from_receipt(receipt, 2);
        edit_cell(&mut request.files, "page-124-0020.tti", 1, 7, b'A');
        if bitmap {
            edit_cell(&mut request.files, &name, 1, 1, b'A');
        } else {
            rewrite(&mut request, "page-124-0020.tti", |records| {
                let words = records
                    .iter_mut()
                    .find_map(|record| match record {
                        TeletextTtiRecord::Packet {
                            row: 27,
                            designation: 4,
                            words,
                        } => Some(words),
                        _ => None,
                    })
                    .unwrap();
                words[4] ^= 1 << 6;
            });
        }
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        let NativeTeletextEditorialImportError::Tti(error) = error else {
            panic!("generated asset edit must fail native reconciliation: {error:?}");
        };
        assert_eq!(
            error.failure,
            if bitmap {
                NativeTeletextEditorialTtiFailure::Structure
            } else {
                NativeTeletextEditorialTtiFailure::Packet
            }
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(concept.head().id)
                .await
                .unwrap()
                .unwrap(),
            *concept.head()
        );
        assert_pointers(fixture, day, &flow.accepted[1]).await;
        assert_retained_storage(fixture, flow).await;
    }
}

fn revert_request(receipt: &TeletextDesignExportReceipt) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, 2);
    let before = request.files.clone();
    edit_cell(&mut request.files, "page-124-0020.tti", 1, 7, b'A');
    for (original, edited) in before.iter().zip(&request.files) {
        if original.name != "page-124-0020.tti" {
            assert_eq!(original, edited);
        }
    }
    request
}

#[tokio::test]
async fn no_bank_polish_concept_previews_publishes_reopens_and_reverts_with_read_only_companion() {
    let (mut fixture, flow) = setup_case(response).await;
    polish_carrier::write_editor_inputs(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    for original in &flow.accepted {
        assert_authored(original.service());
        assert!(original.service().compiler_drcs_resources().is_empty());
    }
    let concept = import(&fixture, request(&flow, 1, 7, 0x5b)).await;
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_concept(&flow, restored.encoded().unwrap(), 0x5b);
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_concept(&flow, accepted.service(), 0x5b);
    assert_eq!(
        external_editor_probe::preview_page(accepted.service(), 124),
        preview
    );
    fixture = fixture.reopen().await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    assert_eq!(fresh.metadata.revision, 2);
    assert_eq!(fresh.metadata.design_digest, concept.digest());
    assert!(exported.sidecar().project.assets.iter().any(|asset| matches!(&asset.scope,TeletextExportAssetScope::DrcsTtiSet {resource_names} if resource_names.len()==1)));
    reject_generated_edits(&fixture, &flow, &fresh, &concept, &accepted).await;
    let reverted = import(&fixture, revert_request(&fresh)).await;
    assert_eq!(reverted.head().id, concept.head().id);
    assert_eq!(reverted.head().revision, 3);
    assert_concept(
        &flow,
        drcs_bitmap::restored(&fixture, &reverted)
            .await
            .encoded()
            .unwrap(),
        b'A',
    );
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    assert_eq!(
        qtm_carrier::stored_receipt(&fixture, fresh.metadata.id).await,
        fresh
    );
    retained.assert_unchanged(&fixture, &flow).await;
    fixture = fixture.reopen().await;
    assert_concept(
        &flow,
        drcs_bitmap::restored(&fixture, &reverted)
            .await
            .encoded()
            .unwrap(),
        b'A',
    );
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn no_bank_polish_rejects_a_changed_enlarged_fragment_outside_original_free_region() {
    let (mut fixture, flow) = setup_case(protected_response).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request(&flow, 1, 7, 0x5b),
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Effects(error) = error else {
        panic!("protected enlarged fragment must reject no-bank import: {error:?}");
    };
    assert_eq!(
        error.reason,
        TeletextEditorialEffectsFailure::ProtectedDisplay
    );
    assert_eq!(error.page, Some(channel_documents::address(124, 32)));
    let column = polish_carrier::composition(flow.accepted[0].service())
        .display_geometry()
        .left_width()
        + 8;
    assert_eq!(error.position, Some((1, column)));
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
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
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn assert_actual_tti_edit(original: &[u8], edited: &[u8]) {
    let mut expected = parse_teletext_tti(original, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let cells = expected
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    assert_eq!(cells[7], b'A');
    cells[7] = 0x5b;
    let parsed = parse_teletext_tti(edited, policy().tti).unwrap();
    assert_eq!(parsed.records().len(), expected.len());
    // Numeric spelling and record order belong to the editor. Every parsed
    // record, including all other OL bytes and protected packets, stays exact.
    for record in parsed.records() {
        let index = expected
            .iter()
            .position(|candidate| candidate == record)
            .expect("genuine Save As may change only Base row1 column7 A to literal5B");
        expected.remove(index);
    }
    assert!(expected.is_empty());
}

fn actual_editor_proof(directory: &Path, original: &[u8], edited: &[u8]) -> Value {
    let bytes = live_client::read_bounded(&directory.join("editor-1/editor-proof.json"), 16 * 1024)
        .unwrap();
    let proof: Value = serde_json::from_slice(&bytes).unwrap();
    for (field, expected) in [
        ("editor_name", "QTeletextMaker"),
        ("editor_version", "1.0"),
        (
            "editor_source_commit",
            "cac2167bac933a6160cca45a45d5d4ee95355898",
        ),
        (
            "editor_binary_sha256",
            "af7cb11220a411011917db0739f27a35b5c6ab725552b52b8a09bc01e296568a",
        ),
    ] {
        assert_eq!(proof[field].as_str(), Some(expected));
    }
    assert_eq!(
        proof["original_sha256"].as_str(),
        Some(drcs_bitmap::hash(original).as_str())
    );
    assert_eq!(
        proof["saved_sha256"].as_str(),
        Some(drcs_bitmap::hash(edited).as_str())
    );
    // This fixture receipt binds bytes and reported editor identity only. Root
    // separately inspects the actual owned GUI process and retained images.
    proof
}

pub(super) fn actual_input_hashes(flow: &Flow, directory: &Path) -> Vec<(String, String)> {
    let mut names: Vec<_> = flow.exported[0]
        .files()
        .iter()
        .map(|file| format!("native-original-1/{}", file.name))
        .collect();
    names.extend([
        "editor-1/edited.tti".into(),
        "editor-1/editor-proof.json".into(),
    ]);
    names
        .into_iter()
        .map(|name| {
            let bytes = drcs_bitmap::read_retained_editor_file(&directory.join(&name));
            (name, drcs_bitmap::hash(&bytes))
        })
        .collect()
}

pub(super) fn actual_request(
    flow: &Flow,
    directory: &Path,
) -> (NativeTeletextEditorialImportRequest, Value) {
    for file in flow.exported[0].files() {
        let retained = drcs_bitmap::read_retained_editor_file(
            &directory.join("native-original-1").join(&file.name),
        );
        assert_eq!(
            retained, file.bytes,
            "all retained editor inputs must equal this fresh no-bank export"
        );
    }
    let name = &flow.exported[0].sidecar().project.pages[0].name;
    assert_eq!(name, "page-124-0020.tti");
    let original = &flow.exported[0]
        .files()
        .iter()
        .find(|file| &file.name == name)
        .unwrap()
        .bytes;
    let edited = drcs_bitmap::read_retained_editor_file(&directory.join("editor-1/edited.tti"));
    let proof = actual_editor_proof(directory, original, &edited);
    assert_actual_tti_edit(original, &edited);
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request.name = "Actual QTM no-bank Polish Base concept".into();
    request
        .files
        .iter_mut()
        .find(|file| &file.name == name)
        .unwrap()
        .bytes = edited;
    (request, proof)
}

/// Set `ERSATZRS_QTM_COMPILER_POLISH_DIR` to retained fresh no-bank inputs and
/// genuine QTM Save As. The separate regular test's existing editor-output env
/// prepares native-original-1; older authored-bank input cannot substitute it.
#[tokio::test]
#[ignore = "requires fresh no-bank native-original-1 files, actual pinned QTM editor-1/edited.tti and editor-proof.json; no ordinary/client claim"]
async fn actual_qtm_no_bank_polish_edit_previews_publishes_and_reopens() {
    let directory =
        std::path::PathBuf::from(client_capture::selected("ERSATZRS_QTM_COMPILER_POLISH_DIR"));
    let (mut fixture, flow) = setup_case(response).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let (request, proof) = actual_request(&flow, &directory);
    let hashes = actual_input_hashes(&flow, &directory);
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_concept(&flow, restored.encoded().unwrap(), 0x5b);
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert_concept(
        &flow,
        drcs_bitmap::restored(&fixture, &concept)
            .await
            .encoded()
            .unwrap(),
        0x5b,
    );
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_concept(&flow, accepted.service(), 0x5b);
    assert_eq!(
        external_editor_probe::preview_page(accepted.service(), 124),
        preview
    );
    fixture = fixture.reopen().await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    assert_concept(&flow, recover(&fixture, 1).await.service(), 0x5b);
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(actual_input_hashes(&flow, &directory), hashes);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    polish_delivery::write_json(
        &directory,
        "native-proof.json",
        &json!({"probe_version":1,
        "scope":"retained fresh QTM no-bank Polish Save As to native concept, preview, explicit publication and reopen",
        "editor_name":proof["editor_name"],"editor_version":proof["editor_version"],
        "editor_source_commit":proof["editor_source_commit"],"editor_binary_sha256":proof["editor_binary_sha256"],
        "original_sha256":proof["original_sha256"],"saved_sha256":proof["saved_sha256"],"input_hashes":hashes,
        "design_id":concept.head().id,"revision":concept.head().revision,"digest":concept.digest(),
        "generation_id":accepted.generation().id,"request_id":accepted.generation().request.id,
        "exact_literal_polish_identity_and_generated_mask_and_shared_preview_verified":true,
        "originals_profiles_bindings_package_receipts_config_and_night_unchanged":true,
        "ordinary_delivery_proven":false,"independent_decoding_proven":false,"actual_client_proven":false,
        "limitations":"GUI provenance and images remain external evidence; no headed admin, hydration, ordinary stream, warm transition, independent decoder, suitable-client or full-engine/platform acceptance claim"}),
    );
}
