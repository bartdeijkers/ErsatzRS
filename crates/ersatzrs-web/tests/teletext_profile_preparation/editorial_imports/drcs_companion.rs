//! Protected DRCS editor companions from installed synthetic V3 originals.
//! Native integration is separate from the opt-in actual editor-loading proof.

use super::*;
use ersatzrs_addon_contract::TeletextResourceKindWire;
use ersatzrs_app::teletext_export_project::{
    parse_teletext_export_receipt_body, write_teletext_export_receipt_body,
    TeletextExportAssetScope, TeletextExportReceiptBody, TeletextExportSidecar,
};
use ersatzrs_core::repositories::{TeletextDesignExportSave, TeletextDesignRepository as _};
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::{
    EncodedTeletextAuthoredService, TeletextResourceFullUpdate, TeletextServiceResourceTransmission,
};
use ersatzrs_infra::repositories::SqliteTeletextDesignRepository;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

const PIXELS: [u16; 10] = [
    0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009,
];

fn resource(kind: TeletextResourceKind, page: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(1, page).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap()
}

fn resource_wire(identity: TeletextResourceIdentity) -> Value {
    json!({"kind":identity.kind(),"page":identity.page(),"subpage":identity.logical_subpage().get()})
}

fn mot() -> TeletextMotLevel25Declaration {
    let unused = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(1).unwrap(),
    };
    let object = TeletextMotObjectLink::new(
        unused,
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 0 }; 2],
    );
    let inactive =
        TeletextMotDrcsLink::new(unused, TeletextMotSubpageExtent::new(0).unwrap(), false);
    let mut normal = [inactive; 7];
    normal[0] = TeletextMotDrcsLink::new(
        TeletextX27LinkTarget::Page {
            page: resource(TeletextResourceKind::Drcs, 0xc0).page(),
        },
        TeletextMotSubpageExtent::new(0).unwrap(),
        false,
    );
    let association = TeletextMotAssociationSelection::new(
        false,
        Some(TeletextMotPublicLinkIndex::new(1).unwrap()),
    );
    TeletextMotLevel25Declaration::new(
        resource(TeletextResourceKind::Mot, 0xfe),
        [0x24, 0x25]
            .into_iter()
            .map(|page| {
                TeletextMotPageAssociation::new(
                    page,
                    TeletextMotAssociationSelection::new(false, None),
                    association,
                )
            })
            .collect(),
        TeletextMotLevel25Links::new(
            object,
            [object; 7],
            inactive,
            normal,
            TeletextMotEnhancementCounts::new(1, 0, 1),
        )
        .unwrap(),
    )
    .unwrap()
}

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut response = external_editor_probe::editor_response(theme, page, subcode);
    let drcs = resource(TeletextResourceKind::Drcs, 0xc0);
    let table = TeletextDrcsSubtable::new(
        drcs,
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
            bitmap: TeletextDrcsBitmap::new(PIXELS).unwrap(),
        }],
    )
    .unwrap();
    let mut table = serde_json::to_value(table).unwrap();
    table["identity"] = resource_wire(drcs);
    response["service"]["drcs"] = json!([table]);
    let declaration = mot();
    let mut declaration = serde_json::to_value(declaration).unwrap();
    declaration["identity"] = resource_wire(resource(TeletextResourceKind::Mot, 0xfe));
    response["service"]["mots"] = json!([declaration]);
    for selection in response["service"]["mot_selections"]
        .as_array_mut()
        .unwrap()
    {
        selection["mot"] = resource_wire(resource(TeletextResourceKind::Mot, 0xfe));
    }
    for enhancement in response["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["resources"]["drcs"]["normal"] = json!(drcs.page());
        let commands = enhancement["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        let glyph = commands
            .iter_mut()
            .find(|command| command["operation"]["kind"] == "g0")
            .unwrap();
        glyph["operation"] =
            json!({"kind":"drcs","reference":{"table":"normal","subtable":0,"slot":0}});
    }
    response
}

pub(super) fn encoding_policy(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut encoding = channel_documents::policy();
    encoding.transmissions = [
        resource(TeletextResourceKind::Drcs, 0xc0),
        resource(TeletextResourceKind::Mot, 0xfe),
    ]
    .into_iter()
    .map(|resource| TeletextServiceResourceTransmission {
        resource,
        counter: TeletextResourceUpdateCounter::new(1).unwrap(),
        update: TeletextResourceFullUpdate::Changed,
        last_subpage: true,
    })
    .collect();
    encoding
}

