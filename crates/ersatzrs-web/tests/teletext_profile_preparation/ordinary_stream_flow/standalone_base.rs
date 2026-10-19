//! Exact standalone base subpages before and after an actual served-host restart.
//! The headed sibling supplies captures; this uses the existing AV/transport gate.

use super::*;

#[path = "standalone_base/greek_unicode.rs"]
mod greek_unicode;

#[path = "standalone_base/scoped_restart.rs"]
mod scoped_restart;

#[path = "standalone_base/updates.rs"]
mod updates;

#[path = "standalone_base/profile_lifecycle.rs"]
mod profile_lifecycle;

fn response(theme: &str) -> Value {
    let mut value: Value = serde_json::from_str(include_str!(
        "../../fixtures/teletext-level15-restart-v2.json"
    ))
    .unwrap();
    assert!(matches!(theme, "day" | "night"));
    if theme == "night" {
        for page in value["pages"].as_array_mut().unwrap() {
            for row in page["rows"].as_array_mut().unwrap() {
                for run in row["runs"].as_array_mut().unwrap() {
                    if run["style"]["foreground"] == "red" {
                        run["style"]["foreground"] = json!("blue");
                    }
                }
            }
        }
    }
    value
}

fn assert_base(service: &Arc<EncodedTeletextAuthoredService>) {
    assert!(service.magazine_units().is_empty());
    assert!(service.declared().resource_inventory().is_empty());
    assert!(service.compiler_drcs_resources().is_empty());
    assert_eq!(service.pages().len(), 3);
    for (index, (page, subcode)) in [(200, 0), (200, 32), (201, 0)].into_iter().enumerate() {
        let owner = channel_documents::address(page, subcode);
        assert_eq!(
            service.pages()[index].inventory_record(),
            ersatzrs_ffmpeg::teletext::TeletextPhysicalPageInventoryRecord::Display(owner)
        );
        let preview = project_teletext_preview(
            service,
            owner,
            TeletextColourPixelOptions {
                reveal_concealed: false,
                flash_phase: TeletextFlashPhase::Original,
            },
            client_capture::preview_budget(),
        )
        .unwrap();
        assert!(preview.raster().diagnostics().is_empty());
        assert_eq!(preview.raster().width(), 480);
        let (target_page, target_subcode) = [(200, 32), (201, 0), (200, 0)][index];
        assert_eq!(preview.navigation().len(), 1);
        let target = channel_documents::address(target_page, target_subcode);
        assert_eq!(preview.navigation()[0].target.page, target.page);
        assert_eq!(preview.navigation()[0].target.subcode, Some(target.subcode));
    }
}

#[test]
fn shared_restart_fixture_is_standalone_and_preserves_exact_subpage_navigation() {
    for theme in ["day", "night"] {
        let mut budget = super::super::documents::validation_budget();
        budget.service.max_objects = 0;
        budget.declarations.max_objects = 0;
        budget.declarations.max_resources = 0;
        budget.declarations.max_mots = 0;
        let original = ersatzrs_app::teletext_pages_v2::parse_declared_service(
            &serde_json::to_vec(&response(theme)).unwrap(),
            GENERATION,
            budget.service,
            budget.declarations,
        )
        .unwrap();
        let service = ersatzrs_ffmpeg::teletext::encode_teletext_authored_service(
            original,
            &[],
            &super::super::documents::policy().encoding,
        )
        .unwrap();
        assert_base(&Arc::new(service));
    }
}

async fn accept_base(fixture: &Fixture, channel: i64) -> AcceptedTeletextOutputCandidate {
    let request = request_base(fixture, channel, None).await;
    publish_base(fixture, request, None).await
}

fn base_policy(
) -> ersatzrs_web::host::teletext_channel_documents::TeletextChannelDocumentEncodingPolicy {
    // The browser explicitly chooses 200/0000. Two subpages make inference
    // deliberately ambiguous; preserve the existing EntryRequired safeguard.
    let mut policy = channel_documents::policy();
    policy.entry = Some(channel_documents::address(200, 0));
    policy
}

