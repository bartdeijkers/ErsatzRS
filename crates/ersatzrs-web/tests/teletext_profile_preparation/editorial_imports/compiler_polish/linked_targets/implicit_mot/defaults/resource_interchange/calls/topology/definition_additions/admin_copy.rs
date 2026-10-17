//! Headed Copy forms create concepts; native checks do not claim editor/client proof.
use super::*;
use futures_util::FutureExt as _;
use std::{panic::AssertUnwindSafe, path::Path, time::Duration};

const CASE: &str = "teletext-definition-copy";
const NEW_IDS: [u16; 4] = [25, 26, 27, 28];

#[derive(serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct BrowserComplete {
    probe_version: u8,
    case: String,
    design_id: i64,
    revision: i64,
    digest: String,
    new_ids: [u16; 4],
}
struct ExpectedCopies {
    definitions: Vec<TeletextEnhancementObjectDefinition>,
    rasters: Vec<TeletextRaster>,
    units: Vec<Vec<[u8; 46]>>,
    companions: Vec<(u16, Vec<TeletextTtiRecord>)>,
}
fn four_copies(theme: &str, page: u16, subcode: u16) -> Value {
    let mut value = super::super::response(theme, page, subcode);
    let objects = value["service"]["objects"].as_array_mut().unwrap();
    let original = objects
        .iter()
        .find(|object| object["identity"]["object_id"] == 21)
        .unwrap()
        .clone();
    for id in NEW_IDS {
        let mut copy = original.clone();
        copy["identity"]["object_id"] = json!(id);
        objects.push(copy);
    }
    value
}
fn companion_name(exported: &ExportedOriginalTeletextDesign, id: u16) -> &str {
    let resource = protected_file(exported, TeletextResourceKindWire::Pop, 0xc0);
    &exported.sidecar().project.assets.iter().find(|asset| matches!(&asset.scope,
        TeletextExportAssetScope::ObjectContextTtiV1 {resource_name,object_id,editor_page:256,editor_ordinal:0}
            if *object_id==id && resource_name==resource)).unwrap().name
}
async fn qualify_copies() -> ExpectedCopies {
    let (fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        four_copies,
        Fixture::new_persistent().await,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    let service = flow.accepted[0].service();
    let target = ExpectedCopies {
        definitions: NEW_IDS
            .into_iter()
            .map(|id| {
                service
                    .declared()
                    .semantic()
                    .objects()
                    .definition(seed::target(id))
                    .unwrap()
                    .clone()
            })
            .collect(),
        rasters: rasters(service),
        units: service
            .pages()
            .iter()
            .map(|page| page.data_units().to_vec())
            .collect(),
        companions: NEW_IDS
            .into_iter()
            .map(|id| {
                (
                    id,
                    file_records(&flow.exported[0], companion_name(&flow.exported[0], id)),
                )
            })
            .collect(),
    };
    fixture.state.pool.close().await;
    target
}
fn ready(origin: &live_client::LiveOrigin, flow: &Flow, marker: &Path) -> Value {
    json!({"probe_version":1,"case":CASE,"base_url":origin.base_url(),"channel_id":1,
        "design_id":flow.saved[0].head().id,"revision":1,"digest":flow.saved[0].digest(),
        "source_object":seed::target(21),"new_ids":NEW_IDS,"marker_file":marker,"timeout_seconds":600})
}
fn assert_output(service: &EncodedTeletextAuthoredService, target: &ExpectedCopies) {
    let pixels = rasters(service);
    assert_literal(&pixels, State::Dormant);
    assert_eq!(pixels, target.rasters, "all three complete native previews");
    assert_eq!(service.pages().len(), target.units.len());
    for (page, units) in service.pages().iter().zip(&target.units) {
        assert_eq!(
            page.data_units(),
            units,
            "every independently encoded native unit"
        );
    }
}
async fn assert_concept(
    fixture: &Fixture,
    flow: &Flow,
    complete: &BrowserComplete,
    target: &ExpectedCopies,
) -> TeletextDesignRevision {
    assert_eq!(complete.probe_version, 1);
    assert_eq!(complete.case, CASE);
    assert_eq!(complete.design_id, flow.saved[0].head().id);
    assert_eq!(complete.revision, 5);
    assert_eq!(complete.new_ids, NEW_IDS);
    let application = &fixture.state.application;
    let head = application
        .teletext_design_head(complete.design_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(head.revision, 5);
    assert!(!head.deleted);
    assert!(application
        .teletext_design_revision(head.id, 6, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .is_none());
    let stored = application
        .teletext_design_revision(head.id, 5, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.digest, complete.digest);
    let documents = decode_teletext_design_documents(&stored.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert_eq!(overlay.schema_version(), 18);
    assert_eq!(overlay.resource_object_definition_additions().len(), 4);
    let source = flow.accepted[0]
        .service()
        .declared()
        .semantic()
        .objects()
        .definition(seed::target(21))
        .unwrap();
    for ((addition, expected), id) in overlay
        .resource_object_definition_additions()
        .iter()
        .zip(&target.definitions)
        .zip(NEW_IDS)
    {
        let mut copy = source.clone();
        copy.identity = seed::target(id);
        assert_eq!(
            addition.definition(),
            &copy,
            "complete captured source body/type/resource, only ID changes"
        );
        assert_eq!(addition.definition(), expected);
    }
    assert!(
        overlay.pages().is_empty()
            && overlay.local_program_operands().is_empty()
            && overlay.local_object_operands().is_empty()
            && overlay.local_object_glyph_program_overlays().is_empty()
            && overlay.resource_object_operands().is_empty()
            && overlay.resource_object_program_overlays().is_empty()
            && overlay.drcs_bitmap_overlays().is_empty()
    );
    let restored = reload_original_teletext_design_revision(
        application,
        head.id,
        5,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    let original = flow.saved[0].candidate();
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(restored.metadata(), original.metadata());
    assert_eq!(restored.transmissions(), original.transmissions());
    assert_eq!(restored.sources().len(), original.sources().len());
    for (after, before) in restored.sources().iter().zip(original.sources()) {
        assert_eq!(after.bytes(), before.bytes());
        assert_eq!(after.authoring(), before.authoring());
    }
    assert_eq!(
        restored.producers()[0].original(),
        original.producers()[0].original()
    );
    assert_output(restored.encoded().unwrap(), target);
    assert_original_pointers(fixture, flow).await;
    stored
}
async fn assert_exported(
    fixture: &Fixture,
    stored: &TeletextDesignRevision,
    target: &ExpectedCopies,
) {
    let result = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: stored.design_id,
            revision: 5,
            design_digest: stored.digest.clone(),
            expected_design_head: 5,
            pages: None,
        },
        policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextDesignMutation::Saved(exported) = result else {
        panic!("all four copied definitions must export from current concept")
    };
    assert_eq!(exported.metadata().revision, 5);
    assert_eq!(
        exported.sidecar().project.editor_profile,
        "qteletextmaker-1.0-carrier-v8"
    );
    for (id, records) in &target.companions {
        assert_eq!(
            &file_records(&exported, companion_name(&exported, *id)),
            records,
            "complete independent context for each new identity"
        );
    }
}
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires headed Chromium/Firefox localized Copy forms and a fresh explicit ready path"]
async fn headed_admin_resource_definition_copy_localized_concepts_and_cold_reopen() {
    let ready_path = std::path::PathBuf::from(crate::client_capture::selected(
        "ERSATZRS_TELETEXT_POLISH_ADMIN_READY",
    ));
    assert!(ready_path.is_absolute());
    let directory = ready_path.parent().unwrap();
    assert!(
        directory.is_dir(),
        "caller supplies a fresh existing proof directory"
    );
    let target = Box::pin(qualify_copies()).await;
    let fixture = Fixture::new_persistent().await;
    // Locale trials restore these canonical preferences; seed them before both
    // exact configuration snapshots rather than excluding UI rows afterward.
    for (key, value) in [
        (ersatzrs_domain::CONFIG_KEY_PAGES_LANGUAGE, "en-us"),
        (ersatzrs_domain::CONFIG_KEY_PAGES_IS_DARK_MODE, "False"),
    ] {
        ordinary_stream_flow::setting(&fixture, key, value).await;
    }
    http_routes::configure_authoring_policy(&fixture).await;
    let (mut fixture, flow) = Box::pin(setup_with_fixture_policy_and_writer(
        super::super::response,
        fixture,
        seed::encoding,
        authoring_metadata::write_compact,
    ))
    .await;
    assert_eq!(
        rasters(flow.accepted[0].service()),
        target.rasters,
        "unused copies preserve every original display pixel"
    );
    let retained = resource_bank_admin::Retained::capture(&fixture).await;
    let marker_path = directory.join("browser-complete.json");
    assert!(!marker_path.exists(), "completion must be a fresh marker");
    let mut origin = live_client::LiveOrigin::start(&fixture).await.unwrap();
    let outcome = AssertUnwindSafe(async {
        live_client::write_new(
            &ready_path,
            &serde_json::to_vec_pretty(&ready(&origin, &flow, &marker_path)).unwrap(),
        )
        .unwrap();
        let complete: BrowserComplete = tokio::time::timeout(
            Duration::from_mins(10),
            resource_bank_admin::marker(directory, "browser-complete.json"),
        )
        .await
        .expect("headed Copy proof must finish within600seconds")
        .expect("closed valid browser completion marker");
        let stored = Box::pin(assert_concept(&fixture, &flow, &complete, &target)).await;
        Box::pin(assert_exported(&fixture, &stored, &target)).await;
        retained.assert_unchanged(&fixture, &flow).await;
        (complete, stored)
    })
    .catch_unwind()
    .await;
    let closed = origin.close().await;
    let (complete, stored) = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    closed.expect("owned headed Copy router must close");
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    Box::pin(assert_concept(&fixture, &flow, &complete, &target)).await;
    Box::pin(assert_exported(&fixture, &stored, &target)).await;
    retained.assert_unchanged(&fixture, &flow).await;
    assert!(fixture.playout.try_recv().is_err());
    live_client::write_new(&directory.join("native-copy-success.json"), &serde_json::to_vec_pretty(&json!({"probe_version":1,"case":CASE,"browser":complete,
        "schema_version":18,"concept_revision":5,"copied_ids":NEW_IDS,"four_complete_definitions_only_id_changed":true,
        "all_three_previews_and_native_units_exact":true,"fresh_export_all_four_identities":true,"cold_reopen":true,
        "original_publication_night_profile_and_package_unchanged":true,"ordinary_stream_or_client_acceptance_claim":false,"actual_qtm_claim":false,"full_engine_acceptance_claim":false})).unwrap()).unwrap();
    fixture.state.pool.close().await;
}
