//! Every generated full identity has its own protected indexed group and link.

use super::*;
use ersatzrs_addon_contract::TeletextResourceKindWire;

#[derive(Clone, Copy, Debug)]
enum Change {
    Bitmap,
    Modes,
    Target,
    RequiredSubpage,
}

fn bank_name(
    exported: &ExportedOriginalTeletextDesign,
    identity: TeletextResourceIdentity,
) -> &str {
    assert_eq!(identity.kind(), TeletextResourceKind::Drcs);
    assert_eq!(identity.logical_subpage().get(), 1);
    let raw = exported
        .sidecar()
        .project
        .assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope, TeletextExportAssetScope::Resource {identity:wire,..}
            if wire.kind == TeletextResourceKindWire::Drcs
                && wire.page.magazine == identity.page().magazine()
                && wire.page.page == identity.page().page()
                && wire.subpage == identity.logical_subpage().get())
        })
        .unwrap();
    exported.sidecar().project.assets.iter().find(|asset| {
        matches!(&asset.scope, TeletextExportAssetScope::DrcsTtiSet {resource_names}
            if resource_names.get(usize::from(identity.logical_subpage().get())) == Some(&Some(raw.name.clone())))
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

fn change_bank(
    request: &mut NativeTeletextEditorialImportRequest,
    exported: &ExportedOriginalTeletextDesign,
    identity: TeletextResourceIdentity,
    change: Change,
) {
    rewrite(request, bank_name(exported, identity), |records| {
        let mut selected = false;
        let mut changed = false;
        for record in records {
            match record {
                TeletextTtiRecord::PageNumber {
                    page,
                    editor_ordinal,
                } => {
                    assert_eq!(
                        *page,
                        (u16::from(identity.page().magazine()) << 8)
                            | u16::from(identity.page().page())
                    );
                    selected = *editor_ordinal == u64::from(identity.logical_subpage().get());
                }
                TeletextTtiRecord::BaseRow { row: 1, cells }
                    if selected && matches!(change, Change::Bitmap) =>
                {
                    let mask: [u16; 10] = std::array::from_fn(|row| {
                        (u16::from(cells[row * 2] & 63) << 6) | u16::from(cells[row * 2 + 1] & 63)
                    });
                    assert_eq!(mask, MASK);
                    assert!(cells[..20].iter().all(|cell| cell & 0x40 != 0));
                    cells[1] ^= 1;
                    let mut replacement = MASK;
                    replacement[0] ^= 1;
                    TeletextDrcsBitmap::new(replacement).unwrap();
                    assert_eq!(
                        (u16::from(cells[0] & 63) << 6) | u16::from(cells[1] & 63),
                        replacement[0]
                    );
                    assert!(!changed);
                    changed = true;
                }
                TeletextTtiRecord::Packet {
                    row: 28,
                    designation: 3,
                    words,
                } if selected && matches!(change, Change::Modes) => {
                    assert_eq!(words[0], 5);
                    assert_eq!(words[1] & 15, 0);
                    words[1] ^= 1;
                    assert_eq!(words[1] & 15, 1);
                    assert!(!changed);
                    changed = true;
                }
                _ => {}
            }
        }
        assert!(changed, "the exact generated S1 group must be changed");
    });
}

fn change_link(
    request: &mut NativeTeletextEditorialImportRequest,
    service: &EncodedTeletextAuthoredService,
    owner: u16,
    identity: TeletextResourceIdentity,
    change: Change,
) {
    let resource = service
        .compiler_drcs_resources()
        .iter()
        .find(|resource| resource.table().identity() == identity)
        .unwrap();
    assert_eq!(resource.consumers(), [address(owner)]);
    let links = resource.compositional_links(address(owner)).unwrap();
    let role = TeletextX27LinkRole::Drcs;
    let old = links.link(role);
    assert_eq!(
        old.target(),
        TeletextX27LinkTarget::Page {
            page: identity.page()
        }
    );
    assert_eq!(old.required_subpages().get(), 3);
    let target = if matches!(change, Change::Target) {
        TeletextRawDataPageNumber::new(identity.page().magazine(), identity.page().page() ^ 2)
            .unwrap()
    } else {
        identity.page()
    };
    let mask = if matches!(change, Change::RequiredSubpage) {
        1
    } else {
        3
    };
    let replacement = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page { page: target },
        old.validity(),
        TeletextSubpageRequirementMask::new(mask),
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
        selected(role),
    )
    .unwrap();
    rewrite(request, &format!("page-{owner}-0020.tti"), |records| {
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
        assert_eq!(
            (words[index] >> 6) & 15,
            u32::from(identity.page().page() & 15)
        );
        assert_eq!(words[index + 1], 3 << 2);
        if matches!(change, Change::Target) {
            words[index] ^= 1 << 7;
        } else {
            words[index + 1] ^= 2 << 2;
        }
        assert_eq!((words[index] >> 6) & 15, u32::from(target.page() & 15));
        assert_eq!(words[index + 1], u32::from(mask) << 2);
    });
}

async fn reject(
    fixture: &Fixture,
    request: NativeTeletextEditorialImportRequest,
    owner: u16,
    identity: TeletextResourceIdentity,
    change: Change,
) {
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    let NativeTeletextEditorialImportError::Tti(error) = error else {
        panic!("generated identity {identity:?}, {change:?} must fail reconciliation: {error:?}");
    };
    assert_eq!(
        error.failure,
        if matches!(change, Change::Bitmap) {
            NativeTeletextEditorialTtiFailure::Structure
        } else {
            NativeTeletextEditorialTtiFailure::Packet
        }
    );
    if matches!(change, Change::Bitmap | Change::Modes) {
        assert_eq!(error.resource, Some(identity));
        assert_eq!(
            error.page,
            (u16::from(identity.page().magazine()) << 8) | u16::from(identity.page().page())
        );
        assert_eq!(error.subcode, u16::from(identity.logical_subpage().get()));
        assert_eq!(
            error.row,
            Some(if matches!(change, Change::Bitmap) {
                1
            } else {
                28
            })
        );
        if matches!(change, Change::Modes) {
            assert_eq!(error.designation, Some(3));
        }
    } else {
        assert_eq!(error.resource, None);
        assert_eq!((error.page, error.subcode), (owner, 32));
        assert_eq!(error.row, Some(27));
        assert_eq!(error.designation, Some(4));
    }
}

async fn assert_retained(
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
    oracle::assert_service(
        flow,
        drcs_bitmap::restored(fixture, concept)
            .await
            .encoded()
            .unwrap(),
    );
}

#[tokio::test]
async fn three_linked_targets_reject_each_generated_identity_bitmap_modes_and_links() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        input::response,
        Fixture::new_persistent().await,
        input::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    oracle::assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, prepared_request(&flow)).await;
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    oracle::assert_service(&flow, day.service());
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    oracle::assert_mapping(&exported);
    for owner in 124..127 {
        let identity = normal_identity(owner, 1);
        for change in [
            Change::Bitmap,
            Change::Modes,
            Change::Target,
            Change::RequiredSubpage,
        ] {
            let mut request = drcs_bitmap::from_receipt(&fresh, 2);
            let sidecar = request.sidecar.clone();
            if matches!(change, Change::Bitmap | Change::Modes) {
                change_bank(&mut request, &exported, identity, change);
            } else {
                change_link(&mut request, day.service(), owner, identity, change);
            }
            assert_eq!(request.sidecar, sidecar);
            reject(&fixture, request, owner, identity, change).await;
            Box::pin(assert_retained(
                &fixture, &flow, &retained, &concept, &day, &fresh,
            ))
            .await;
        }
    }
    fixture = fixture.reopen().await;
    Box::pin(assert_retained(
        &fixture, &flow, &retained, &concept, &day, &fresh,
    ))
    .await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
