//! Resource-rich guide-sized synthetic workload under finite experimental policy.
//! This proves native flow/admission, not production defaults, RSS or client playback.

use super::resource_measurement as rich;
use super::*;
use crate::designs::{budget as small_document_budget, config, deadline};
use ersatzrs_app::{
    teletext_design_documents::TeletextDesignDocumentBudget,
    teletext_pages_v2::parse_declared_service,
};
use ersatzrs_core::repositories::{
    TeletextDesignMutation, TeletextGenerationMutation, TeletextPublicationMutation,
};
use ersatzrs_web::host::{
    teletext_designs::{
        reload_original_teletext_design_revision, request_original_teletext_design_publication,
        save_original_teletext_design_candidate, NativeTeletextDesignError,
        SavedOriginalTeletextDesign,
    },
    teletext_generations::{
        accept_original_teletext_generation, reload_accepted_teletext_generation,
        AcceptedTeletextOutputCandidate, NativeTeletextGenerationError, TeletextGenerationTarget,
    },
    teletext_preview::{project_teletext_preview, TeletextPreviewBudget},
};
use std::{collections::BTreeSet, time::Instant};

fn set_credit(budget: &mut TeletextAuthoredServiceEncodingBudget, name: &str, maximum: usize) {
    match name {
        "display_updates" => budget.max_display_updates = Some(maximum),
        "glyph_comparison_work" => budget.max_glyph_comparison_work = Some(maximum),
        "display_comparison_updates" => budget.max_display_comparison_updates = Some(maximum),
        "realization_updates" => budget.max_realization_updates = Some(maximum),
        "placement_work" => budget.max_placement_work = Some(maximum),
        "assembly_work" => budget.max_assembly_work = Some(maximum),
        "ancestry_work" => budget.max_ancestry_work = Some(maximum),
        "collected_commands" => budget.max_collected_commands = Some(maximum),
        "collected_drcs_references" => budget.max_collected_drcs_references = Some(maximum),
        "spatial_event_visits" => budget.max_spatial_event_visits = Some(maximum),
        "spatial_invocation_visits" => budget.max_spatial_invocation_visits = Some(maximum),
        "identity_checks" => budget.max_identity_checks = Some(maximum),
        "attribute_cell_updates" => budget.max_attribute_cell_updates = Some(maximum),
        _ => panic!("unknown trusted workload bucket"),
    }
}

