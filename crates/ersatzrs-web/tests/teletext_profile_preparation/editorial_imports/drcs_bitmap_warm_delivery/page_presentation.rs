//! One continuing ordinary response per viewer joins a new immutable page-X28
//! original and genuine QTM bank concept to explicit publication over old M29.
//! This native TCP gate supplies no headed-client or complete engine acceptance.

use super::*;
use ersatzrs_app::{
    teletext_design_documents::decode_teletext_design_documents,
    teletext_export_project::{parse_teletext_export_receipt_body, TeletextExportAssetScope},
};
use ersatzrs_domain::{
    ChannelTeletextBindingRead, TeletextEffectivePresentation, TeletextPresentationSource,
    TeletextProfileRead,
};
use ersatzrs_ffmpeg::teletext::EncodedTeletextAuthoredService;

const DIRECTORY_ENV: &str = "ERSATZRS_TELETEXT_M29_BANK_WARM_DELIVERY_DIR";
const ORIGINAL_HASH: &str = "d1e9a9d49a28be14adb1e727c11ab0cfe908bbd0df83ca49c48ef4e170a0a870";
const EDITED_HASH: &str = "6b4ac2e70bab6d73b6bdbb52bf9590887c7094582c14bbbca87433225aaebda6";

struct Retained {
    package: String,
    profiles: Vec<TeletextProfileRead>,
    bindings: [Vec<ChannelTeletextBindingRead>; 2],
    night_response: Vec<u8>,
}

impl Retained {
    async fn capture(fixture: &Fixture) -> Self {
        Self {
            package: calculate_addon_content_sha256(&fixture.root.path().join("package"))
                .await
                .unwrap(),
            profiles: fixture
                .state
                .application
                .list_teletext_profiles(ADDON)
                .await
                .unwrap(),
            bindings: [
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(1)
                    .await
                    .unwrap(),
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(2)
                    .await
                    .unwrap(),
            ],
            night_response: std::fs::read(
                fixture.root.path().join("authoring-response-night.json"),
            )
            .unwrap(),
        }
    }

    async fn assert_unchanged(&self, fixture: &Fixture, flow: &Flow) {
        assert_eq!(
            calculate_addon_content_sha256(&fixture.root.path().join("package"))
                .await
                .unwrap(),
            self.package
        );
        assert_eq!(
            fixture
                .state
                .application
                .list_teletext_profiles(ADDON)
                .await
                .unwrap(),
            self.profiles
        );
        for (index, bindings) in self.bindings.iter().enumerate() {
            assert_eq!(
                fixture
                    .state
                    .application
                    .channel_teletext_bindings(i64::try_from(index + 1).unwrap())
                    .await
                    .unwrap(),
                *bindings
            );
        }
        assert_eq!(
            std::fs::read(fixture.root.path().join("authoring-response-night.json")).unwrap(),
            self.night_response
        );
        assert_eq!(designs::config(fixture).await, flow.config);
        // The new original requires exactly one explicit installed-producer
        // preparation. No import, publication or reopen may replay that call.
        let mut calls = flow.calls.clone();
        calls.extend_from_slice(b"day:teletext.pages.v3\n");
        assert_eq!(
            std::fs::read(fixture.root.path().join("authoring-calls")).unwrap(),
            calls
        );
        for index in 0..2 {
            assert_eq!(
                original(fixture, &flow.saved[index]).await,
                flow.originals[index]
            );
            assert_eq!(
                receipt(fixture, &flow.exported[index]).await,
                flow.receipts[index]
            );
        }
        generations::assert_same_output(&recover(fixture, 2).await, &flow.accepted[1]);
    }
}

struct Prepared {
    saved: SavedOriginalTeletextDesign,
    original: TeletextDesignRevision,
    exported: ExportedOriginalTeletextDesign,
    receipt: TeletextDesignExportReceipt,
    concept: SavedEditorialTeletextDesign,
    pending: TeletextPublicationRequest,
}

async fn old_activation(
    fixture: &Fixture,
    flow: &Flow,
    day_pending: Option<&TeletextPublicationRequest>,
) {
    for (index, channel) in [1, 2].into_iter().enumerate() {
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(channel)
                .await
                .unwrap(),
            Some(flow.accepted[index].generation().clone())
        );
        let expected = if index == 0 {
            day_pending.unwrap_or(&flow.pending[0])
        } else {
            &flow.pending[1]
        };
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(channel)
                .await
                .unwrap(),
            Some(expected.clone())
        );
    }
}

