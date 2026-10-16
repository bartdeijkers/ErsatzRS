//! Prepared native Defaults[0,2,3] ownership, not editor/stream/client proof.

use super::*;

#[path = "defaults/oracle.rs"]
mod checks;
#[path = "defaults/fixture.rs"]
mod seed;

fn protected_file(
    exported: &ExportedOriginalTeletextDesign,
    kind: TeletextResourceKindWire,
    raw: u8,
) -> &str {
    exported
        .sidecar()
        .project
        .assets
        .iter()
        .find_map(|asset| match &asset.scope {
            TeletextExportAssetScope::Resource { identity, .. }
                if identity.kind == kind
                    && identity.page.magazine == 2
                    && identity.page.page == raw
                    && identity.subpage == 0 =>
            {
                Some(asset.name.as_str())
            }
            _ => None,
        })
        .unwrap()
}

pub(in super::super) fn assert_protected_reexport(
    flow: &Flow,
    exported: &ExportedOriginalTeletextDesign,
) {
    let original = &flow.exported[0];
    let mut names = vec![(mot_file(original), mot_file(exported))];
    for (kind, raw) in [
        (TeletextResourceKindWire::Pop, 0xc0),
        (TeletextResourceKindWire::Gpop, 0xd0),
    ] {
        names.push((
            protected_file(original, kind, raw),
            protected_file(exported, kind, raw),
        ));
    }
    for (before, after) in names {
        let before = original
            .files()
            .iter()
            .find(|file| file.name == before)
            .unwrap();
        let after = exported
            .files()
            .iter()
            .find(|file| file.name == after)
            .unwrap();
        assert_eq!(after.bytes, before.bytes);
    }
}

async fn reject_object_header(
    fixture: &Fixture,
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
    kind: TeletextResourceKindWire,
    raw: u8,
) {
    let mut request = drcs_bitmap::from_receipt(receipt, 2);
    let name = protected_file(exported, kind, raw);
    let before = request.files.clone();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    assert_eq!(file.bytes.len() % 46, 0);
    assert_eq!(&file.bytes[..2], &[2, 44]);
    assert_eq!(file.bytes[3], 0xe4);
    // Literal Hamming8/4 MRAG nibbles 2/0 authenticate magazine2/header row0.
    assert_eq!(
        (file.bytes[4].reverse_bits(), file.bytes[5].reverse_bits()),
        (0x49, 0x15)
    );
    // First header text byte, after eight Hamming-coded page/control nibbles.
    // Both values have odd parity before on-wire reversal; MRAG/identity and
    // object bodies remain byte exact. This is a protected RAW-header test,
    // not a semantic default-object mutation or a malformed-packet negative.
    let offset = 14;
    assert_eq!(file.bytes[offset].reverse_bits(), b' ');
    file.bytes[offset] = 0xc1_u8.reverse_bits();
    assert_eq!(file.bytes[offset].count_ones() % 2, 1);
    assert_eq!(before.len(), request.files.len());
    for (old, new) in before.iter().zip(&request.files) {
        assert_eq!(old.name, new.name);
        if old.name == name {
            assert_eq!(old.bytes.len(), new.bytes.len());
            assert_eq!(
                old.bytes
                    .iter()
                    .zip(&new.bytes)
                    .enumerate()
                    .filter_map(|(index, (a, b))| (a != b).then_some(index))
                    .collect::<Vec<_>>(),
                vec![offset]
            );
        } else {
            assert_eq!(old, new);
        }
    }
    assert_eq!(
        request.sidecar,
        write_teletext_export_sidecar(exported.sidecar(), policy().project).unwrap()
    );
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, NativeTeletextEditorialImportError::Association),
        "{error:?}"
    );
}

async fn assert_state(
    fixture: &Fixture,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
    concept: &SavedEditorialTeletextDesign,
    day: &AcceptedTeletextOutputCandidate,
    fresh: &TeletextDesignExportReceipt,
) {
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
    retained.assert_unchanged(fixture, flow).await;
    assert_eq!(
        qtm_carrier::stored_receipt(fixture, fresh.metadata.id).await,
        *fresh
    );
    checks::assert_effective(flow, recover(fixture, 1).await.service());
    checks::assert_effective(
        flow,
        drcs_bitmap::restored(fixture, concept)
            .await
            .encoded()
            .unwrap(),
    );
}

async fn rejections(
    fixture: &Fixture,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
    concept: &SavedEditorialTeletextDesign,
    day: &AcceptedTeletextOutputCandidate,
    exported: &ExportedOriginalTeletextDesign,
    fresh: &TeletextDesignExportReceipt,
) {
    for (kind, raw) in [
        (TeletextResourceKindWire::Pop, 0xc0),
        (TeletextResourceKindWire::Gpop, 0xd0),
    ] {
        reject_object_header(fixture, exported, fresh, kind, raw).await;
        Box::pin(assert_state(fixture, flow, retained, concept, day, fresh)).await;
    }
    reject_mot(fixture, exported, fresh).await;
    Box::pin(assert_state(fixture, flow, retained, concept, day, fresh)).await;
    reject_generated(fixture, exported, fresh).await;
    Box::pin(assert_state(fixture, flow, retained, concept, day, fresh)).await;
}

#[tokio::test]
async fn implicit_mot_rendered_defaults_previews_publishes_protects_resources_and_reopens() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        seed::response,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    checks::assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, prepared_request(&flow)).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let previews = Box::pin(assert_restored(&fixture, &flow, &concept)).await;
    checks::assert_effective(
        &flow,
        drcs_bitmap::restored(&fixture, &concept)
            .await
            .encoded()
            .unwrap(),
    );
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(
        Box::pin(assert_restored(&fixture, &flow, &concept)).await,
        previews
    );
    checks::assert_effective(
        &flow,
        drcs_bitmap::restored(&fixture, &concept)
            .await
            .encoded()
            .unwrap(),
    );
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    checks::assert_effective(&flow, day.service());
    assert_eq!(oracle::previews(day.service(), 0x5b), previews);
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    oracle::assert_mapping(&exported);
    assert_protected_reexport(&flow, &exported);
    Box::pin(rejections(
        &fixture, &flow, &retained, &concept, &day, &exported, &fresh,
    ))
    .await;
    fixture = fixture.reopen().await;
    Box::pin(assert_state(
        &fixture, &flow, &retained, &concept, &day, &fresh,
    ))
    .await;
    assert_eq!(
        oracle::previews(recover(&fixture, 1).await.service(), 0x5b),
        previews
    );
    assert_eq!(
        oracle::previews(recover(&fixture, 2).await.service(), b'A'),
        oracle::previews(flow.accepted[1].service(), b'A')
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

pub(in super::super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    seed::response(theme, page, subcode)
}

pub(in super::super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    seed::encoding()
}

pub(in super::super) fn assert_originals(flow: &Flow) {
    checks::assert_originals(flow);
}

pub(in super::super) fn assert_effective(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    checks::assert_effective(flow, service);
}
