//! Synthetic selection snapshots exercise the shared compiler, not installation,
//! import permission, storage, client acceptance or publication/playback state.

use std::time::{Duration, Instant};

use ersatzrs_domain::*;
use ersatzrs_ffmpeg::teletext::{
    project_encoded_teletext_display, EncodedTeletextAuthoredPage, EncodedTeletextPage,
    TeletextEncodedDisplayRecord, TeletextEncodingRegion, TeletextRasterPixel,
};
use serde_json::json;

use super::{
    encode_original_channel_documents, encode_original_channel_documents_with_physical_overlays,
    ChannelTeletextDocumentEncodingError, EncodedChannelTeletextOriginals,
    TeletextChannelDocumentEncodingPolicy, TeletextProfileDocument,
};
use crate::host::{
    teletext_preview::{project_teletext_preview, TeletextPreviewBudget},
    teletext_runtime_policy::{
        parse_native_teletext_authoring_input, parse_native_teletext_authoring_policy,
    },
};

const ADDON: &str = "org.ersatzrs.synthetic-overlay";

fn address(page: u16) -> TeletextPageAddress {
    TeletextPageAddress {
        page: TeletextPageNumber::new(page).unwrap(),
        subcode: TeletextSubcode::new(16).unwrap(),
    }
}

fn snapshot(channel: i64, profile: i64) -> TeletextChannelSelectionSnapshot {
    TeletextChannelSelectionSnapshot {
        channel_id: channel,
        bindings: vec![TeletextSelectionSnapshot {
            contract: TeletextPageContract::V1,
            channel_id: channel,
            addon_id: ADDON.to_owned(),
            binding_revision: 1,
            profile_id: profile,
            profile_revision: 1,
            installation: TeletextInstallationSnapshot {
                addon_id: ADDON.to_owned(),
                state_revision: "synthetic-installation-revision".to_owned(),
                version: "1.0.0".to_owned(),
                package_sha256: "a".repeat(64),
                content_sha256: "b".repeat(64),
                manifest_sha256: "c".repeat(64),
            },
            effective_settings: vec![],
        }],
    }
}

fn bounds(channel: i64) -> (TeletextChannelDocumentEncodingPolicy, TeletextPreviewBudget) {
    let mut value: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    let root = value.as_object_mut().unwrap();
    root.remove("source");
    root.remove("recovery_timeout_ms");
    root.insert("operation_timeout_ms".to_owned(), json!(1000));
    root.insert(
        "preview".to_owned(),
        json!({
            "max_pages":16,"max_model_records":32_768,"max_pixels":168_000,
            "max_diagnostics":1400,"max_work":50_000_000
        }),
    );
    let policy = parse_native_teletext_authoring_policy(&value.to_string()).unwrap();
    let input = parse_native_teletext_authoring_input(
        &json!({"input_version":1,"channel_id":channel,"entry_addon_id":ADDON,
            "entry":{"page":123,"subcode":16},"v1_region":"western_latin",
            "resource_transmissions":[]})
        .to_string(),
        &policy,
    )
    .unwrap();
    (policy.encoding_policy(&input).unwrap(), policy.preview)
}

fn draft(colour: TeletextColour, text: &str) -> TeletextPageSetDraft {
    let language = TeletextLanguage::new("eng".to_owned()).unwrap();
    let pages = [123, 124]
        .map(|number| TeletextPageDraft {
            address: address(number),
            kind: TeletextPageKind::FullPage,
            language: language.clone(),
            national_option: TeletextNationalOption::new(0).unwrap(),
            rows: vec![TeletextRow {
                row: 1,
                runs: vec![TeletextRun {
                    column: 8,
                    style: TeletextStyle {
                        foreground: colour,
                        ..TeletextStyle::default()
                    },
                    content: TeletextRunContent::Text {
                        text: text.to_owned(),
                    },
                }],
            }],
            navigation: vec![TeletextNavigationLink {
                role: TeletextNavigationRole::Red,
                target: TeletextNavigationTarget {
                    page: address(if number == 123 { 124 } else { 123 }).page,
                    subcode: Some(address(123).subcode),
                },
            }],
        })
        .into();
    TeletextPageSetDraft {
        language,
        initial_page: address(123).page,
        pages,
    }
}

fn deadline() -> Instant {
    Instant::now() + Duration::from_secs(30)
}

fn original(
    selection: &TeletextChannelSelectionSnapshot,
    source: TeletextPageSetDraft,
) -> EncodedChannelTeletextOriginals {
    encode_original_channel_documents(
        selection,
        Some(ADDON),
        vec![TeletextProfileDocument::Base(source)],
        deadline(),
        Some(TeletextEncodingRegion::WesternLatin),
        bounds(selection.channel_id).0,
    )
    .unwrap()
}

