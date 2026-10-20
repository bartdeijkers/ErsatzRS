//! Actual shared host encoder services, not detached caller display models.
use super::*;
use crate::host::{
    teletext_channel_documents::{
        encode_original_channel_documents_with_physical_overlays, EncodedChannelTeletextOriginals,
        TeletextChannelDocumentEncodingPolicy,
    },
    teletext_runtime_policy::{
        parse_native_teletext_authoring_input, parse_native_teletext_authoring_policy,
    },
};
use ersatzrs_app::teletext_profile_documents::TeletextProfileDocument;
use ersatzrs_domain::*;
use ersatzrs_ffmpeg::teletext::{
    render_teletext_composition, TeletextEncodingRegion, TeletextRasterBudget,
};
use serde_json::json;
use std::time::Duration;
mod base_text;
mod bitmaps;
mod charset_context;
mod compiler_resources;
mod local_object_attributes;
mod main_presence;
mod operands;
mod resource_operands;

const ADDON: &str = "org.ersatzrs.synthetic-editor-effects";
fn address(page: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(16).unwrap(),
    }
}
fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}
fn snapshot() -> TeletextChannelSelectionSnapshot {
    TeletextChannelSelectionSnapshot {
        channel_id: 1,
        bindings: vec![TeletextSelectionSnapshot {
            contract: TeletextPageContract::V1,
            channel_id: 1,
            addon_id: ADDON.into(),
            binding_revision: 1,
            profile_id: 1,
            profile_revision: 1,
            installation: TeletextInstallationSnapshot {
                addon_id: ADDON.into(),
                state_revision: "synthetic-installation".into(),
                version: "1.0.0".into(),
                package_sha256: "a".repeat(64),
                content_sha256: "b".repeat(64),
                manifest_sha256: "c".repeat(64),
            },
            effective_settings: vec![],
        }],
    }
}
fn policy() -> TeletextChannelDocumentEncodingPolicy {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    let root = value.as_object_mut().unwrap();
    root.remove("source");
    root.remove("recovery_timeout_ms");
    root.insert("operation_timeout_ms".into(), json!(1000));
    root.insert("preview".into(),json!({"max_pages":16,"max_model_records":32_768,"max_pixels":168_000,"max_diagnostics":1400,"max_work":50_000_000}));
    let policy = parse_native_teletext_authoring_policy(&value.to_string()).unwrap();
    let input=parse_native_teletext_authoring_input(&json!({"input_version":1,"channel_id":1,"entry_addon_id":ADDON,"entry":{"page":123,"subcode":16},"v1_region":"western_latin","resource_transmissions":[]}).to_string(),&policy).unwrap();
    policy.encoding_policy(&input).unwrap()
}
fn draft(text: &str) -> TeletextPageSetDraft {
    let language = TeletextLanguage::new("eng".into()).unwrap();
    TeletextPageSetDraft {
        language: language.clone(),
        initial_page: address(123).page,
        pages: [123, 124]
            .map(|page| TeletextPageDraft {
                address: address(page),
                kind: TeletextPageKind::FullPage,
                language: language.clone(),
                national_option: TeletextNationalOption::new(0).unwrap(),
                rows: vec![TeletextRow {
                    row: 1,
                    runs: vec![TeletextRun {
                        column: 8,
                        style: TeletextStyle::default(),
                        content: TeletextRunContent::Text { text: text.into() },
                    }],
                }],
                navigation: vec![TeletextNavigationLink {
                    role: TeletextNavigationRole::Red,
                    target: TeletextNavigationTarget {
                        page: address(if page == 123 { 124 } else { 123 }).page,
                        subcode: Some(address(page).subcode),
                    },
                }],
            })
            .into(),
    }
}
fn encode(
    source: TeletextPageSetDraft,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
) -> EncodedChannelTeletextOriginals {
    encode_original_channel_documents_with_physical_overlays(
        &snapshot(),
        Some(ADDON),
        vec![TeletextProfileDocument::Base(source)],
        overlays,
        deadline(),
        Some(TeletextEncodingRegion::WesternLatin),
        policy(),
    )
    .unwrap()
}
fn service(original: &EncodedChannelTeletextOriginals) -> &EncodedTeletextAuthoredService {
    original.encoded.as_ref().unwrap()
}
fn row(original: &EncodedChannelTeletextOriginals) -> [u8; 40] {
    std::array::from_fn(|column| {
        display(&service(original).pages()[0])
            .unwrap()
            .1
            .base_display()
            .cell(1, u8::try_from(column).unwrap())
            .unwrap()
            .code()
    })
}
fn overlay(row: u8, cells: [u8; 40]) -> TeletextPhysicalBaseOverlay {
    TeletextPhysicalBaseOverlay::new(
        address(123),
        vec![TeletextPhysicalBaseRow::new(row, cells).unwrap()],
    )
    .unwrap()
}
fn project(kind: &str, row: u8, column: u8, width: u8, height: u8) -> TeletextExportProject {
    serde_json::from_value(json!({"schema_version":1,"editor_profile":"qteletextmaker-1.0",
        "original":{"design_id":1,"revision":1,"digest":"d".repeat(64),"expected_head":1,"document_version":1},
        "channel_id":1,"entry":{"page":123,"subcode":16},"language":"eng","v1_region":"western_latin",
        "producers":[{"ordinal":0,"addon_id":ADDON,"contract":3,"binding_revision":1,"profile_id":1,"profile_revision":1,
            "installation":{"state_revision":"synthetic-installation","version":"1.0.0","package_sha256":"a".repeat(64),"content_sha256":"b".repeat(64),"manifest_sha256":"c".repeat(64)},
            "authoring":{"schema_version":1,"template":{"id":"grid","revision":"r1"},"theme":{"id":"day","overrides":[]},
                "pages":[{"page":{"page":123,"subcode":16},"regions":[{"id":"edit","kind":kind,"area":"main","row":row,"column":column,"width":width,"height":height}],"dynamic_fields":[]}]}}],
        "pages":[{"name":"p123.tti","page":{"page":123,"subcode":16},"producer_ordinal":0,"editor_page":291,"editor_ordinal":0,"language":"eng","kind":"full_page","header_controls":128,"navigation":[],"materialized_magazine":null,"tti_bytes":1,"tti_sha256":"e".repeat(64)}],"assets":[]
    })).unwrap()
}
fn validate(
    original: &EncodedChannelTeletextOriginals,
    edited: &EncodedChannelTeletextOriginals,
    project: &TeletextExportProject,
    overlays: &[TeletextPhysicalBaseOverlay],
) -> Result<()> {
    validate_teletext_editorial_effects(
        service(original),
        service(edited),
        project,
        overlays,
        &policy().encoding,
        deadline(),
    )
}
#[test]
fn ordinary_single_cell_zone_edit_preserves_protected_semantics_and_pixels() {
    let original = encode(draft("ASCII"), vec![]);
    let mut cells = row(&original);
    cells[8] = b'Z';
    let overlays = vec![overlay(1, cells)];
    let edited = encode(draft("ASCII"), overlays.clone());
    validate(&original, &edited, &project("zone", 1, 8, 1, 1), &overlays).unwrap();
    let links = |source: &EncodedChannelTeletextOriginals| {
        project_encoded_teletext_display(display(&service(source).pages()[0]).unwrap().0, 128)
            .unwrap()
            .records
            .into_iter()
            .find_map(|record| match record {
                TeletextEncodedDisplayRecord::EditorialLinks { nibbles, crc } => {
                    Some((nibbles, crc))
                }
                _ => None,
            })
            .unwrap()
    };
    let before_links = links(&original);
    let after_links = links(&edited);
    assert_eq!(before_links.0, after_links.0);
    assert_ne!(before_links.1, after_links.1);
    let reference = encode(draft("ZSCII"), vec![]);
    let options = TeletextColourPixelOptions {
        reveal_concealed: false,
        flash_phase: TeletextFlashPhase::Original,
    };
    let budget = TeletextRasterBudget {
        max_pixels: 168_000,
        max_diagnostics: 1400,
    };
    let actual = render_teletext_composition(
        display(&service(&edited).pages()[0]).unwrap().1,
        options,
        budget,
    )
    .unwrap();
    let expected = render_teletext_composition(
        display(&service(&reference).pages()[0]).unwrap().1,
        options,
        budget,
    )
    .unwrap();
    assert_eq!(actual.pixels(), expected.pixels());
    assert_eq!(
        service(&original).pages()[1].data_units(),
        service(&edited).pages()[1].data_units()
    );
}
#[test]
fn unchanged_protected_authored_spaces_have_equivalent_coded_blank_semantics() {
    let original = encode(draft("A SCII"), vec![]);
    let mut cells = row(&original);
    cells[8] = b'Z';
    let overlays = vec![overlay(1, cells)];
    let edited = encode(draft("A SCII"), overlays.clone());
    validate(&original, &edited, &project("zone", 1, 8, 1, 1), &overlays).unwrap();
}

