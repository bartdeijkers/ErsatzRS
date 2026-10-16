//! Valid resource/packet changes must not borrow the Base-cell edit authority.

use super::*;
use ersatzrs_addon_contract::TeletextResourceKindWire;
use ersatzrs_app::teletext_export_project::TeletextExportAssetScope;
use ersatzrs_web::host::teletext_editorial_designs::{
    NativeTeletextEditorialImportError, NativeTeletextEditorialTtiFailure,
};

fn bank_name(
    exported: &ExportedOriginalTeletextDesign,
    resource: TeletextResourceIdentity,
) -> &str {
    let kind = match resource.kind() {
        TeletextResourceKind::Drcs => TeletextResourceKindWire::Drcs,
        TeletextResourceKind::Gdrcs => TeletextResourceKindWire::Gdrcs,
        _ => panic!("this fixture has only Normal and Global DRCS banks"),
    };
    let raw = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope, TeletextExportAssetScope::Resource {identity,..}
            if identity.kind==kind && identity.page.magazine==resource.page().magazine()
            && identity.page.page==resource.page().page()
            && identity.subpage==resource.logical_subpage().get())
        })
        .unwrap();
    exported.sidecar().project.assets.iter().find(|asset| {
        matches!(&asset.scope, TeletextExportAssetScope::DrcsTtiSet {resource_names}
            if resource_names.get(usize::from(resource.logical_subpage().get()))==Some(&Some(raw.name.clone())))
    }).unwrap().name.as_str()
}

fn rewrite(
    request: &mut NativeTeletextEditorialImportRequest,
    name: &str,
    modify: impl FnOnce(&mut [TeletextTtiRecord]),
) {
    let before = request.files.clone();
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
        &TeletextTtiDocument::checked_from_records(records.clone(), policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(
        parse_teletext_tti(&file.bytes, policy().tti)
            .unwrap()
            .records(),
        records
    );
    assert_eq!(before.len(), request.files.len());
    for (old, new) in before.iter().zip(&request.files) {
        assert_eq!(old.name, new.name);
        if old.name == name {
            assert_ne!(old.bytes, new.bytes);
        } else {
            assert_eq!(old, new);
        }
    }
}

fn change_bitmap(
    request: &mut NativeTeletextEditorialImportRequest,
    exported: &ExportedOriginalTeletextDesign,
    resource: TeletextResourceIdentity,
    slot: u8,
    expected: [u16; 10],
) {
    rewrite(request, bank_name(exported, resource), |records| {
        let mut group = None;
        let mut ordinal = 0_u8;
        let mut changed = false;
        for record in records {
            if matches!(record, TeletextTtiRecord::PageNumber { .. }) {
                group = Some(ordinal);
                ordinal += 1;
            }
            if group != Some(resource.logical_subpage().get()) {
                continue;
            }
            let TeletextTtiRecord::BaseRow { row, cells } = record else {
                continue;
            };
            if *row != slot / 2 + 1 {
                continue;
            }
            let offset = usize::from(slot % 2) * 20;
            let bitmap: [u16; 10] = std::array::from_fn(|row| {
                (u16::from(cells[offset + row * 2] & 63) << 6)
                    | u16::from(cells[offset + row * 2 + 1] & 63)
            });
            assert_eq!(bitmap, expected);
            cells[offset + 1] ^= 1;
            let mut replacement = expected;
            replacement[0] ^= 1;
            TeletextDrcsBitmap::new(replacement).unwrap();
            assert_eq!(
                replacement[0],
                (u16::from(cells[offset] & 63) << 6) | u16::from(cells[offset + 1] & 63)
            );
            assert!(!changed);
            changed = true;
        }
        assert!(changed, "the actual receipted S1/slot must be changed");
    });
}

fn change_link(
    request: &mut NativeTeletextEditorialImportRequest,
    service: &EncodedTeletextAuthoredService,
    kind: TeletextResourceKind,
) {
    let page = if kind == TeletextResourceKind::Drcs {
        124
    } else {
        128
    };
    let role = if kind == TeletextResourceKind::Drcs {
        TeletextX27LinkRole::Drcs
    } else {
        TeletextX27LinkRole::Gdrcs
    };
    let resource = service
        .compiler_drcs_resources()
        .iter()
        .find(|resource| resource.table().identity().kind() == kind)
        .unwrap();
    let links = resource.compositional_links(address(page)).unwrap();
    let old = links.link(role);
    let TeletextX27LinkTarget::Page { page: target } = old.target() else {
        panic!("generated link must select its real bank")
    };
    let replacement = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: TeletextRawDataPageNumber::new(target.magazine(), target.page() ^ 2).unwrap(),
        },
        old.validity(),
        old.required_subpages(),
    )
    .unwrap();
    let selected = |candidate| {
        if candidate == role {
            replacement
        } else {
            links.link(candidate)
        }
    };
    TeletextX27Level25Links::new(
        selected(TeletextX27LinkRole::Gpop),
        selected(TeletextX27LinkRole::Pop),
        selected(TeletextX27LinkRole::Gdrcs),
        selected(TeletextX27LinkRole::Drcs),
    )
    .unwrap();
    rewrite(request, &format!("page-{page}-0020.tti"), |records| {
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
        let index = usize::from(role.wire_code()) * 2;
        assert_eq!((words[index] >> 6) & 15, u32::from(target.page() & 15));
        words[index] ^= 1 << 7;
        assert_eq!(
            (words[index] >> 6) & 15,
            u32::from((target.page() ^ 2) & 15)
        );
    });
}

