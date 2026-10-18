//! Historical originals and current publication authority remain separate.
use super::*;
use ersatzrs_domain::{TeletextDesignRevision, TeletextProfileAvailability};

pub(super) type Settings = Vec<(String, Option<String>, Option<String>, Option<String>)>;

pub(super) struct Captured {
    pub design: SavedOriginalTeletextDesign,
    revision: TeletextDesignRevision,
    source: Vec<u8>,
    metadata: Value,
    units: OriginalTransport,
}
pub(super) async fn capture(
    fixture: &Fixture,
    channel: i64,
    revision: u8,
    theme: &str,
) -> Captured {
    let candidate = super::super::candidate(fixture, channel).await;
    assert_eq!(candidate.sources()[0].contract(), TeletextPageContract::V3);
    let source = candidate.sources()[0].bytes().to_vec();
    let metadata = package::response(revision, theme)["authoring"].clone();
    assert_eq!(
        serde_json::to_value(candidate.sources()[0].authoring().unwrap()).unwrap(),
        metadata
    );
    let units = transport(candidate.encoded().unwrap());
    let design = designs::save(fixture, candidate, None).await;
    let stored = fixture
        .state
        .application
        .teletext_design_revision(design.head().id, 1, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    Captured {
        design,
        revision: stored,
        source,
        metadata,
        units,
    }
}
pub(super) async fn verify(fixture: &Fixture, captured: &Captured) {
    assert_eq!(
        fixture
            .state
            .application
            .teletext_design_revision(
                captured.design.head().id,
                1,
                designs::budget().max_serialized_bytes
            )
            .await
            .unwrap()
            .unwrap(),
        captured.revision
    );
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        captured.design.head().id,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(restored.sources()[0].bytes(), captured.source);
    assert_eq!(
        serde_json::to_value(restored.sources()[0].authoring().unwrap()).unwrap(),
        captured.metadata
    );
    assert_eq!(
        restored.selection(),
        captured.design.candidate().selection()
    );
    assert_eq!(
        restored
            .guide_inputs()
            .unwrap()
            .request(TeletextPageContract::V3),
        captured
            .design
            .candidate()
            .requests()
            .request(TeletextPageContract::V3)
    );
    assert_eq!(transport(restored.encoded().unwrap()), captured.units);
    drop(restored);
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: captured.design.candidate().selection().channel_id,
            design_id: captured.design.head().id,
            revision: 1,
            owner: Some(channel_documents::address(123, 16)),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    let expected_preview = ersatzrs_web::host::teletext_preview::project_teletext_preview(
        captured.design.candidate().encoded().unwrap(),
        channel_documents::address(123, 16),
        ersatzrs_domain::TeletextColourPixelOptions {
            reveal_concealed: false,
            flash_phase: ersatzrs_domain::TeletextFlashPhase::Original,
        },
        ersatzrs_web::host::teletext_preview::TeletextPreviewBudget {
            max_pages: 16,
            max_model_records: 32_768,
            max_pixels: 168_000,
            max_diagnostics: 1400,
            max_work: 50_000_000,
        },
    )
    .unwrap();
    assert_eq!(preview.raster(), Some(expected_preview.raster()));
    assert_eq!(preview.authoring().len(), 1);
    let authoring = &preview.authoring()[0];
    assert_eq!(
        serde_json::to_value(&authoring.template).unwrap(),
        captured.metadata["template"]
    );
    assert_eq!(
        serde_json::to_value(&authoring.theme).unwrap(),
        captured.metadata["theme"]
    );
    assert_eq!(
        serde_json::to_value(&authoring.regions).unwrap(),
        captured.metadata["pages"][0]["regions"]
    );
    assert_eq!(
        serde_json::to_value(&authoring.dynamic_fields).unwrap(),
        captured.metadata["pages"][0]["dynamic_fields"]
    );
}
pub(super) async fn pointers(fixture: &Fixture) -> (Vec<(i64, i64)>, Vec<(i64, i64)>) {
    (sqlx::query_as(r#"SELECT "ChannelId","RequestId" FROM "TeletextPublicationRequestCurrent" ORDER BY "ChannelId""#)
        .fetch_all(&fixture.state.pool).await.unwrap(),
     sqlx::query_as(r#"SELECT "ChannelId","GenerationId" FROM "TeletextAcceptedGenerationCurrent" ORDER BY "ChannelId""#)
        .fetch_all(&fixture.state.pool).await.unwrap())
}
pub(super) async fn settings(fixture: &Fixture) -> Settings {
    sqlx::query_as(r#"SELECT "Key","Value","ReferenceKind","Reference" FROM "AddonSetting" WHERE "AddonId"=? ORDER BY "Key""#)
        .bind(ADDON).fetch_all(&fixture.state.pool).await.unwrap()
}

pub(super) async fn reopen(
    mut fixture: Fixture,
    captured: [&Captured; 3],
    accepted: &ersatzrs_web::host::teletext_generations::AcceptedTeletextOutputCandidate,
    request: &ersatzrs_domain::TeletextPublicationRequest,
    settings_before: &Settings,
    calls: &[u8],
) -> Fixture {
    fixture = fixture.reopen().await;
    for captured in captured {
        Box::pin(verify(&fixture, captured)).await;
    }
    let recovered = generations::recover(&fixture).await.unwrap();
    assert_eq!(recovered.generation(), accepted.generation());
    assert_eq!(
        fixture
            .state
            .application
            .teletext_publication_request(1)
            .await
            .unwrap()
            .as_ref(),
        Some(request)
    );
    assert_eq!(package::calls(&fixture).await, calls);
    assert_eq!(&settings(&fixture).await, settings_before);
    assert!(fixture.playout.try_recv().is_err());
    fixture
}
pub(super) async fn stale(
    fixture: &Fixture,
    day: &TeletextProfile,
    night: &TeletextProfile,
    binding: &ChannelTeletextBinding,
) {
    let before = package::calls(fixture).await;
    let profiles = fixture
        .state
        .application
        .list_teletext_profiles(ADDON)
        .await
        .unwrap();
    assert_eq!(profiles.len(), 2);
    for expected in [day, night] {
        let current = profiles
            .iter()
            .find(|current| current.profile.id == expected.id)
            .unwrap();
        assert_eq!(&current.profile, expected);
        assert_eq!(
            current.availability,
            TeletextProfileAvailability::StaleInstallation
        );
    }
    assert!(
        ersatzrs_web::host::teletext_authoring::observe_teletext_authoring_selection(
            &fixture.state.application,
            1,
            designs::deadline()
        )
        .await
        .is_err()
    );
    assert!(matches!(
        fixture
            .state
            .application
            .save_teletext_profile(&TeletextProfileSave {
                addon_id: ADDON.to_owned(),
                name: day.name.clone(),
                settings: day.settings.clone(),
                expected_installation: day.validated_installation.clone(),
                existing: Some((day.id, day.revision)),
            })
            .await
            .unwrap(),
        TeletextProfileMutation::Rejected(TeletextProfileRejection::Conflict)
    ));
    assert!(matches!(
        fixture
            .state
            .application
            .save_channel_teletext_binding(&ChannelTeletextBindingSave {
                channel_id: 1,
                addon_id: ADDON.to_owned(),
                profile_id: day.id,
                enabled: true,
                expected_installation: day.validated_installation.clone(),
                expected_profile_revision: day.revision,
                expected_binding_revision: Some(binding.revision),
            })
            .await
            .unwrap(),
        TeletextProfileMutation::Rejected(TeletextProfileRejection::Conflict)
    ));
    assert_eq!(
        fixture
            .state
            .application
            .channel_teletext_bindings(1)
            .await
            .unwrap()[0]
            .binding,
        *binding
    );
    assert_eq!(package::calls(fixture).await, before);
}
