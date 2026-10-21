//! Exact steady mixed-colour header carrier through native Local call interchange.
use super::*;
use ersatzrs_ffmpeg::teletext::TeletextMainCompositionSource;

const OUTER: u16 = 65534;
const CHILD: u16 = 65535;

fn response(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = authoring_metadata::response(theme, page, subcode);
    let owner = json!({"page":page,"subcode":subcode});
    let outer = json!({"scope":"local","owner":owner,"object_id":OUTER});
    let child = json!({"scope":"local","owner":owner,"object_id":CHILD});
    let style = json!({"foreground":"white","background":"black","height":"normal",
        "boxed":false,"flash":false,"conceal":false});
    let mut blue = style.clone();
    blue["background"] = json!("blue");
    value["service"]["pages"][0]["rows"]
        .as_array_mut()
        .unwrap()
        .push(json!({
        "row":0,"runs":[
            {"column":8,"style":style,"content":{"kind":"text","text":"Z"}},
            {"column":20,"style":blue,"content":{"kind":"text","text":"Z".repeat(20)}}
        ]}));
    value["service"]["magazines"][0]["presentation"]["panels"] = json!({
        "left":false,"right":false,"status":"level25_and35","width":0});
    value["service"]["page_enhancements"][0]["local_program"] = json!({"commands":[
        {"kind":"invoke_object","target":outer,"object_type":"active","offset":null}
    ]});
    value["service"]["objects"] = json!([
        {"identity":outer,"object_type":"active","program":{"commands":[
            {"kind":"set_position","position":{"row":0,"column":8}},
            {"kind":"invoke_object","target":child,"object_type":"passive","offset":null}
        ]}},
        {"identity":child,"object_type":"passive","program":{"commands":[
            {"kind":"at_column","column":0,"operation":{"kind":"foreground","colour":2}},
            {"kind":"at_column","column":0,"operation":{"kind":"g0","code":65}}
        ]}}
    ]);
    value["authoring"]["pages"][0]["regions"].as_array_mut().unwrap().push(json!({
        "id":"header-call-zone","kind":"zone","area":"main", "row":0,"column":8,"width":32,"height":1
    }));
    serde_json::from_value::<ersatzrs_addon_contract::TeletextPagesV3Response>(value.clone())
        .expect("steady mixed-row header source retains the closed V3 contract");
    value
}

fn target(flow: &Flow, stage: u8) -> EncodedTeletextAuthoredService {
    let mut value = response("day", 124, 32)["service"].clone();
    let limits = policy().encoding.validation;
    let parse = |value: &Value| {
        ersatzrs_app::teletext_pages_v2::parse_declared_service(
            &serde_json::to_vec(value).unwrap(),
            GENERATION,
            limits.service,
            limits.declarations,
        )
        .unwrap()
    };
    assert_eq!(parse(&value), *flow.accepted[0].service().declared());
    match stage {
        0 => {}
        1 => value["objects"][1]["program"]["commands"][1]["operation"]["code"] = json!(66),
        2 => {
            value["objects"][0]["program"]["commands"]
                .as_array_mut()
                .unwrap()
                .truncate(1);
        }
        _ => unreachable!(),
    }
    let expected = ersatzrs_ffmpeg::teletext::encode_teletext_authored_service(
        parse(&value),
        flow.saved[0].candidate().transmissions(),
        &policy().encoding.encoding,
    )
    .expect("independent complete steady-header target must qualify before import");
    assert_eq!(
        non_x26_records(flow.accepted[0].service()),
        non_x26_records(&expected)
    );
    expected
}

fn request(
    receipt: &TeletextDesignExportReceipt,
    head: i64,
    expected: &EncodedTeletextAuthoredService,
) -> NativeTeletextEditorialImportRequest {
    let mut request = drcs_bitmap::from_receipt(receipt, head);
    "Steady mixed-colour Local header call structure".clone_into(&mut request.name);
    replace_packets(&mut request, native_words(expected));
    assert_request_native(&request, expected);
    request
}