fn hash(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(text, "{byte:02x}").unwrap();
    }
    text
}

fn companion(flow: &Flow, index: usize) -> (&str, &str) {
    flow.exported[index]
        .sidecar()
        .project
        .assets
        .iter()
        .find_map(|asset| match &asset.scope {
            TeletextExportAssetScope::DrcsTti {
                resource_name,
                editor_ordinal: 0,
            } => Some((asset.name.as_str(), resource_name.as_str())),
            _ => None,
        })
        .unwrap()
}

fn assert_companion(flow: &Flow, index: usize) {
    let exported = &flow.exported[index];
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v3"
    );
    let (name, raw_name) = companion(flow, index);
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == name)
        .unwrap();
    let raw = exported
        .files()
        .iter()
        .find(|file| file.name == raw_name)
        .unwrap();
    let mapping = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| asset.name == raw_name)
        .unwrap();
    assert!(
        matches!(&mapping.scope, TeletextExportAssetScope::Resource { identity, physical_subcode: 0x0110 }
        if identity.kind == TeletextResourceKindWire::Drcs && identity.page.magazine == 1
        && identity.page.page == 0xc0 && identity.subpage == 0)
    );
    let original = flow.saved[index]
        .candidate()
        .encoded()
        .unwrap()
        .pages()
        .iter()
        .find(|page| {
            page.resource_header().is_some_and(|header| {
                header.identity() == resource(TeletextResourceKind::Drcs, 0xc0)
            })
        })
        .unwrap();
    assert_eq!(raw.bytes, original.data_units().concat());
    assert_eq!(mapping.sha256, hash(&raw.bytes));
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    assert!(document.records().contains(&TeletextTtiRecord::PageNumber {
        page: 0x1c0,
        editor_ordinal: 0
    }));
    assert!(document
        .records()
        .contains(&TeletextTtiRecord::SubpageOrdinal(0)));
    let cells = document
        .records()
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    for (pair, pixels) in cells[..20].chunks_exact(2).zip(PIXELS) {
        assert_eq!(
            pair,
            [
                0x40 | u8::try_from(pixels >> 6).unwrap(),
                0x40 | u8::try_from(pixels & 63).unwrap()
            ]
        );
    }
    assert_eq!(&cells[20..], &[b' '; 20]);
    assert_modes(document.records());
}

fn assert_modes(records: &[TeletextTtiRecord]) {
    let words = records
        .iter()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 28,
                designation: 3,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    assert_eq!(words[0], 5);
    for slot in 0..48 {
        let mode = (0..4).fold(0, |mode, bit| {
            let offset = 18 + slot * 4 + bit;
            mode | (((words[offset / 18] >> (offset % 18)) & 1) << bit)
        });
        assert_eq!(mode, if slot == 0 { 0 } else { 15 });
    }
}

fn assert_resources(
    original: &EncodedTeletextAuthoredService,
    edited: &EncodedTeletextAuthoredService,
) {
    let resources = |service: &EncodedTeletextAuthoredService| {
        service
            .pages()
            .iter()
            .filter(|page| page.resource_header().is_some())
            .map(|page| (page.resource_header(), page.data_units().to_vec()))
            .collect::<Vec<_>>()
    };
    assert_eq!(resources(original), resources(edited));
    assert_eq!(original.magazine_units(), edited.magazine_units());
}

pub(super) fn has_drcs(service: &EncodedTeletextAuthoredService) -> bool {
    service.pages().iter().any(|page| {
        page.resource_header()
            .is_some_and(|header| header.identity().kind() == TeletextResourceKind::Drcs)
    })
}

async fn assert_rejected_companions(fixture: &Fixture, flow: &Flow) {
    let (name, _) = companion(flow, 0);
    for missing in [false, true] {
        let mut edited = request(flow, 2, 8, b'Q');
        if missing {
            edited.files.retain(|file| file.name != name);
        } else {
            edit_cell(&mut edited.files, name, 1, 0, b'A');
        }
        assert!(import_teletext_editorial_design(
            &fixture.state.application,
            edited,
            policy(),
            designs::deadline()
        )
        .await
        .is_err());
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
    }
}

