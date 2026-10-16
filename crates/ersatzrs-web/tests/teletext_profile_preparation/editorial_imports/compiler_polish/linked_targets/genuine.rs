//! One live receipt joins three actual saves to native publication and TCP.

use super::super::super::{client_capture, ordinary_stream_flow};
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::PathBuf, time::Duration};

#[path = "genuine/inputs.rs"]
mod inputs;

#[derive(Clone, Copy)]
enum Case {
    ExplicitLinks,
    ImplicitMot,
    RenderedDefaults,
    EnglishTwoPairs,
    EnglishAlias,
}

impl Case {
    fn assert_originals(self, flow: &Flow) {
        match self {
            Self::ExplicitLinks => oracle::assert_originals(flow),
            Self::ImplicitMot => implicit_mot::assert_originals(flow),
            Self::RenderedDefaults => implicit_mot::defaults::assert_originals(flow),
            Self::EnglishTwoPairs => english::assert_originals(flow),
            Self::EnglishAlias => english::assert_originals_with_primary(flow, 0x10),
        }
    }

    fn assert_service(self, flow: &Flow, service: &EncodedTeletextAuthoredService) {
        match self {
            Self::ExplicitLinks => oracle::assert_service(flow, service),
            Self::ImplicitMot => implicit_mot::assert_effective(flow, service),
            Self::RenderedDefaults => implicit_mot::defaults::assert_effective(flow, service),
            Self::EnglishTwoPairs => english::assert_service(flow, service),
            Self::EnglishAlias => english::assert_service_with_primary(flow, service, 0x10),
        }
    }

    fn code(self) -> u8 {
        if matches!(self, Self::EnglishTwoPairs | Self::EnglishAlias) {
            0x7c
        } else {
            0x5b
        }
    }

    fn previews(self, service: &EncodedTeletextAuthoredService) -> Vec<TeletextRaster> {
        if matches!(self, Self::EnglishAlias) {
            english::previews_with_primary(service, 0x10)
        } else if matches!(self, Self::EnglishTwoPairs) {
            english::previews(service)
        } else {
            oracle::previews(service, 0x5b)
        }
    }

    fn annotate_proof(self, proof: &mut Value) {
        if matches!(self, Self::ImplicitMot) {
            proof["fixture"] = json!("implicit-mot-three-linked-targets");
            proof["original_mot"] = json!("1FE/S9");
            proof["original_resource_inventory_count"] = json!(5);
            proof["original_mot_and_resolved_roles_defaults_preserved"] = json!(true);
            if let Some(resources) = proof.get_mut("authored_resources") {
                resources.as_array_mut().unwrap().push(json!("1FE/S9"));
            }
        }
        if matches!(self, Self::RenderedDefaults) {
            proof["fixture"] = json!("implicit-mot-rendered-defaults-three-linked-targets");
            proof["original_mot"] = json!("1FE/S9");
            proof["original_resource_inventory_count"] = json!(7);
            proof["actual_default_slots"] = json!([0, 2, 3]);
            proof["default_objects"] = json!([
                {"slot":0,"resource":"2D0/S0","object_id":20,"type":"active"},
                {"slot":2,"resource":"2C0/S0","object_id":22,"type":"adaptive"},
                {"slot":3,"resource":"2C0/S0","object_id":21,"type":"passive"}]);
            proof["visible_passive_p_green_4_1_checked"] = json!(true);
            proof["original_mot_and_resolved_roles_defaults_preserved"] = json!(true);
            proof["original_mot_pop_gpop_raw_assets_byte_exact"] = json!(true);
            if let Some(resources) = proof.get_mut("authored_resources") {
                resources.as_array_mut().unwrap().extend([
                    json!("1FE/S9"),
                    json!("2D0/S0"),
                    json!("2C0/S0"),
                ]);
            }
        }
        if matches!(self, Self::EnglishTwoPairs | Self::EnglishAlias) {
            proof["fixture"] = json!("english-two-pair-three-linked-targets");
            proof["original_resource_inventory_count"] = json!(4);
            proof["original_charset_pairs"] = json!(["latin_latin", "greek_greek"]);
            proof["third_hebrew_pair_added"] = json!(false);
            proof["unrelated_greek_4_12_preserved"] = json!(true);
            proof["literal_original_english_7c_preserved"] = json!(true);
            proof["slot"] = json!(8);
            proof["mask"] = json!(english::ENGLISH_7C);
        }
        if matches!(self, Self::EnglishAlias) {
            proof["fixture"] = json!("english-primary10-alias-three-linked-targets");
            proof["original_primary"] = json!(0x10);
            proof["original_source_primary10_preserved"] = json!(true);
            proof["actual_x28_primary10_preserved"] = json!(true);
        }
    }
}