fn packet(
    original: &EncodedChannelTeletextOriginals,
    owner: TeletextPageAddress,
) -> &EncodedTeletextPage {
    original
        .encoded
        .as_ref()
        .unwrap()
        .pages()
        .iter()
        .find_map(|page| match page {
            EncodedTeletextAuthoredPage::NativeDisplay(page)
                if page.encoded().page().address() == owner =>
            {
                Some(page.encoded().page())
            }
            EncodedTeletextAuthoredPage::CompiledDisplay(page)
                if page.page().address() == owner =>
            {
                Some(page.page())
            }
            _ => None,
        })
        .unwrap()
}

fn row(original: &EncodedChannelTeletextOriginals) -> [u8; 40] {
    project_encoded_teletext_display(packet(original, address(123)), 128)
        .unwrap()
        .records
        .into_iter()
        .find_map(|record| match record {
            TeletextEncodedDisplayRecord::BaseRow { row: 1, cells } => Some(cells),
            _ => None,
        })
        .unwrap()
}

fn overlay(cells: [u8; 40]) -> TeletextPhysicalBaseOverlay {
    TeletextPhysicalBaseOverlay::new(
        address(123),
        vec![TeletextPhysicalBaseRow::new(1, cells).unwrap()],
    )
    .unwrap()
}

fn compile_overlay(
    selection: &TeletextChannelSelectionSnapshot,
    source: TeletextPageSetDraft,
    cells: [u8; 40],
) -> Result<EncodedChannelTeletextOriginals, ChannelTeletextDocumentEncodingError> {
    encode_original_channel_documents_with_physical_overlays(
        selection,
        Some(ADDON),
        vec![TeletextProfileDocument::Base(source)],
        vec![overlay(cells)],
        deadline(),
        Some(TeletextEncodingRegion::WesternLatin),
        bounds(selection.channel_id).0,
    )
}

#[test]
fn synthetic_profiles_share_packet_preview_and_preserve_original_associations() {
    let first = snapshot(1, 1);
    let second = snapshot(2, 2);
    assert_eq!(first.bindings[0].addon_id, second.bindings[0].addon_id);
    assert_ne!(first.bindings[0].profile_id, second.bindings[0].profile_id);
    let mut previews = Vec::new();
    for (selection, colour, expected_rgb) in [
        (&first, TeletextColour::Red, [15, 0, 0]),
        (&second, TeletextColour::Blue, [0, 0, 15]),
    ] {
        let source = draft(colour, "ASCII");
        let before = original(selection, source.clone());
        let unchanged_page = packet(&before, address(124)).clone();
        let mut cells = row(&before);
        assert_eq!(cells[8], b'A');
        cells[8] = b'Z';
        let after = compile_overlay(selection, source.clone(), cells).unwrap();
        assert_eq!(row(&after), cells);
        assert_eq!(packet(&after, address(124)), &unchanged_page);
        assert_eq!(before.provenance, after.provenance);
        assert_eq!(before.metadata, after.metadata);
        assert_eq!(before.descriptor, after.descriptor);
        assert_eq!(before.v1_region, after.v1_region);
        assert_eq!(before.transmissions, after.transmissions);
        assert_eq!(before.producers.len(), 1);
        assert_eq!(after.producers.len(), 1);
        assert_eq!(
            before.producers[0].original(),
            after.producers[0].original()
        );
        assert_eq!(after.producers[0].original().semantic().base(), &source);
        assert!(after.producers[0]
            .original()
            .resource_inventory()
            .is_empty());
        let encoded = after.encoded.as_ref().unwrap();
        assert_eq!(encoded.declared().semantic().base(), &source);
        assert_eq!(
            encoded.declared().resource_inventory(),
            before
                .encoded
                .as_ref()
                .unwrap()
                .declared()
                .resource_inventory()
        );
        assert_eq!(
            encoded.magazine_units(),
            before.encoded.as_ref().unwrap().magazine_units()
        );
        let preview = project_teletext_preview(
            encoded,
            address(123),
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            bounds(selection.channel_id).1,
        )
        .unwrap();
        assert_eq!(preview.navigation(), source.pages[0].navigation);
        assert_eq!(preview.raster().owner(), address(123));
        assert!(preview.raster().diagnostics().is_empty());
        assert!(preview.raster().pixels().iter().any(|pixel| matches!(
            pixel,
            TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb)) if rgb.get() == expected_rgb
        )));
        let original_preview = project_teletext_preview(
            before.encoded.as_ref().unwrap(),
            address(123),
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            bounds(selection.channel_id).1,
        )
        .unwrap();
        assert_ne!(preview.raster(), original_preview.raster());
        assert_eq!(row(&before)[8], b'A');
        previews.push(preview.raster().clone());
    }
    assert_ne!(previews[0], previews[1]);
}

