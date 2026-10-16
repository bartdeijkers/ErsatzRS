//! Protected DRCS editor companions from installed synthetic V3 originals.
//! Native integration is separate from the opt-in actual editor-loading proof.

use super::*;
use ersatzrs_addon_contract::TeletextResourceKindWire;
use ersatzrs_app::teletext_export_project::{
    parse_teletext_export_receipt_body, write_teletext_export_receipt_body,
    TeletextExportAssetMapping, TeletextExportAssetScope, TeletextExportReceiptBody,
    TeletextExportSidecar,
};
use ersatzrs_core::repositories::{TeletextDesignExportSave, TeletextDesignRepository as _};
use ersatzrs_domain::teletext::*;
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_drcs, EncodedTeletextAuthoredService, TeletextResourceFullUpdate,
    TeletextServiceResourceTransmission,
};
use ersatzrs_infra::repositories::SqliteTeletextDesignRepository;
use serde_json::{json, Value};
use sha2::{Digest as _, Sha256};
use std::fmt::Write as _;

const PIXELS: [u16; 10] = [
    0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009,
];

const MIRRORED_PIXELS: [u16; 10] = [
    0x801, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x900,
];

fn resource(kind: TeletextResourceKind, page: u8) -> TeletextResourceIdentity {
    indexed_resource(kind, page, 0)
}

fn indexed_resource(kind: TeletextResourceKind, page: u8, subpage: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(1, page).unwrap(),
        TeletextResourceSubpageIndex::new(subpage).unwrap(),
    )
    .unwrap()
}

fn resource_wire(identity: TeletextResourceIdentity) -> Value {
    json!({"kind":identity.kind(),"page":identity.page(),"subpage":identity.logical_subpage().get()})
}

fn mot() -> TeletextMotLevel25Declaration {
    indexed_mot(0, true)
}

fn indexed_mot(maximum: u8, normal_target: bool) -> TeletextMotLevel25Declaration {
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
        if normal_target {
            TeletextX27LinkTarget::Page {
                page: resource(TeletextResourceKind::Drcs, 0xc0).page(),
            }
        } else {
            unused
        },
        TeletextMotSubpageExtent::new(maximum).unwrap(),
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
            TeletextMotEnhancementCounts::new(1, 0, u8::from(normal_target)),
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

fn indexed_table(subpage: u8, pixels: [u16; 10]) -> Value {
    let identity = indexed_resource(TeletextResourceKind::Drcs, 0xc0, subpage);
    let table = TeletextDrcsSubtable::new(
        identity,
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
            bitmap: TeletextDrcsBitmap::new(pixels).unwrap(),
        }],
    )
    .unwrap();
    let mut value = serde_json::to_value(table).unwrap();
    value["identity"] = resource_wire(identity);
    value
}

fn indexed_response(theme: &str, page: u16, subcode: u16, sparse: bool) -> Value {
    let mut value = response(theme, page, subcode);
    let mut tables = vec![indexed_table(1, MIRRORED_PIXELS)];
    if !sparse {
        tables.insert(0, indexed_table(0, PIXELS));
    }
    value["service"]["drcs"] = json!(tables);
    let declaration = indexed_mot(u8::from(!sparse), !sparse);
    let mut declaration = serde_json::to_value(declaration).unwrap();
    declaration["identity"] = resource_wire(resource(TeletextResourceKind::Mot, 0xfe));
    value["service"]["mots"] = json!([declaration]);
    for enhancement in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        let subtable = u8::from(sparse || enhancement["address"]["page"] == 125);
        let operation =
            json!({"kind":"drcs","reference":{"table":"normal","subtable":subtable,"slot":0}});
        let commands = enhancement["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        let glyph = commands
            .iter_mut()
            .find(|command| command["operation"]["kind"] == "drcs")
            .unwrap();
        glyph["operation"] = operation.clone();
        // Same selected normal bank; no reliance on a decoder reselecting its
        // normal latch after the first glyph at this Level 2.5 page.
        commands.push(json!({"kind":"at_column","column":10,"operation":operation}));
        if sparse {
            let unused = json!({"target":{"kind":"unspecified","magazine":1},"validity":"level25_only","required_subpages":65535});
            enhancement["compositional_links"] = json!({"gpop":unused,"pop":unused,"gdrcs":unused,
                "drcs":{"target":{"kind":"page","page":{"magazine":1,"page":192}},
                    "validity":"level25_only","required_subpages":2}});
        }
    }
    value
}

