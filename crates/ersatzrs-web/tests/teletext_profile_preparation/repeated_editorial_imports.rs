//! Installed editor baselines are repeatable concepts, never implicit publication.
//! Tests exercise native backends; external-editor/client acceptance is separate.

use super::{
    authoring_metadata, channel_documents, designs, editorial_imports,
    effective_editorial_imports as scoped, Fixture,
};
use ersatzrs_app::{
    teletext_design_documents::{
        decode_teletext_design_documents, decode_teletext_effective_design_documents,
        TeletextSavedEditorialOverlay,
    },
    teletext_export_project::write_teletext_export_sidecar,
    teletext_tti::{
        parse_teletext_tti, write_teletext_tti, TeletextTtiDocument, TeletextTtiRecord,
    },
};
use ersatzrs_core::repositories::{TeletextDesignMutation, TeletextDesignRejection};
use ersatzrs_domain::{TeletextDesignRevision, TeletextPageAddress};
use ersatzrs_ffmpeg::teletext::TeletextRaster;
use ersatzrs_web::host::{
    teletext_authoring::TeletextAuthoringIdentity,
    teletext_design_exports::{
        export_original_teletext_design, ExportedOriginalTeletextDesign,
        NativeTeletextExportRequest,
    },
    teletext_designs::reload_original_teletext_design_revision,
    teletext_editorial_designs::{
        import_teletext_editorial_design, NativeTeletextEditorialImportError,
        NativeTeletextEditorialImportRequest, SavedEditorialTeletextDesign,
    },
    teletext_effective_designs::reload_effective_original_teletext_design_revision,
    teletext_preview::project_teletext_preview,
};
use serde_json::{json, Value};

fn page_name(exported: &ExportedOriginalTeletextDesign, owner: TeletextPageAddress) -> &str {
    &exported
        .sidecar()
        .project
        .pages
        .iter()
        .find(|page| page.page.page == owner.page.get() && page.page.subcode == owner.subcode.get())
        .unwrap()
        .name
}

pub(super) fn request(
    exported: &ExportedOriginalTeletextDesign,
    head: i64,
    edits: &[(TeletextPageAddress, u8, u8)],
) -> NativeTeletextEditorialImportRequest {
    let mut files = exported.files().to_vec();
    for &(owner, row, code) in edits {
        editorial_imports::edit_cell(&mut files, page_name(exported, owner), row, 8, code);
    }
    NativeTeletextEditorialImportRequest {
        name: "Repeated native editor concept".to_owned(),
        receipt_id: exported.metadata().id,
        receipt_digest: exported.metadata().digest.clone(),
        expected_design_head: head,
        sidecar: write_teletext_export_sidecar(
            exported.sidecar(),
            editorial_imports::policy().project,
        )
        .unwrap(),
        files,
    }
}

async fn revision(
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

fn overlay(stored: &TeletextDesignRevision) -> Option<TeletextSavedEditorialOverlay> {
    match stored.document_version {
        1 => decode_teletext_design_documents(&stored.document, designs::budget())
            .unwrap()
            .editorial_overlay()
            .cloned(),
        2 => decode_teletext_effective_design_documents(&stored.document, designs::budget())
            .unwrap()
            .editorial_overlay()
            .cloned(),
        _ => panic!("unexpected stored role"),
    }
}

fn assert_root_context(root: &TeletextDesignRevision, current: &TeletextDesignRevision) {
    assert_eq!(root.document_version, current.document_version);
    assert_eq!(root.design_id, current.design_id);
    let root_value: Value = serde_json::from_slice(&root.document).unwrap();
    let mut current_value: Value = serde_json::from_slice(&current.document).unwrap();
    current_value
        .as_object_mut()
        .unwrap()
        .remove("editorial_overlay");
    // Includes exact source strings/settings/requests, complete origins and
    // Off membership, original expiry, resource lifecycle and editor metadata.
    assert_eq!(current_value, root_value);
    if let Some(overlay) = overlay(current) {
        assert_eq!(overlay.provenance().original_design_id, root.design_id);
        assert_eq!(overlay.provenance().original_revision, root.revision);
        assert_eq!(overlay.provenance().original_digest, root.digest);
    }
}

fn assert_rows(stored: &TeletextDesignRevision, expected: &[(TeletextPageAddress, u8, u8)]) {
    let actual = overlay(stored)
        .unwrap()
        .pages()
        .iter()
        .flat_map(|page| {
            page.rows()
                .iter()
                .map(move |row| (page.owner(), row.row(), row.cells()[8]))
        })
        .collect::<Vec<_>>();
    assert_eq!(actual, expected);
}

fn restore_rows(
    latest: &ExportedOriginalTeletextDesign,
    root: &ExportedOriginalTeletextDesign,
    head: i64,
    rows: &[(TeletextPageAddress, u8)],
) -> NativeTeletextEditorialImportRequest {
    let mut request = request(latest, head, &[]);
    for &(owner, row) in rows {
        let cells = original_row(root, owner, row);
        let file = request
            .files
            .iter_mut()
            .find(|file| file.name == page_name(latest, owner))
            .unwrap();
        let mut records = parse_teletext_tti(&file.bytes, editorial_imports::policy().tti)
            .unwrap()
            .records()
            .to_vec();
        let TeletextTtiRecord::BaseRow { cells: target, .. } = records.iter_mut().find(|record|
            matches!(record, TeletextTtiRecord::BaseRow { row: actual, .. } if *actual == row)
        ).unwrap() else { panic!("latest changed row must exist") };
        *target = cells;
        file.bytes = write_teletext_tti(
            &TeletextTtiDocument::checked_from_records(records, editorial_imports::policy().tti)
                .unwrap(),
            editorial_imports::policy().tti,
        )
        .unwrap();
    }
    request
}

fn original_row(
    root: &ExportedOriginalTeletextDesign,
    owner: TeletextPageAddress,
    row: u8,
) -> [u8; 40] {
    let original = root
        .files()
        .iter()
        .find(|file| file.name == page_name(root, owner))
        .unwrap();
    let parsed = parse_teletext_tti(&original.bytes, editorial_imports::policy().tti).unwrap();
    parsed
        .records()
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: actual, cells } if *actual == row => Some(*cells),
            _ => None,
        })
        .unwrap_or([b' '; 40])
}

