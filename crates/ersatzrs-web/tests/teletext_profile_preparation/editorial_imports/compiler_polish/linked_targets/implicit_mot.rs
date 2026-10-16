//! Prepared three-owner editorial flow with authenticated implicit MOT roles.
//! No actual editor, browser, ordinary delivery or client claim.

use super::*;
use ersatzrs_addon_contract::TeletextResourceKindWire;

fn mot_identity() -> TeletextResourceIdentity {
    // All three displays are in magazine1; MOT2FE cannot own these addresses.
    TeletextResourceIdentity::new(
        TeletextResourceKind::Mot,
        TeletextRawDataPageNumber::new(1, 0xfe).unwrap(),
        TeletextResourceSubpageIndex::new(9).unwrap(),
    )
    .unwrap()
}

fn mot() -> TeletextMotLevel25Declaration {
    let unused = TeletextX27LinkTarget::Unspecified {
        magazine: TeletextX27Magazine::new(1).unwrap(),
    };
    let objects = TeletextMotObjectLink::new(
        unused,
        TeletextMotSubpageExtent::new(0).unwrap(),
        true,
        TeletextMotFallback::new(0).unwrap(),
        [TeletextMotDefaultObject::NoObject { raw_number: 77 }; 2],
    );
    let inactive =
        TeletextMotDrcsLink::new(unused, TeletextMotSubpageExtent::new(0).unwrap(), true);
    let linked = |identity: TeletextResourceIdentity| {
        TeletextMotDrcsLink::new(
            TeletextX27LinkTarget::Page {
                page: identity.page(),
            },
            TeletextMotSubpageExtent::new(0).unwrap(),
            false,
        )
    };
    let mut normal = [inactive; 7];
    for page in 124..127 {
        normal[usize::from(page - 124)] = linked(normal_identity(page, 0));
    }
    let associations = (124..127)
        .map(|page| {
            TeletextMotPageAssociation::new(
                address(page).page.wire_page(),
                TeletextMotAssociationSelection::new(false, None),
                TeletextMotAssociationSelection::new(
                    true,
                    Some(
                        TeletextMotPublicLinkIndex::new(u8::try_from(page - 123).unwrap()).unwrap(),
                    ),
                ),
            )
        })
        .collect();
    TeletextMotLevel25Declaration::new(
        mot_identity(),
        associations,
        TeletextMotLevel25Links::new(
            objects,
            [objects; 7],
            linked(authored_identities()[3]),
            normal,
            TeletextMotEnhancementCounts::new(0, 0, 0),
        )
        .unwrap(),
    )
    .unwrap()
}

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = input::response(theme, page, subcode);
    for page in value["service"]["page_enhancements"]
        .as_array_mut()
        .unwrap()
    {
        page["compositional_links"] = Value::Null;
    }
    let identity = json!({"kind":"mot","page":mot_identity().page(),"subpage":9});
    let mut declaration = serde_json::to_value(mot()).unwrap();
    declaration["identity"] = identity.clone();
    value["service"]["mots"] = json!([declaration]);
    for selection in value["service"]["mot_selections"].as_array_mut().unwrap() {
        selection["mot"] = identity.clone();
    }
    let bytes = serde_json::to_vec(&value).unwrap();
    assert!(
        bytes.len() <= encoding().input.max_response_bytes,
        "{}",
        bytes.len()
    );
    value
}

pub(super) fn encoding(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    let mut policy = input::encoding();
    assert_eq!(policy.transmissions.len(), 4);
    policy
        .transmissions
        .push(TeletextServiceResourceTransmission {
            resource: mot_identity(),
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        });
    policy
}

