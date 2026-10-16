//! Compiler-owned national glyphs cannot become authored editor authority.
use super::*;
use crate::host::teletext_editorial_tti::{self, resources};
use ersatzrs_app::{teletext_export_project::TeletextExportAssetMapping, teletext_tti::*};
use ersatzrs_ffmpeg::teletext::{
    encode_teletext_authored_service_before, lift_teletext_v1_page_set,
    project_encoded_teletext_drcs,
};

#[path = "compiler_resources/mixed_bank.rs"]
mod mixed_bank;

fn attributes(column: u8, wide: bool, high: bool) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::Attributes {
            double_height: high,
            double_width: wide,
            boxed: false,
            conceal: false,
            invert: false,
            underline_or_separated_mosaics: false,
        },
    }
}
fn glyph(column: u8, code: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::G0 {
            code: TeletextGlyphCode::new(code).unwrap(),
        },
    }
}
fn definition(
    id: u16,
    object_type: TeletextObjectType,
    commands: Vec<TeletextEnhancementCommand>,
) -> TeletextEnhancementObjectDefinition {
    TeletextEnhancementObjectDefinition {
        identity: TeletextObjectIdentity::local(address(123), TeletextObjectId::new(id)),
        object_type,
        program: TeletextEnhancementProgram::new(commands).unwrap(),
    }
}
fn original(primary: u8) -> TeletextDeclaredService {
    let policy = policy();
    let mut base = draft("A");
    base.pages[0].rows[0].runs[0].column = 7;
    base.pages[0].rows.extend(
        [(2, 8, "é"), (3, 1, "B")].map(|(row, column, text)| TeletextRow {
            row,
            runs: vec![TeletextRun {
                column,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: text.into() },
            }],
        }),
    );
    let lifted = lift_teletext_v1_page_set(
        base,
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let mut pages: Vec<_> = lifted.page_enhancements().values().cloned().collect();
    let page = pages
        .iter_mut()
        .find(|page| page.address == address(123))
        .unwrap();
    page.presentation.as_mut().unwrap().primary =
        TeletextPageCharsetDesignation::new(primary).unwrap();
    let objects = vec![
        definition(
            0xffff,
            TeletextObjectType::Active,
            vec![
                attributes(0, true, true),
                glyph(0, b'A'),
                attributes(2, false, false),
            ],
        ),
        definition(
            0xfffe,
            TeletextObjectType::Adaptive,
            vec![
                attributes(0, true, false),
                attributes(2, false, false),
                glyph(2, b'X'),
            ],
        ),
    ];
    page.local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 7).unwrap(),
            },
            TeletextEnhancementCommand::InvokeObject {
                target: objects[0].identity,
                object_type: TeletextObjectType::Active,
                offset: Some(TeletextObjectOffset::new(0, 1).unwrap()),
            },
            TeletextEnhancementCommand::InvokeObject {
                target: objects[1].identity,
                object_type: TeletextObjectType::Adaptive,
                offset: None,
            },
        ])
        .unwrap(),
    );
    let semantic = TeletextSemanticService::new(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages,
            magazines: vec![],
            objects,
            drcs: vec![],
        },
        policy.validation.service,
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
        policy.validation.declarations,
    )
    .unwrap()
}
fn compile(
    code: u8,
) -> (
    EncodedTeletextAuthoredService,
    Vec<TeletextPhysicalBaseOverlay>,
) {
    compile_at(0x08, code)
}

fn compile_at(
    primary: u8,
    code: u8,
) -> (
    EncodedTeletextAuthoredService,
    Vec<TeletextPhysicalBaseOverlay>,
) {
    let declared = original(primary);
    let mut cells = [b' '; 40];
    cells[7] = code;
    let rows = vec![overlay(1, cells)];
    let edited = declared
        .with_physical_base_overlays(rows.clone(), policy().validation.service)
        .unwrap();
    let service =
        encode_teletext_authored_service_before(edited, &[], &policy().encoding, deadline())
            .unwrap();
    assert!(service.declared().semantic().drcs().subtables().is_empty());
    assert!(service.declared().resource_inventory().is_empty());
    if primary == 0x08 && code == 0x5b {
        assert_enlarged_polish(&service);
    }
    (service, rows)
}