async fn reject(
    fixture: &Fixture,
    request: NativeTeletextEditorialImportRequest,
) -> NativeTeletextEditorialImportError {
    import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err()
}

async fn assert_head(fixture: &Fixture, expected: &ersatzrs_domain::TeletextDesignHead) {
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(expected.id)
            .await
            .unwrap()
            .unwrap(),
        *expected
    );
}

#[tokio::test]
async fn dual_scope_rejects_both_generated_bank_bitmaps_and_links_after_reexport() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        scope_fixture::response,
        Fixture::new_persistent().await,
        scope_fixture::encoding_policy,
        authoring_metadata::write_compact,
    ))
    .await;
    oracle::assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, prepared_request(&flow)).await;
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_concept(&flow, day.service());
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    assert!(matches!(
        reject(&fixture, drcs_bitmap::from_receipt(&fresh, 2)).await,
        NativeTeletextEditorialImportError::NoChanges
    ));
    for kind in [TeletextResourceKind::Drcs, TeletextResourceKind::Gdrcs] {
        for bitmap in [true, false] {
            let mut request = drcs_bitmap::from_receipt(&fresh, 2);
            if bitmap {
                let table = day
                    .service()
                    .compiler_drcs_resources()
                    .iter()
                    .find(|resource| resource.table().identity().kind() == kind)
                    .unwrap()
                    .table();
                let glyph = table
                    .glyphs()
                    .iter()
                    .find(|glyph| glyph.slot.get() == 0)
                    .unwrap();
                assert_eq!(glyph.bitmap.get(), oracle::mask(124, true));
                change_bitmap(
                    &mut request,
                    &exported,
                    table.identity(),
                    0,
                    oracle::mask(124, true),
                );
            } else {
                change_link(&mut request, day.service(), kind);
            }
            let error = reject(&fixture, request).await;
            let NativeTeletextEditorialImportError::Tti(error) = error else {
                panic!("native protected bank reconciliation must reject: {error:?}")
            };
            assert_eq!(
                error.failure,
                if bitmap {
                    NativeTeletextEditorialTtiFailure::Structure
                } else {
                    NativeTeletextEditorialTtiFailure::Packet
                }
            );
            assert_head(&fixture, concept.head()).await;
            compiler_polish::assert_pointers(&fixture, &day, &flow.accepted[1]).await;
            retained.assert_unchanged(&fixture, &flow).await;
            assert_eq!(
                qtm_carrier::stored_receipt(&fixture, fresh.metadata.id).await,
                fresh
            );
        }
    }
    fixture = fixture.reopen().await;
    assert_head(&fixture, concept.head()).await;
    compiler_polish::assert_pointers(&fixture, &day, &flow.accepted[1]).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert_eq!(
        qtm_carrier::stored_receipt(&fixture, fresh.metadata.id).await,
        fresh
    );
    assert_concept(&flow, recover(&fixture, 1).await.service());
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