async fn prepare_concept(fixture: &Fixture, flow: &Flow, actual: &Path) -> Prepared {
    let input = drcs_bitmap::read_retained_editor_file(&actual.join("native-input.tti"));
    let edited = drcs_bitmap::read_retained_editor_file(&actual.join("edited.tti"));
    assert_eq!(drcs_bitmap::hash(&input), ORIGINAL_HASH);
    assert_eq!(drcs_bitmap::hash(&edited), EDITED_HASH);
    authoring_metadata::write(
        fixture,
        "day",
        &drcs_bank::explicit_page_response("day", 124, 32),
    );
    let candidate = prepare_with_policy(fixture, 1, drcs_companion::dense_encoding_policy()).await;
    assert_eq!(candidate.selection(), flow.accepted[0].selection());
    let saved = designs::save(fixture, candidate, None).await;
    assert_ne!(saved.head().id, flow.saved[0].head().id);
    assert_ne!(saved.head().id, flow.saved[1].head().id);
    let original = original(fixture, &saved).await;
    let exported = export(fixture, &saved).await;
    let receipt = receipt(fixture, &exported).await;
    assert_ne!(receipt.metadata.id, flow.receipts[0].metadata.id);
    old_activation(fixture, flow, None).await;
    let body = parse_teletext_export_receipt_body(&receipt.receipt, policy().project).unwrap();
    let banks: Vec<_> = body
        .project
        .assets
        .iter()
        .filter(|asset| {
            matches!(&asset.scope,
        TeletextExportAssetScope::DrcsTtiSet { resource_names }
        if resource_names.len() == 2 && resource_names.iter().all(Option::is_some))
        })
        .collect();
    assert_eq!(banks.len(), 1);
    let mut request = drcs_bitmap::from_receipt(&receipt, 1);
    let bank = request
        .files
        .iter_mut()
        .find(|file| file.name == banks[0].name)
        .unwrap();
    assert_eq!(
        bank.bytes, input,
        "fresh page-X28 original must export the exact genuine editor bank input"
    );
    bank.bytes = edited;
    let concept = import(fixture, request).await;
    assert_eq!(concept.head().id, saved.head().id);
    assert_eq!(concept.head().revision, 2);
    old_activation(fixture, flow, None).await;
    let requested =
        ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication(
            &fixture.state.application,
            concept.head().id,
            2,
            concept.digest().to_owned(),
            2,
            Some(flow.pending[0].id),
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap();
    let TeletextPublicationMutation::Requested(requested) = requested else {
        panic!("the new immutable original's genuine concept must explicitly request publication");
    };
    let pending = requested.request().clone();
    old_activation(fixture, flow, Some(&pending)).await;
    Prepared {
        saved,
        original,
        exported,
        receipt,
        concept,
        pending,
    }
}

async fn transition_to_page_presentation(
    fixture: &Fixture,
    flow: &Flow,
    origin: &live_tcp::Origin,
    actual: &Path,
    directory: &Path,
) -> (Prepared, AcceptedTeletextOutputCandidate) {
    let before = original_cycles(origin, flow, [0; 3]).await;
    for (index, observed) in before.iter().enumerate() {
        write_units(
            directory,
            &format!("viewer-{index}-original-units.json"),
            observed,
        );
    }
    let prepared = prepare_concept(fixture, flow, actual).await;
    let held = original_cycles(
        origin,
        flow,
        origin.captures().each_ref().map(Capture::position),
    )
    .await;
    let offsets = origin.captures().each_ref().map(Capture::position);
    let accepted = generations::accepted(
        accept_original_teletext_generation(
            &fixture.state.application,
            TeletextGenerationTarget {
                request: prepared.pending.clone(),
                expected_design_head: 2,
                expected_generation_id: Some(flow.accepted[0].generation().id),
            },
            designs::budget(),
            channel_documents::policy(),
            designs::deadline(),
        )
        .await
        .unwrap(),
    );
    assert_eq!(
        accepted.generation().request.design_id,
        prepared.saved.head().id
    );
    assert_eq!(accepted.generation().request.revision, 2);
    assert_eq!(accepted.selection(), flow.accepted[0].selection());
    let day = pattern(&accepted, 2);
    let night = pattern(&flow.accepted[1], 1);
    let views = origin.captures();
    let (a, b, c) = tokio::join!(
        wait_cycle(&views[0], offsets[0], &day),
        wait_cycle(&views[1], offsets[1], &day),
        wait_cycle(&views[2], offsets[2], &night)
    );
    let updated = [a, b, c];
    let (a, b, c) = tokio::join!(
        wait_cycle(&views[0], updated[0].end, &day),
        wait_cycle(&views[1], updated[1].end, &day),
        wait_cycle(&views[2], updated[2].end, &night)
    );
    let repeated = [a, b, c];
    for index in 0..3 {
        write_units(
            directory,
            &format!("viewer-{index}-updated-units.json"),
            &updated[index],
        );
        write_units(
            directory,
            &format!("viewer-{index}-repeat-units.json"),
            &repeated[index],
        );
    }
    live_tcp::write_new(&directory.join("wire-offsets.json"), &serde_json::to_vec_pretty(&json!({
        "original_complete_pes_end":before.each_ref().map(|cycle|cycle.end),
        "pending_complete_pes_end":held.each_ref().map(|cycle|cycle.end),
        "pre_accept_offsets":offsets,"updated_complete_pes_end":updated.each_ref().map(|cycle|cycle.end),
        "repeat_complete_pes_end":repeated.each_ref().map(|cycle|cycle.end),
        "old_day_design_id":flow.saved[0].head().id,"new_day_design_id":prepared.saved.head().id,
        "new_receipt_id":prepared.receipt.metadata.id,"day_s2":2,"night_s2":1,
        "old_day_m29":true,"new_day_full_page_x28":true,"new_day_m29":false,
        "same_response":true,"original_tti_sha256":ORIGINAL_HASH,"genuine_edited_tti_sha256":EDITED_HASH
    })).unwrap()).unwrap();
    (prepared, accepted)
}

fn assert_full_page_handover(
    old: &EncodedTeletextAuthoredService,
    new: &EncodedTeletextAuthoredService,
) {
    fn page(
        value: &EncodedTeletextAuthoredPage,
    ) -> &ersatzrs_ffmpeg::teletext::EncodedTeletextPage {
        match value {
            EncodedTeletextAuthoredPage::NativeDisplay(page) => page.encoded().page(),
            EncodedTeletextAuthoredPage::CompiledDisplay(page) => page.page(),
            _ => unreachable!("resources were compared separately"),
        }
    }

    assert!(!old.magazine_units().is_empty());
    assert!(new.magazine_units().is_empty());
    assert!(new
        .declared()
        .semantic()
        .magazine_presentations()
        .is_empty());
    assert_eq!(
        old.declared().semantic().base(),
        new.declared().semantic().base()
    );
    assert_eq!(old.inventory(), new.inventory());
    for page in &old.declared().semantic().base().pages {
        let Some(TeletextEffectivePresentation::Extension {
            source: TeletextPresentationSource::Magazine,
            defaults: expected,
        }) = old
            .declared()
            .semantic()
            .effective_presentation(page.address)
        else {
            panic!("old display must inherit actual M29");
        };
        assert!(
            matches!(new.declared().semantic().effective_presentation(page.address),
            Some(TeletextEffectivePresentation::Extension { source: TeletextPresentationSource::Page, defaults })
            if defaults == expected)
        );
        assert_eq!(
            external_editor_probe::preview_page(old, page.address.page.get()),
            external_editor_probe::preview_page(new, page.address.page.get())
        );
    }
    for (old, new) in old.pages().iter().zip(new.pages()) {
        assert_eq!(old.resource_header(), new.resource_header());
        if old.resource_header().is_some() {
            assert_eq!(old.data_units(), new.data_units());
            continue;
        }
        let old = project_encoded_teletext_display(page(old), 128).unwrap();
        let new = project_encoded_teletext_display(page(new), 128).unwrap();
        assert_eq!(old.header, new.header);
        assert!(new.records.iter().any(|record| matches!(
            record,
            TeletextEncodedDisplayRecord::Triplets {
                row: 28,
                designation: 0,
                ..
            }
        )));
        let without_x28 = |records: Vec<_>| {
            records
                .into_iter()
                .filter(|record| {
                    !matches!(
                        record,
                        TeletextEncodedDisplayRecord::Triplets { row: 28, .. }
                    )
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(
            without_x28(old.records),
            without_x28(new.records),
            "all original fallback, navigation, local X26 and other packets stay exact"
        );
    }
}

async fn assert_retained_result(
    fixture: &mut Fixture,
    flow: &Flow,
    retained: &Retained,
    prepared: &Prepared,
    accepted: &AcceptedTeletextOutputCandidate,
) {
    retained.assert_unchanged(fixture, flow).await;
    assert_eq!(original(fixture, &prepared.saved).await, prepared.original);
    assert_eq!(receipt(fixture, &prepared.exported).await, prepared.receipt);
    let restored = drcs_bitmap::restored(fixture, &prepared.concept).await;
    let original_service = prepared.saved.candidate().encoded().unwrap();
    assert_full_page_handover(flow.accepted[0].service(), original_service);
    drcs_bank::assert_native_edit(original_service, restored.encoded().unwrap(), false);
    drcs_bank::assert_native_edit(original_service, accepted.service(), false);
    let revision = drcs_bitmap::stored(fixture, &prepared.concept).await;
    let documents =
        decode_teletext_design_documents(&revision.document, designs::budget()).unwrap();
    let overlay = documents.editorial_overlay().unwrap();
    assert!(overlay.pages().is_empty());
    assert!(overlay.local_program_operands().is_empty());
    assert_eq!(overlay.drcs_bitmap_overlays().len(), 1);
    assert_eq!(
        overlay.provenance().original_digest,
        prepared.original.digest
    );
    assert_eq!(
        overlay.provenance().export_receipt_id,
        prepared.receipt.metadata.id
    );
    let bitmap = overlay.drcs_bitmap_overlays()[0];
    assert_eq!(bitmap.identity().kind(), TeletextResourceKind::Drcs);
    assert_eq!(bitmap.identity().page().magazine(), 1);
    assert_eq!(bitmap.identity().page().page(), 0xc0);
    assert_eq!(bitmap.identity().logical_subpage().get(), 0);
    assert_eq!(bitmap.slot().get(), 0);
    assert_eq!(bitmap.expected().get()[0], 0x801);
    assert_eq!(bitmap.replacement().get()[0], 0x803);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap(),
        Some(prepared.pending.clone())
    );
    generations::assert_same_output(&recover(fixture, 1).await, accepted);
    assert!(fixture.playout.try_recv().is_err());
}

#[tokio::test]
#[ignore = "requires genuine dense QTM files, FFmpeg9/TSDuck and a create-new same-response M29 handover directory"]
async fn same_ordinary_viewers_publish_genuine_bank_concept_over_m29_with_full_page_x28() {
    let directory = PathBuf::from(client_capture::selected(DIRECTORY_ENV));
    let actual = PathBuf::from(client_capture::selected(
        "ERSATZRS_QTM_DRCS_BANK_CALIBRATION_DIR",
    ));
    std::fs::create_dir(&directory).unwrap();
    live_tcp::terminal_trace(&directory).unwrap();
    let tools = Tools::load().await;
    let (mut fixture, flow, source) = Box::pin(source_fixture_with_response(
        &tools,
        &directory,
        drcs_bank::dense_response,
        drcs_companion::dense_encoding_policy,
    ))
    .await;
    let retained = Retained::capture(&fixture).await;
    let mut origin = live_tcp::Origin::start(&fixture, &directory).await.unwrap();
    let mut observers = [
        live_tcp::Observer::start_for(&origin, 1, "day"),
        live_tcp::Observer::start_for(&origin, 1, "observer"),
        live_tcp::Observer::start_for(&origin, 2, "night"),
    ];
    let outcome = AssertUnwindSafe(async {
        tokio::time::timeout(Duration::from_secs(55), async {
            live_tcp::wait_attached(
                &origin,
                tokio::time::Instant::now() + Duration::from_secs(15),
            )
            .await
            .unwrap();
            Box::pin(transition_to_page_presentation(
                &fixture, &flow, &origin, &actual, &directory,
            ))
            .await
        })
        .await
        .expect("bounded same-response M29/editor publication exceeded55seconds")
    })
    .catch_unwind()
    .await;
    let stopped = AssertUnwindSafe(close(&mut origin, &mut observers))
        .catch_unwind()
        .await;
    live_tcp::save_captures(&directory, &origin).unwrap();
    if let Err(panic) = stopped {
        fixture.state.pool.close().await;
        std::panic::resume_unwind(panic);
    }
    let (prepared, accepted) = match outcome {
        Ok(value) => value,
        Err(panic) => {
            fixture.state.pool.close().await;
            std::panic::resume_unwind(panic);
        }
    };
    assert!(origin.single_requests());
    // Full synchronous pixel/model oracles run only after the paced producers
    // stop; they must not starve the current-thread physical delivery clock.
    assert_retained_result(&mut fixture, &flow, &retained, &prepared, &accepted).await;
    fixture = fixture.reopen().await;
    assert_retained_result(&mut fixture, &flow, &retained, &prepared, &accepted).await;
    fixture.state.pool.close().await;
    let reports = verify_with_decoder(
        &directory,
        &tools,
        &source,
        &origin,
        "teletext-editor-bank-warm-decoder.py",
        "indexed-warm-edited",
    )
    .await;
    live_tcp::write_new(&directory.join("warm-delivery.json"), &serde_json::to_vec_pretty(&json!({
        "schema_version":1,"source_requests_per_viewer":1,"source_resets_before_start":1,"source_resets_after_start":0,
        "source_sha256":live_tcp::hash(&source),"viewers":reports,"owned_streams_closed":true,
        "old_day_design_id":flow.saved[0].head().id,"new_day_design_id":prepared.saved.head().id,
        "new_concept_revision":2,"new_export_receipt_id":prepared.receipt.metadata.id,
        "old_m29_to_explicit_page_x28":true,"genuine_retained_bank_import":true,
        "channel2_profiles_bindings_package_originals_and_receipts_unchanged":true,
        "exact_additional_prepare_calls":1,"application_reopen_preserves_accepted_state":true,
        "limitations":"Native same-response publication and persistent independent MAIN bank decoding only; no headed warm client/GUI, host restart, natural EOF, side-panel, full Level2.5 or platform acceptance"
    })).unwrap()).unwrap();
}