fn original_carrier(service: &EncodedTeletextAuthoredService) -> usize {
    let positions = encoded(service).command_positions();
    let carrier: Vec<_> = positions
        .iter()
        .filter(|position| {
            position.source().object == Some(identity(OUTER))
                && position.original_object_source()
                    == Some(TeletextMainCompositionSource::HeaderPosition { command: 0 })
        })
        .collect();
    assert_eq!(carrier.len(), 1);
    let carrier = carrier[0];
    assert_eq!(carrier.first_word(), carrier.semantic_word());
    assert_eq!(
        native_words(service)[carrier.semantic_word()],
        word(8, 7, 0)
    );
    assert_eq!(carrier.original_local_command(), None);
    assert_eq!(carrier.original_base_glyph(), None);
    let child: Vec<_> = positions
        .iter()
        .filter(|position| {
            position.original_object_source()
                == Some(TeletextMainCompositionSource::Object {
                    identity: identity(CHILD),
                    command: 1,
                })
        })
        .collect();
    assert_eq!(child.len(), 1);
    assert_eq!(
        native_words(service)[child[0].semantic_word()],
        word(0, 9, 65)
    );
    carrier.semantic_word()
}

fn self_index_preview(service: &EncodedTeletextAuthoredService, page: u16) -> TeletextRaster {
    assert_eq!(page, 124);
    let preview = project_teletext_preview(
        service,
        channel_documents::address(page, 32),
        TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: TeletextFlashPhase::Original,
        },
        preview_budget(),
    )
    .unwrap();
    assert!(preview.raster().diagnostics().is_empty());
    assert_eq!(
        preview.navigation(),
        &[TeletextNavigationLink {
            role: TeletextNavigationRole::Index,
            target: TeletextNavigationTarget {
                page: TeletextPageNumber::new(124).unwrap(),
                subcode: None
            },
        }]
    );
    preview.raster().clone()
}

async fn fresh(
    fixture: &Fixture,
    concept: &SavedEditorialTeletextDesign,
) -> TeletextDesignExportReceipt {
    let exported = drcs_bitmap::reexport(fixture, concept).await;
    receipt(fixture, &exported).await
}

async fn import_stage(
    fixture: &Fixture,
    request: NativeTeletextEditorialImportRequest,
    stage: &'static str,
) -> SavedEditorialTeletextDesign {
    let result = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_or_else(|error| panic!("{stage}: {error:?}"));
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("{stage}: authorized native edit must save a concept: {result:?}");
    };
    saved
}

async fn scalar_roundtrip(
    fixture: &Fixture,
    flow: &Flow,
    expected: &EncodedTeletextAuthoredService,
) -> TeletextDesignExportReceipt {
    let concept = import_stage(
        fixture,
        request(&flow.receipts[0], 1, expected),
        "original child scalar A-to-B",
    )
    .await;
    assert_eq!(concept.head().revision, 2);
    let restored = drcs_bitmap::restored(fixture, &concept).await;
    assert_eq!(restored.encoded().unwrap().declared(), expected.declared());
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(expected)
    );
    let current = fresh(fixture, &concept).await;
    let undone = import_stage(
        fixture,
        request(&current, 2, flow.accepted[0].service()),
        "child scalar exact root undo",
    )
    .await;
    assert_eq!(undone.head().revision, 3);
    assert_eq!(
        drcs_bitmap::stored(fixture, &undone).await.document,
        flow.originals[0].document
    );
    fresh(fixture, &undone).await
}

async fn forged_carrier_refuses(fixture: &Fixture, flow: &Flow, carrier: usize) {
    let mut forged = request(&flow.receipts[0], 1, flow.accepted[0].service());
    let mut words = native_words(flow.accepted[0].service());
    words[carrier] = word(8, 7, 1);
    replace_packets(&mut forged, words);
    let error = import_teletext_editorial_design(
        &fixture.state.application,
        forged,
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert!(
        matches!(error, NativeTeletextEditorialImportError::Tti(_)),
        "derived steady carrier must stay protected: {error:?}"
    );
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_head(flow.originals[0].design_id)
            .await
            .unwrap()
            .unwrap()
            .revision,
        1
    );
    assert_original_pointers_with_width(fixture, flow, 480).await;
}

fn forged_structural_words(
    original: &EncodedTeletextAuthoredService,
    carrier: usize,
    case: u8,
) -> Vec<u32> {
    let mut words = native_words(original);
    assert_eq!(words[carrier], word(8, 7, 0));
    match case {
        0 => words[carrier] = word(9, 7, 0),
        1 => words[carrier] = word(8, 7, 1),
        2 => words[carrier] = word(8, 7, 4),
        3 => {
            words.insert(carrier, words[carrier]);
            // Keep relocated native object markers/calls valid: refusal must
            // protect the extra derived carrier, not a broken child pointer.
            for marker in encoded(original)
                .definition_positions()
                .iter()
                .map(|position| usize::from(position.absolute_index()))
                .filter(|marker| *marker > carrier)
            {
                for value in &mut words {
                    for mode in 17..=23 {
                        if *value == pointer(marker, mode) {
                            *value = pointer(marker + 1, mode);
                            break;
                        }
                    }
                }
            }
        }
        _ => unreachable!(),
    }
    words
}

