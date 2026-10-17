//! A shared raw page does not merge authored and compiler editing permissions.

use super::*;
use ersatzrs_ffmpeg::teletext::{TeletextResourceFullUpdate, TeletextServiceResourceTransmission};

fn resource(kind: TeletextResourceKind) -> TeletextResourceIdentity {
    let (magazine, page) = if kind == TeletextResourceKind::Drcs {
        (2, 0xa0)
    } else {
        (1, 0xa1)
    };
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(magazine, page).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap()
}

fn linked(identity: TeletextResourceIdentity) -> TeletextX27LinkDeclaration {
    TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Page {
            page: identity.page(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(1),
    )
    .unwrap()
}

fn occupied_original() -> TeletextDeclaredService {
    let original = original(0x08);
    let source = original.semantic();
    assert!(source.drcs().subtables().is_empty());
    assert!(source.magazine_presentations().is_empty());
    let normal = resource(TeletextResourceKind::Drcs);
    let global = resource(TeletextResourceKind::Gdrcs);
    let mut pages: Vec<_> = source.page_enhancements().values().cloned().collect();
    let page = pages
        .iter_mut()
        .find(|page| page.address == address(123))
        .unwrap();
    page.resources.drcs =
        TeletextDrcsPageLinks::new(Some(normal.page()), Some(global.page())).unwrap();
    let unused = TeletextX27LinkDeclaration::new(
        TeletextX27LinkTarget::Unspecified {
            magazine: TeletextX27Magazine::new(2).unwrap(),
        },
        TeletextX27LinkValidity::Level25Only,
        TeletextSubpageRequirementMask::new(u16::MAX),
    )
    .unwrap();
    page.compositional_links =
        Some(TeletextX27Level25Links::new(unused, unused, linked(global), linked(normal)).unwrap());
    let tables = [normal, global].map(|identity| {
        TeletextDrcsSubtable::new(
            identity,
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                bitmap: TeletextDrcsBitmap::new([0; 10]).unwrap(),
            }],
        )
        .unwrap()
    });
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: source.base().clone(),
            pages,
            magazines: vec![],
            objects: source.objects().definitions().to_vec(),
            drcs: tables.into(),
        },
        policy().validation.service,
    )
    .unwrap();
    TeletextDeclaredService::new(
        semantic,
        vec![],
        [123, 124]
            .map(|page| TeletextDisplayMotSelection {
                display: address(page),
                mot: None,
            })
            .into(),
        vec![],
        policy().validation.declarations,
    )
    .unwrap()
}

fn encode(declared: TeletextDeclaredService) -> EncodedTeletextAuthoredService {
    let transmissions: Vec<_> = declared
        .resource_inventory()
        .iter()
        .map(|&resource| TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect();
    encode_teletext_authored_service_before(
        declared,
        &transmissions,
        &policy().encoding,
        deadline(),
    )
    .unwrap()
}

fn compile_mixed(
    code: u8,
) -> (
    EncodedTeletextAuthoredService,
    Vec<TeletextPhysicalBaseOverlay>,
) {
    let mut cells = [b' '; 40];
    cells[7] = code;
    let rows = vec![overlay(1, cells)];
    let declared = occupied_original()
        .with_physical_base_overlays(rows.clone(), policy().validation.service)
        .unwrap();
    let service = encode(declared);
    assert_eq!(service.declared().resource_inventory().len(), 2);
    if code == 0x5b {
        assert_enlarged_polish(&service);
        assert_eq!(service.compiler_drcs_resources().len(), 1);
        let generated = service.compiler_drcs_resources()[0].table();
        assert_eq!(
            generated.identity().page(),
            resource(TeletextResourceKind::Drcs).page()
        );
        assert_eq!(generated.identity().logical_subpage().get(), 1);
        assert_eq!(generated.glyphs()[0].slot.get(), 0);
        assert_eq!(
            generated.glyphs()[0].bitmap.get(),
            [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0]
        );
    } else {
        assert!(service.compiler_drcs_resources().is_empty());
    }
    (service, rows)
}

fn append_group(
    records: &mut Vec<TeletextTtiRecord>,
    page: &EncodedTeletextAuthoredPage,
    ordinal: u64,
    max_units: usize,
) {
    let projection = project_encoded_teletext_drcs(page, max_units).unwrap();
    records.extend([
        TeletextTtiRecord::PageNumber {
            page: 0x2a0,
            editor_ordinal: ordinal,
        },
        TeletextTtiRecord::SubpageOrdinal(ordinal),
        TeletextTtiRecord::PageStatus(
            crate::host::teletext_export_project::TeletextEditorCarrier::Legacy
                .status(projection.header.controls.raw(), 0),
        ),
    ]);
    for record in projection.records {
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
            TeletextEncodedDisplayRecord::EditorialLinks { .. } => unreachable!(),
        }
    }
}

