//! Shared populated resources in a small native service, not worst-case/RSS proof.

use super::*;
use crate::designs::{budget as small_document_budget, config, deadline, units};
use ersatzrs_addon_contract::TeletextPagesV2Response;
use ersatzrs_app::teletext_design_documents::TeletextDesignDocumentBudget;
use ersatzrs_core::repositories::{
    TeletextDesignMutation, TeletextGenerationMutation, TeletextPublicationMutation,
};
use ersatzrs_web::host::{
    teletext_designs::{
        reload_original_teletext_design_revision, request_original_teletext_design_publication,
        save_original_teletext_design_candidate, SavedOriginalTeletextDesign,
    },
    teletext_generations::{
        accept_original_teletext_generation, reload_accepted_teletext_generation,
        AcceptedTeletextOutputCandidate, TeletextGenerationTarget,
    },
};
use std::time::Instant;

fn resource(magazine: u8, kind: TeletextResourceKind, page: u8) -> TeletextResourceIdentity {
    TeletextResourceIdentity::new(
        kind,
        TeletextRawDataPageNumber::new(magazine, page).unwrap(),
        TeletextResourceSubpageIndex::ZERO,
    )
    .unwrap()
}

fn resources(magazine: u8) -> [TeletextResourceIdentity; 4] {
    [
        resource(magazine, TeletextResourceKind::Pop, 0xa0),
        resource(magazine, TeletextResourceKind::Gpop, 0xa1),
        resource(magazine, TeletextResourceKind::Drcs, 0xc0),
        resource(magazine, TeletextResourceKind::Gdrcs, 0xc1),
    ]
}

fn resource_wire(identity: TeletextResourceIdentity) -> Value {
    // Producer DTOs call logical S1 `subpage`; the domain calls it
    // `logical_subpage`. They are distinct serialization contracts.
    json!({"kind":identity.kind(),"page":identity.page(),
        "subpage":identity.logical_subpage().get()})
}

fn object_wire(identity: TeletextObjectIdentity) -> Value {
    json!({"scope":identity.scope(),"resource":resource_wire(identity.resource().unwrap()),
        "object_id":identity.object_id().get()})
}

fn resource_policy() -> TeletextChannelDocumentEncodingPolicy {
    let mut policy = policy();
    // Finite fixture bounds admit 16 displays/eight resources. Normalization
    // limits remain unchanged; no production policy is inferred.
    policy.input.max_response_bytes = 131_072;
    policy.validation.service.max_records = 32;
    policy.validation.service.max_collected_drcs_references = 64;
    policy.validation.declarations.max_displays = 16;
    policy.composition.service = policy.validation.service;
    policy.composition.declarations = policy.validation.declarations;
    policy.encoding.max_pages = 32;
    policy.encoding.max_units = 2048;
    policy.transmissions = [2, 3]
        .into_iter()
        .flat_map(resources)
        .map(|resource| TeletextServiceResourceTransmission {
            resource,
            counter: TeletextResourceUpdateCounter::new(1).unwrap(),
            update: TeletextResourceFullUpdate::Changed,
            last_subpage: true,
        })
        .collect();
    policy
}

fn document_budget() -> TeletextDesignDocumentBudget {
    let mut budget = small_document_budget();
    budget.max_serialized_bytes = 1024 * 1024;
    budget.max_response_bytes = resource_policy().input.max_response_bytes;
    budget.validation = resource_policy().validation;
    budget
}

fn object(identity: TeletextObjectIdentity, column: u8, code: u8) -> Value {
    let mut value = serde_json::to_value(TeletextEnhancementObjectDefinition {
        identity,
        object_type: TeletextObjectType::Passive,
        program: TeletextEnhancementProgram::new(vec![
            TeletextEnhancementCommand::SetPosition {
                position: TeletextEnhancementPosition::new(1, column).unwrap(),
            },
            TeletextEnhancementCommand::AtColumn {
                column,
                operation: TeletextColumnOperation::G0 {
                    code: TeletextGlyphCode::new(code).unwrap(),
                },
            },
        ])
        .unwrap(),
    })
    .unwrap();
    value["identity"] = object_wire(identity);
    value
}

fn drcs(identity: TeletextResourceIdentity, bits: u16) -> Value {
    let mut value = serde_json::to_value(
        TeletextDrcsSubtable::new(
            identity,
            vec![TeletextDrcsGlyphDefinition {
                slot: TeletextDrcsGlyphSlot::new(0).unwrap(),
                bitmap: TeletextDrcsBitmap::new([bits; 10]).unwrap(),
            }],
        )
        .unwrap(),
    )
    .unwrap();
    value["identity"] = resource_wire(identity);
    value
}