async fn assert_published_recovery(
    mut fixture: Fixture,
    flow: &Flow,
    edited: &SavedEditorialTeletextDesign,
) {
    assert_original_pointers(&fixture, flow).await;
    let raster = assert_concept_preview(&fixture, flow, edited, 2).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, flow).await;
    assert_eq!(
        assert_concept_preview(&fixture, flow, edited, 2).await,
        raster
    );
    let accepted = publish_and_accept(&fixture, flow, edited).await;
    assert_eq!(generations::preview(accepted.service()).raster(), &raster);
    assert_resources(
        flow.saved[0].candidate().encoded().unwrap(),
        accepted.service(),
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn protected_drcs_companion_free_edit_preview_publication_reopen_and_isolation() {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        encoding_policy,
    ))
    .await;
    assert_companion(&flow, 0);
    assert_companion(&flow, 1);
    assert_rejected_companions(&fixture, &flow).await;
    let edited = import(&fixture, request(&flow, 2, 8, b'Q')).await;
    assert_eq!(edited.head().revision, 2);
    assert_published_recovery(fixture, &flow, &edited).await;
}

fn v2_body(flow: &Flow) -> TeletextExportReceiptBody {
    let mut body =
        parse_teletext_export_receipt_body(&flow.receipts[0].receipt, policy().project).unwrap();
    let companions = body
        .project
        .assets
        .iter()
        .filter_map(|asset| {
            matches!(asset.scope, TeletextExportAssetScope::DrcsTti { .. })
                .then_some(asset.name.clone())
        })
        .collect::<Vec<_>>();
    body.project
        .assets
        .retain(|asset| !companions.contains(&asset.name));
    body.files.retain(|file| !companions.contains(&file.name));
    "qteletextmaker-1.0-carrier-v2".clone_into(&mut body.project.editor_profile);
    assert!(!companions.is_empty());
    body
}

async fn seed_v2_receipt(fixture: &Fixture, flow: &Flow) -> TeletextDesignExportReceipt {
    let body = v2_body(flow);
    let bytes = write_teletext_export_receipt_body(&body, policy().project).unwrap();
    let original = &flow.originals[0];
    // Fixture-only historical baseline: retain native assets and filenames while
    // omitting the v3-derived editor companions, exactly as v2 exported them.
    let result = SqliteTeletextDesignRepository(fixture.state.pool.clone())
        .create_export(
            &TeletextDesignExportSave {
                design_id: original.design_id,
                revision: original.revision,
                design_digest: original.digest.clone(),
                expected_design_head: 1,
                receipt_version: 1,
                receipt: bytes.clone(),
            },
            policy().project.max_bytes,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap();
    let TeletextDesignMutation::Saved(metadata) = result else {
        panic!("controlled v2 receipt must persist");
    };
    let receipt = super::qtm_carrier::stored_receipt(fixture, metadata.id).await;
    assert_eq!(receipt.receipt, bytes);
    assert_eq!(receipt.metadata.digest, hash(&bytes));
    receipt
}

fn v2_request(receipt: &TeletextDesignExportReceipt) -> NativeTeletextEditorialImportRequest {
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    let mut files = body.files;
    edit_cell(&mut files, &body.project.pages[0].name, 2, 8, b'Q');
    NativeTeletextEditorialImportRequest {
        name: "Synthetic retained v2 DRCS concept".to_owned(),
        receipt_id: receipt.metadata.id,
        receipt_digest: receipt.metadata.digest.clone(),
        expected_design_head: 1,
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
        files,
    }
}

#[tokio::test]
async fn retained_v2_drcs_receipt_without_companion_still_recovers_edited_concept() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        encoding_policy,
    ))
    .await;
    let receipt = seed_v2_receipt(&fixture, &flow).await;
    let edited = import(&fixture, v2_request(&receipt)).await;
    assert_original_pointers(&fixture, &flow).await;
    let raster = assert_concept_preview(&fixture, &flow, &edited, 2).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        assert_concept_preview(&fixture, &flow, &edited, 2).await,
        raster
    );
    assert_eq!(
        super::qtm_carrier::stored_receipt(&fixture, receipt.metadata.id).await,
        receipt
    );
    let accepted = publish_and_accept(&fixture, &flow, &edited).await;
    assert_resources(
        flow.saved[0].candidate().encoded().unwrap(),
        accepted.service(),
    );
    fixture = fixture.reopen().await;
    generations::assert_same_output(&recover(&fixture, 1).await, &accepted);
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert_eq!(
        super::qtm_carrier::stored_receipt(&fixture, receipt.metadata.id).await,
        receipt
    );
    fixture.state.pool.close().await;
}
