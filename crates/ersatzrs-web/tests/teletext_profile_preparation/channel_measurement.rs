//! Sparse service-size observations, not production defaults or worst-case/RSS proof.

use super::*;
use crate::designs::{budget as small_document_budget, deadline, units};
use ersatzrs_app::teletext_design_documents::TeletextDesignDocumentBudget;
use ersatzrs_core::repositories::{
    TeletextDesignMutation, TeletextGenerationMutation, TeletextPublicationMutation,
};
use ersatzrs_web::host::{
    teletext_designs::{
        request_original_teletext_design_publication, save_original_teletext_design_candidate,
        SavedOriginalTeletextDesign,
    },
    teletext_generations::{
        accept_original_teletext_generation, reload_accepted_teletext_generation,
        AcceptedTeletextOutputCandidate, TeletextGenerationTarget,
    },
};
use std::time::Instant;

fn measurement_policy() -> TeletextChannelDocumentEncodingPolicy {
    // Only this sparse hundred-page experiment uses these enlarged fixture bounds.
    // Per-page normalization remains unchanged; no production policy is inferred.
    let mut policy = policy();
    policy.input.max_response_bytes = 1024 * 1024;
    policy.validation.service.max_records = 256;
    policy.validation.declarations.max_displays = 128;
    policy.composition.service = policy.validation.service;
    policy.composition.declarations = policy.validation.declarations;
    policy.encoding.max_pages = 128;
    policy.encoding.max_units = 6400;
    policy
}

fn document_budget() -> TeletextDesignDocumentBudget {
    let mut budget = small_document_budget();
    budget.max_serialized_bytes = 4 * 1024 * 1024;
    budget.max_response_bytes = 1024 * 1024;
    budget.validation = measurement_policy().validation;
    budget
}

fn fifty_pages(first: u16, theme: &str) -> Value {
    let mut document = response(DocumentFixture::Authored, theme, first, 0, "nld");
    document["magazines"][0]["magazine"] = json!(2);
    for field in ["pages", "page_enhancements", "mot_selections"] {
        document[field] = json!([]);
    }
    for number in first..first + 50 {
        let mut page = response(DocumentFixture::Authored, theme, number, 0, "nld");
        // Standalone producer validation requires every target in its own service.
        // Close each fifty-page cycle; this measures no cross-producer navigation.
        page["pages"][0]["navigation"][0]["target"]["page"] =
            json!(first + (number - first + 1) % 50);
        for field in ["pages", "page_enhancements", "mot_selections"] {
            document[field]
                .as_array_mut()
                .unwrap()
                .push(page[field][0].take());
        }
    }
    document
}

async fn installed_service() -> Fixture {
    let fixture = Fixture::new_persistent().await;
    install_secondary(&fixture).await;
    for (addon, label, first, theme, hash) in [
        (ADDON, "first", 200, "day", "a".repeat(64)),
        (SECONDARY, "second", 250, "night", "b".repeat(64)),
    ] {
        sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256"=? WHERE "AddonId"=?"#)
            .bind(hash)
            .bind(addon)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        install_contract(&fixture, addon, label, 2).await;
        write_response(&fixture, label, &fifty_pages(first, theme));
        let profile = fixture.profile_for(addon, label, theme, "good").await;
        fixture.bind(1, &profile, None).await;
    }
    fixture
}

fn model_counts(candidate: &EncodedChannelTeletextDocumentCandidate) -> Value {
    let encoded = candidate.encoded().unwrap();
    let semantic = encoded.declared().semantic();
    let mut normalized_commands = 0;
    let mut normalized_events = 0;
    let mut normalized_origins = 0;
    let mut physical_events = 0;
    let mut physical_origins = 0;
    for page in encoded.pages() {
        let EncodedTeletextAuthoredPage::CompiledDisplay(page) = page else {
            panic!("sparse authored input must produce compiled displays only");
        };
        let original = page.normalized().original();
        let physical = page.normalized().physical().replay();
        normalized_commands += original.program().commands().len();
        normalized_events += original.spatial_trace().events().len();
        normalized_origins += original.display_resolution().origins().len();
        physical_events += physical.spatial_trace().events().len();
        physical_origins += physical.display_resolution().origins().len();
    }
    let authored_commands: usize = semantic
        .page_enhancements()
        .values()
        .filter_map(|page| page.local_program.as_ref())
        .map(|program| program.commands().len())
        .sum();
    let collected_commands: usize = semantic
        .base()
        .pages
        .iter()
        .map(|page| semantic.collected(page.address).unwrap().1.command_count())
        .sum();
    json!({
        "source_bytes":candidate.sources().iter().map(|source| source.bytes().len()).collect::<Vec<_>>(),
        "request_v1_bytes":candidate.requests().request(TeletextPageContract::V1).map_or(0, <[u8]>::len),
        "request_v2_bytes":candidate.requests().request(TeletextPageContract::V2).map_or(0, <[u8]>::len),
        "display_records":semantic.base().pages.len(),
        "resource_records":encoded.declared().resource_inventory().len(),
        "object_records":semantic.objects().definitions().len(),
        "authored_commands":authored_commands,
        "collected_commands":collected_commands,
        "normalized_commands":normalized_commands,
        "normalized_events":normalized_events,
        "normalized_origins":normalized_origins,
        "physical_events":physical_events,
        "physical_origins":physical_origins,
        "page_units":encoded.pages().iter().map(|page|page.data_units().len()).sum::<usize>(),
        "magazine_units":encoded.magazine_units().len(),
        "total_units":encoded.unit_count(),
        "encoded_unit_bytes":encoded.unit_count().checked_mul(46).unwrap()
    })
}