fn eight_pages(first: u16, theme: &str, magazine: u8) -> Value {
    let [pop, gpop, normal, global] = resources(magazine);
    let public_object = TeletextObjectIdentity::public(pop, TeletextObjectId::new(10)).unwrap();
    let global_object = TeletextObjectIdentity::global(gpop, TeletextObjectId::new(20)).unwrap();
    let mut document = response(DocumentFixture::Authored, theme, first, 0, "nld");
    document["magazines"][0]["magazine"] = json!(magazine);
    document["objects"] = json!([
        object(public_object, 12, b'B'),
        object(global_object, 16, b'C')
    ]);
    document["drcs"] = json!([drcs(normal, 0x801), drcs(global, 0x402)]);
    for field in ["pages", "page_enhancements", "mot_selections"] {
        document[field] = json!([]);
    }
    let link = |identity: TeletextResourceIdentity| json!({"target":{"kind":"page","page":identity.page()}, "validity":"level25_only","required_subpages":1});
    for number in first..first + 8 {
        let mut page = response(DocumentFixture::Authored, theme, number, 0, "nld");
        page["pages"][0]["navigation"][0]["target"]["page"] =
            json!(first + (number - first + 1) % 8);
        let enhancement = &mut page["page_enhancements"][0];
        enhancement["compositional_links"] =
            json!({"pop":link(pop),"gpop":link(gpop),"drcs":link(normal),"gdrcs":link(global)});
        enhancement["resources"]["objects"] = json!({"public":pop.page(),"global":gpop.page()});
        enhancement["resources"]["drcs"] = json!({"normal":normal.page(),"global":global.page()});
        let commands = enhancement["local_program"]["commands"]
            .as_array_mut()
            .unwrap();
        for target in [public_object, global_object] {
            commands.push(json!({"kind":"invoke_object","target":object_wire(target),"object_type":"passive","offset":null}));
        }
        commands.push(json!({"kind":"set_position","position":{"row":2,"column":8}}));
        for (table, column) in [("normal", 8), ("global", 10)] {
            commands.push(json!({"kind":"at_column","column":column,"operation":{"kind":"drcs","reference":{"table":table,"subtable":0,"slot":0}}}));
        }
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
    for (addon, label, first, theme, magazine, hash) in [
        (ADDON, "first", 200, "day", 2, "a".repeat(64)),
        (SECONDARY, "second", 300, "night", 3, "b".repeat(64)),
    ] {
        sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256"=? WHERE "AddonId"=?"#)
            .bind(hash)
            .bind(addon)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
        install_contract(&fixture, addon, label, 2).await;
        let document = eight_pages(first, theme, magazine);
        serde_json::from_value::<TeletextPagesV2Response>(document.clone())
            .expect("generated resource fixture must match the actual producer V2 DTO");
        write_response(&fixture, label, &document);
        let profile = fixture.profile_for(addon, label, theme, "good").await;
        fixture.bind(1, &profile, None).await;
    }
    fixture
}

fn counts(candidate: &EncodedChannelTeletextDocumentCandidate) -> Value {
    let encoded = candidate.encoded().unwrap();
    let semantic = encoded.declared().semantic();
    let mut normalized_commands = 0;
    let mut normalized_events = 0;
    let mut normalized_invocations = 0;
    let mut normalized_origins = 0;
    let mut physical_events = 0;
    let mut physical_invocations = 0;
    let mut physical_origins = 0;
    for page in encoded.pages() {
        if let EncodedTeletextAuthoredPage::CompiledDisplay(page) = page {
            let original = page.normalized().original();
            let physical = page.normalized().physical().replay();
            normalized_commands += original.program().commands().len();
            normalized_events += original.spatial_trace().events().len();
            normalized_invocations += original.spatial_trace().invocations().len();
            normalized_origins += original.display_resolution().origins().len();
            physical_events += physical.spatial_trace().events().len();
            physical_invocations += physical.spatial_trace().invocations().len();
            physical_origins += physical.display_resolution().origins().len();
        }
    }
    let mut collected_commands = 0;
    let mut collected_invocations = 0;
    let mut collected_drcs_references = 0;
    for page in &semantic.base().pages {
        let references = &semantic.collected(page.address).unwrap().1;
        collected_commands += references.command_count();
        collected_invocations += references.object_references().len();
        collected_drcs_references += references.references().len();
    }
    json!({
        "source_bytes":candidate.sources().iter().map(|source|source.bytes().len()).collect::<Vec<_>>(),
        "request_v2_bytes":candidate.requests().request(TeletextPageContract::V2).unwrap().len(),
        "display_records":semantic.base().pages.len(),"resource_records":encoded.declared().resource_inventory().len(),
        "object_records":semantic.objects().definitions().len(),"drcs_subtables":semantic.drcs().subtables().len(),
        "drcs_defined_glyphs":semantic.drcs().subtables().iter().map(|table|table.glyphs().len()).sum::<usize>(),
        "authored_commands":semantic.page_enhancements().values().filter_map(|page|page.local_program.as_ref()).map(|program|program.commands().len()).sum::<usize>()
            +semantic.objects().definitions().iter().map(|object|object.program.commands().len()).sum::<usize>(),
        "collected_commands":collected_commands,"collected_invocations":collected_invocations,"collected_drcs_references":collected_drcs_references,
        "normalized_commands":normalized_commands,"normalized_events":normalized_events,"normalized_invocations":normalized_invocations,"normalized_origins":normalized_origins,
        "physical_events":physical_events,"physical_invocations":physical_invocations,"physical_origins":physical_origins,
        "page_units":encoded.pages().iter().map(|page|page.data_units().len()).sum::<usize>(),
        "magazine_units":encoded.magazine_units().len(),"total_units":encoded.unit_count(),"encoded_unit_bytes":encoded.unit_count().checked_mul(46).unwrap()
    })
}

fn previews(encoded: &EncodedTeletextAuthoredService) -> Vec<TeletextRaster> {
    encoded
        .pages()
        .iter()
        .filter_map(|page| match page {
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => Some(
                render_teletext_composition(
                    page.normalized().original(),
                    TeletextColourPixelOptions {
                        reveal_concealed: false,
                        flash_phase: TeletextFlashPhase::Original,
                    },
                    TeletextRasterBudget {
                        max_pixels: 168_000,
                        max_diagnostics: 1400,
                    },
                )
                .unwrap(),
            ),
            _ => None,
        })
        .collect()
}

fn assert_same_service(
    actual: &EncodedTeletextAuthoredService,
    expected: &EncodedTeletextAuthoredService,
) {
    assert_eq!(actual.declared(), expected.declared());
    assert_eq!(actual.inventory(), expected.inventory());
    assert_eq!(actual.magazine_units(), expected.magazine_units());
    assert_eq!(units(actual), units(expected));
    for (actual, expected) in actual.pages().iter().zip(expected.pages()) {
        if let (
            EncodedTeletextAuthoredPage::CompiledDisplay(actual),
            EncodedTeletextAuthoredPage::CompiledDisplay(expected),
        ) = (actual, expected)
        {
            assert_eq!(
                actual.normalized().original(),
                expected.normalized().original()
            );
            assert_eq!(
                actual.normalized().physical(),
                expected.normalized().physical()
            );
        }
    }
    assert_eq!(previews(actual), previews(expected));
}

fn metered_reencode(
    prepared: &EncodedChannelTeletextDocumentCandidate,
) -> (TeletextAuthoredServiceEncodingUsage, u128) {
    let mut policy = resource_policy();
    policy.encoding.max_display_updates = Some(usize::MAX);
    policy.encoding.max_glyph_comparison_work = Some(usize::MAX);
    policy.encoding.max_display_comparison_updates = Some(usize::MAX);
    policy.encoding.max_realization_updates = Some(usize::MAX);
    policy.encoding.max_placement_work = Some(usize::MAX);
    policy.encoding.max_assembly_work = Some(usize::MAX);
    policy.encoding.max_ancestry_work = Some(usize::MAX);
    let started = Instant::now();
    let (metered, usage) = encode_teletext_authored_service_with_work_usage(
        prepared.encoded().unwrap().declared().clone(),
        &policy.transmissions,
        &policy.encoding,
        Some(deadline()),
    );
    let micros = started.elapsed().as_micros();
    assert_same_service(&metered.unwrap(), prepared.encoded().unwrap());
    assert!(usage.display_updates.unwrap() > 0);
    assert!(usage.glyph_comparison_work.unwrap() > 0);
    assert!(usage.display_comparison_updates.unwrap() > 0);
    assert!(usage.realization_updates.unwrap() > 0);
    assert!(usage.placement_work.unwrap() > 0);
    assert!(usage.assembly_work.unwrap() > 0);
    assert!(usage.ancestry_work.unwrap() > 0);
    (usage, micros)
}

async fn save(
    fixture: &Fixture,
    candidate: EncodedChannelTeletextDocumentCandidate,
) -> (SavedOriginalTeletextDesign, u128) {
    let started = Instant::now();
    let saved = save_original_teletext_design_candidate(
        &fixture.state.application,
        "Synthetic shared-resource measurement".to_owned(),
        None,
        candidate,
        document_budget(),
        deadline(),
    )
    .await
    .unwrap();
    let save_micros = started.elapsed().as_micros();
    let TeletextDesignMutation::Saved(saved) = saved else {
        panic!("complete resource service must save");
    };
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    (saved, save_micros)
}

async fn explicitly_accept(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
) -> (AcceptedTeletextOutputCandidate, u128) {
    let started = Instant::now();
    let requested = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        None,
        document_budget(),
        resource_policy(),
        deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = requested else {
        panic!("complete resource service must yield a pending request");
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
            request: requested.request().clone(),
            expected_design_head: saved.head().revision,
            expected_generation_id: None,
        },
        document_budget(),
        resource_policy(),
        deadline(),
    )
    .await
    .unwrap();
    let publication_micros = started.elapsed().as_micros();
    let TeletextGenerationMutation::Accepted(accepted) = accepted else {
        panic!("frozen resource originals must accept explicitly");
    };
    (accepted, publication_micros)
}