pub(super) fn dense_response(theme: &str, page: u16, subcode: u16) -> Value {
    indexed_response(theme, page, subcode, false)
}

pub(super) fn sparse_response(theme: &str, page: u16, subcode: u16) -> Value {
    indexed_response(theme, page, subcode, true)
}

fn indexed_encoding_policy(
    sparse: bool,
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut encoding = channel_documents::policy();
    let mut resources = vec![indexed_resource(TeletextResourceKind::Drcs, 0xc0, 1)];
    if !sparse {
        resources.insert(0, resource(TeletextResourceKind::Drcs, 0xc0));
    }
    resources.push(resource(TeletextResourceKind::Mot, 0xfe));
    encoding.transmissions = resources
        .into_iter()
        .map(|resource| TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: resource.kind() == TeletextResourceKind::Mot
                || resource.logical_subpage().get() == 1,
        })
        .collect();
    encoding
}

pub(super) fn dense_encoding_policy(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    indexed_encoding_policy(false)
}

pub(super) fn sparse_encoding_policy(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    indexed_encoding_policy(true)
}

/// Expected private Unicode from the actual fixed native local DRCS reference.
pub(super) fn expected_drcs_unicode(service: &EncodedTeletextAuthoredService, page: u16) -> u16 {
    let (_, program) = service
        .declared()
        .semantic()
        .effective_program(channel_documents::address(page, 32))
        .unwrap();
    let reference = program
        .commands()
        .iter()
        .find_map(|command| match command {
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::Drcs { reference },
            } => Some(reference),
            _ => None,
        })
        .unwrap();
    let scope = if reference.table == TeletextDrcsTableScope::Normal {
        16
    } else {
        0
    };
    0xf000 + ((scope + u16::from(reference.subtable.get())) << 6) + u16::from(reference.slot.get())
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
            TeletextExportAssetScope::DrcsTtiSet { resource_names }
                if resource_names.len() == 1 =>
            {
                Some((asset.name.as_str(), resource_names[0].as_deref().unwrap()))
            }
            _ => None,
        })
        .unwrap()
}

fn assert_companion(flow: &Flow, index: usize) {
    let exported = &flow.exported[index];
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v4"
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
    assert_mode_map(records, true);
}

fn assert_mode_map(records: &[TeletextTtiRecord], defined: bool) {
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
        assert_eq!(mode, if defined && slot == 0 { 0 } else { 15 });
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
            matches!(
                asset.scope,
                TeletextExportAssetScope::DrcsTti { .. }
                    | TeletextExportAssetScope::DrcsTtiSet { .. }
            )
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
    seed_receipt(fixture, flow, body).await
}