async fn setup_case(case: Case, fixture: Fixture) -> (Fixture, Flow) {
    match case {
        Case::ExplicitLinks => {
            Box::pin(setup_with_fixture_policy_and_writer(
                input::response,
                fixture,
                input::encoding,
                authoring_metadata::write_compact,
            ))
            .await
        }
        Case::ImplicitMot => {
            Box::pin(setup_with_fixture_policy_and_writer(
                implicit_mot::response,
                fixture,
                implicit_mot::encoding,
                authoring_metadata::write_compact,
            ))
            .await
        }
        Case::RenderedDefaults => {
            Box::pin(setup_with_fixture_policy_and_writer(
                implicit_mot::defaults::response,
                fixture,
                implicit_mot::defaults::encoding,
                authoring_metadata::write_compact,
            ))
            .await
        }
        Case::EnglishTwoPairs => {
            Box::pin(setup_with_fixture_policy_and_writer(
                english::response,
                fixture,
                input::encoding,
                authoring_metadata::write_compact,
            ))
            .await
        }
        Case::EnglishAlias => {
            Box::pin(setup_with_fixture_policy_and_writer(
                english::alias_response,
                fixture,
                input::encoding,
                authoring_metadata::write_compact,
            ))
            .await
        }
    }
}

async fn assert_case_restored(
    case: Case,
    fixture: &Fixture,
    flow: &Flow,
    concept: &SavedEditorialTeletextDesign,
) -> Vec<TeletextRaster> {
    if matches!(case, Case::EnglishAlias) {
        return Box::pin(english::restored_with_primary(fixture, flow, concept, 0x10)).await;
    }
    if matches!(case, Case::EnglishTwoPairs) {
        return Box::pin(english::restored(fixture, flow, concept)).await;
    }
    let previews = Box::pin(assert_restored(fixture, flow, concept)).await;
    if matches!(case, Case::ImplicitMot | Case::RenderedDefaults) {
        let restored = drcs_bitmap::restored(fixture, concept).await;
        case.assert_service(flow, restored.encoded().unwrap());
    }
    previews
}

fn assert_accepted(case: Case, flow: &Flow, accepted: &[AcceptedTeletextOutputCandidate; 2]) {
    case.assert_service(flow, accepted[0].service());
    assert_eq!(accepted[0].selection(), flow.accepted[0].selection());
    generations::assert_same_output(&accepted[1], &flow.accepted[1]);
    assert!(accepted[1].service().compiler_drcs_resources().is_empty());
    let day = &accepted[0].selection().bindings[0];
    let night = &accepted[1].selection().bindings[0];
    assert_eq!(day.addon_id, night.addon_id);
    assert_ne!(day.profile_id, night.profile_id);
}