#[tokio::test]
async fn sixteen_authored_pages_with_shared_objects_and_drcs_preserve_native_flow() {
    let mut fixture = installed_service().await;
    let before = config(&fixture).await;
    let started = Instant::now();
    let prepared = candidate(&fixture, resource_policy()).await.unwrap();
    let prepare_micros = started.elapsed().as_micros();
    let observed = counts(&prepared);
    assert_eq!(observed["display_records"], 16);
    assert_eq!(observed["resource_records"], 8);
    assert_eq!(observed["object_records"], 4);
    assert_eq!(observed["drcs_subtables"], 4);
    assert_eq!(observed["drcs_defined_glyphs"], 4);
    assert_eq!(observed["collected_invocations"], 32);
    assert_eq!(observed["collected_drcs_references"], 32);
    assert_eq!(previews(prepared.encoded().unwrap()).len(), 16);
    assert_ne!(
        prepared.producers()[0].pages()[0]
            .effective_defaults()
            .palette,
        prepared.producers()[1].pages()[0]
            .effective_defaults()
            .palette
    );
    let (usage, metered_encode_micros) = metered_reencode(&prepared);
    let (saved, save_micros) = save(&fixture, prepared).await;
    let started = Instant::now();
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        document_budget(),
        resource_policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let preview_micros = started.elapsed().as_micros();
    assert_same_service(
        restored.encoded().unwrap(),
        saved.candidate().encoded().unwrap(),
    );
    for (restored, original) in restored.sources().iter().zip(saved.candidate().sources()) {
        assert_eq!(restored.bytes(), original.bytes());
    }
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
    let (accepted, publication_micros) = explicitly_accept(&fixture, &saved).await;
    assert_same_service(accepted.service(), saved.candidate().encoded().unwrap());
    let started = Instant::now();
    let reloaded = reload_accepted_teletext_generation(
        &fixture.state.application,
        1,
        document_budget(),
        resource_policy(),
        deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let reload_micros = started.elapsed().as_micros();
    assert_eq!(reloaded.generation(), accepted.generation());
    assert_same_service(reloaded.service(), saved.candidate().encoded().unwrap());
    assert_requests(&fixture, 2, 2);
    assert_eq!(config(&fixture).await, before);
    assert!(fixture.playout.try_recv().is_err());
    eprintln!(
        "{}",
        json!({"fixture":"sixteen_displays_shared_pop_gpop_drcs_gdrcs_two_themes",
            "counts":observed,"stored_document_bytes":revision.document.len(),
            "admitted_work":{"display_updates":usage.display_updates.unwrap(),
                "glyph_comparison_work":usage.glyph_comparison_work.unwrap(),
                "display_comparison_updates":usage.display_comparison_updates.unwrap(),
                "realization_updates":usage.realization_updates.unwrap(),
                "placement_work":usage.placement_work.unwrap(),
                "assembly_work":usage.assembly_work.unwrap(),
                "ancestry_work":usage.ancestry_work.unwrap()},
            "timings_micros":{"prepare_and_encode":prepare_micros,"metered_reencode":metered_encode_micros,"save":save_micros,"historical_preview_reload":preview_micros,"request_and_accept":publication_micros,"accepted_reload":reload_micros}
        })
    );
    fixture.state.pool.close().await;
}