async fn effective_units(fixture: &Fixture, stored: &TeletextDesignRevision) -> Vec<Vec<[u8; 46]>> {
    let restored = reload_effective_original_teletext_design_revision(
        &fixture.state.application,
        stored.design_id,
        stored.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let encoded = restored.encoded().unwrap();
    std::iter::once(encoded.magazine_units().to_vec())
        .chain(
            encoded
                .pages()
                .iter()
                .map(|page| page.data_units().to_vec()),
        )
        .collect()
}

async fn assert_unchanged_latest(
    fixture: &Fixture,
    exported: &ExportedOriginalTeletextDesign,
    head: i64,
) {
    let before = scoped::stable_state(fixture).await;
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    assert!(matches!(
        import_teletext_editorial_design(
            &fixture.state.application,
            request(exported, head, &[]),
            editorial_imports::policy(),
            designs::deadline(),
        )
        .await,
        Err(NativeTeletextEditorialImportError::NoChanges)
    ));
    assert_eq!(scoped::stable_state(fixture).await, before);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
            .fetch_one(&fixture.state.pool)
            .await
            .unwrap(),
        count
    );
}

#[tokio::test]
async fn scoped_repeated_export_import_preserves_cumulative_rows_replace_undo_reopen_and_publication(
) {
    let mut flow = scoped::setup().await;
    let owner = channel_documents::address(123, 16);
    let before = scoped::stable_state(&flow.fixture.fixture).await;
    let calls = flow.fixture.calls();
    let configuration = designs::config(&flow.fixture.fixture).await;
    let first = scoped::import(
        &flow.fixture.fixture,
        request(&flow.exported[0], 1, &[(owner, 2, b'A')]),
    )
    .await;
    assert_eq!(first.head().revision, 2);
    let stored = revision(&flow.fixture.fixture, &first).await;
    let exported = scoped::export(&flow.fixture.fixture, &stored).await;
    let second = scoped::import(
        &flow.fixture.fixture,
        request(&exported, 2, &[(owner, 3, b'B')]),
    )
    .await;
    assert_eq!(second.head().revision, 3);
    let stored = revision(&flow.fixture.fixture, &second).await;
    assert_root_context(&flow.originals[0], &stored);
    assert_rows(&stored, &[(owner, 2, b'A'), (owner, 3, b'B')]);
    let pixels = scoped::assert_concept(&flow.fixture.fixture, &flow.originals[0], &second).await;
    assert_ne!(pixels, flow.pixels[0]);
    flow.fixture.fixture = flow.fixture.fixture.reopen().await;
    assert_eq!(
        scoped::assert_concept(&flow.fixture.fixture, &flow.originals[0], &second).await,
        pixels
    );
    let final_concept = replace_and_undo(&flow, &second, owner).await;
    assert_eq!(scoped::stable_state(&flow.fixture.fixture).await, before);
    assert_eq!(flow.fixture.calls(), calls);
    assert_eq!(designs::config(&flow.fixture.fixture).await, configuration);
    assert_eq!(
        scoped::original(&flow.fixture.fixture, flow.identities[1]).await,
        flow.originals[1]
    );
    let other = super::effective_authoring::preview(&flow.fixture, flow.identities[1], false).await;
    assert_eq!(other.raster(), Some(&flow.pixels[1]));
    drop(other);
    scoped::explicit_select_concept(&flow.fixture, &final_concept).await;
}