#[test]
fn generated_omitted_and_outside_regions_do_not_grant_byte_edits() {
    let original = encode(draft("ASCII"), vec![]);
    let mut cells = row(&original);
    cells[8] = b'Z';
    let overlays = vec![overlay(1, cells)];
    let edited = encode(draft("ASCII"), overlays.clone());
    let mut absent = project("zone", 1, 8, 1, 1);
    absent.producers[0].authoring = None;
    for metadata in [
        project("generated", 1, 8, 1, 1),
        project("zone", 1, 9, 1, 1),
        absent,
    ] {
        let error = validate(&original, &edited, &metadata, &overlays).unwrap_err();
        assert_eq!(error.reason, TeletextEditorialEffectsFailure::ProtectedByte);
        assert_eq!(error.position, Some((1, 8)));
    }
}
#[test]
fn spacing_control_cannot_escape_zone_to_protected_right_cells() {
    let original = encode(draft("ASCII"), vec![]);
    for code in [0x01, 0x08, 0x18, 0x0b, 0x00, 0x10, 0x0e, 0x0f] {
        let mut cells = row(&original);
        cells[7] = code;
        if code == 0x0b {
            cells[6] = code;
        }
        let overlays = vec![overlay(1, cells)];
        let edited = encode(draft("ASCII"), overlays.clone());
        let error =
            validate(&original, &edited, &project("zone", 1, 6, 2, 1), &overlays).unwrap_err();
        assert_eq!(error.reason, TeletextEditorialEffectsFailure::ProtectedBase);
        assert_eq!(error.position, Some((1, 8)));
    }
}
#[test]
fn spacing_effects_on_protected_blank_cells_are_not_screen_colour_equivalence() {
    let original = encode(draft("A"), vec![]);
    let mut cells = row(&original);
    cells[9] = 0x08;
    let overlays = vec![overlay(1, cells)];
    let edited = encode(draft("A"), overlays.clone());
    let failure =
        validate(&original, &edited, &project("zone", 1, 9, 1, 1), &overlays).unwrap_err();
    assert_eq!(
        failure.reason,
        TeletextEditorialEffectsFailure::ProtectedBase
    );
    assert_eq!(failure.position, Some((1, 10)));
}

