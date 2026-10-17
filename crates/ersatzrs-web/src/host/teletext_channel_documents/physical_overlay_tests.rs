//! Synthetic selection snapshots exercise the shared compiler, not installation,
//! import permission, storage, client acceptance or publication/playback state.

#[path = "resource_operand_tests.rs"]
mod resource_operand_tests;

use std::time::{Duration, Instant};

use ersatzrs_app::teletext_design_documents::{
    append_teletext_design_editorial_overlay, append_teletext_effective_design_editorial_overlay,
    decode_teletext_design_documents, decode_teletext_effective_design_documents,
    TeletextDesignDocumentBudget, TeletextSavedEditorialOverlay, TeletextSavedEditorialProvenance,
};
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
    teletext_designs::{compile_restored_parts, NativeTeletextDesignError},
    teletext_effective_designs::compile_effective_restored_parts,
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

fn storage_budget() -> TeletextDesignDocumentBudget {
    TeletextDesignDocumentBudget {
        max_serialized_bytes: 128 * 1024,
        max_documents: 8,
        max_response_bytes: 64 * 1024,
        max_context_bytes: 32 * 1024,
        max_context_field_bytes: 2048,
        max_resource_transmissions: 8,
        validation: bounds(1).0.validation,
    }
}

fn stored_original(effective: bool) -> Vec<u8> {
    let mut response = serde_json::to_value(draft(TeletextColour::Red, "ASCII")).unwrap();
    response["schema_version"] = json!(2);
    response["generation"] = json!("saved-operand-replay");
    response["page_enhancements"] = json!([{
        "address":address(123),"presentation":null,"compositional_links":null,
        "local_program":{"commands":[
            {"kind":"set_position","position":{"row":1,"column":0}},
            {"kind":"at_column","column":8,"operation":{"kind":"foreground","colour":1}},
            {"kind":"at_column","column":8,"operation":{"kind":"g0","code":65}}
        ]},
        "resources":{"objects":{"public":null,"global":null},
            "drcs":{"normal":null,"global":null},"defaults":[null,null,null,null]}
    }]);
    for key in ["magazines", "objects", "drcs", "mots", "empty_object_pages"] {
        response[key] = json!([]);
    }
    response["mot_selections"] = json!([
        {"display":address(123),"mot":null},{"display":address(124),"mot":null}
    ]);
    let member = json!({"contract":2,"channel_id":1,"addon_id":ADDON,"binding_revision":1,
        "profile_id":1,"profile_revision":1,"effective_settings":[],
        "installation":{"addon_id":ADDON,"state_revision":"synthetic-installation-revision",
            "version":"1.0.0","package_sha256":"a".repeat(64),
            "content_sha256":"b".repeat(64),"manifest_sha256":"c".repeat(64)}});
    let mut value = json!({"format_version":1,"selection":{"channel_id":1,"bindings":[member.clone()]},
        "generation":"saved-operand-replay","entry_addon_id":ADDON,"entry":address(123),
        "v1_region":null,"edit_policy":{"policy_version":1,"kind":"original_only"},
        "resource_transmissions":[],"documents":[{"contract":2,"generation":"saved-operand-replay",
            "response":response.to_string()}]});
    if effective {
        let request = ersatzrs_app::teletext_guide::build_request_for_schema(
            &ersatzrs_app::teletext_guide::SuppliedTeletextGuideContext {
                generation: "saved-operand-replay",
                locale: "en",
                supported_locales: &["en"],
                timezone: "UTC",
                captured_at: "2026-10-15T12:00:00Z".parse().unwrap(),
                channels: &[],
            },
            2,
        )
        .unwrap();
        value.as_object_mut().unwrap().remove("selection");
        value["format_version"] = json!(2);
        value["effective_selection"] = json!({"channel_id":1,"valid_until":null,
            "bindings":[{"origin":"base","base":member,"scripted":null}]});
        value["producer_bindings"] = json!([0]);
        value["guide_inputs"] = json!({"input_version":1,"v1":null,
            "v2":std::str::from_utf8(&request).unwrap()});
    }
    serde_json::to_vec(&value).unwrap()
}

fn saved_overlay(
    operands: bool,
    rows: Vec<TeletextPhysicalBaseOverlay>,
) -> TeletextSavedEditorialOverlay {
    let provenance = TeletextSavedEditorialProvenance {
        export_receipt_id: 3,
        export_receipt_digest: "d".repeat(64),
        original_design_id: 7,
        original_revision: 2,
        original_digest: "e".repeat(64),
    };
    if operands {
        TeletextSavedEditorialOverlay::new_with_local_program_operands(
            provenance,
            rows,
            vec![TeletextLocalProgramOperandOverlay::new(
                address(123),
                1,
                8,
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(1).unwrap(),
                },
                TeletextColumnOperation::Foreground {
                    colour: TeletextColourReference::new(2).unwrap(),
                },
            )
            .unwrap()],
        )
        .unwrap()
    } else {
        TeletextSavedEditorialOverlay::new(provenance, rows).unwrap()
    }
}