fn credits(usage: TeletextAuthoredServiceEncodingUsage) -> [(&'static str, usize); 13] {
    [
        ("display_updates", usage.display_updates.unwrap()),
        (
            "glyph_comparison_work",
            usage.glyph_comparison_work.unwrap(),
        ),
        (
            "display_comparison_updates",
            usage.display_comparison_updates.unwrap(),
        ),
        ("realization_updates", usage.realization_updates.unwrap()),
        ("placement_work", usage.placement_work.unwrap()),
        ("assembly_work", usage.assembly_work.unwrap()),
        ("ancestry_work", usage.ancestry_work.unwrap()),
        ("collected_commands", usage.collected_commands.unwrap()),
        (
            "collected_drcs_references",
            usage.collected_drcs_references.unwrap(),
        ),
        ("spatial_event_visits", usage.spatial_event_visits.unwrap()),
        (
            "spatial_invocation_visits",
            usage.spatial_invocation_visits.unwrap(),
        ),
        ("identity_checks", usage.identity_checks.unwrap()),
        (
            "attribute_cell_updates",
            usage.attribute_cell_updates.unwrap(),
        ),
    ]
}

fn usage_json(usage: TeletextAuthoredServiceEncodingUsage) -> Value {
    Value::Object(
        credits(usage)
            .into_iter()
            .map(|(name, count)| (name.to_owned(), json!(count)))
            .collect(),
    )
}

fn workload_policy() -> TeletextChannelDocumentEncodingPolicy {
    let mut policy = rich::resource_policy();
    policy.input.max_producers = 2;
    policy.input.max_response_bytes = 1024 * 1024;
    policy.validation.service.max_records = 144;
    policy.validation.service.max_collected_drcs_references = 512;
    policy.validation.declarations.max_displays = 128;
    policy.composition.service = policy.validation.service;
    policy.composition.declarations = policy.validation.declarations;
    policy.composition.max_producers = 2;
    policy.composition.max_storage_bytes = 32 * 1024 * 1024;
    policy.encoding.max_pages = 160;
    policy.encoding.max_units = 6400;
    policy.entry = Some(address(300, 0));
    // Initial finite experimental ceilings, expressed in each existing meter's
    // own units. They are neither production defaults nor a CPU/RSS guarantee.
    for (name, maximum) in [
        ("display_updates", 134_217_728),
        ("glyph_comparison_work", 262_144),
        ("display_comparison_updates", 33_554_432),
        ("realization_updates", 131_072),
        ("placement_work", 4096),
        // 3366 original visits + 102 displays * two catalogue passes *
        // (four definitions + eight commands + twelve reconstruction slots).
        ("assembly_work", 8262),
        ("ancestry_work", 4096),
        ("collected_commands", 8192),
        ("collected_drcs_references", 1024),
        ("spatial_event_visits", 16_384),
        ("spatial_invocation_visits", 4096),
        ("identity_checks", 131_072),
        ("attribute_cell_updates", 32_768),
    ] {
        set_credit(&mut policy.encoding, name, maximum);
    }
    policy
}

fn exact_policy(
    usage: TeletextAuthoredServiceEncodingUsage,
) -> TeletextChannelDocumentEncodingPolicy {
    let mut policy = workload_policy();
    for (name, maximum) in credits(usage) {
        set_credit(&mut policy.encoding, name, maximum);
    }
    policy
}

fn document_budget() -> TeletextDesignDocumentBudget {
    let mut budget = small_document_budget();
    budget.max_documents = 2;
    budget.max_serialized_bytes = 4 * 1024 * 1024;
    budget.max_response_bytes = 1024 * 1024;
    budget.validation = workload_policy().validation;
    budget
}

fn fifty_pages_with_subpage(first: u16, theme: &str, magazine: u8) -> Value {
    let mut document = rich::pages(first, 50, theme, magazine);
    for field in ["pages", "page_enhancements", "mot_selections"] {
        let mut subpage = document[field][0].clone();
        let address_field = if field == "mot_selections" {
            "display"
        } else {
            "address"
        };
        subpage[address_field]["subcode"] = json!(16);
        if field == "pages" {
            subpage["navigation"] = json!([{"role":"index","target":{"page":first,"subcode":0}}]);
        }
        document[field].as_array_mut().unwrap().push(subpage);
    }
    document["pages"][0]["navigation"]
        .as_array_mut()
        .unwrap()
        .push(json!({"role":"red","target":{"page":first,"subcode":16}}));
    document
}

fn active_pressure_document() -> Value {
    let mut document = rich::pages(200, 1, "day", 2);
    document["pages"][0]["rows"][0]["runs"][0]["content"]["text"] = json!("£");
    document["objects"][0]["object_type"] = json!("active");
    document["objects"][0]["program"] = json!({"commands":[
        {"kind":"set_position","position":{"row":0,"column":0}},
        {"kind":"at_column","column":0,"operation":{"kind":"g0","code":65}}]});
    let commands = document["page_enhancements"][0]["local_program"]["commands"]
        .as_array_mut()
        .unwrap();
    commands.remove(2); // Preserve the Active A, rather than a higher-priority Local A.
    commands[2]["object_type"] = json!("active");
    document
}

fn measured(
    candidate: &EncodedChannelTeletextDocumentCandidate,
) -> (TeletextAuthoredServiceEncodingUsage, u128) {
    let policy = workload_policy();
    let started = Instant::now();
    let (result, usage) = encode_teletext_authored_service_with_work_usage(
        candidate.encoded().unwrap().declared().clone(),
        &policy.transmissions,
        &policy.encoding,
        Some(deadline()),
    );
    let micros = started.elapsed().as_micros();
    rich::assert_same_service(&result.unwrap(), candidate.encoded().unwrap());
    (usage, micros)
}

fn exact_and_short(
    candidate: &EncodedChannelTeletextDocumentCandidate,
    usage: TeletextAuthoredServiceEncodingUsage,
) -> Vec<&'static str> {
    let expected = candidate.encoded().unwrap();
    let policy = exact_policy(usage);
    let (result, exact_usage) = encode_teletext_authored_service_with_work_usage(
        expected.declared().clone(),
        &policy.transmissions,
        &policy.encoding,
        Some(deadline()),
    );
    assert_eq!(exact_usage, usage);
    rich::assert_same_service(&result.unwrap(), expected);
    let mut tested = Vec::new();
    for (name, count) in credits(usage) {
        if count == 0 {
            continue;
        } // Zero is already an exact exhausted credit.
        let mut short = exact_policy(usage);
        set_credit(&mut short.encoding, name, count - 1);
        let (result, admitted) = encode_teletext_authored_service_with_work_usage(
            expected.declared().clone(),
            &short.transmissions,
            &short.encoding,
            Some(deadline()),
        );
        assert!(
            result.unwrap_err().display.is_some(),
            "must reject a complete authored display"
        );
        assert!(
            credits(admitted)
                .into_iter()
                .find(|(bucket, _)| *bucket == name)
                .unwrap()
                .1
                < count
        );
        tested.push(name);
    }
    tested
}

