//! One pinned independent decoder keeps its cache across actual carousel updates.
//! Linux x64 ABI calibration only; no client, sink or restart acceptance claim.

use ersatzrs_domain::*;

use super::authored_service_tests as fixtures;
use super::*;

const A: [u16; 10] = [
    0x801, 0x800, 0x400, 0x200, 0x100, 0x080, 0x040, 0x020, 0x010, 0x009,
];
const B: [u16; 10] = [
    0x802, 0x001, 0x002, 0x004, 0x008, 0x010, 0x020, 0x040, 0x080, 0x101,
];

fn candidate(
    pixels: [u16; 10],
    counter: u8,
    update: TeletextResourceFullUpdate,
) -> EncodedTeletextAuthoredService {
    let mut input = fixtures::input();
    input.base.pages.truncate(1);
    let address = input.base.pages[0].address;
    input.base.pages[0].rows = [(1, 8, "T"), (2, 1, "B")]
        .into_iter()
        .map(|(row, column, text)| TeletextRow {
            row,
            runs: vec![TeletextRun {
                column,
                style: TeletextStyle::default(),
                content: TeletextRunContent::Text { text: text.into() },
            }],
        })
        .collect();
    let resource = TeletextResourceIdentity::new(
        TeletextResourceKind::Drcs,
        TeletextRawDataPageNumber::new(2, 0xc0).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap();
    input.pages.truncate(1);
    let page = &mut input.pages[0];
    page.local_program = Some(
        TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, 8).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column: 8,
                operation: TeletextColumnOperation::Drcs {
                    reference: TeletextDrcsGlyphReference {
                        table: TeletextDrcsTableScope::Normal,
                        subtable: TeletextDrcsSubtableIndex::new(0).unwrap(),
                        slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                    },
                },
            },
        ])
        .unwrap(),
    );
    page.resources.objects = TeletextResolvedObjectPageLinks {
        public: None,
        global: None,
    };
    page.resources.defaults = [None; 4];
    page.resources.drcs = TeletextDrcsPageLinks::new(Some(resource.page()), None).unwrap();
    input.objects.clear();
    input.drcs = vec![TeletextDrcsSubtable::new(
        resource,
        vec![TeletextDrcsGlyphDefinition {
            slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
            bitmap: TeletextDrcsBitmap::new(pixels).unwrap(),
        }],
    )
    .unwrap()];
    let mot = fixtures::drcs_mot(resource, true);
    let selection = TeletextDisplayMotSelection {
        display: address,
        mot: Some(mot.identity()),
    };
    let mut semantic_budget = fixtures::semantic_limits();
    semantic_budget.max_collected_drcs_references = 1;
    let semantic = TeletextSemanticService::new(input, semantic_budget).unwrap();
    let declared = TeletextDeclaredService::new(
        semantic,
        vec![mot],
        vec![selection],
        vec![],
        fixtures::declaration_limits(),
    )
    .unwrap();
    let mut transmissions = fixtures::transmissions(&declared);
    for tx in &mut transmissions {
        if tx.resource == resource {
            tx.counter = TeletextResourceUpdateCounter::new(counter).unwrap();
            tx.update = update;
        } else {
            // The identical MOT stays unchanged through all four generations.
            tx.counter = TeletextResourceUpdateCounter::new(0).unwrap();
            tx.update = TeletextResourceFullUpdate::Unchanged;
        }
    }
    let mut budget = fixtures::limits();
    budget
        .normalization
        .composition
        .collection
        .max_drcs_references = 1;
    budget.normalization.replay.max_drcs_references = 1;
    encode_teletext_authored_service(declared, &transmissions, &budget).unwrap()
}

fn stage(carousel: &mut TeletextCarousel, state: &str, next_ticks: &mut u64) -> serde_json::Value {
    let mut units = Vec::new();
    let mut cycles = 0;
    while cycles < 2 {
        let batch = carousel.next_batch().unwrap();
        assert_eq!(batch.presentation_ticks(), *next_ticks);
        *next_ticks += 1_800;
        units.extend(batch.data_units().iter().map(|unit| unit.to_vec()));
        if batch.kind() == TeletextCarouselBatchKind::Terminator {
            cycles += 1;
        }
    }
    assert!(carousel.is_cycle_boundary());
    assert!(units.len() <= 256);
    serde_json::json!({"state":state,"units":units})
}

#[tokio::test]
#[ignore = "Requires Python 3 and the receipted original native libzvbi 0.2.42 decoder"]
async fn consecutive_complete_carousel_drcs_updates_decode_with_one_warm_cache() {
    let mut carousel =
        TeletextCarousel::new_authored(candidate(A, 7, TeletextResourceFullUpdate::Changed))
            .unwrap();
    let mut next_ticks = 0;
    let mut stages = vec![stage(&mut carousel, "a_changed", &mut next_ticks)];
    for (pixels, counter, update, label) in [
        (A, 7, TeletextResourceFullUpdate::Unchanged, "a_unchanged"),
        (B, 0, TeletextResourceFullUpdate::Changed, "b_changed"),
        (B, 0, TeletextResourceFullUpdate::Unchanged, "b_repeat"),
    ] {
        carousel
            .replace_authored_at_boundary(candidate(pixels, counter, update))
            .unwrap();
        stages.push(stage(&mut carousel, label, &mut next_ticks));
    }
    let request = serde_json::to_vec(&serde_json::json!({
        "expected_version":"0.2.42",
        "sequences":[{"name":"normal_drcs_update_wrap","stages":stages}],
    }))
    .unwrap();
    if let Some(path) =
        crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_RESOURCE_UPDATE_FIXTURE"])
    {
        std::fs::write(path, &request).unwrap();
    }
    let python = crate::locator::first_non_empty_env(&["ERSATZRS_TELETEXT_TEST_PYTHON"])
        .unwrap_or_else(|| "python3".into());
    let script = test_manifest_dir!().join("../../scripts/e2e/teletext-resource-update-decoder.py");
    let report =
        crate::teletext_probe::run_probe(&python, &[script.to_str().unwrap()], &request, 16 * 1024)
            .await
            .unwrap();
    let report: serde_json::Value = serde_json::from_slice(&report).unwrap();
    assert_eq!(report["decoder_version"], "0.2.42");
    assert_eq!(report["sequences_passed"], 1);
    assert_eq!(report["checkpoints_passed"], 4);
    println!("Independent warm-cache DRCS updates: {report}");
}