#[test]
fn free_row_insertion_is_allowed_when_protected_rows_remain_exact() {
    let original = encode(draft("ASCII"), vec![]);
    let mut cells = [b' '; 40];
    cells[8..13].copy_from_slice(b"ADDED");
    let overlays = vec![overlay(2, cells)];
    let edited = encode(draft("ASCII"), overlays.clone());
    validate(&original, &edited, &project("free", 2, 0, 40, 1), &overlays).unwrap();
}

#[test]
fn double_height_cannot_change_protected_lower_row_footprints() {
    let original = encode(draft("ASCII"), vec![]);
    for code in [0x0d, 0x0f] {
        let mut cells = row(&original);
        cells[7] = code;
        let overlays = vec![overlay(1, cells)];
        let edited = encode(draft("ASCII"), overlays.clone());
        let error =
            validate(&original, &edited, &project("zone", 1, 0, 40, 1), &overlays).unwrap_err();
        assert_eq!(
            error.reason,
            TeletextEditorialEffectsFailure::ProtectedFootprint
        );
        assert_eq!(error.position.unwrap().0, 2);
    }
}

#[test]
fn enhanced_physical_spacing_is_editable_only_with_all_affected_cells_in_scope() {
    let original = encode(draft("ASCII"), vec![]);
    for (code, height) in [(0x00, 1), (0x10, 1), (0x0e, 1), (0x0f, 2)] {
        let mut cells = row(&original);
        cells[7] = code;
        let overlays = vec![overlay(1, cells)];
        let edited = encode(draft("ASCII"), overlays.clone());
        validate(
            &original,
            &edited,
            &project("free", 1, 0, 40, height),
            &overlays,
        )
        .unwrap();
    }
}
#[test]
fn whole_free_row_can_change_spacing_when_every_effect_stays_within_it() {
    let original = encode(draft("ASCII"), vec![]);
    let mut cells = row(&original);
    cells[7] = 0x01;
    let overlays = vec![overlay(1, cells)];
    let edited = encode(draft("ASCII"), overlays.clone());
    validate(&original, &edited, &project("free", 1, 0, 40, 1), &overlays).unwrap();
}
#[test]
fn protected_navigation_and_unchanged_page_mutation_reject() {
    let original = encode(draft("ASCII"), vec![]);
    let mut changed = draft("ASCII");
    changed.pages[0].navigation.clear();
    let edited = encode(changed, vec![]);
    assert_eq!(
        validate(&original, &edited, &project("free", 1, 0, 40, 1), &[])
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
    let mut changed = draft("ASCII");
    changed.pages[1].rows[0].runs[0].content = TeletextRunContent::Text {
        text: "OTHER".into(),
    };
    let edited = encode(changed, vec![]);
    assert_eq!(
        validate(&original, &edited, &project("free", 1, 0, 40, 1), &[])
            .unwrap_err()
            .reason,
        TeletextEditorialEffectsFailure::ProtectedStructure
    );
}
#[test]
fn original_deadline_and_encoder_bounds_are_not_renewed() {
    let original = encode(draft("ASCII"), vec![]);
    let mut budget = policy().encoding;
    budget.max_pages = 0;
    let result = validate_teletext_editorial_effects(
        service(&original),
        service(&original),
        &project("free", 1, 0, 40, 1),
        &[],
        &budget,
        deadline(),
    )
    .unwrap_err();
    assert_eq!(result.reason, TeletextEditorialEffectsFailure::Budget);
    let expired = Instant::now().checked_sub(Duration::from_secs(1)).unwrap();
    let result = validate_teletext_editorial_effects(
        service(&original),
        service(&original),
        &project("free", 1, 0, 40, 1),
        &[],
        &policy().encoding,
        expired,
    )
    .unwrap_err();
    assert_eq!(result.reason, TeletextEditorialEffectsFailure::TimedOut);
}

fn resource_service(
    resource_byte: u8,
    screen_colour: u8,
    overlays: Vec<TeletextPhysicalBaseOverlay>,
) -> EncodedTeletextAuthoredService {
    let policy = policy();
    let lifted = ersatzrs_ffmpeg::teletext::lift_teletext_v1_page_set(
        draft("ASCII"),
        TeletextEncodingRegion::WesternLatin,
        policy.validation.service,
    )
    .unwrap();
    let mut presentation = lifted.page_enhancements()[&address(123)]
        .presentation
        .unwrap();
    presentation.screen_colour = TeletextColourReference::new(screen_colour).unwrap();
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Pop,
        TeletextRawDataPageNumber::new(1, resource_byte).unwrap(),
        TeletextResourceSubpageIndex::new(0).unwrap(),
    )
    .unwrap();
    let semantic = TeletextSemanticService::new_with_object_companions(
        TeletextResolvedServiceDraft {
            base: lifted.base().clone(),
            pages: lifted.page_enhancements().values().cloned().collect(),
            magazines: vec![TeletextMagazinePresentationDraft {
                magazine: 1,
                presentation,
            }],
            objects: vec![],
            drcs: vec![],
        },
        vec![resource],
        policy.validation.service,
    )
    .unwrap()
    .with_physical_base_overlays(overlays, policy.validation.service)
    .unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![],
        [123, 124]
            .map(|page| TeletextDisplayMotSelection {
                display: address(page),
                mot: None,
            })
            .into(),
        vec![resource],
        policy.validation.declarations,
    )
    .unwrap();
    ersatzrs_ffmpeg::teletext::encode_teletext_authored_service_before(
        declared,
        &[
            ersatzrs_ffmpeg::teletext::TeletextServiceResourceTransmission {
                resource,
                counter: TeletextResourceUpdateCounter::new(1).unwrap(),
                update: ersatzrs_ffmpeg::teletext::TeletextResourceFullUpdate::Changed,
                last_subpage: true,
            },
        ],
        &policy.encoding,
        deadline(),
    )
    .unwrap()
}