fn stored_revision(effective: bool, document: Vec<u8>) -> TeletextDesignRevision {
    TeletextDesignRevision {
        design_id: 7,
        revision: 3,
        document_version: if effective { 2 } else { 1 },
        digest: "f".repeat(64),
        document,
    }
}

fn append_saved(
    effective: bool,
    original: &[u8],
    saved: &TeletextSavedEditorialOverlay,
) -> Vec<u8> {
    if effective {
        append_teletext_effective_design_editorial_overlay(original, saved, storage_budget())
            .unwrap()
            .into_bytes()
    } else {
        append_teletext_design_editorial_overlay(original, saved, storage_budget())
            .unwrap()
            .into_bytes()
    }
}

fn check_restored_operand(
    producers: &[TeletextProducerPresentation],
    encoded: &ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService,
    expected_colour: u8,
) -> Vec<Vec<[u8; 46]>> {
    let local = |service: &TeletextDeclaredService| {
        service
            .semantic()
            .page_enhancements()
            .get(&address(123))
            .unwrap()
            .local_program
            .as_ref()
            .unwrap()
            .commands()[1]
    };
    let foreground = |value| TeletextEnhancementCommand::AtColumn {
        column: 8,
        operation: TeletextColumnOperation::Foreground {
            colour: TeletextColourReference::new(value).unwrap(),
        },
    };
    assert_eq!(local(producers[0].original()), foreground(1));
    assert_eq!(local(encoded.declared()), foreground(expected_colour));
    assert_eq!(
        encoded.declared().semantic().base(),
        producers[0].original().semantic().base()
    );
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &encoded.pages()[0] else {
        panic!("saved local program must use complete native compilation");
    };
    let expected_word = 8 | (u32::from(expected_colour) << 11);
    let records = project_encoded_teletext_display(page.page(), 128)
        .unwrap()
        .records;
    assert!(records.iter().any(|record| matches!(record,
        TeletextEncodedDisplayRecord::Triplets { row: 26, words, .. } if words.contains(&expected_word))));
    let preview = project_teletext_preview(
        encoded,
        address(123),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        bounds(1).1,
    )
    .unwrap();
    assert_eq!(
        preview.navigation(),
        producers[0].original().semantic().base().pages[0].navigation
    );
    assert!(preview.raster().diagnostics().is_empty());
    assert!(preview.raster().pixels().iter().any(|pixel| matches!(pixel,
        TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(rgb))
        if rgb.get() == if expected_colour == 2 { [0, 15, 0] } else { [15, 0, 0] })));
    encoded
        .pages()
        .iter()
        .map(|page| page.data_units().to_vec())
        .collect()
}

fn replay_saved(
    effective: bool,
    document: &[u8],
    rows: Vec<TeletextPhysicalBaseOverlay>,
    expected_colour: u8,
) -> Result<Vec<Vec<[u8; 46]>>, NativeTeletextDesignError> {
    let stored = stored_revision(effective, document.to_vec());
    if effective {
        let parts = decode_teletext_effective_design_documents(document, storage_budget())
            .unwrap()
            .into_parts();
        let restored =
            compile_effective_restored_parts(stored, parts, bounds(1).0, deadline(), rows)?;
        Ok(check_restored_operand(
            restored.producers(),
            restored.encoded().unwrap(),
            expected_colour,
        ))
    } else {
        let parts = decode_teletext_design_documents(document, storage_budget())
            .unwrap()
            .into_parts();
        let restored = compile_restored_parts(stored, parts, bounds(1).0, deadline(), rows)?;
        Ok(check_restored_operand(
            restored.producers(),
            restored.encoded().unwrap(),
            expected_colour,
        ))
    }
}