async fn replace_and_undo(
    flow: &scoped::Flow,
    second: &SavedEditorialTeletextDesign,
    owner: TeletextPageAddress,
) -> SavedEditorialTeletextDesign {
    let stored = revision(&flow.fixture.fixture, second).await;
    let exported = scoped::export(&flow.fixture.fixture, &stored).await;
    let replacement = scoped::import(
        &flow.fixture.fixture,
        request(&exported, 3, &[(owner, 2, b'C')]),
    )
    .await;
    let stored = revision(&flow.fixture.fixture, &replacement).await;
    assert_eq!(stored.revision, 4);
    assert_root_context(&flow.originals[0], &stored);
    assert_rows(&stored, &[(owner, 2, b'C'), (owner, 3, b'B')]);
    let exported = scoped::export(&flow.fixture.fixture, &stored).await;
    let partial = scoped::import(
        &flow.fixture.fixture,
        restore_rows(&exported, &flow.exported[0], 4, &[(owner, 2)]),
    )
    .await;
    let stored = revision(&flow.fixture.fixture, &partial).await;
    assert_eq!(stored.revision, 5);
    assert_root_context(&flow.originals[0], &stored);
    let root_free = original_row(&flow.exported[0], owner, 2)[8];
    let root_zone = original_row(&flow.exported[0], owner, 3)[8];
    assert_rows(&stored, &[(owner, 2, root_free), (owner, 3, b'B')]);
    let exported = scoped::export(&flow.fixture.fixture, &stored).await;
    let undone = scoped::import(
        &flow.fixture.fixture,
        restore_rows(&exported, &flow.exported[0], 5, &[(owner, 3)]),
    )
    .await;
    let stored = revision(&flow.fixture.fixture, &undone).await;
    assert_eq!(stored.revision, 6);
    assert_root_context(&flow.originals[0], &stored);
    assert_rows(&stored, &[(owner, 2, root_free), (owner, 3, root_zone)]);
    assert_ne!(stored.digest, flow.originals[0].digest);
    assert_eq!(
        effective_units(&flow.fixture.fixture, &stored).await,
        effective_units(&flow.fixture.fixture, &flow.originals[0]).await
    );
    let identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: stored.design_id,
        revision: stored.revision,
    };
    let preview = super::effective_authoring::preview(&flow.fixture, identity, false).await;
    assert_eq!(preview.raster(), Some(&flow.pixels[0]));
    drop(preview);
    let exported = scoped::export(&flow.fixture.fixture, &stored).await;
    assert_unchanged_latest(&flow.fixture.fixture, &exported, 6).await;
    undone
}

fn two_subpages(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = authoring_metadata::response(theme, page, subcode);
    let second = authoring_metadata::response(theme, page, subcode + 1);
    for key in ["pages", "page_enhancements", "mot_selections"] {
        value["service"][key]
            .as_array_mut()
            .unwrap()
            .extend(second["service"][key].as_array().unwrap().iter().cloned());
    }
    value["authoring"]["pages"].as_array_mut().unwrap().extend(
        second["authoring"]["pages"]
            .as_array()
            .unwrap()
            .iter()
            .cloned(),
    );
    value
}

async fn subset_export(
    fixture: &Fixture,
    original: &TeletextDesignRevision,
    owner: TeletextPageAddress,
) -> ExportedOriginalTeletextDesign {
    let TeletextDesignMutation::Saved(exported) = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: original.design_id,
            revision: original.revision,
            design_digest: original.digest.clone(),
            expected_design_head: original.revision,
            pages: Some(vec![owner]),
        },
        editorial_imports::policy(),
        designs::deadline(),
    )
    .await
    .unwrap() else {
        panic!("legal owner subset must export")
    };
    assert_eq!(exported.sidecar().project.pages.len(), 1);
    exported
}

async fn legacy_pixels(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
    owner: TeletextPageAddress,
) -> TeletextRaster {
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        concept.head().id,
        concept.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    project_teletext_preview(
        restored.encoded().unwrap(),
        owner,
        ersatzrs_domain::TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: ersatzrs_domain::TeletextFlashPhase::Original,
        },
        editorial_imports::preview_budget(),
    )
    .unwrap()
    .raster()
    .clone()
}

