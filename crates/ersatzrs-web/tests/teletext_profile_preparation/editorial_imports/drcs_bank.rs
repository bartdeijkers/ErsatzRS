//! Receipt-bound multi-subtable imports. Native proof does not imply GUI,
//! transport or independent decoding acceptance; the retained GUI case is opt-in.

use super::*;
use ersatzrs_app::{
    teletext_design_documents::decode_teletext_design_documents,
    teletext_export_project::TeletextExportAssetScope, teletext_tti::TeletextTtiCycleUnit,
};
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;
use serde_json::{json, Value};

type Groups = Vec<Vec<TeletextTtiRecord>>;

fn free_glyphs(mut value: Value) -> Value {
    for page in value["authoring"]["pages"].as_array_mut().unwrap() {
        for column in [8, 10] {
            page["regions"].as_array_mut().unwrap().push(json!({
                "id":format!("free-glyph-{column}"), "kind":"free", "area":"main",
                "row":1, "column":column, "width":1, "height":1
            }));
        }
    }
    value
}

pub(super) fn dense_response(theme: &str, page: u16, subcode: u16) -> Value {
    free_glyphs(drcs_companion::dense_response(theme, page, subcode))
}

pub(super) fn sparse_response(theme: &str, page: u16, subcode: u16) -> Value {
    free_glyphs(drcs_companion::sparse_response(theme, page, subcode))
}

fn protected_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = dense_response(theme, page, subcode);
    // The second native subtable still has a protected consumer. Replacing
    // correctly labelled bitmap bodies cannot circumvent that consumer's zones.
    value["authoring"]["pages"][1]["regions"]
        .as_array_mut()
        .unwrap()
        .retain(|region| !region["id"].as_str().unwrap().starts_with("free-glyph-"));
    value
}

async fn setup_bank(sparse: bool) -> (Fixture, Flow) {
    if sparse {
        Box::pin(setup_with_fixture_policy(
            sparse_response,
            Fixture::new_persistent().await,
            drcs_companion::sparse_encoding_policy,
        ))
        .await
    } else {
        Box::pin(setup_with_fixture_policy(
            dense_response,
            Fixture::new_persistent().await,
            drcs_companion::dense_encoding_policy,
        ))
        .await
    }
}

fn bank_name(flow: &Flow) -> &str {
    &drcs_companion::indexed_companion(flow, 0).0.name
}

fn bank_groups(request: &NativeTeletextEditorialImportRequest, name: &str) -> Groups {
    let bytes = &request
        .files
        .iter()
        .find(|file| file.name == name)
        .unwrap()
        .bytes;
    drcs_companion::groups(parse_teletext_tti(bytes, policy().tti).unwrap().records())
}

fn replace_groups(request: &mut NativeTeletextEditorialImportRequest, name: &str, groups: Groups) {
    // Encode groups independently so malformed cross-group identity/order cases
    // reach the native import parser rather than failing in this test builder.
    let bytes = groups
        .into_iter()
        .flat_map(|records| {
            write_teletext_tti(
                &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
                policy().tti,
            )
            .unwrap()
        })
        .collect();
    request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes = bytes;
}

fn cells(group: &mut [TeletextTtiRecord]) -> &mut [u8; 40] {
    group
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap()
}

fn words(group: &mut [TeletextTtiRecord]) -> &mut [u32; 13] {
    group
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 28,
                designation: 3,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap()
}

fn writer_defaults(groups: &mut Groups, offset: u64) {
    for group in groups {
        for record in group.iter_mut() {
            match record {
                TeletextTtiRecord::PageNumber { editor_ordinal, .. }
                | TeletextTtiRecord::SubpageOrdinal(editor_ordinal) => *editor_ordinal += offset,
                _ => {}
            }
        }
        if !group
            .iter()
            .any(|record| matches!(record, TeletextTtiRecord::PageStatus(_)))
        {
            group.push(TeletextTtiRecord::PageStatus(0x8000));
        }
        group.push(TeletextTtiRecord::Region(0));
        group.push(TeletextTtiRecord::Cycle {
            value: 20,
            unit: TeletextTtiCycleUnit::Time,
        });
    }
}

pub(super) fn bitmap_request(
    flow: &Flow,
    sparse: bool,
    offset: u64,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let name = bank_name(flow);
    let mut groups = bank_groups(&request, name);
    // Each packed six-bit pair encodes twelve glyph pixels. Add x=10 to the
    // 0x801 first row of slot zero without touching its other nine rows.
    cells(&mut groups[usize::from(sparse)])[1] = b'C';
    writer_defaults(&mut groups, offset);
    replace_groups(&mut request, name, groups);
    request
}