fn previous_assembly_ceiling_rejects(candidate: &EncodedChannelTeletextDocumentCandidate) {
    use ersatzrs_ffmpeg::teletext::{
        TeletextAuthoredServiceEncodingFailure, TeletextCompiledDisplayPageEncodingFailure,
        TeletextComposedMainEncodingError, TeletextMainCompositionFailure,
        TeletextNormalizedMainProgramFailure,
    };

    let mut policy = workload_policy();
    policy.encoding.max_assembly_work = Some(8192);
    let (result, admitted) = encode_teletext_authored_service_with_work_usage(
        candidate.encoded().unwrap().declared().clone(),
        &policy.transmissions,
        &policy.encoding,
        Some(deadline()),
    );
    let error = result.unwrap_err();
    assert_eq!(error.display, Some(address(300, 16)));
    // 101 complete displays use 8181 visits; ten of the final derivation's
    // eleven remaining credits are admitted before the next two-command scan.
    assert_eq!(admitted.assembly_work, Some(8191));
    let TeletextAuthoredServiceEncodingFailure::Compiled(error) = error.reason else {
        panic!("expected compiled-page assembly refusal");
    };
    let TeletextCompiledDisplayPageEncodingFailure::Normalization(error) = error.reason else {
        panic!("expected normalization assembly refusal");
    };
    let TeletextNormalizedMainProgramFailure::Original(error) = error.reason else {
        panic!("expected original header derivation refusal");
    };
    let TeletextComposedMainEncodingError::Composition(error) = *error else {
        panic!("expected composition header derivation refusal");
    };
    assert!(matches!(
        error.reason,
        TeletextMainCompositionFailure::ObjectHeaderWorkBudget { maximum: 11 }
    ));
}

fn navigation_and_preview(service: &EncodedTeletextAuthoredService, first: u16) {
    for subcode in [0, 16] {
        let preview = project_teletext_preview(
            service,
            address(first, subcode),
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            TeletextPreviewBudget {
                max_pages: 160,
                max_model_records: 32_768,
                max_pixels: 168_000,
                max_diagnostics: 1400,
                max_work: 50_000_000,
            },
        )
        .unwrap();
        assert!(preview.raster().diagnostics().is_empty());
        let role = if subcode == 0 {
            TeletextNavigationRole::Red
        } else {
            TeletextNavigationRole::Index
        };
        let link = preview
            .navigation()
            .iter()
            .find(|link| link.role == role)
            .unwrap();
        assert_eq!(link.target.page, address(first, 0).page);
        assert_eq!(
            link.target.subcode,
            Some(address(first, if subcode == 0 { 16 } else { 0 }).subcode)
        );
    }
}

async fn save(
    fixture: &Fixture,
    candidate: EncodedChannelTeletextDocumentCandidate,
) -> SavedOriginalTeletextDesign {
    match save_original_teletext_design_candidate(
        &fixture.state.application,
        "Synthetic guide-sized resource workload".to_owned(),
        None,
        candidate,
        document_budget(),
        deadline(),
    )
    .await
    .unwrap()
    {
        TeletextDesignMutation::Saved(saved) => saved,
        TeletextDesignMutation::Rejected(error) => panic!("workload save failed: {error:?}"),
    }
}

async fn accept(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    usage: TeletextAuthoredServiceEncodingUsage,
) -> AcceptedTeletextOutputCandidate {
    let request = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        None,
        document_budget(),
        exact_policy(usage),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(request) = request else {
        panic!("workload must yield an explicit pending request");
    };
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    let accepted = accept_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            request: request.request().clone(),
            expected_design_head: saved.head().revision,
            expected_generation_id: None,
        },
        document_budget(),
        exact_policy(usage),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextGenerationMutation::Accepted(accepted) = accepted else {
        panic!("frozen workload must accept explicitly");
    };
    accepted
}