#[tokio::test]
async fn version_one_repeated_subset_import_keeps_an_earlier_owner_and_authenticates_union_root() {
    let (mut fixture, flow) = Box::pin(editorial_imports::setup_with_fixture_policy(
        two_subpages,
        Fixture::new_persistent().await,
        two_subpage_policy,
    ))
    .await;
    let first_owner = channel_documents::address(124, 32);
    let second_owner = channel_documents::address(124, 33);
    let baseline = scoped::stable_state(&fixture).await;
    let calls = std::fs::read(fixture.root.path().join("authoring-calls")).unwrap();
    let first_export = subset_export(&fixture, &flow.originals[0], first_owner).await;
    let first = scoped::import(
        &fixture,
        request(&first_export, 1, &[(first_owner, 2, b'A')]),
    )
    .await;
    let first_pixels = legacy_pixels(&fixture, &first, first_owner).await;
    let first_stored = revision(&fixture, &first).await;
    let old_receipt = overlay(&first_stored)
        .unwrap()
        .provenance()
        .export_receipt_id;
    let second_export = subset_export(&fixture, &first_stored, second_owner).await;
    let second = scoped::import(
        &fixture,
        request(&second_export, 2, &[(second_owner, 3, b'B')]),
    )
    .await;
    let stored = revision(&fixture, &second).await;
    assert_eq!(stored.document_version, 1);
    assert_eq!(stored.revision, 3);
    assert_root_context(&flow.originals[0], &stored);
    assert_rows(&stored, &[(first_owner, 2, b'A'), (second_owner, 3, b'B')]);
    let provenance = overlay(&stored).unwrap().provenance().clone();
    assert_ne!(provenance.export_receipt_id, old_receipt);
    let receipt = fixture
        .state
        .application
        .teletext_design_export_receipt(
            provenance.export_receipt_id,
            editorial_imports::policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(receipt.metadata.revision, 1);
    assert_eq!(receipt.metadata.design_digest, flow.originals[0].digest);
    assert_eq!(
        legacy_pixels(&fixture, &second, first_owner).await,
        first_pixels
    );
    let second_pixels = legacy_pixels(&fixture, &second, second_owner).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        legacy_pixels(&fixture, &second, first_owner).await,
        first_pixels
    );
    assert_eq!(
        legacy_pixels(&fixture, &second, second_owner).await,
        second_pixels
    );
    assert_eq!(scoped::stable_state(&fixture).await, baseline);
    assert_eq!(
        std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
        calls
    );
    let latest = subset_export(&fixture, &stored, second_owner).await;
    assert_unchanged_latest(&fixture, &latest, 3).await;
}

fn two_subpage_policy(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = channel_documents::policy();
    policy.entry = Some(channel_documents::address(124, 32));
    policy
}

#[tokio::test]
async fn repeated_latest_receipt_rejects_root_relabel_protected_resource_and_stale_head() {
    let flow = scoped::setup().await;
    let owner = channel_documents::address(123, 16);
    let first = scoped::import(
        &flow.fixture.fixture,
        request(&flow.exported[0], 1, &[(owner, 2, b'A')]),
    )
    .await;
    let stored = revision(&flow.fixture.fixture, &first).await;
    let exported = scoped::export(&flow.fixture.fixture, &stored).await;
    let baseline = scoped::stable_state(&flow.fixture.fixture).await;
    let count = flow.fixture.design_count().await;
    let calls = flow.fixture.calls();
    let mut stale = request(&exported, 1, &[(owner, 3, b'B')]);
    assert!(matches!(
        import_teletext_editorial_design(
            &flow.fixture.fixture.state.application,
            stale,
            editorial_imports::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
        TeletextDesignMutation::Rejected(TeletextDesignRejection::Conflict)
    ));
    for resource in [false, true] {
        stale = request(&exported, 2, &[(owner, 3, b'B')]);
        if resource {
            let name = &exported.sidecar().project.assets[0].name;
            stale
                .files
                .iter_mut()
                .find(|file| &file.name == name)
                .unwrap()
                .bytes[0] ^= 1;
        } else {
            let mut value: Value = serde_json::from_slice(&stale.sidecar).unwrap();
            value["project"]["original"]["revision"] = json!(1);
            stale.sidecar = serde_json::to_vec(&value).unwrap();
        }
        assert!(import_teletext_editorial_design(
            &flow.fixture.fixture.state.application,
            stale,
            editorial_imports::policy(),
            designs::deadline(),
        )
        .await
        .is_err());
        assert_eq!(scoped::stable_state(&flow.fixture.fixture).await, baseline);
        assert_eq!(flow.fixture.design_count().await, count);
        assert_eq!(flow.fixture.calls(), calls);
    }
}

#[path = "repeated_editorial_imports/operands.rs"]
mod operands;