async fn request_base(
    fixture: &Fixture,
    channel: i64,
    expected_request: Option<i64>,
) -> ersatzrs_domain::TeletextPublicationRequest {
    let policy = base_policy();
    let candidate = prepare_channel_teletext_document_candidate(
        &fixture.state.application,
        channel,
        Some(ADDON),
        &guide(&[]),
        policy.clone(),
    )
    .await
    .unwrap();
    let saved = designs::save(fixture, candidate, None).await;
    let requested = request_original_teletext_design_publication(
        &fixture.state.application,
        saved.head().id,
        saved.head().revision,
        saved.digest().to_owned(),
        saved.head().revision,
        expected_request,
        designs::budget(),
        policy.clone(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextPublicationMutation::Requested(requested) = requested else {
        panic!("explicit base request")
    };
    assert_eq!(
        designs::units(saved.candidate().encoded().unwrap()),
        designs::units(requested.design().encoded().unwrap())
    );
    requested.request().clone()
}

async fn publish_base(
    fixture: &Fixture,
    request: ersatzrs_domain::TeletextPublicationRequest,
    expected_generation_id: Option<i64>,
) -> AcceptedTeletextOutputCandidate {
    let accepted = accept_original_teletext_generation(
        &fixture.state.application,
        TeletextGenerationTarget {
            expected_design_head: request.revision,
            request,
            expected_generation_id,
        },
        designs::budget(),
        base_policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    let TeletextGenerationMutation::Accepted(accepted) = accepted else {
        panic!("explicit base acceptance")
    };
    accepted
}

#[tokio::test]
#[ignore = "requires actual standalone1.5 served-host restart captures plus pinned FFmpeg/ffprobe9 and TSDuck"]
async fn served_level15_restart_retains_exact_subpages_navigation_and_source_av() {
    let tools = Tools::load().await;
    let prefix = client_capture::selected("ERSATZRS_TELETEXT_RESTART_CAPTURE");
    let (fixture, _) = installed_profiles_with(&tools, response, 12).await;
    let source = tokio::fs::read(format!("{prefix}.source.ts"))
        .await
        .unwrap();
    let accepted = [
        accept_base(&fixture, 1).await,
        accept_base(&fixture, 2).await,
    ];
    for candidate in &accepted {
        assert_base(candidate.service());
    }
    assert_ne!(
        designs::units(accepted[0].service()),
        designs::units(accepted[1].service())
    );
    for (index, suffix) in [
        (0, "before.day"),
        (1, "before.night"),
        (0, "after.day"),
        (1, "after.night"),
    ] {
        let bytes = tokio::fs::read(format!("{prefix}.{suffix}.ts"))
            .await
            .unwrap();
        let report = PathBuf::from(format!("{prefix}.{suffix}.analyze.json"));
        client_capture::verify_transport_with_qualified_authored_pts_gaps(
            &tools.tsanalyze,
            &tools.ffprobe,
            &bytes,
            0,
            Some(&report),
            false,
        )
        .await;
        client_capture::verify_av(&tools.ffmpeg, &source, &bytes).await;
        assert_emitted_model(&bytes, accepted[index].service());
        for page in ["200", "201"] {
            let text = client_capture::decode_page_text(&tools.ffmpeg, &bytes, page).await;
            assert!(text.contains(&format!("BASE {page}/")));
            assert!(text.contains("£Á"));
        }
        // Preserve actual emitted order for the existing independent decoder,
        // including every subpage/header. No reconstruction from expected units.
        let mut assembler = SuppliedTeletextAssembler::new(512).unwrap();
        let mut units = Vec::new();
        for packet in bytes.chunks_exact(188) {
            if let TeletextAssemblyEvent::Complete(pes) =
                assembler.push(packet.try_into().unwrap()).unwrap()
            {
                units.extend(
                    pes.units()
                        .filter_map(|unit| unit.teletext_unit())
                        .map(|unit| unit.to_vec()),
                );
            }
        }
        assembler.finish().unwrap();
        let mut output = tokio::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(format!("{prefix}.{suffix}.units.json"))
            .await
            .unwrap();
        output
            .write_all(&serde_json::to_vec(&units).unwrap())
            .await
            .unwrap();
    }
    fixture.state.pool.close().await;
}

#[tokio::test]
#[ignore = "requires actual ordinary Next capture, source and pinned FFmpeg9/TSDuck"]
async fn served_next_level15_wrapper_retains_model_and_source_av() {
    let tools = Tools::load().await;
    let root = client_capture::selected("ERSATZRS_TELETEXT_NEXT_CAPTURE");
    let (fixture, _) = installed_profiles_with(&tools, response, 12).await;
    let source = tokio::fs::read(format!("{root}/next-source.ts"))
        .await
        .unwrap();
    let bytes = tokio::fs::read(format!("{root}/next-ordinary.ts"))
        .await
        .unwrap();
    let accepted = accept_base(&fixture, 1).await;
    assert_base(accepted.service());
    super::super::editorial_imports::live_updates::live_tcp::assert_uninterrupted_source_clock(
        &bytes,
    );
    client_capture::verify_av(&tools.ffmpeg, &source, &bytes).await;
    assert_emitted_model(&bytes, accepted.service());
    scoped_restart::export_units(&format!("{root}/capture"), "next", &bytes).await;
    fixture.state.pool.close().await;
}

// Keep the existing complete transport/audio-language oracle independent from
// the authored-model/source-AV gate. A failure here remains a mandatory F5 gap.
#[tokio::test]
#[ignore = "requires actual ordinary Next capture and pinned ffprobe9/TSDuck"]
async fn served_next_level15_wrapper_retains_audio_metadata() {
    let tools = Tools::load().await;
    let root = client_capture::selected("ERSATZRS_TELETEXT_NEXT_CAPTURE");
    let bytes = tokio::fs::read(format!("{root}/next-ordinary.ts"))
        .await
        .unwrap();
    client_capture::verify_transport_with_qualified_authored_pts_gaps(
        &tools.tsanalyze,
        &tools.ffprobe,
        &bytes,
        0,
        None,
        false,
    )
    .await;
}