pub(super) async fn seed_receipt(
    fixture: &Fixture,
    flow: &Flow,
    body: TeletextExportReceiptBody,
) -> TeletextDesignExportReceipt {
    let bytes = write_teletext_export_receipt_body(&body, policy().project).unwrap();
    let original = &flow.originals[0];
    // Fixture-only historical baseline, with exact original native assets and
    // the selected old carrier recipe. Production export remains the factory.
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

fn indexed_companion(
    flow: &Flow,
    index: usize,
) -> (&TeletextExportAssetMapping, &[Option<String>]) {
    flow.exported[index]
        .sidecar()
        .project
        .assets
        .iter()
        .find_map(|asset| match &asset.scope {
            TeletextExportAssetScope::DrcsTtiSet { resource_names } => {
                Some((asset, resource_names.as_slice()))
            }
            _ => None,
        })
        .unwrap()
}

fn groups(records: &[TeletextTtiRecord]) -> Vec<Vec<TeletextTtiRecord>> {
    let mut output = Vec::new();
    for record in records {
        if matches!(record, TeletextTtiRecord::PageNumber { .. }) {
            output.push(Vec::new());
        }
        output.last_mut().unwrap().push(record.clone());
    }
    output
}

fn assert_indexed_set(flow: &Flow, index: usize, sparse: bool) {
    let exported = &flow.exported[index];
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v4"
    );
    let (asset, names) = indexed_companion(flow, index);
    assert_eq!(names.len(), 2);
    assert_eq!(names[0].is_none(), sparse);
    assert!(names[1].is_some());
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == asset.name)
        .unwrap();
    assert_eq!(asset.bytes, file.bytes.len());
    assert_eq!(asset.sha256, hash(&file.bytes));
    let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let grouped = groups(document.records());
    assert_eq!(grouped.len(), 2);
    for (ordinal, (records, name)) in grouped.iter().zip(names).enumerate() {
        assert!(records.contains(&TeletextTtiRecord::PageNumber {
            page: 0x1c0,
            editor_ordinal: u64::try_from(ordinal).unwrap()
        }));
        assert!(records.contains(&TeletextTtiRecord::SubpageOrdinal(
            u64::try_from(ordinal).unwrap()
        )));
        assert_mode_map(records, name.is_some());
        if let Some(name) = name {
            let raw = exported
                .files()
                .iter()
                .find(|file| file.name == *name)
                .unwrap();
            let mapping = exported
                .sidecar()
                .project
                .assets
                .iter()
                .find(|asset| asset.name == *name)
                .unwrap();
            assert!(
                matches!(&mapping.scope, TeletextExportAssetScope::Resource { identity, physical_subcode }
                if identity.kind == TeletextResourceKindWire::Drcs && identity.page.magazine == 1
                && identity.page.page == 0xc0 && usize::from(identity.subpage) == ordinal
                && *physical_subcode == (0x0110 | u16::try_from(ordinal).unwrap()))
            );
            let resource = flow.saved[index]
                .candidate()
                .encoded()
                .unwrap()
                .pages()
                .iter()
                .find(|page| {
                    page.resource_header().is_some_and(|header| {
                        header.identity()
                            == indexed_resource(
                                TeletextResourceKind::Drcs,
                                0xc0,
                                u8::try_from(ordinal).unwrap(),
                            )
                    })
                })
                .unwrap();
            assert_eq!(raw.bytes, resource.data_units().concat());
            assert_eq!(mapping.sha256, hash(&raw.bytes));
            let cells = records
                .iter()
                .find_map(|record| match record {
                    TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
                    _ => None,
                })
                .unwrap();
            let pixels = if ordinal == 0 {
                PIXELS
            } else {
                MIRRORED_PIXELS
            };
            for (pair, row) in cells[..20].chunks_exact(2).zip(pixels) {
                assert_eq!(
                    pair,
                    [
                        0x40 | u8::try_from(row >> 6).unwrap(),
                        0x40 | u8::try_from(row & 63).unwrap()
                    ]
                );
            }
            assert_eq!(&cells[20..], &[b' '; 20]);
        } else {
            assert_eq!(records.len(), 3);
            assert!(!records
                .iter()
                .any(|record| matches!(record, TeletextTtiRecord::BaseRow { .. })));
        }
    }
    assert_native_banks(flow.saved[index].candidate().encoded().unwrap(), sparse);
}

fn assert_native_banks(service: &EncodedTeletextAuthoredService, sparse: bool) {
    let mut banks = service
        .pages()
        .iter()
        .filter_map(EncodedTeletextAuthoredPage::resource_header)
        .filter(|header| header.identity().kind() == TeletextResourceKind::Drcs)
        .map(|header| {
            assert_eq!(header.last_packet().get(), 1);
            assert_eq!(header.update_counter().get(), 1);
            assert_eq!(
                header.physical_subcode(),
                0x0110 | u16::from(header.identity().logical_subpage().get())
            );
            header.identity().logical_subpage().get()
        })
        .collect::<Vec<_>>();
    banks.sort_unstable();
    assert_eq!(banks, if sparse { vec![1] } else { vec![0, 1] });
    let mot = &service.declared().mots()[0];
    let normal = mot
        .links()
        .drcs_link(Some(TeletextMotPublicLinkIndex::new(1).unwrap()));
    assert_eq!(normal.extent().maximum().get(), u8::from(!sparse));
    assert_eq!(
        matches!(normal.target(), TeletextX27LinkTarget::Unspecified { .. }),
        sparse
    );
    assert!(service
        .pages()
        .iter()
        .filter_map(EncodedTeletextAuthoredPage::resource_header)
        .any(|header| {
            header.identity().kind() == TeletextResourceKind::Mot
                && header.physical_subcode() == 0x1510
        }));
}