#[test]
fn restored_compilers_refuse_unapplied_resource_intent_in_both_envelope_roles() {
    // Compiler admission only: this does not authenticate a resource edit against
    // a receipt. Even mixed typed intent must not be dropped by either role.
    for effective in [false, true] {
        for kind in [TeletextResourceKind::Pop, TeletextResourceKind::Gpop] {
            let resource = TeletextResourceIdentity::new(
                kind,
                TeletextRawDataPageNumber::new(2, 0xa0).unwrap(),
                TeletextResourceSubpageIndex::new(15).unwrap(),
            )
            .unwrap();
            let id = TeletextObjectId::new(65520);
            let target = if kind == TeletextResourceKind::Pop {
                TeletextObjectIdentity::public(resource, id)
            } else {
                TeletextObjectIdentity::global(resource, id)
            }
            .unwrap();
            let local = saved_overlay(true, vec![]);
            let saved = TeletextSavedEditorialOverlay::new_with_resource_object_operands(
                local.provenance().clone(),
                vec![],
                local.local_program_operands().to_vec(),
                vec![],
                vec![],
                vec![],
                vec![TeletextResourceObjectOperandOverlay::new(
                    target,
                    0,
                    8,
                    TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(1).unwrap(),
                    },
                    TeletextColumnOperation::Foreground {
                        colour: TeletextColourReference::new(2).unwrap(),
                    },
                )
                .unwrap()],
            )
            .unwrap();
            let document = stored_original(effective);
            let stored = stored_revision(effective, document.clone());
            let rejected = if effective {
                let mut parts =
                    decode_teletext_effective_design_documents(&document, storage_budget())
                        .unwrap()
                        .into_parts();
                parts.editorial_overlay = Some(saved);
                compile_effective_restored_parts(stored, parts, bounds(1).0, deadline(), vec![])
                    .map(|_| ())
            } else {
                let mut parts = decode_teletext_design_documents(&document, storage_budget())
                    .unwrap()
                    .into_parts();
                parts.editorial_overlay = Some(saved);
                compile_restored_parts(stored, parts, bounds(1).0, deadline(), vec![]).map(|_| ())
            };
            assert!(matches!(
                rejected,
                Err(NativeTeletextDesignError::EditorialValidationUnavailable)
            ));
        }
    }
}

#[test]
fn restored_compilers_apply_saved_operands_and_replay_v1_rows_exactly() {
    // Real decoding for both envelope roles, without import permission,
    // scripted selection, durable storage or publication claims.
    for effective in [false, true] {
        for (operands, include_rows) in [(false, true), (true, true), (true, false)] {
            let rows = if include_rows {
                vec![overlay(row(&original(
                    &snapshot(1, 1),
                    draft(TeletextColour::Red, "ASCII"),
                )))]
            } else {
                vec![]
            };
            let saved = saved_overlay(operands, rows.clone());
            assert_eq!(saved.schema_version(), if operands { 2 } else { 1 });
            let original = stored_original(effective);
            let document = append_saved(effective, &original, &saved);
            let unchanged = document.clone();
            let expected_colour = if operands { 2 } else { 1 };
            let first = replay_saved(effective, &document, rows.clone(), expected_colour).unwrap();
            let reloaded = replay_saved(effective, &document, rows, expected_colour).unwrap();
            assert_eq!(first, reloaded);
            assert_eq!(document, unchanged);
        }
    }
}

#[test]
fn stale_saved_operand_returns_no_candidate_in_both_envelope_roles() {
    for effective in [false, true] {
        let mut original: serde_json::Value =
            serde_json::from_slice(&stored_original(effective)).unwrap();
        let mut response: serde_json::Value =
            serde_json::from_str(original["documents"][0]["response"].as_str().unwrap()).unwrap();
        response["page_enhancements"][0]["local_program"]["commands"][1]["operation"]["colour"] =
            json!(3);
        original["documents"][0]["response"] = json!(response.to_string());
        let original = serde_json::to_vec(&original).unwrap();
        let saved = saved_overlay(true, vec![]);
        let rejected = if effective {
            append_teletext_effective_design_editorial_overlay(&original, &saved, storage_budget())
                .map(|_| ())
        } else {
            append_teletext_design_editorial_overlay(&original, &saved, storage_budget())
                .map(|_| ())
        };
        assert!(matches!(rejected.unwrap_err().reason,
            ersatzrs_app::teletext_design_documents::TeletextDesignDocumentFailure::EditorialOverlay));
        // The structural codec rejects first. The shared native compiler must
        // independently retain the exact-operand precondition for typed callers.
        let documents = if effective {
            decode_teletext_effective_design_documents(&original, storage_budget())
                .unwrap()
                .into_parts()
                .documents
        } else {
            decode_teletext_design_documents(&original, storage_budget())
                .unwrap()
                .into_parts()
                .documents
        };
        let Err(ChannelTeletextDocumentEncodingError::Composition(error)) =
            super::encode_original_producer_documents_with_operands(
                Some(0),
                documents,
                vec![],
                deadline(),
                None,
                bounds(1).0,
                saved.local_program_operands(),
            )
        else {
            panic!("stale exact-original operand must not escape into a native candidate");
        };
        assert!(matches!(error.reason,
            TeletextProducerCompositionFailure::Semantic(reason) if matches!(*reason,
                TeletextServiceError::LocalProgramOperandOverlay {
                    reason: TeletextLocalProgramOperandFailure::CommandMismatch, .. })));
    }
}

#[test]
fn stored_physical_rows_still_require_authenticated_reconstruction() {
    for effective in [false, true] {
        let saved = saved_overlay(true, vec![overlay([b' '; 40])]);
        let document = append_saved(effective, &stored_original(effective), &saved);
        assert!(matches!(
            replay_saved(effective, &document, vec![], 2),
            Err(NativeTeletextDesignError::EditorialValidationUnavailable)
        ));
    }
}
