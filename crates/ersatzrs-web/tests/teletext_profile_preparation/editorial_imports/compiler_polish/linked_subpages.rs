//! Prepared native flow for mixed authored/compiler groups in one indexed bank.
//! Actual editor saves, ordinary delivery and client proof remain separate.

use super::*;
use ersatzrs_ffmpeg::teletext::{TeletextResourceFullUpdate, TeletextServiceResourceTransmission};

fn identity(kind: TeletextResourceKind, magazine: u8, page: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(magazine, page).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap()
}

fn bank_ids() -> [TeletextResourceIdentity; 2] {
    [
        identity(TeletextResourceKind::Drcs, 2, 0xa0),
        identity(TeletextResourceKind::Gdrcs, 1, 0xa1),
    ]
}

fn occupied_response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = response(theme, page, subcode);
    value["service"]["drcs"] = json!(bank_ids().map(|identity| json!({
        "identity":{"kind":identity.kind(),"page":identity.page(),"subpage":0},
        "glyphs":[{"slot":0,"bitmap":[0,0,0,0,0,0,0,0,0,0]}]
    })));
    let unused = json!({"target":{"kind":"unspecified","magazine":1},"validity":"level25_only","required_subpages":65535});
    let selected = |identity: TeletextResourceIdentity| json!({"target":{"kind":"page","page":identity.page()},"validity":"level25_only","required_subpages":1});
    for enhancement in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        enhancement["resources"]["drcs"] =
            json!({"normal":bank_ids()[0].page(),"global":bank_ids()[1].page()});
        enhancement["compositional_links"] = json!({"pop":unused,"gpop":unused,"drcs":selected(bank_ids()[0]),"gdrcs":selected(bank_ids()[1])});
    }
    value
}

fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = channel_documents::policy();
    policy.transmissions = bank_ids()
        .map(|resource| TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .into();
    policy
}

fn assert_service(flow: &Flow, service: &EncodedTeletextAuthoredService, national: bool) {
    let expected = flow.accepted[0]
        .service()
        .declared()
        .clone()
        .with_physical_base_overlays(
            vec![expected_overlay(flow, if national { 0x5b } else { b'A' })],
            channel_documents::policy().validation.service,
        )
        .unwrap();
    assert_eq!(service.declared(), &expected);
    assert_eq!(
        service.compiler_drcs_resources().len(),
        usize::from(national)
    );
    for old in flow.accepted[0]
        .service()
        .pages()
        .iter()
        .filter(|page| page.resource_header().is_some())
    {
        let identity = old.resource_header().unwrap().identity();
        let current = service
            .pages()
            .iter()
            .find(|page| {
                page.resource_header()
                    .is_some_and(|header| header.identity() == identity)
            })
            .unwrap();
        assert_eq!(current.data_units(), old.data_units());
    }
    if national {
        let generated = &service.compiler_drcs_resources()[0];
        assert_eq!(generated.table().identity().page(), bank_ids()[0].page());
        assert_eq!(generated.table().identity().logical_subpage().get(), 1);
        assert_eq!(generated.table().glyphs()[0].bitmap.get(), MASK);
        let links = generated
            .compositional_links(channel_documents::address(124, 32))
            .unwrap();
        assert_eq!(
            links
                .link(TeletextX27LinkRole::Drcs)
                .required_subpages()
                .get(),
            3
        );
        assert_eq!(
            links
                .link(TeletextX27LinkRole::Gdrcs)
                .required_subpages()
                .get(),
            1
        );
    }
}

#[tokio::test]
async fn occupied_links_native_concept_exports_mixed_bank_previews_publishes_and_reopens() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy(
        occupied_response,
        Fixture::new_persistent().await,
        encoding,
    ))
    .await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    assert!(flow
        .accepted
        .iter()
        .all(|candidate| candidate.service().compiler_drcs_resources().is_empty()));
    let concept = import(&fixture, request(&flow, 1, 7, 0x5b)).await;
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let restored = drcs_bitmap::restored(&fixture, &concept).await;
    assert_service(&flow, restored.encoded().unwrap(), true);
    let preview = external_editor_probe::preview_page(restored.encoded().unwrap(), 124);
    let accepted = publish_and_accept(&fixture, &flow, &concept).await;
    assert_service(&flow, accepted.service(), true);
    assert_eq!(
        external_editor_probe::preview_page(accepted.service(), 124),
        preview
    );
    fixture = fixture.reopen().await;
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    let asset = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| asset.name == "drcs-set-normal-2a0.tti")
        .unwrap();
    let TeletextExportAssetScope::DrcsTtiSet { resource_names } = &asset.scope else {
        panic!("actual mixed bank mapping")
    };
    assert_eq!(resource_names.len(), 2);
    for (subpage, name) in resource_names.iter().enumerate() {
        let name = name.as_ref().unwrap();
        let raw = exported
            .sidecar()
            .project
            .assets
            .iter()
            .find(|asset| asset.name == *name)
            .unwrap();
        let scope = serde_json::to_value(&raw.scope).unwrap();
        assert_eq!(scope["identity"]["kind"], "drcs");
        assert_eq!(scope["identity"]["page"], json!({"magazine":2,"page":160}));
        assert_eq!(scope["identity"]["subpage"], subpage);
    }
    let file = exported
        .files()
        .iter()
        .find(|file| file.name == asset.name)
        .unwrap();
    let tti = parse_teletext_tti(&file.bytes, policy().tti).unwrap();
    let ordinals: Vec<_> = tti
        .records()
        .iter()
        .filter_map(|record| match record {
            TeletextTtiRecord::SubpageOrdinal(ordinal) => Some(*ordinal),
            _ => None,
        })
        .collect();
    assert_eq!(ordinals, vec![0, 1]);
    // A new concept may retire S1; imported old mixed groups remain receipt-
    // authenticated, and never turn into new addon-authored definitions.
    let reverted = import(&fixture, revert_request(&fresh)).await;
    assert_eq!(reverted.head().revision, 3);
    assert_service(
        &flow,
        drcs_bitmap::restored(&fixture, &reverted)
            .await
            .encoded()
            .unwrap(),
        false,
    );
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    assert_eq!(
        qtm_carrier::stored_receipt(&fixture, fresh.metadata.id).await,
        fresh
    );
    fixture = fixture.reopen().await;
    assert_service(&flow, recover(&fixture, 1).await.service(), true);
    assert_pointers(&fixture, &accepted, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