fn assert_enlarged_polish(service: &EncodedTeletextAuthoredService) {
    let composition = service
        .pages()
        .iter()
        .find_map(|page| {
            display(page)
                .filter(|(_, composition)| composition.owner() == address(123))
                .map(|(_, composition)| composition)
        })
        .unwrap();
    assert_eq!(composition.base_display().primary().get(), 0x08);
    let resolved = composition.display_resolution();
    let origin = resolved.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(resolved.cell(1, 8).unwrap().origin(), Some(origin));
    assert_ne!(resolved.cell(1, 9).unwrap().origin(), Some(origin));
    assert_eq!(
        resolved
            .origin_attributes(origin)
            .unwrap()
            .double_width()
            .value(),
        Some(true)
    );
    let candidate_index = resolved.origins()[origin].candidate_index();
    let candidate = composition.glyph_layers().candidates()[candidate_index];
    assert_eq!(candidate.priority(), TeletextGlyphPriority::Base);
    assert_eq!(candidate.base().unwrap().code(), 0x5b);
    assert!(
        matches!(composition.glyph_intent().records()[candidate_index].glyph(),
        Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.code() == 0x5b
        && glyph.national_subset() == Some(TeletextNationalSubset::Polish))
    );
}
#[test]
fn no_bank_polish_base_adds_and_removes_only_proven_compiler_resources() {
    let (before, _) = compile(b'A');
    let (after, rows) = compile(0x5b);
    assert!(before.compiler_drcs_resources().is_empty());
    assert_eq!(after.compiler_drcs_resources().len(), 1);
    let declared = after.compiler_drcs_resources()[0].table();
    assert_eq!(
        declared.glyphs()[0].bitmap.get(),
        [0, 0x3fc, 0x18, 0x30, 0x1f8, 0xc0, 0x180, 0x3fc, 0, 0]
    );
    validate_teletext_editorial_effects(
        &before,
        &after,
        &project("free", 1, 7, 2, 1),
        &rows,
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    let (reverted, reverted_rows) = compile(b'A');
    validate_teletext_editorial_effects(
        &after,
        &reverted,
        &project("free", 1, 7, 2, 1),
        &reverted_rows,
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    assert!(validate_teletext_editorial_effects(
        &before,
        &after,
        &project("zone", 1, 9, 1, 1),
        &rows,
        &policy().encoding,
        deadline()
    )
    .is_err());
    assert!(super::super::compiler_resources::check(&before, &after, &[], deadline()).is_err());
}

#[test]
fn no_bank_turkish_base_preserves_editor_zone_and_compiler_ownership() {
    let (before, before_rows) = compile_at(0x16, b'A');
    let (after, rows) = compile_at(0x16, 0x23);
    assert!(before.compiler_drcs_resources().is_empty());
    assert_eq!(after.compiler_drcs_resources().len(), 1);
    let generated = after.compiler_drcs_resources()[0].table();
    assert_eq!(generated.glyphs().len(), 1);
    assert_eq!(generated.glyphs()[0].slot.get(), 2);
    assert_eq!(
        generated.glyphs()[0].bitmap.get(),
        [0, 0, 0x3f0, 0xc0, 0xd8, 0xd8, 0xd8, 0xde, 0, 0]
    );
    let composition = after
        .pages()
        .iter()
        .find_map(|page| display(page).filter(|(_, c)| c.owner() == address(123)))
        .unwrap()
        .1;
    assert_eq!(composition.base_display().primary().get(), 0x16);
    let resolved = composition.display_resolution();
    let origin = resolved.cell(1, 7).unwrap().origin().unwrap();
    assert_eq!(resolved.cell(1, 8).unwrap().origin(), Some(origin));
    let index = resolved.origins()[origin].candidate_index();
    assert!(
        matches!(composition.glyph_intent().records()[index].glyph(),
        Some(TeletextGlyphIdentity::G0 {glyph}) if glyph.code() == 0x23
        && glyph.national_subset() == Some(TeletextNationalSubset::Turkish))
    );
    for (original, edited, edits) in [
        (&before, &after, rows.as_slice()),
        (&after, &before, before_rows.as_slice()),
    ] {
        validate_teletext_editorial_effects(
            original,
            edited,
            &project("free", 1, 7, 2, 1),
            edits,
            &policy().encoding,
            deadline(),
        )
        .unwrap();
    }
    assert!(validate_teletext_editorial_effects(
        &before,
        &after,
        &project("zone", 1, 9, 1, 1),
        &rows,
        &policy().encoding,
        deadline(),
    )
    .is_err());
    assert!(super::super::compiler_resources::check(&before, &after, &[], deadline()).is_err());
}

fn tti_budget() -> TeletextTtiBudget {
    TeletextTtiBudget {
        max_bytes: 16_384,
        max_records: 128,
    }
}
fn bytes(records: Vec<TeletextTtiRecord>) -> Vec<u8> {
    write_teletext_tti(
        &TeletextTtiDocument::checked_from_records(records, tti_budget()).unwrap(),
        tti_budget(),
    )
    .unwrap()
}
fn companion(
    service: &EncodedTeletextAuthoredService,
) -> (resources::DrcsTtiBank, Vec<TeletextTtiRecord>) {
    let resource = &service.compiler_drcs_resources()[0];
    let page = service
        .pages()
        .iter()
        .find(|page| {
            page.resource_header()
                .is_some_and(|header| header.identity() == resource.table().identity())
        })
        .unwrap();
    let projection = project_encoded_teletext_drcs(page, service.unit_count()).unwrap();
    let identity = resource.table().identity();
    let raw_page = (u16::from(identity.page().magazine()) << 8) | u16::from(identity.page().page());
    let mut records = vec![
        TeletextTtiRecord::PageNumber {
            page: raw_page,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(0),
        TeletextTtiRecord::PageStatus(
            crate::host::teletext_export_project::TeletextEditorCarrier::Legacy
                .status(projection.header.controls.raw(), 0),
        ),
    ];
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
    let mut project = project("free", 1, 7, 2, 1);
    let raw: TeletextExportAssetMapping = serde_json::from_value(json!({"name":"resource.dvb","scope":{"kind":"resource",
        "identity":{"kind":identity.kind(),"page":{"magazine":identity.page().magazine(),"page":identity.page().page()},"subpage":identity.logical_subpage().get()},
        "physical_subcode":page.resource_header().unwrap().physical_subcode()},"unit_count":page.data_units().len(),"bytes":page.data_units().len()*46,"sha256":"e".repeat(64)})).unwrap();
    let companion: TeletextExportAssetMapping = serde_json::from_value(json!({"name":"bank.tti","scope":{"kind":"drcs_tti_set","resource_names":["resource.dvb"]},"unit_count":0,"bytes":1,"sha256":"f".repeat(64)})).unwrap();
    project.assets = vec![raw, companion.clone()];
    (
        resources::mapping(&companion, &project, service).unwrap(),
        records,
    )
}
#[test]
fn generated_companion_is_read_only_and_bound_to_export_baseline_when_bank_disappears() {
    let (service, _) = compile(0x5b);
    let (bank, records) = companion(&service);
    let original = bytes(records.clone());
    assert!(resources::reconcile(
        bank,
        &original,
        &original,
        &service,
        (tti_budget(), service.unit_count()),
        deadline()
    )
    .unwrap()
    .is_empty());
    let (reverted, _) = compile(b'A');
    resources::validate_with_baseline(
        bank,
        &original,
        &reverted,
        &service,
        (tti_budget(), service.unit_count()),
        deadline(),
    )
    .unwrap();
    let mut changed = records;
    let cells = changed
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap();
    cells[1] ^= 1;
    let error = resources::reconcile(
        bank,
        &original,
        &bytes(changed),
        &service,
        (tti_budget(), service.unit_count()),
        deadline(),
    )
    .err()
    .unwrap();
    assert_eq!(
        error.failure,
        teletext_editorial_tti::NativeTeletextEditorialTtiFailure::Structure
    );
    assert_eq!(
        error.resource,
        Some(service.compiler_drcs_resources()[0].table().identity())
    );
}
#[test]
fn generated_resource_mode_packet_remains_protected() {
    let (service, _) = compile(0x5b);
    let (bank, records) = companion(&service);
    let original = bytes(records.clone());
    let mut changed = records;
    let words = changed
        .iter_mut()
        .find_map(|record| match record {
            TeletextTtiRecord::Packet {
                row: 28,
                designation: 3,
                words,
            } => Some(words),
            _ => None,
        })
        .unwrap();
    words[3] ^= 1;
    let error = resources::reconcile(
        bank,
        &original,
        &bytes(changed),
        &service,
        (tti_budget(), service.unit_count()),
        deadline(),
    )
    .unwrap_err();
    assert_eq!(
        error.failure,
        teletext_editorial_tti::NativeTeletextEditorialTtiFailure::Packet
    );
    assert_eq!((error.row, error.designation), (Some(28), Some(3)));
}

fn display_file(
    service: &EncodedTeletextAuthoredService,
) -> (
    ersatzrs_app::teletext_export_project::TeletextExportPageMapping,
    Vec<TeletextTtiRecord>,
    u16,
) {
    let encoded = service
        .pages()
        .iter()
        .find_map(|page| {
            display(page)
                .filter(|(_, composition)| composition.owner() == address(123))
                .map(|(page, _)| page)
        })
        .unwrap();
    let projection = project_encoded_teletext_display(encoded, service.unit_count()).unwrap();
    let mut mapping = project("free", 1, 7, 2, 1).pages.remove(0);
    mapping.header_controls = projection.header.controls.raw();
    let status = crate::host::teletext_export_project::TeletextEditorCarrier::V4
        .status(mapping.header_controls, 0x08);
    let mut records = vec![
        TeletextTtiRecord::PageNumber {
            page: 0x123,
            editor_ordinal: 0,
        },
        TeletextTtiRecord::SubpageOrdinal(0),
        TeletextTtiRecord::PageStatus(status),
        TeletextTtiRecord::Region(1),
        TeletextTtiRecord::Cycle {
            value: 20,
            unit: TeletextTtiCycleUnit::Time,
        },
    ];
    for record in projection.records {
        match record {
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
            TeletextEncodedDisplayRecord::HeaderText(_)
            | TeletextEncodedDisplayRecord::EditorialLinks { .. } => {}
        }
    }
    (mapping, records, status)
}

#[test]
fn generated_display_x27_is_still_immutable_uploaded_input() {
    let (service, _) = compile(0x5b);
    let (mapping, records, status) = display_file(&service);
    let original = bytes(records.clone());
    let mut changed = records;
    let words = changed
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
    words[4] ^= 1 << 6;
    let error = teletext_editorial_tti::reconcile_teletext_editorial_tti_with_operands(
        &mapping,
        &original,
        &bytes(changed),
        tti_budget(),
        deadline(),
        status,
        &service,
    )
    .err()
    .unwrap();
    assert_eq!(
        error.failure,
        teletext_editorial_tti::NativeTeletextEditorialTtiFailure::Packet
    );
    assert_eq!((error.row, error.designation), (Some(27), Some(4)));
}