pub(super) fn assert_originals(flow: &Flow) {
    for accepted in &flow.accepted {
        let service = accepted.service();
        let declared = service.declared();
        assert_eq!(declared.semantic().base().pages.len(), 3);
        assert_eq!(declared.resource_inventory().len(), 5);
        assert_eq!(declared.mots(), &[mot()]);
        assert!(declared.defaults().is_empty());
        assert!(service.compiler_drcs_resources().is_empty());
        for identity in authored_identities() {
            let table = declared
                .semantic()
                .drcs()
                .subtables()
                .iter()
                .find(|table| table.identity() == identity)
                .unwrap();
            assert_eq!(table.glyphs().len(), 1);
            assert_eq!(table.glyphs()[0].slot.get(), 0);
            assert_eq!(table.glyphs()[0].bitmap.get(), [0; 10]);
        }
        for page in 124..127 {
            let metadata = &declared.semantic().page_enhancements()[&address(page)];
            assert!(metadata.compositional_links.is_none());
            assert_eq!(metadata.resources.defaults, [None; 4]);
            let selection = declared
                .selections()
                .iter()
                .find(|selection| selection.display == address(page))
                .unwrap();
            assert_eq!(selection.mot, Some(mot_identity()));
            let resolution = declared
                .resolutions()
                .iter()
                .find(|resolution| resolution.display() == address(page))
                .unwrap();
            assert_eq!(resolution.mot().unwrap().selected_mot(), mot_identity());
            assert_eq!(resolution.mot().unwrap().mot_links(), mot().links());
            assert_eq!(resolution.defaults(), &[None; 4]);
            for role in [TeletextX27LinkRole::Pop, TeletextX27LinkRole::Gpop] {
                assert_eq!(
                    *resolution.role(role),
                    TeletextMotRoleResolution::Unselected
                );
            }
            for (scope, role, target) in [
                (
                    TeletextDrcsTableScope::Normal,
                    TeletextX27LinkRole::Drcs,
                    normal_identity(page, 0).page(),
                ),
                (
                    TeletextDrcsTableScope::Global,
                    TeletextX27LinkRole::Gdrcs,
                    authored_identities()[3].page(),
                ),
            ] {
                assert_eq!(metadata.resources.drcs.page(scope), Some(target));
                let TeletextMotRoleResolution::Selected {
                    target: actual,
                    requirements,
                    original_mot,
                    ..
                } = *resolution.role(role)
                else {
                    panic!("original role must be authenticated by MOT")
                };
                assert_eq!(actual, target);
                assert_eq!(
                    requirements,
                    TeletextMotResolutionRequirements::MotExtent(
                        TeletextMotSubpageExtent::new(0).unwrap()
                    )
                );
                let candidate = original_mot.unwrap();
                assert_eq!(candidate.target, target);
                assert!(!candidate.supersedable);
                assert!(
                    matches!(candidate.provenance,TeletextMotLinkProvenance::Mot {identity,..} if identity == mot_identity())
                );
            }
        }
        oracle::previews(service, b'A');
    }
}

pub(super) fn assert_effective(flow: &Flow, service: &EncodedTeletextAuthoredService) {
    // The existing oracle includes ALL original resource pages, including MOT,
    // in its exact-byte check and retains the complete original declaration.
    oracle::assert_service(flow, service);
    for page in 124..127 {
        let original = flow.accepted[0]
            .service()
            .declared()
            .resolutions()
            .iter()
            .find(|resolution| resolution.display() == address(page))
            .unwrap();
        let generated = service
            .compiler_drcs_resources()
            .iter()
            .find(|resource| resource.table().identity() == normal_identity(page, 1))
            .unwrap();
        let links = *generated.compositional_links(address(page)).unwrap();
        let actual =
            TeletextMotLevel25Resolution::resolve(&mot(), address(page), Some(links)).unwrap();
        let old_mot = original.mot().unwrap();
        assert_eq!(actual.selected_mot(), old_mot.selected_mot());
        assert_eq!(actual.association(), old_mot.association());
        assert_eq!(actual.mot_links(), old_mot.mot_links());
        assert_eq!(actual.default_sources(), old_mot.default_sources());
        assert_eq!(actual.default_conflicts(), old_mot.default_conflicts());
        for role in TeletextX27LinkRole::ALL {
            match (*original.role(role), *actual.role(role)) {
                (TeletextMotRoleResolution::Unselected, TeletextMotRoleResolution::Unselected) => {}
                (
                    TeletextMotRoleResolution::Selected {
                        target,
                        original_mot,
                        ..
                    },
                    TeletextMotRoleResolution::Selected {
                        target: current,
                        requirements,
                        original_mot: retained,
                        ..
                    },
                ) => {
                    assert_eq!(current, target);
                    assert_eq!(retained, original_mot);
                    assert_eq!(
                        links.link(role).target(),
                        TeletextX27LinkTarget::Page { page: target }
                    );
                    assert_eq!(
                        links.link(role).validity(),
                        TeletextX27LinkValidity::Level25Only
                    );
                    let mask = if role == TeletextX27LinkRole::Drcs {
                        3
                    } else {
                        1
                    };
                    assert_eq!(
                        requirements,
                        TeletextMotResolutionRequirements::X27Mask(
                            TeletextSubpageRequirementMask::new(mask)
                        )
                    );
                    assert_eq!(links.link(role).required_subpages().get(), mask);
                }
                state => {
                    panic!("materialized X27 must retain original MOT role meaning: {state:?}")
                }
            }
        }
    }
}