#[test]
fn actual_resource_and_magazine_units_stay_exact_and_mutations_reject() {
    let original = resource_service(0xa0, 0, vec![]);
    let mut cells = [b' '; 40];
    cells[8..13].copy_from_slice(b"ZSCII");
    let overlays = vec![overlay(1, cells)];
    let edited = resource_service(0xa0, 0, overlays.clone());
    validate_teletext_editorial_effects(
        &original,
        &edited,
        &project("zone", 1, 8, 1, 1),
        &overlays,
        &policy().encoding,
        deadline(),
    )
    .unwrap();
    assert!(!original.magazine_units().is_empty());
    assert_eq!(original.magazine_units(), edited.magazine_units());
    let before = original
        .pages()
        .iter()
        .find(|page| page.resource_header().is_some())
        .unwrap();
    let after = edited
        .pages()
        .iter()
        .find(|page| page.resource_header().is_some())
        .unwrap();
    assert_eq!(before.data_units(), after.data_units());
    for changed in [
        resource_service(0xa1, 0, overlays.clone()),
        resource_service(0xa0, 1, overlays.clone()),
    ] {
        let failure = validate_teletext_editorial_effects(
            &original,
            &changed,
            &project("zone", 1, 8, 1, 1),
            &overlays,
            &policy().encoding,
            deadline(),
        )
        .unwrap_err();
        assert_eq!(
            failure.reason,
            TeletextEditorialEffectsFailure::ProtectedStructure
        );
    }
}