async fn rejected_promotion(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
    policy: TeletextChannelDocumentEncodingPolicy,
) {
    let error = accept_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            request: accepted.generation().request.clone(),
            expected_design_head: saved.head().revision,
            expected_generation_id: Some(accepted.generation().id),
        },
        document_budget(),
        policy,
        deadline(),
    )
    .await
    .unwrap_err();
    assert!(matches!(
        error,
        NativeTeletextGenerationError::Design(NativeTeletextDesignError::Encoding(_))
    ));
}

fn first_producer_amplification(
    saved: &SavedOriginalTeletextDesign,
    usage: TeletextAuthoredServiceEncodingUsage,
) -> TeletextChannelDocumentEncodingPolicy {
    let candidate = saved.candidate();
    let mut policy = exact_policy(usage);
    let first = parse_declared_service(
        candidate.sources()[0].bytes(),
        candidate.generation(),
        policy.validation.service,
        policy.validation.declarations,
    )
    .unwrap();
    let transmissions = policy
        .transmissions
        .iter()
        .copied()
        .filter(|record| record.resource.page().magazine() == 2)
        .collect::<Vec<_>>();
    policy.encoding.normalization.max_resource_objects = 3;
    let (result, first_usage) = encode_teletext_authored_service_with_work_usage(
        first,
        &transmissions,
        &policy.encoding,
        Some(deadline()),
    );
    assert_eq!(result.unwrap().declared().semantic().base().pages.len(), 51);
    policy = exact_policy(usage);
    policy.encoding.max_display_updates = first_usage.display_updates;
    let (result, admitted) = encode_teletext_authored_service_with_work_usage(
        candidate.encoded().unwrap().declared().clone(),
        &policy.transmissions,
        &policy.encoding,
        Some(deadline()),
    );
    assert!(result.unwrap_err().display.unwrap().page.get() >= 300);
    assert_eq!(admitted.display_updates, first_usage.display_updates);
    policy
}

async fn denied_work_preserves_accepted_state(
    fixture: &mut Fixture,
    saved: &SavedOriginalTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
    usage: TeletextAuthoredServiceEncodingUsage,
) {
    let before = fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap();
    let before_config = config(fixture).await;
    let calls = std::fs::read(fixture.root.path().join("channel-calls")).unwrap();
    let mut producers = exact_policy(usage);
    producers.input.max_producers = 1;
    assert!(matches!(
        candidate(fixture, producers).await,
        Err(Error::Preparation(
            ChannelTeletextDocumentError::ProducerBudget {
                count: 2,
                maximum: 1
            }
        ))
    ));
    let first = first_producer_amplification(saved, usage);
    rejected_promotion(fixture, saved, accepted, first).await;
    let mut allocation = exact_policy(usage);
    allocation.encoding.normalization.max_resource_objects = 3;
    rejected_promotion(fixture, saved, accepted, allocation).await;
    assert_eq!(
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap(),
        before
    );
    assert_eq!(config(fixture).await, before_config);
    assert_eq!(
        std::fs::read(fixture.root.path().join("channel-calls")).unwrap(),
        calls
    );
    assert!(fixture.playout.try_recv().is_err());
}