pub(super) fn mot_file(exported: &ExportedOriginalTeletextDesign) -> &str {
    exported.sidecar().project.assets.iter().find(|asset|matches!(&asset.scope,
        TeletextExportAssetScope::Resource {identity,..}
            if identity.kind == TeletextResourceKindWire::Mot
                && identity.page.magazine == 1 && identity.page.page == 0xfe && identity.subpage == 9))
        .expect("the exact original MOT remains a protected raw DVB asset").name.as_str()
}

fn changed_mot_request(
    exported: &ExportedOriginalTeletextDesign,
    receipt: &TeletextDesignExportReceipt,
) -> NativeTeletextEditorialImportRequest {
    const HAM: [u8; 16] = [
        0x15, 0x02, 0x49, 0x5e, 0x64, 0x73, 0x38, 0x2f, 0xd0, 0xc7, 0x8c, 0x9b, 0xa1, 0xb6, 0xfd,
        0xea,
    ];
    let mut request = drcs_bitmap::from_receipt(receipt, 2);
    let before = request.files.clone();
    let name = mot_file(exported);
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == name)
        .unwrap();
    assert_eq!(file.bytes.len() % 46, 0);
    for unit in file.bytes.chunks_exact(46) {
        assert_eq!(&unit[..2], &[2, 44]);
        assert_eq!(unit[3], 0xe4);
    }
    // The real MOT has header, X28/0, then body row1. Replace only one payload
    // nibble with another valid Hamming8/4 code; framing and MRAG remain exact.
    let unit = &file.bytes[2 * 46..3 * 46];
    let address: Vec<_> = unit[4..6]
        .iter()
        .map(|byte| {
            HAM.iter()
                .position(|code| *code == byte.reverse_bits())
                .unwrap()
        })
        .collect();
    let mrag = address[0] | (address[1] << 4);
    assert_eq!((mrag & 7, mrag >> 3), (1, 1));
    let offset = 2 * 46 + 6;
    let original = HAM
        .iter()
        .position(|code| *code == file.bytes[offset].reverse_bits())
        .unwrap();
    file.bytes[offset] = HAM[(original + 1) % 16].reverse_bits();
    assert_eq!(
        HAM.iter()
            .position(|code| *code == file.bytes[offset].reverse_bits()),
        Some((original + 1) % 16)
    );
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
    request
}

async fn reject_mot(
    fixture: &Fixture,
    exported: &ExportedOriginalTeletextDesign,
    fresh: &TeletextDesignExportReceipt,
) {
    let request = changed_mot_request(exported, fresh);
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
        "protected MOT rawasset mutation: {error:?}"
    );
}