async fn forged_structural_carriers_refuse(
    fixture: &Fixture,
    flow: &Flow,
    receipt: &TeletextDesignExportReceipt,
    original: &EncodedTeletextAuthoredService,
    carrier: usize,
) {
    for case in 0..4 {
        let mut forged = request(receipt, 4, original);
        replace_packets(
            &mut forged,
            forged_structural_words(original, carrier, case),
        );
        let error = import_teletext_editorial_design(
            &fixture.state.application,
            forged,
            policy(),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        assert!(
            matches!(error, NativeTeletextEditorialImportError::Tti(_)),
            "structural derived carrier {case} must refuse: {error:?}"
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_head(flow.originals[0].design_id)
                .await
                .unwrap()
                .unwrap()
                .revision,
            4
        );
        assert!(fixture
            .state
            .application
            .teletext_design_revision(
                flow.originals[0].design_id,
                5,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .is_none());
        assert_eq!(
            qtm_carrier::stored_receipt(fixture, receipt.metadata.id).await,
            *receipt
        );
        assert_original_pointers_with_width(fixture, flow, 480).await;
    }
}

#[tokio::test]
async fn steady_header_flash_carrier_preserves_native_scalar_call_deletion_and_exact_root_undo() {
    let (mut fixture, flow) = Box::pin(setup_with_response(response)).await;
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let original = target(&flow, 0);
    let scalar = target(&flow, 1);
    let removed = target(&flow, 2);
    assert_eq!(
        designs::units(&original),
        designs::units(flow.accepted[0].service())
    );
    let carrier = original_carrier(&original);
    let unchanged = import_teletext_editorial_design(
        &fixture.state.application,
        drcs_bitmap::from_receipt(&flow.receipts[0], 1),
        policy(),
        designs::deadline(),
    )
    .await;
    assert!(matches!(
        unchanged,
        Err(NativeTeletextEditorialImportError::NoChanges)
    ));
    Box::pin(forged_carrier_refuses(&fixture, &flow, carrier)).await;
    let current = Box::pin(scalar_roundtrip(&fixture, &flow, &scalar)).await;
    // Qualification expects a supported Local call removal to save. Preserve
    // any actual parser refusal; do not turn it into an expected-red assertion.
    let concept = import_stage(
        &fixture,
        request(&current, 3, &removed),
        "Local call deletion to AP-only",
    )
    .await;
    assert_eq!(concept.head().revision, 4);
    let intent = TeletextLocalObjectGlyphProgramOverlay::new_with_invocation_structure(
        identity(OUTER),
        original
            .declared()
            .semantic()
            .objects()
            .definition(identity(OUTER))
            .unwrap()
            .program
            .clone(),
        removed
            .declared()
            .semantic()
            .objects()
            .definition(identity(OUTER))
            .unwrap()
            .program
            .clone(),
        vec![Some(0)],
    )
    .unwrap();
    Box::pin(assert_call_concept_with_preview(
        &fixture,
        &flow,
        &concept,
        &removed,
        &intent,
        21,
        self_index_preview,
    ))
    .await;
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    fixture = fixture.reopen().await;
    Box::pin(assert_call_concept_with_preview(
        &fixture,
        &flow,
        &concept,
        &removed,
        &intent,
        21,
        self_index_preview,
    ))
    .await;
    let current = fresh(&fixture, &concept).await;
    Box::pin(forged_structural_carriers_refuse(
        &fixture, &flow, &current, &original, carrier,
    ))
    .await;
    let undone = import_stage(
        &fixture,
        request(&current, 4, &original),
        "Local call exact root undo",
    )
    .await;
    assert_eq!(undone.head().revision, 5);
    assert_eq!(
        drcs_bitmap::stored(&fixture, &undone).await.document,
        flow.originals[0].document
    );
    let restored = drcs_bitmap::restored(&fixture, &undone).await;
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        designs::units(&original)
    );
    fixture = fixture.reopen().await;
    assert_original_pointers_with_width(&fixture, &flow, 480).await;
    retained.assert_unchanged_width(&fixture, &flow, 480).await;
    assert_retained_storage(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