fn mixed_companion(
    service: &EncodedTeletextAuthoredService,
) -> (
    resources::DrcsTtiBank,
    Vec<TeletextTtiRecord>,
    TeletextExportProject,
) {
    let mut project = project("free", 1, 7, 2, 1);
    project.assets.clear();
    let mut records = Vec::new();
    let mut names = Vec::new();
    for (ordinal, name) in [(0, "authored-normal.dvb"), (1, "compiler-normal.dvb")] {
        let page = service
            .pages()
            .iter()
            .find(|page| {
                page.resource_header().is_some_and(|header| {
                    header.identity().page() == resource(TeletextResourceKind::Drcs).page()
                        && u64::from(header.identity().logical_subpage().get()) == ordinal
                })
            })
            .unwrap();
        let header = page.resource_header().unwrap();
        assert_eq!(header.identity().kind(), TeletextResourceKind::Drcs);
        project.assets.push(
            serde_json::from_value(json!({"name":name,"scope":{"kind":"resource",
            "identity":{"kind":"drcs","page":{"magazine":2,"page":160},"subpage":ordinal},
            "physical_subcode":header.physical_subcode()},"unit_count":page.data_units().len(),
            "bytes":page.data_units().len()*46,"sha256":"e".repeat(64)}))
            .unwrap(),
        );
        names.push(Some(name));
        append_group(&mut records, page, ordinal, service.unit_count());
    }
    let companion: TeletextExportAssetMapping = serde_json::from_value(json!({"name":"mixed-bank.tti",
        "scope":{"kind":"drcs_tti_set","resource_names":names},"unit_count":0,"bytes":1,"sha256":"f".repeat(64)})).unwrap();
    project.assets.push(companion.clone());
    (
        resources::mapping(&companion, &project, service).unwrap(),
        records,
        project,
    )
}

fn change_group(records: &mut [TeletextTtiRecord], selected: u64, packet: bool) {
    let mut group = None;
    let mut changed = false;
    for record in records {
        match record {
            TeletextTtiRecord::PageNumber { editor_ordinal, .. } => group = Some(*editor_ordinal),
            TeletextTtiRecord::BaseRow { row: 1, cells } if !packet && group == Some(selected) => {
                cells[1] ^= 1;
                changed = true;
            }
            TeletextTtiRecord::Packet {
                row: 28,
                designation: 3,
                words,
            } if packet && group == Some(selected) => {
                words[3] ^= 1;
                changed = true;
            }
            _ => {}
        }
    }
    assert!(changed);
}