fn changed_generated_request(
    exported: &ExportedOriginalTeletextDesign,
    fresh: &TeletextDesignExportReceipt,
) -> NativeTeletextEditorialImportRequest {
    let identity = normal_identity(124, 1);
    let assets = &exported.sidecar().project.assets;
    let raw = assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope,
        TeletextExportAssetScope::Resource {identity:wire,..}
            if wire.kind == TeletextResourceKindWire::Drcs
                && wire.page.magazine == identity.page().magazine()
                && wire.page.page == identity.page().page() && wire.subpage == 1)
        })
        .unwrap();
    let mapping = assets
        .iter()
        .find(|asset| {
            matches!(&asset.scope,
        TeletextExportAssetScope::DrcsTtiSet {resource_names}
            if resource_names.get(1) == Some(&Some(raw.name.clone())))
        })
        .unwrap();
    let mut request = drcs_bitmap::from_receipt(fresh, 2);
    let before = request.files.clone();
    let file = request
        .files
        .iter_mut()
        .find(|file| file.name == mapping.name)
        .unwrap();
    let mut records = parse_teletext_tti(&file.bytes, policy().tti)
        .unwrap()
        .records()
        .to_vec();
    let mut selected = false;
    let mut changed = false;
    for record in &mut records {
        match record {
            TeletextTtiRecord::PageNumber {
                page,
                editor_ordinal,
            } => {
                assert_eq!(*page, 0x2a0);
                selected = *editor_ordinal == 1;
            }
            TeletextTtiRecord::BaseRow { row: 1, cells } if selected => {
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
            _ => {}
        }
    }
    assert!(changed, "the exact compiler-owned S1 group must be changed");
    file.bytes = write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, policy().tti).unwrap(),
        policy().tti,
    )
    .unwrap();
    assert_eq!(before.len(), request.files.len());
    for (old, new) in before.iter().zip(&request.files) {
        assert_eq!(old.name, new.name);
        if old.name == mapping.name {
            assert_ne!(old.bytes, new.bytes);
        } else {
            assert_eq!(old, new);
        }
    }
    request
}

async fn reject_generated(
    fixture: &Fixture,
    exported: &ExportedOriginalTeletextDesign,
    fresh: &TeletextDesignExportReceipt,
) {
    let request = changed_generated_request(exported, fresh);
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
    let NativeTeletextEditorialImportError::Tti(error) = error else {
        panic!("implicit-MOT generated bitmap must remain protected: {error:?}");
    };
    assert_eq!(error.failure, NativeTeletextEditorialTtiFailure::Structure);
    assert_eq!(error.resource, Some(normal_identity(124, 1)));
    assert_eq!((error.page, error.subcode), (0x2a0, 1));
    assert_eq!(error.row, Some(1));
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
    assert_effective(flow, recover(fixture, 1).await.service());
    assert_effective(
        flow,
        drcs_bitmap::restored(fixture, concept)
            .await
            .encoded()
            .unwrap(),
    );
}

#[tokio::test]
async fn implicit_mot_three_target_concept_previews_publishes_protects_mot_and_reopens() {
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        response,
        Fixture::new_persistent().await,
        encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, prepared_request(&flow)).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, &flow).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let previews = Box::pin(assert_restored(&fixture, &flow, &concept)).await;
    assert_effective(
        &flow,
        drcs_bitmap::restored(&fixture, &concept)
            .await
            .encoded()
            .unwrap(),
    );
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, &flow).await;
    assert_eq!(
        Box::pin(assert_restored(&fixture, &flow, &concept)).await,
        previews
    );
    let day = publish_and_accept(&fixture, &flow, &concept).await;
    assert_effective(&flow, day.service());
    assert_eq!(oracle::previews(day.service(), 0x5b), previews);
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    oracle::assert_mapping(&exported);
    let original_mot = flow.exported[0]
        .files()
        .iter()
        .find(|file| file.name == mot_file(&flow.exported[0]))
        .unwrap();
    let concept_mot = exported
        .files()
        .iter()
        .find(|file| file.name == mot_file(&exported))
        .unwrap();
    assert_eq!(concept_mot.bytes, original_mot.bytes);
    reject_mot(&fixture, &exported, &fresh).await;
    Box::pin(assert_state(
        &fixture, &flow, &retained, &concept, &day, &fresh,
    ))
    .await;
    reject_generated(&fixture, &exported, &fresh).await;
    Box::pin(assert_state(
        &fixture, &flow, &retained, &concept, &day, &fresh,
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

#[path = "implicit_mot/defaults.rs"]
pub(super) mod defaults;