fn assert_bitmap_preview(service: &EncodedTeletextAuthoredService, sparse: bool, theme: &str) {
    for page_number in [124, 125] {
        let bank = u8::from(sparse || page_number == 125);
        assert_eq!(
            expected_drcs_unicode(service, page_number),
            0xf400 + (u16::from(bank) << 6)
        );
        let owner = channel_documents::address(page_number, 32);
        let composition = service
            .pages()
            .iter()
            .find_map(|page| match page {
                EncodedTeletextAuthoredPage::NativeDisplay(page)
                    if page.composition().owner() == owner =>
                {
                    Some(page.composition())
                }
                EncodedTeletextAuthoredPage::CompiledDisplay(page)
                    if page.page().address() == owner =>
                {
                    Some(page.normalized().original())
                }
                _ => None,
            })
            .unwrap();
        let left = usize::from(composition.display_geometry().left_width());
        let raster = external_editor_probe::preview_page(service, page_number);
        let rows = if bank == 0 { PIXELS } else { MIRRORED_PIXELS };
        for column in [8, 10] {
            for (row, pixels) in rows.into_iter().enumerate() {
                for x in 0..12 {
                    let rgb = if pixels & (1 << (11 - x)) == 0 {
                        [0, 0, 0]
                    } else if theme == "day" {
                        [15, 0, 0]
                    } else {
                        [0, 0, 15]
                    };
                    assert!(
                        matches!(raster.pixel((left + column) * 12 + x, 10 + row),
                        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(value))) if value.get() == rgb),
                        "page {page_number} column {column} pixel {x},{row}"
                    );
                }
            }
        }
    }
}