#[test]
fn mixed_bank_authored_bitmap_edit_preserves_separate_compiler_permissions() {
    let (before, baseline_rows) = compile_mixed(0x5b);
    assert_eq!(baseline_rows.len(), 1);
    let (bank, records, project) = mixed_companion(&before);
    let original = bytes(records.clone());
    let limits = (tti_budget(), before.unit_count());
    assert!(
        resources::reconcile(bank, &original, &original, &before, limits, deadline())
            .unwrap()
            .is_empty()
    );
    let mut records = records;
    change_group(&mut records, 0, false);
    let edited = bytes(records);
    let changes =
        resources::reconcile(bank, &original, &edited, &before, limits, deadline()).unwrap();
    assert_eq!(changes.len(), 1);
    assert_eq!(changes[0].identity(), resource(TeletextResourceKind::Drcs));
    assert_eq!(changes[0].slot().get(), 0);
    assert_eq!(changes[0].expected().get(), [0; 10]);
    assert_eq!(
        changes[0].replacement().get(),
        [1, 0, 0, 0, 0, 0, 0, 0, 0, 0]
    );
    let after = encode(
        before
            .declared()
            .clone()
            .with_drcs_bitmap_overlays(&changes, policy().validation.service)
            .unwrap(),
    );
    assert_eq!(
        after
            .declared()
            .semantic()
            .physical_base_overlay(address(123)),
        baseline_rows.first()
    );
    validate_teletext_editorial_transition_with_operands(
        &before,
        &after,
        &project,
        TeletextEditorialEdits {
            rows: &[],
            operands: &[],
            object_operands: &[],
            resource_operands: &[],
            resource_programs: &[],
            object_programs: &[],
            bitmaps: &changes,
        },
        TeletextEditorialEdits {
            rows: &baseline_rows,
            operands: &[],
            object_operands: &[],
            resource_operands: &[],
            resource_programs: &[],
            object_programs: &[],
            bitmaps: &changes,
        },
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    resources::validate_with_baseline(
        bank,
        &edited,
        &after,
        &before,
        (tti_budget(), before.unit_count().max(after.unit_count())),
        deadline(),
    )
    .unwrap();
    assert_eq!(
        after.compiler_drcs_resources()[0].table(),
        before.compiler_drcs_resources()[0].table()
    );
}

#[test]
fn mixed_bank_generated_pixels_and_both_group_mode_packets_remain_protected() {
    let (service, _) = compile_mixed(0x5b);
    let (bank, records, _) = mixed_companion(&service);
    let original = bytes(records.clone());
    for (group, packet) in [(1, false), (0, true), (1, true)] {
        let mut changed = records.clone();
        change_group(&mut changed, group, packet);
        let error = resources::reconcile(
            bank,
            &original,
            &bytes(changed),
            &service,
            (tti_budget(), service.unit_count()),
            deadline(),
        )
        .unwrap_err();
        let identity = if group == 0 {
            resource(TeletextResourceKind::Drcs)
        } else {
            service.compiler_drcs_resources()[0].table().identity()
        };
        assert_eq!(error.resource, Some(identity));
        assert_eq!(
            error.failure,
            if packet {
                teletext_editorial_tti::NativeTeletextEditorialTtiFailure::Packet
            } else {
                teletext_editorial_tti::NativeTeletextEditorialTtiFailure::Structure
            }
        );
        assert_eq!(error.row, Some(if packet { 28 } else { 1 }));
        if packet {
            assert_eq!(error.designation, Some(3));
        }
    }
}

#[test]
fn mixed_bank_retirement_keeps_old_generated_group_only_as_read_only_baseline() {
    let (before, _) = compile_mixed(0x5b);
    let generated = before.compiler_drcs_resources()[0].table().identity();
    let (bank, records, project) = mixed_companion(&before);
    let original = bytes(records);
    let (after, rows) = compile_mixed(b'A');
    resources::validate_with_baseline(
        bank,
        &original,
        &after,
        &before,
        (tti_budget(), before.unit_count().max(after.unit_count())),
        deadline(),
    )
    .unwrap();
    validate_teletext_editorial_effects(
        &before,
        &after,
        &project,
        &rows,
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    assert!(after.compiler_drcs_resources().is_empty());
    assert!(!after.declared().resource_inventory().contains(&generated));
    assert!(!after
        .declared()
        .semantic()
        .drcs()
        .subtables()
        .iter()
        .any(|table| table.identity() == generated));
    assert!(!after.pages().iter().any(|page| page
        .resource_header()
        .is_some_and(|header| header.identity() == generated)));
    assert_eq!(
        after.declared().semantic().drcs(),
        before.declared().semantic().drcs()
    );
}
