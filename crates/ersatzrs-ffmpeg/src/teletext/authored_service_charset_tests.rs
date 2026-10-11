//! Shared repertoire consensus survives complete resource-backed normalization.

use super::*;

fn designation(column: u8, value: u8) -> TeletextEnhancementCommand {
    TeletextEnhancementCommand::AtColumn {
        column,
        operation: TeletextColumnOperation::DesignateCharset {
            designation: TeletextEnhancementCharsetDesignation::new(value).unwrap(),
        },
    }
}

fn shared_service(active_designation: u8) -> TeletextDeclaredService {
    let mut draft = input();
    for page in &mut draft.base.pages {
        page.rows.push(TeletextRow {
            row: 2,
            runs: vec![TeletextRun {
                column: 1,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: "B".into() },
            }],
        });
    }
    draft.pages[0].local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 5).unwrap(),
            },
            designation(5, 0),
            TeletextEnhancementCommand::InvokeObject {
                target: target(30),
                object_type: TeletextObjectType::Active,
                offset: None,
            },
            TeletextEnhancementCommand::AtColumn {
                column: 10,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(b'A').unwrap(),
                },
            },
        ])
        .unwrap(),
    );
    draft.objects.push(TeletextEnhancementObjectDefinition {
        identity: target(30),
        object_type: TeletextObjectType::Active,
        program: TeletextEnhancementProgram::new(vec![designation(0, active_designation)]).unwrap(),
    });
    declared(draft)
}

fn selected_glyph<'a>(
    display: &TeletextDisplayResolution,
    intent: &'a TeletextGlyphIntent,
) -> &'a TeletextExecutedGlyphIntent {
    let origin = display.cell(1, 10).unwrap().origin().unwrap();
    &intent.records()[display.origins()[origin].candidate_index()]
}

fn assert_alias_consensus(
    record: &TeletextExecutedGlyphIntent,
    charset: &TeletextCharsetExecution,
) {
    assert_eq!(record.disposition(), TeletextGlyphDisposition::Assigned);
    assert_eq!(
        record.selected_pair(),
        Some(TeletextCharsetPair::LatinLatin)
    );
    assert_eq!(record.reason(), None);
    let Some(TeletextGlyphIdentity::G0 { glyph }) = record.glyph() else {
        panic!("shared aliases must resolve an actual G0 identity");
    };
    assert_eq!(glyph.repertoire(), TeletextG0Repertoire::Latin);
    assert_eq!(glyph.code(), b'A');
    assert_eq!(glyph.national_subset(), None);
    let raw = record.context().selection().unwrap();
    assert_eq!(raw.value(), None);
    assert_eq!(raw.declaration(), None);
    let contenders: Vec<_> = charset.pending_contenders(raw.pending().unwrap()).collect();
    assert_eq!(contenders.len(), 2);
    assert!(contenders.iter().any(|declaration| {
        declaration.designation().get() == 0
            && declaration.source()
                == (TeletextEnhancementSource {
                    object: None,
                    command: 1,
                })
            && declaration.invocation().is_none()
    }));
    assert!(contenders.iter().any(|declaration| {
        declaration.designation().get() == 1
            && declaration.source()
                == (TeletextEnhancementSource {
                    object: Some(target(30)),
                    command: 0,
                })
            && declaration.invocation().is_some()
    }));
}