async fn assert_rejected_indexed_set(fixture: &Fixture, flow: &Flow, sparse: bool) {
    let name = indexed_companion(flow, 0).0.name.clone();
    for tamper in 0..3 {
        let mut edited = request(flow, 2, 8, b'Q');
        if tamper == 0 {
            edited.files.retain(|file| file.name != name);
        } else {
            let file = edited
                .files
                .iter_mut()
                .find(|file| file.name == name)
                .unwrap();
            let document = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
            let mut grouped = groups(document.records());
            if tamper == 1 {
                let cells = grouped[1]
                    .iter_mut()
                    .find_map(|record| match record {
                        TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
                        _ => None,
                    })
                    .unwrap();
                cells[0] ^= 1;
            } else if sparse {
                grouped[0].push(TeletextTtiRecord::BaseRow {
                    row: 1,
                    cells: [b' '; 40],
                });
            } else {
                grouped.swap(0, 1);
            }
            let records = grouped.into_iter().flatten().collect();
            file.bytes = write_teletext_tti(
                &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
                policy().tti,
            )
            .unwrap();
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

async fn indexed_flow(sparse: bool) {
    let (response, encoding) = if sparse {
        (
            sparse_response as fn(&str, u16, u16) -> Value,
            sparse_encoding_policy as fn() -> _,
        )
    } else {
        (
            dense_response as fn(&str, u16, u16) -> Value,
            dense_encoding_policy as fn() -> _,
        )
    };
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        encoding,
    ))
    .await;
    for (index, theme) in [(0, "day"), (1, "night")] {
        assert_indexed_set(&flow, index, sparse);
        assert_bitmap_preview(
            flow.saved[index].candidate().encoded().unwrap(),
            sparse,
            theme,
        );
    }
    assert_rejected_indexed_set(&fixture, &flow, sparse).await;
    let edited = import(&fixture, request(&flow, 2, 8, b'Q')).await;
    assert_eq!(edited.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    let raster = assert_concept_preview(&fixture, &flow, &edited, 2).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        assert_concept_preview(&fixture, &flow, &edited, 2).await,
        raster
    );
    let accepted = publish_and_accept(&fixture, &flow, &edited).await;
    assert_resources(
        flow.saved[0].candidate().encoded().unwrap(),
        accepted.service(),
    );
    assert_native_banks(accepted.service(), sparse);
    assert_bitmap_preview(accepted.service(), sparse, "day");
    assert_bitmap_preview(flow.accepted[1].service(), sparse, "night");
    assert_eq!(
        external_editor_probe::preview_page(accepted.service(), 125),
        external_editor_probe::preview_page(flow.accepted[0].service(), 125)
    );
    fixture = fixture.reopen().await;
    let recovered = recover(&fixture, 1).await;
    generations::assert_same_output(&recovered, &accepted);
    assert_native_banks(recovered.service(), sparse);
    assert_bitmap_preview(recovered.service(), sparse, "day");
    generations::assert_same_output(&recover(&fixture, 2).await, &flow.accepted[1]);
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn indexed_dense_drcs_set_preview_publication_reopen_and_channel_isolation() {
    Box::pin(indexed_flow(false)).await;
}

#[tokio::test]
async fn indexed_sparse_drcs_set_null_group_preserves_native_absence_and_publication() {
    Box::pin(indexed_flow(true)).await;
}

pub(super) fn v3_body(flow: &Flow) -> TeletextExportReceiptBody {
    // Keep the unchanged original S1=0 fixture. A V3 singleton is not a V4 set
    // renamed: it used this native asset's basename and immediate file order.
    let mut body = v2_body(flow);
    "qteletextmaker-1.0-carrier-v3".clone_into(&mut body.project.editor_profile);
    let raw_index = body
        .project
        .assets
        .iter()
        .position(|asset| {
            matches!(
                &asset.scope, TeletextExportAssetScope::Resource { identity, .. }
                if identity.kind == TeletextResourceKindWire::Drcs && identity.subpage == 0
            )
        })
        .unwrap();
    let raw_name = body.project.assets[raw_index].name.clone();
    let native = flow.saved[0]
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
    let projected = project_encoded_teletext_drcs(native, policy().project.max_units).unwrap();
    assert_eq!(projected.header.controls.raw(), 0xf9);
    let mut records = vec![
        TeletextTtiRecord::PageNumber {
            page: 0x1c0,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(0),
        // Literal original resource PS: erase/update/suppression/serial, no NOS.
        TeletextTtiRecord::PageStatus(0xc07c),
    ];
    for record in projected.records {
        match record {
            TeletextEncodedDisplayRecord::HeaderText(_) => {}
            TeletextEncodedDisplayRecord::BaseRow { row, cells } => {
                records.push(TeletextTtiRecord::BaseRow { row, cells });
            }
            TeletextEncodedDisplayRecord::Triplets {
                row,
                designation,
                words,
            } => records.push(TeletextTtiRecord::Packet {
                row,
                designation,
                words,
            }),
            TeletextEncodedDisplayRecord::EditorialLinks { .. } => {
                panic!("DRCS has no editorial link packet")
            }
        }
    }
    let bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    let name = format!("{}.tti", raw_name.trim_end_matches(".dvb"));
    body.project.assets.insert(
        raw_index + 1,
        TeletextExportAssetMapping {
            name: name.clone(),
            scope: TeletextExportAssetScope::DrcsTti {
                resource_name: raw_name.clone(),
                editor_ordinal: 0,
            },
            unit_count: 0,
            bytes: bytes.len(),
            sha256: hash(&bytes),
        },
    );
    let raw_file_index = body
        .files
        .iter()
        .position(|file| file.name == raw_name)
        .unwrap();
    body.files
        .insert(raw_file_index + 1, TeletextExportFile { name, bytes });
    body
}

#[tokio::test]
async fn retained_v3_singleton_drcs_receipt_recovers_after_v4_default() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        response,
        Fixture::new_persistent().await,
        encoding_policy,
    ))
    .await;
    let retained = seed_receipt(&fixture, &flow, v3_body(&flow)).await;
    let mut request = v2_request(&retained);
    request.name = "Synthetic retained V3 singleton DRCS concept".to_owned();
    let edited = import(&fixture, request).await;
    assert_original_pointers(&fixture, &flow).await;
    let raster = assert_concept_preview(&fixture, &flow, &edited, 2).await;
    fixture = fixture.reopen().await;
    assert_eq!(
        assert_concept_preview(&fixture, &flow, &edited, 2).await,
        raster
    );
    assert_eq!(
        super::qtm_carrier::stored_receipt(&fixture, retained.metadata.id).await,
        retained
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
        super::qtm_carrier::stored_receipt(&fixture, retained.metadata.id).await,
        retained
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