pub(super) fn assert_native_edit(
    before: &EncodedTeletextAuthoredService,
    after: &EncodedTeletextAuthoredService,
    sparse: bool,
) {
    assert_eq!(before.inventory(), after.inventory());
    assert_eq!(before.magazine_units(), after.magazine_units());
    let edited_s1 = u8::from(sparse);
    for (a, b) in before.pages().iter().zip(after.pages()) {
        assert_eq!(a.resource_header(), b.resource_header());
        let changed = a.resource_header().is_some_and(|header| {
            header.identity().kind() == TeletextResourceKind::Drcs
                && header.identity().logical_subpage().get() == edited_s1
        });
        if changed {
            assert_ne!(a.data_units(), b.data_units());
        } else {
            // Includes every display fallback packet and the untouched bank.
            assert_eq!(a.data_units(), b.data_units());
        }
    }
    for page in [124, 125] {
        let a = external_editor_probe::preview_page(before, page);
        let b = external_editor_probe::preview_page(after, page);
        let changed: Vec<_> = a
            .pixels()
            .iter()
            .zip(b.pixels())
            .enumerate()
            .filter_map(|(index, (a, b))| (a != b).then_some(index))
            .collect();
        let expected = if sparse || page == 124 {
            vec![
                (4 + 8) * 12 + 10 + a.width() * 10,
                (4 + 10) * 12 + 10 + a.width() * 10,
            ]
        } else {
            Vec::new()
        };
        assert_eq!(changed, expected, "page {page}");
        for index in changed {
            assert!(matches!(b.pixels()[index],
                TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) if rgb.get() == [15,0,0]));
        }
    }
    drcs_companion::assert_native_banks(after, sparse);
}

async fn prove_flow(
    fixture: Fixture,
    flow: Flow,
    request: NativeTeletextEditorialImportRequest,
    sparse: bool,
) {
    let package = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    let concept = import(&fixture, request).await;
    assert_original_pointers(&fixture, &flow).await;
    let revision = drcs_bitmap::stored(&fixture, &concept).await;
    let decoded = decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = decoded.editorial_overlay().unwrap();
    assert_eq!(overlay.drcs_bitmap_overlays().len(), 1);
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    let bitmap = overlay.drcs_bitmap_overlays()[0];
    assert_eq!(bitmap.identity().page().page(), 0xc0);
    assert_eq!(bitmap.identity().logical_subpage().get(), u8::from(sparse));
    assert_eq!(bitmap.slot().get(), 0);
    assert_eq!(bitmap.expected().get()[0], 0x801);
    assert_eq!(bitmap.replacement().get()[0], 0x803);
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
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_native_edit(
        flow.accepted[0].service(),
        restored.encoded().unwrap(),
        sparse,
    );
    // Independent channel/profile/theme remains byte exact before publication.
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    drcs_companion::assert_bitmap_preview(flow.accepted[1].service(), sparse, "night");
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    assert!(exported.sidecar().project.assets.iter().any(|asset|
        matches!(&asset.scope, TeletextExportAssetScope::DrcsTtiSet { resource_names } if resource_names.len() == 2)));
    let before_file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == bank_name(&flow))
        .unwrap();
    let after_file = exported
        .files()
        .iter()
        .find(|file| file.name == bank_name(&flow))
        .unwrap();
    let before_groups = drcs_companion::groups(
        parse_teletext_tti(&before_file.bytes, policy().tti)
            .unwrap()
            .records(),
    );
    let after_groups = drcs_companion::groups(
        parse_teletext_tti(&after_file.bytes, policy().tti)
            .unwrap()
            .records(),
    );
    assert_eq!(after_groups.len(), 2);
    // Re-export restores native receipt ordinals; it does not perpetuate QTM's
    // display-only +1 Save As offset or materialize an undefined padding group.
    let untouched = usize::from(!sparse);
    assert_eq!(before_groups[untouched], after_groups[untouched]);
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_native_edit(flow.accepted[0].service(), accepted.service(), sparse);
    let fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert_eq!(
        calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap(),
        package
    );
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn dense_bank_canonical_and_qtm_shifted_bitmap_concepts_publish_and_reopen() {
    for offset in [0, 1] {
        let (fixture, flow) = Box::pin(setup_bank(false)).await;
        let request = bitmap_request(&flow, false, offset);
        Box::pin(prove_flow(fixture, flow, request, false)).await;
    }
}

#[tokio::test]
async fn sparse_bank_preserves_undefined_padding_with_qtm_defaults() {
    let (fixture, flow) = Box::pin(setup_bank(true)).await;
    let request = bitmap_request(&flow, true, 1);
    Box::pin(prove_flow(fixture, flow, request, true)).await;
}