async fn restore(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    accepted: &AcceptedTeletextOutputCandidate,
    usage: TeletextAuthoredServiceEncodingUsage,
) {
    let historical = reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        document_budget(),
        exact_policy(usage),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    rich::assert_same_service(
        historical.encoded().unwrap(),
        saved.candidate().encoded().unwrap(),
    );
    // The historical comparison is complete before the next full reconstruction.
    drop(historical);
    let recovered = reload_accepted_teletext_generation(
        &fixture.state.application,
        1,
        document_budget(),
        exact_policy(usage),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(recovered.generation(), accepted.generation());
    rich::assert_same_service(recovered.service(), saved.candidate().encoded().unwrap());
    for first in [200, 300] {
        navigation_and_preview(recovered.service(), first);
    }
}

#[tokio::test]
async fn hundred_page_numbers_and_subpages_with_shared_resources_follow_finite_native_flow() {
    let mut fixture = rich::installed_documents(
        [
            fifty_pages_with_subpage(200, "day", 2),
            fifty_pages_with_subpage(300, "night", 3),
        ],
        ["day", "night"],
    )
    .await;
    let before = config(&fixture).await;
    let started = Instant::now();
    let prepared = candidate(&fixture, workload_policy()).await.unwrap();
    let prepare_micros = started.elapsed().as_micros();
    let counts = rich::counts(&prepared);
    let numbers = prepared
        .encoded()
        .unwrap()
        .declared()
        .semantic()
        .base()
        .pages
        .iter()
        .map(|page| page.address.page)
        .collect::<BTreeSet<_>>();
    assert_eq!(numbers.len(), 100);
    assert_eq!(counts["display_records"], 102);
    assert_eq!(counts["resource_records"], 8);
    assert_eq!(counts["object_records"], 4);
    assert_eq!(counts["drcs_subtables"], 4);
    assert_eq!(counts["collected_invocations"], 204);
    assert_eq!(counts["collected_drcs_references"], 204);
    assert_ne!(
        prepared.producers()[0].pages()[0]
            .effective_defaults()
            .palette,
        prepared.producers()[1].pages()[0]
            .effective_defaults()
            .palette
    );
    for first in [200, 300] {
        navigation_and_preview(prepared.encoded().unwrap(), first);
    }
    let (usage, meter_micros) = measured(&prepared);
    assert_eq!(usage.assembly_work, Some(8262));
    previous_assembly_ceiling_rejects(&prepared);
    let cutoffs = exact_and_short(&prepared, usage);
    let started = Instant::now();
    let saved = save(&fixture, prepared).await;
    let save_micros = started.elapsed().as_micros();
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    let revision = fixture
        .state
        .application
        .teletext_design_revision(
            saved.head().id,
            saved.head().revision,
            document_budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    let started = Instant::now();
    let accepted = accept(&fixture, &saved, usage).await;
    let publication_micros = started.elapsed().as_micros();
    rich::assert_same_service(accepted.service(), saved.candidate().encoded().unwrap());
    let started = Instant::now();
    restore(&fixture, &saved, &accepted, usage).await;
    let restore_micros = started.elapsed().as_micros();
    denied_work_preserves_accepted_state(&mut fixture, &saved, &accepted, usage).await;
    assert_requests(&fixture, 2, 2);
    assert_eq!(config(&fixture).await, before);
    eprintln!(
        "{}",
        json!({"fixture":"hundred_numbers_102_displays_pop_gpop_drcs_gdrcs_two_themes",
        "counts":counts,"stored_document_bytes":revision.document.len(),"admitted_work":usage_json(usage),
        "one_less_buckets":cutoffs,"timings_micros":{"prepare_and_encode":prepare_micros,
            "metered_reencode":meter_micros,"save":save_micros,"request_accept":publication_micros,
            "historical_accepted_reload":restore_micros}})
    );
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn active_resource_over_national_fallback_has_positive_carrier_identity_pressure() {
    let mut fixture = rich::installed_documents(
        [active_pressure_document(), rich::pages(300, 1, "night", 3)],
        ["day", "night"],
    )
    .await;
    let before = config(&fixture).await;
    let prepared = candidate(&fixture, workload_policy()).await.unwrap();
    let (usage, _) = measured(&prepared);
    assert!(usage.identity_checks.unwrap() > 0);
    let cutoffs = exact_and_short(&prepared, usage);
    assert!(cutoffs.contains(&"identity_checks"));
    let saved = save(&fixture, prepared).await;
    let accepted = accept(&fixture, &saved, usage).await;
    rich::assert_same_service(accepted.service(), saved.candidate().encoded().unwrap());
    let recovered = reload_accepted_teletext_generation(
        &fixture.state.application,
        1,
        document_budget(),
        exact_policy(usage),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    rich::assert_same_service(recovered.service(), accepted.service());
    assert_requests(&fixture, 2, 2);
    assert_eq!(config(&fixture).await, before);
    assert!(fixture.playout.try_recv().is_err());
    eprintln!(
        "{}",
        json!({"fixture":"active_pop_over_british_fallback_with_pop_gpop_drcs_gdrcs",
        "admitted_work":usage_json(usage),"one_less_buckets":cutoffs})
    );
    fixture.state.pool.close().await;
}