async fn publish_actual(
    case: Case,
    mut fixture: Fixture,
    flow: &Flow,
    directory: &Path,
) -> (Fixture, [AcceptedTeletextOutputCandidate; 2], Value) {
    let (request, proofs) = inputs::request(case, flow, directory);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let concept = import(&fixture, request).await;
    assert_eq!(concept.head().id, flow.saved[0].head().id);
    assert_eq!(concept.head().revision, 2);
    assert_original_pointers(&fixture, flow).await;
    retained.assert_unchanged(&fixture, flow).await;
    let previews = Box::pin(assert_case_restored(case, &fixture, flow, &concept)).await;
    fixture = fixture.reopen().await;
    assert_original_pointers(&fixture, flow).await;
    retained.assert_unchanged(&fixture, flow).await;
    assert_eq!(
        Box::pin(assert_case_restored(case, &fixture, flow, &concept)).await,
        previews
    );
    let day = publish_and_accept(&fixture, flow, &concept).await;
    let accepted = [day, recover(&fixture, 2).await];
    assert_accepted(case, flow, &accepted);
    assert_eq!(case.previews(accepted[0].service()), previews);
    let exported = drcs_bitmap::reexport(&fixture, &concept).await;
    let fresh = receipt(&fixture, &exported).await;
    oracle::assert_mapping(&exported);
    if matches!(case, Case::ImplicitMot) {
        let original_mot = flow.exported[0]
            .files()
            .iter()
            .find(|file| file.name == implicit_mot::mot_file(&flow.exported[0]))
            .unwrap();
        let exported_mot = exported
            .files()
            .iter()
            .find(|file| file.name == implicit_mot::mot_file(&exported))
            .unwrap();
        assert_eq!(exported_mot.bytes, original_mot.bytes);
    }
    if matches!(case, Case::RenderedDefaults) {
        implicit_mot::defaults::assert_protected_reexport(flow, &exported);
    }
    fixture = fixture.reopen().await;
    case.assert_service(flow, recover(&fixture, 1).await.service());
    assert_eq!(
        case.previews(recover(&fixture, 1).await.service()),
        previews
    );
    assert_pointers(&fixture, &accepted[0], &accepted[1]).await;
    retained.assert_unchanged(&fixture, flow).await;
    assert_eq!(
        qtm_carrier::stored_receipt(&fixture, fresh.metadata.id).await,
        fresh
    );
    let mut proof = json!({"probe_version":1,
        "scope":"same live fresh receipt, three genuine pinned QTM Base edits, native concept/shared preview/explicit publication/reopen and three mixed indexed reexports",
        "editor_proofs":proofs,"input_hashes":inputs::hashes(flow,directory),
        "design_id":concept.head().id,"revision":2,"digest":concept.digest(),
        "generation_id":accepted[0].generation().id,"request_id":accepted[0].generation().request.id,
        "shared_three_page_previews_equal":true,"mixed_bank_assets":exported.sidecar().project.assets,
        "generated_normal_pages":["2A0","2A2","2A3"],"generated_s1":1,"slot":0,"mask":MASK,
        "original_authored_banks_byte_exact":true,"night_unchanged":true,
        "ordinary_delivery_proven":false,"independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false});
    case.annotate_proof(&mut proof);
    polish_delivery::write_json(directory, "native-proof.json", &proof);
    (fixture, accepted, proofs)
}

async fn check_state(
    case: Case,
    fixture: &Fixture,
    flow: &Flow,
    retained: &resource_bank_admin::Retained,
    accepted: &[AcceptedTeletextOutputCandidate; 2],
    directory: &Path,
    hashes: &[(String, String)],
) {
    retained.assert_unchanged(fixture, flow).await;
    assert_pointers(fixture, &accepted[0], &accepted[1]).await;
    assert_accepted(case, flow, accepted);
    case.assert_service(flow, recover(fixture, 1).await.service());
    assert_eq!(inputs::hashes(flow, directory), hashes);
}

async fn run(case: Case, directory: PathBuf) {
    std::fs::create_dir(&directory).expect("three-target live proof destination must be new");
    live_updates::live_tcp::terminal_trace(&directory).unwrap();
    let tools = drcs_bitmap_delivery::Tools::load().await;
    let (fixture, source) = drcs_bitmap_delivery::playback_fixture(&tools, &directory).await;
    let (fixture, flow) = Box::pin(setup_case(case, fixture)).await;
    case.assert_originals(&flow);
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    inputs::await_editor(case, &fixture, &flow, &directory).await;
    retained.assert_unchanged(&fixture, &flow).await;
    let hashes = inputs::hashes(&flow, &directory);
    let (mut fixture, accepted, proofs) =
        Box::pin(publish_actual(case, fixture, &flow, &directory)).await;
    check_state(
        case, &fixture, &flow, &retained, &accepted, &directory, &hashes,
    )
    .await;
    compiler_polish_delivery::assert_source(&fixture, &directory, &source);
    ordinary_stream_flow::reset_source_start(&fixture).await;
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        let (day, night) = tokio::join!(drcs_bitmap_delivery::capture_tcp(origin.url(1)),drcs_bitmap_delivery::capture_tcp(origin.url(2)));
        let day = polish_delivery::verify_capture(&directory,&tools,&source,1,&day,&accepted[0]).await;
        let night = polish_delivery::verify_capture(&directory,&tools,&source,2,&night,&accepted[1]).await;
        check_state(case,&fixture,&flow,&retained,&accepted,&directory,&hashes).await;
        compiler_polish_delivery::assert_source(&fixture,&directory,&source);
        let mut proof = json!({"probe_version":1,
            "route":"/iptv/hls-direct/{channel}.ts","transport":"actual owned application loopback TCP host",
            "scope":"three genuine same-live QTM saves, native shared preview/publication and cold two-profile ordinary delivery",
            "source_file":"source.ts","source_sha256":live_client::hash(&source),"source_bytes":source.len(),
            "channels":[day,night],"editor_proofs":proofs,"input_hashes":hashes,
            "generated_normal_pages":["2A0","2A2","2A3"],"generated_s1":1,"slot":0,
            "authored_resources":["2A0/S0","2A2/S0","2A3/S0","1A1/S0"],
            "independent_decoding_proven":false,"actual_client_proven":false,"full_level25_proven":false});
        case.annotate_proof(&mut proof);
        polish_delivery::write_json(&directory,"stream-proof.json",&proof);
    }).catch_unwind().await;
    let closed = origin.close().await;
    let cleaned = live_updates::live_tcp::wait_for_cleanup().await;
    if let Err(panic) = outcome {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    closed.expect("owned three-target origin must close");
    cleaned.expect("both response producers must release capacity");
    fixture = fixture.reopen().await;
    check_state(
        case, &fixture, &flow, &retained, &accepted, &directory, &hashes,
    )
    .await;
    compiler_polish_delivery::assert_source(&fixture, &directory, &source);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
    let mut proof = json!({"probe_version":1,
        "genuine_three_qtm_native_preview_publication_ordinary_av_transport_model_reopen":true,
        "input_hashes":hashes,"origin_closed":true,"capacity_released":true,
        "independent_decoding_proven":false,"actual_client_proven":false,"full_engine_acceptance":false});
    case.annotate_proof(&mut proof);
    polish_delivery::write_json(&directory, "success.json", &proof);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires three actual pinned QTM saves after this live fresh export and FFmpeg9/ffprobe/TSDuck; bounded1200s; independent/client acceptance is separate"]
async fn actual_qtm_three_linked_targets_concept_serves_two_ordinary_channels() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_LINKED_TARGETS_DIR",
    ));
    Box::pin(run(Case::ExplicitLinks, directory)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires three actual pinned QTM saves from this live implicit-MOT export and FFmpeg9/ffprobe/TSDuck; bounded1200s; no independent/client acceptance"]
async fn actual_qtm_implicit_mot_three_linked_targets_concept_serves_two_ordinary_channels() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_IMPLICIT_MOT_DIR",
    ));
    Box::pin(run(Case::ImplicitMot, directory)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires three actual pinned QTM saves from this live rendered-defaults MOT export and FFmpeg9/ffprobe/TSDuck; bounded1200s; no independent/client acceptance"]
async fn actual_qtm_mot_rendered_defaults_three_linked_targets_concept_serves_two_ordinary_channels(
) {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_MOT_DEFAULTS_DIR",
    ));
    Box::pin(run(Case::RenderedDefaults, directory)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires three actual pinned QTM saves from this live English two-pair export and FFmpeg9/ffprobe/TSDuck; bounded1200s; no independent/client acceptance"]
async fn actual_qtm_english_two_pair_three_linked_targets_concept_serves_two_ordinary_channels() {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_ENGLISH_TWO_PAIR_DIR",
    ));
    Box::pin(run(Case::EnglishTwoPairs, directory)).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires three actual pinned QTM saves from this live raw-primary10 English alias export and FFmpeg9/ffprobe/TSDuck; bounded1200s; no independent/client acceptance"]
async fn actual_qtm_english_primary10_alias_three_linked_targets_concept_serves_two_ordinary_channels(
) {
    let directory = PathBuf::from(client_capture::selected(
        "ERSATZRS_TELETEXT_ENGLISH_ALIAS_DIR",
    ));
    Box::pin(run(Case::EnglishAlias, directory)).await;
}