async fn save_candidate(
    fixture: &Fixture,
    candidate: EncodedChannelTeletextDocumentCandidate,
) -> SavedOriginalTeletextDesign {
    let result = save_original_teletext_design_candidate(
        &fixture.state.application,
        "Synthetic hundred-page measurement".to_owned(),
        None,
        candidate,
        document_budget(),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(saved) = result else {
        panic!("complete synthetic service must save under its matching fixture budget");
    };
    saved
}

async fn explicitly_accept(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
) -> AcceptedTeletextOutputCandidate {
    let result = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        None,
        document_budget(),
        measurement_policy(),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = result else {
        panic!("complete synthetic service must yield explicit pending publication");
    };
    let result = accept_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            request: requested.request().clone(),
            expected_design_head: saved.head().revision,
            expected_generation_id: None,
        },
        document_budget(),
        measurement_policy(),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextGenerationMutation::Accepted(accepted) = result else {
        panic!("frozen zero-guide originals must accept explicitly");
    };
    accepted
}

#[tokio::test]
async fn hundred_sparse_authored_pages_save_and_reload_with_measured_counts() {
    let mut fixture = installed_service().await;
    let started = Instant::now();
    let prepared = candidate(&fixture, measurement_policy()).await.unwrap();
    let prepare_micros = started.elapsed().as_micros();
    let counts = model_counts(&prepared);
    assert_eq!(counts["display_records"], 100);
    assert_eq!(counts["authored_commands"], 300);
    assert_eq!(counts["resource_records"], 0);
    assert_eq!(prepared.producers()[0].pages().len(), 50);
    assert_eq!(prepared.producers()[1].pages().len(), 50);
    assert_ne!(
        prepared.producers()[0].pages()[0]
            .effective_defaults()
            .palette,
        prepared.producers()[1].pages()[0]
            .effective_defaults()
            .palette
    );
    let expected_units = units(prepared.encoded().unwrap());
    let expected_magazines = prepared.encoded().unwrap().magazine_units().to_vec();
    let started = Instant::now();
    let saved = save_candidate(&fixture, prepared).await;
    let save_micros = started.elapsed().as_micros();
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
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    let started = Instant::now();
    let accepted = explicitly_accept(&fixture, &saved).await;
    let publication_micros = started.elapsed().as_micros();
    assert_eq!(units(accepted.service()), expected_units);
    let started = Instant::now();
    let reloaded = reload_accepted_teletext_generation(
        &fixture.state.application,
        1,
        document_budget(),
        measurement_policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let reload_micros = started.elapsed().as_micros();
    assert_eq!(reloaded.generation(), accepted.generation());
    assert_eq!(units(reloaded.service()), expected_units);
    assert_eq!(reloaded.service().magazine_units(), expected_magazines);
    assert_eq!(reloaded.service().pages().len(), 100);
    assert_requests(&fixture, 2, 2);
    assert!(fixture.playout.try_recv().is_err());
    // Exact numeric fixture observations only: timings are informational and
    // do not measure total RSS, full guide content or worst-case compilation.
    eprintln!(
        "{}",
        json!({
            "fixture":"sparse_authored_200_299_two_themes",
            "counts":counts,
            "stored_document_bytes":revision.document.len(),
            "timings_micros":{
                "prepare_and_encode":prepare_micros,
                "save":save_micros,
                "request_and_accept":publication_micros,
                "accepted_reload":reload_micros
            }
        })
    );
    fixture.state.pool.close().await;
}