async fn rejected(
    fixture: &Fixture,
    flow: &Flow,
    request: NativeTeletextEditorialImportRequest,
    label: &str,
) {
    assert!(
        import_teletext_editorial_design(
            &fixture.state.application,
            request,
            policy(),
            designs::deadline()
        )
        .await
        .is_err(),
        "{label}"
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.saved[0].head().id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1,
        "{label}"
    );
    assert_original_pointers(fixture, flow).await;
}

fn mutate_groups(mut groups: Groups, mutation: u8) -> Groups {
    match mutation {
        0 => {
            groups.pop();
        }
        1 => {
            let mut extra = groups[1].clone();
            for record in &mut extra {
                match record {
                    TeletextTtiRecord::PageNumber { editor_ordinal, .. }
                    | TeletextTtiRecord::SubpageOrdinal(editor_ordinal) => *editor_ordinal = 2,
                    _ => {}
                }
            }
            groups.push(extra);
        }
        2 => {
            let (first, rest) = groups.split_at_mut(1);
            rest[0].clone_from(&first[0]);
        }
        3 => groups.swap(0, 1),
        4 | 5 => {
            for record in &mut groups[1] {
                match record {
                    TeletextTtiRecord::PageNumber { editor_ordinal, .. }
                    | TeletextTtiRecord::SubpageOrdinal(editor_ordinal) => {
                        *editor_ordinal += if mutation == 4 { 1 } else { 7 }
                    }
                    _ => {}
                }
            }
        }
        6 => {
            for record in &mut groups[1] {
                if let TeletextTtiRecord::PageNumber { page, .. } = record {
                    *page = 0x1c1;
                }
            }
        }
        7 => {
            for record in &mut groups[1] {
                if let TeletextTtiRecord::PageStatus(status) = record {
                    *status ^= 1;
                }
            }
        }
        8 => {
            words(&mut groups[1])[0] ^= 1;
        } // X28/3 function
        9 => {
            words(&mut groups[1])[1] ^= 1;
        } // glyph mode
        10 => {
            groups[1].retain(|record| {
                !matches!(
                    record,
                    TeletextTtiRecord::Packet {
                        row: 28,
                        designation: 3,
                        ..
                    }
                )
            });
        }
        11 => {
            cells(&mut groups[1])[21] ^= 1;
        } // undefined second glyph half
        12 => {
            for record in &mut groups[1] {
                if let TeletextTtiRecord::SubpageOrdinal(ordinal) = record {
                    *ordinal += 1;
                }
            }
        }
        13 => {
            for group in &mut groups {
                for record in group {
                    match record {
                        TeletextTtiRecord::PageNumber { editor_ordinal, .. }
                        | TeletextTtiRecord::SubpageOrdinal(editor_ordinal) => {
                            *editor_ordinal += 7;
                        }
                        _ => {}
                    }
                }
            }
        }
        _ => unreachable!(),
    }
    groups
}