#[test]
fn shared_local_and_pop_aliases_preserve_pending_sources_and_full_physical_render() {
    let declared = shared_service(1);
    let saved = declared.clone();
    let tx = transmissions(&declared);
    let candidate = encode_teletext_authored_service(declared, &tx, &limits()).unwrap();
    assert_eq!(candidate.declared(), &saved);
    let EncodedTeletextAuthoredPage::CompiledDisplay(page) = &candidate.pages()[0] else {
        panic!("authored Local enhancement must use the normalized display path");
    };
    let original = page.normalized().original();
    let physical = page.normalized().physical().replay();
    let original_glyph = selected_glyph(original.display_resolution(), original.glyph_intent());
    let physical_glyph = selected_glyph(physical.display_resolution(), physical.glyph_intent());
    assert_alias_consensus(original_glyph, original.charset_execution());
    assert_alias_consensus(physical_glyph, physical.charset_execution());
    assert_eq!(original_glyph.glyph(), physical_glyph.glyph());
    let comparison = compare_teletext_physical_display(
        original,
        physical,
        limits().normalization.display_comparison,
    )
    .unwrap();
    assert_eq!(comparison.compared_cells(), 1000);
    assert!(comparison.diagnostics().is_empty());
    let budget = TeletextRasterBudget {
        max_pixels: 120_000,
        max_diagnostics: 1000,
    };
    for flash_phase in [TeletextFlashPhase::Original, TeletextFlashPhase::Alternate] {
        let options = TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase,
        };
        let before = render_teletext_composition(original, options, budget).unwrap();
        let after = render_teletext_physical_display(physical, options, budget).unwrap();
        assert!(before.diagnostics().is_empty());
        assert!(after.diagnostics().is_empty());
        assert_eq!(before.pixels(), after.pixels());
    }
    assert!(candidate.pages().iter().any(|page| matches!(page,
        EncodedTeletextAuthoredPage::Object(object)
            if object.command_positions().iter().any(|position| position.source()
                == (TeletextEnhancementSource { object: Some(target(30)), command: 0 }))
    )));
}

#[test]
fn divergent_shared_local_and_pop_repertoires_remain_located_and_return_no_candidate() {
    let declared = shared_service(0x37);
    let original = compose_teletext_main_program(
        declared.semantic(),
        owner(0),
        limits().normalization.composition,
    )
    .unwrap();
    let record = selected_glyph(original.display_resolution(), original.glyph_intent());
    let pending = record.context().selection().unwrap().pending().unwrap();
    assert_eq!(record.selected_pair(), None);
    assert_eq!(record.glyph(), None);
    assert_eq!(record.disposition(), TeletextGlyphDisposition::Pending);
    assert_eq!(
        record.reason(),
        Some(TeletextGlyphIntentReason::CharsetOrder { pending })
    );
    let raster = render_teletext_composition(
        &original,
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        TeletextRasterBudget {
            max_pixels: 120_000,
            max_diagnostics: 1000,
        },
    )
    .unwrap();
    assert!(raster
        .diagnostics()
        .iter()
        .any(|diagnostic| diagnostic.visual_position == (1, 10)));
    let tx = transmissions(&declared);
    let failure = encode_teletext_authored_service(declared, &tx, &limits()).unwrap_err();
    assert_eq!(failure.display, Some(owner(0)));
    assert!(matches!(failure.reason, Failure::Compiled(_)));
}

// Intentional opt-in Linux x64 decoder-ABI calibration, not platform acceptance.
#[cfg(all(target_os = "linux", target_arch = "x86_64"))]
#[tokio::test]
#[ignore = "Requires explicitly calibrated Linux x64 libzvbi 0.2.42"]
async fn complete_carousel_shared_charset_aliases_decode() {
    let declared = shared_service(1);
    let tx = transmissions(&declared);
    let service = encode_teletext_authored_service(declared, &tx, &limits()).unwrap();
    let mut carousel = TeletextCarousel::new_authored(service).unwrap();
    let mut units = Vec::new();
    let mut cycles = 0;
    while cycles < 2 {
        let batch = carousel.next_batch().unwrap();
        units.extend_from_slice(batch.data_units());
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            cycles += 1;
        }
    }
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version": "0.2.42",
        "cases": [{
            "name": "shared_local_and_pop_charset_alias_consensus",
            "units": units.iter().map(|unit| unit.to_vec()).collect::<Vec<_>>(),
            "page": 0x200,
            "subcode": 0,
            "cells": [{"row":1,"column":10,"unicode":65,"foreground":7,"background":0}],
        }],
    }))
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_CHARSET_CONSENSUS_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let script = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/e2e/teletext-object-decoder.py");
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(
        report,
        serde_json::json!({"decoder_version":"0.2.42","cases_passed":1})
    );
    println!("Independent shared charset alias decoding: {report}");
}