#[test]
fn unsupported_physical_control_returns_no_compiled_candidate() {
    let selection = snapshot(1, 1);
    let source = draft(TeletextColour::Red, "ASCII");
    let before = original(&selection, source.clone());
    let mut cells = row(&before);
    cells[7] = 0x0e;
    assert!(matches!(
        compile_overlay(&selection, source, cells),
        Err(ChannelTeletextDocumentEncodingError::Encoding(_))
    ));
    assert_ne!(row(&before)[7], 0x0e);
}

#[test]
fn authored_original_retains_inherited_physical_rows_through_legacy_wrapper() {
    let mut selection = snapshot(1, 1);
    let before = original(&selection, draft(TeletextColour::Red, "ASCII"));
    let mut cells = row(&before);
    cells[8] = b'Z';
    let declared = before.producers[0]
        .original()
        .clone()
        .with_physical_base_overlays(vec![overlay(cells)], bounds(1).0.validation.service)
        .unwrap();
    selection.bindings[0].contract = TeletextPageContract::V2;
    let inherited = encode_original_channel_documents(
        &selection,
        Some(ADDON),
        vec![TeletextProfileDocument::Authored(Box::new(
            declared.clone(),
        ))],
        deadline(),
        None,
        bounds(1).0,
    )
    .unwrap();
    assert_eq!(row(&inherited), cells);
    assert_eq!(inherited.producers[0].original(), &declared);
    cells[8] = b'Q';
    let replaced = encode_original_channel_documents_with_physical_overlays(
        &selection,
        Some(ADDON),
        vec![TeletextProfileDocument::Authored(Box::new(
            declared.clone(),
        ))],
        vec![overlay(cells)],
        deadline(),
        None,
        bounds(1).0,
    )
    .unwrap();
    assert_eq!(row(&replaced), cells);
    assert_eq!(row(&inherited)[8], b'Z');
    assert_eq!(replaced.producers[0].original(), &declared);
    assert_eq!(inherited.provenance, replaced.provenance);
    assert_eq!(inherited.metadata, replaced.metadata);
}

#[test]
fn changed_generated_fallback_returns_no_compiled_candidate() {
    let selection = snapshot(1, 1);
    let source = draft(TeletextColour::Red, "é™");
    let before = original(&selection, source.clone());
    let mut cells = row(&before);
    cells[8] = b'X';
    assert!(matches!(
        compile_overlay(&selection, source, cells),
        Err(ChannelTeletextDocumentEncodingError::Encoding(_))
    ));
}

#[test]
fn physical_rows_use_complete_composition_budget_before_encoding() {
    let selection = snapshot(1, 1);
    let source = draft(TeletextColour::Red, "ASCII");
    let before = original(&selection, source.clone());
    let mut policy = bounds(1).0;
    // Original two-page input is below 40 bytes; the extra exact row alone is
    // 40 bytes. This catches use of only the final semantic-service quota.
    policy.composition.max_input_content_bytes = 40;
    assert!(encode_original_channel_documents(
        &selection,
        Some(ADDON),
        vec![TeletextProfileDocument::Base(source.clone())],
        deadline(),
        Some(TeletextEncodingRegion::WesternLatin),
        policy,
    )
    .is_ok());
    let mut policy = bounds(1).0;
    policy.composition.max_input_content_bytes = 40;
    assert!(matches!(
        encode_original_channel_documents_with_physical_overlays(
            &selection,
            Some(ADDON),
            vec![TeletextProfileDocument::Base(source)],
            vec![overlay(row(&before))],
            deadline(),
            Some(TeletextEncodingRegion::WesternLatin),
            policy,
        ),
        Err(ChannelTeletextDocumentEncodingError::Composition(_))
    ));
}

#[test]
fn expired_original_deadline_returns_no_compiled_candidate() {
    let selection = snapshot(1, 1);
    assert!(matches!(
        encode_original_channel_documents_with_physical_overlays(
            &selection,
            Some(ADDON),
            vec![TeletextProfileDocument::Base(draft(
                TeletextColour::Red,
                "ASCII"
            ))],
            vec![overlay([b' '; 40])],
            Instant::now(),
            Some(TeletextEncodingRegion::WesternLatin),
            bounds(1).0,
        ),
        Err(ChannelTeletextDocumentEncodingError::TimedOut)
    ));
}