#[tokio::test]
async fn bank_structure_and_protected_resource_changes_are_rejected_without_publication() {
    let (fixture, flow) = Box::pin(setup_bank(false)).await;
    for mutation in 0..14 {
        let mut request = bitmap_request(&flow, false, 0);
        let name = bank_name(&flow);
        let groups = mutate_groups(bank_groups(&request, name), mutation);
        replace_groups(&mut request, name, groups);
        rejected(&fixture, &flow, request, &format!("mutation {mutation}")).await;
    }
    // Uploaded sidecar claims cannot swap the receipt's native resource names.
    let mut request = bitmap_request(&flow, false, 1);
    let mut sidecar = flow.exported[0].sidecar().clone();
    for asset in &mut sidecar.project.assets {
        if let TeletextExportAssetScope::DrcsTtiSet { resource_names } = &mut asset.scope {
            resource_names.swap(0, 1);
        }
    }
    // Deliberately malformed untrusted input must reach the host. The checked
    // exporter correctly refuses this inconsistent native receipt mapping.
    request.sidecar = serde_json::to_vec(&sidecar).unwrap();
    rejected(&fixture, &flow, request, "receipt mapping swap").await;
    // Raw DVB assets remain immutable even when a TTI bitmap edit is valid.
    let mut request = bitmap_request(&flow, false, 1);
    let raw_name = drcs_companion::indexed_companion(&flow, 0).1[0]
        .as_ref()
        .unwrap();
    request
        .files
        .iter_mut()
        .find(|file| &file.name == raw_name)
        .unwrap()
        .bytes[0] ^= 1;
    rejected(&fixture, &flow, request, "raw resource replacement").await;
    assert_retained_storage(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn sparse_padding_cannot_materialize_rows_or_defined_modes() {
    let (fixture, flow) = Box::pin(setup_bank(true)).await;
    for mutation in 0..2 {
        let mut request = bitmap_request(&flow, true, 1);
        let name = bank_name(&flow);
        let mut groups = bank_groups(&request, name);
        if mutation == 0 {
            groups[0].push(TeletextTtiRecord::BaseRow {
                row: 1,
                cells: [b' '; 40],
            });
        } else {
            words(&mut groups[0])[1] ^= 1;
        }
        replace_groups(&mut request, name, groups);
        rejected(&fixture, &flow, request, "padding materialization").await;
    }
    assert_retained_storage(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn copying_bank_bitmap_bodies_cannot_change_a_protected_consumer() {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        protected_response,
        Fixture::new_persistent().await,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    let name = bank_name(&flow);
    let mut groups = bank_groups(&request, name);
    let a = *cells(&mut groups[0]);
    let b = *cells(&mut groups[1]);
    *cells(&mut groups[0]) = b;
    *cells(&mut groups[1]) = a;
    replace_groups(&mut request, name, groups);
    rejected(&fixture, &flow, request, "protected second-bank consumer").await;
    assert_retained_storage(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn stale_bank_receipt_cannot_overwrite_a_new_concept() {
    let (fixture, flow) = Box::pin(setup_bank(false)).await;
    let concept = import(&fixture, bitmap_request(&flow, false, 1)).await;
    let stale = import_teletext_editorial_design(
        &fixture.state.application,
        bitmap_request(&flow, false, 0),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(!matches!(stale, Ok(TeletextDesignMutation::Saved(_))));
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(concept.head().id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        concept.head().revision
    );
    assert_original_pointers(&fixture, &flow).await;
    assert_retained_storage(&fixture, &flow).await;
    fixture.state.pool.close().await;
}

async fn retained_actual_bank_flow(
    sparse: bool,
    environment: &str,
    source_hash: &str,
    edited_hash: &str,
) {
    let directory = super::super::client_capture::selected(environment);
    let directory = Path::new(&directory);
    let source = drcs_bitmap::read_retained_editor_file(&directory.join("native-input.tti"));
    let edited = drcs_bitmap::read_retained_editor_file(&directory.join("edited.tti"));
    assert_eq!(drcs_bitmap::hash(&source), source_hash);
    assert_eq!(drcs_bitmap::hash(&edited), edited_hash);
    let (fixture, flow) = Box::pin(setup_bank(sparse)).await;
    let name = bank_name(&flow);
    let original_file = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    assert_eq!(
        original_file.bytes, source,
        "retained GUI input must be this exact regenerated bank export"
    );
    let mut request = drcs_bitmap::from_receipt(&flow.receipts[0], 1);
    request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap()
        .bytes = edited;
    Box::pin(prove_flow(fixture, flow, request, sparse)).await;
}

/// Set `ERSATZRS_QTM_DRCS_BANK_CALIBRATION_DIR` to retained actual-2. This reuses
/// pinned `QTeletextMaker` 1.0 Save As evidence; it does not run or prove a new GUI.
#[tokio::test]
#[ignore = "requires retained actual-2 QTeletextMaker calibration files"]
async fn retained_actual_qtm_dense_bank_import_preview_publication_and_reopen() {
    Box::pin(retained_actual_bank_flow(
        false,
        "ERSATZRS_QTM_DRCS_BANK_CALIBRATION_DIR",
        "d1e9a9d49a28be14adb1e727c11ab0cfe908bbd0df83ca49c48ef4e170a0a870",
        "6b4ac2e70bab6d73b6bdbb52bf9590887c7094582c14bbbca87433225aaebda6",
    ))
    .await;
}

/// Set `ERSATZRS_QTM_DRCS_SPARSE_CALIBRATION_DIR` to retained actual-padded-2.
/// This reuses the pinned `QTeletextMaker` 1.0 sparse-bank Save As evidence.
#[tokio::test]
#[ignore = "requires retained actual-padded-2 QTeletextMaker calibration files"]
async fn retained_actual_qtm_sparse_bank_import_preview_publication_and_reopen() {
    Box::pin(retained_actual_bank_flow(
        true,
        "ERSATZRS_QTM_DRCS_SPARSE_CALIBRATION_DIR",
        "0e563bc16e0a5d3e339f0a035b2d802d3463e873cbba2a1c7d4d99f302a0afc7",
        "65e4580a5cd766648cd9499ba1bc2f034c95e62c9243a16edcf0311a2dacc0d9",
    ))
    .await;
}
