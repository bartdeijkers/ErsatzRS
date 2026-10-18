//! Installed V3 originals carry addon-owned edit scopes without granting editing.
//! POSIX fixtures inherit the parent's intentional Unix-only scope; no client proof.

use super::{channel_documents, designs, documents::DocumentFixture, generations, *};
use ersatzrs_app::{
    commands::SetAdminSetting, teletext_channel_documents::ChannelTeletextDocumentError,
    teletext_design_documents::TeletextSavedEditPolicy,
    teletext_profile_documents::BoundTeletextDocumentError,
};
use ersatzrs_domain::{TeletextColourPixel, TeletextPageContract};
use ersatzrs_ffmpeg::teletext::{EncodedTeletextAuthoredService, TeletextRasterPixel};
use ersatzrs_web::host::{
    teletext_authoring::{
        prepare_static_teletext_design, preview_original_teletext_design,
        TeletextAuthoringIdentity, TeletextAuthoringPreview, TeletextAuthoringPreviewRequest,
        TeletextAuthoringScope,
    },
    teletext_channel_documents::{
        prepare_channel_teletext_document_candidate, ChannelTeletextDocumentEncodingError,
        EncodedChannelTeletextDocumentCandidate,
    },
    teletext_designs::{reload_original_teletext_design_revision, SavedOriginalTeletextDesign},
    teletext_work::MAX_CONCURRENT_SETTING,
};
use serde_json::{json, Value};

#[path = "authoring_metadata/template_revisions.rs"]
mod template_revisions;

const AUTHORING_SCRIPT: &str = r#"case "$1" in
check) printf '%s' '{"status":"ready","code":"ready","message":"ok"}'; exit 0;;
pages) ;;
*) exit 9;;
esac
theme="$ERSATZRS_ADDON_SETTING_THEME"
case "$theme" in day|night) ;; *) exit 11;; esac
printf '%s:%s\n' "$theme" "$ERSATZRS_ADDON_CAPABILITY" >> ../authoring-calls
test "$ERSATZRS_ADDON_CAPABILITY" = teletext.pages.v3 || exit 10
cat > "../authoring-input-$theme.json"
cat "../authoring-response-$theme.json"
"#;

pub(super) fn response(theme: &str, page: u16, subcode: u16) -> Value {
    json!({
        "schema_version":3,
        "service":channel_documents::response(DocumentFixture::Authored,theme,page,subcode,"nld"),
        "authoring":{
            "schema_version":1,
            "template":{"id":"synthetic-grid","revision":"r1"},
            "theme":{"id":theme,"overrides":[{"key":"accent","value":{
                "kind":"colour","red":if theme == "day" {15} else {0},"green":0,
                "blue":if theme == "night" {15} else {0}
            }}]},
            "pages":[{
                "page":{"page":page,"subcode":subcode},
                "regions":[
                    {"id":"free-title","kind":"free","area":"main","row":2,"column":1,"width":20,"height":1},
                    {"id":"custom-zone","kind":"zone","area":"main","row":3,"column":1,"width":20,"height":1},
                    {"id":"guide-content","kind":"generated","area":"main","row":4,"column":1,"width":20,"height":1}
                ],
                "dynamic_fields":[{"id":"guide-title","region":"guide-content","binding":"guide.title"}]
            }]
        }
    })
}

fn two_owner_response(theme: &str) -> Value {
    let mut value = response(theme, 123, 16);
    let second = response(theme, 123, 32);
    for key in ["pages", "page_enhancements", "mot_selections"] {
        value["service"][key]
            .as_array_mut()
            .unwrap()
            .push(second["service"][key][0].clone());
    }
    let mut declaration = second["authoring"]["pages"][0].clone();
    // Different declarations for the other subpage expose accidental owner union.
    declaration["regions"][1]["id"] = json!("other-subpage-zone");
    declaration["dynamic_fields"][0]["id"] = json!("other-guide-title");
    value["authoring"]["pages"]
        .as_array_mut()
        .unwrap()
        .push(declaration);
    value
}

pub(super) async fn controller_fixture() -> super::authoring::AuthoringFixture {
    let fixture = Fixture::new_persistent().await;
    install(&fixture).await;
    std::fs::write(
        fixture.root.path().join("package/fixture.sh"),
        super::authoring::SCRIPT.replace("teletext.pages.v2", "teletext.pages.v3"),
    )
    .unwrap();
    let hash = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ContentSha256"=? WHERE "AddonId"=?"#)
        .bind(hash)
        .bind(ADDON)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    for theme in ["day", "night"] {
        write(&fixture, theme, &two_owner_response(theme));
    }
    let profiles = [
        fixture.profile("Captured day", "day", "good").await,
        fixture.profile("Captured night", "night", "good").await,
    ];
    for (channel, profile) in [1, 2].into_iter().zip(&profiles) {
        fixture.bind(channel, profile, None).await;
    }
    super::authoring::setting(
        &fixture,
        "teletext.authoring.policy",
        &super::authoring::policy().to_string(),
    )
    .await;
    super::authoring::setting(&fixture, MAX_CONCURRENT_SETTING, "1").await;
    super::authoring::setting(&fixture, "pages.language", "en-us").await;
    super::authoring::AuthoringFixture { fixture, profiles }
}

pub(super) fn assert_captured_authoring(
    preview: &TeletextAuthoringPreview,
    theme: &str,
    subcode: u16,
) {
    assert_eq!(
        preview.raster().unwrap().owner(),
        channel_documents::address(123, subcode)
    );
    let expected = two_owner_response(theme);
    let page = usize::from(subcode == 32);
    assert_eq!(preview.authoring().len(), 1);
    let declaration = &preview.authoring()[0];
    assert_eq!(declaration.producer_index, 0);
    assert_eq!(declaration.binding_index, 0);
    assert_eq!(declaration.addon_id, ADDON);
    assert_eq!(
        serde_json::to_value(&declaration.template).unwrap(),
        expected["authoring"]["template"]
    );
    assert_eq!(
        serde_json::to_value(&declaration.theme).unwrap(),
        expected["authoring"]["theme"]
    );
    assert_eq!(
        serde_json::to_value(&declaration.regions).unwrap(),
        expected["authoring"]["pages"][page]["regions"]
    );
    assert_eq!(
        serde_json::to_value(&declaration.dynamic_fields).unwrap(),
        expected["authoring"]["pages"][page]["dynamic_fields"]
    );
}

pub(super) async fn edit_profile_and_remove_package(fixture: &super::authoring::AuthoringFixture) {
    let application = &fixture.fixture.state.application;
    let profile = &fixture.profiles[0];
    let mut settings = profile.settings.clone();
    settings
        .iter_mut()
        .find(|setting| setting.key == "THEME")
        .unwrap()
        .value = TeletextProfileValue::Literal("night".to_owned());
    let installation = application
        .teletext_installation_snapshot(ADDON)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    let changed = saved(
        application
            .save_teletext_profile(&TeletextProfileSave {
                addon_id: ADDON.to_owned(),
                name: profile.name.clone(),
                settings,
                expected_installation: installation,
                existing: Some((profile.id, profile.revision)),
            })
            .await
            .unwrap(),
    )
    .profile;
    assert_eq!(changed.revision, profile.revision + 1);
    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId"=?"#)
        .bind(ADDON)
        .execute(&fixture.fixture.state.pool)
        .await
        .unwrap();
    std::fs::remove_dir_all(fixture.fixture.root.path().join("package")).unwrap();
}

pub(super) async fn selected_preview(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
    subcode: Option<u16>,
) -> TeletextAuthoringPreview {
    preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: identity.channel_id,
            design_id: identity.design_id,
            revision: identity.revision,
            owner: subcode.map(|subcode| channel_documents::address(123, subcode)),
        },
        designs::deadline(),
    )
    .await
    .unwrap()
}

pub(super) async fn assert_owner_required(fixture: &Fixture, identity: TeletextAuthoringIdentity) {
    let mut request = super::authoring::preview_request(identity);
    request.owner = None;
    let error =
        preview_original_teletext_design(&fixture.state.application, request, designs::deadline())
            .await
            .unwrap_err();
    assert_eq!(
        error.status,
        ersatzrs_web::host::teletext_authoring::TeletextAuthoringStatus::InvalidInput
    );
}

#[tokio::test]
async fn base_preview_keeps_captured_owner_boundaries_after_profile_edit_package_loss_and_reopen() {
    let mut fixture = controller_fixture().await;
    let mut originals = Vec::new();
    for (channel, theme) in [(1, "day"), (2, "night")] {
        let saved = prepare_static_teletext_design(
            &fixture.fixture.state.application,
            super::authoring::AuthoringFixture::request(channel, fixture.observe(channel).await),
            designs::deadline(),
        )
        .await
        .unwrap();
        let preview = saved.preview.unwrap();
        assert_eq!(preview.scope(), TeletextAuthoringScope::Base);
        assert_captured_authoring(&preview, theme, 16);
        let raster = preview.raster().unwrap().clone();
        drop(preview);
        assert_owner_required(&fixture.fixture, saved.identity).await;
        let initial = selected_preview(&fixture.fixture, saved.identity, Some(16)).await;
        assert_captured_authoring(&initial, theme, 16);
        assert_eq!(initial.raster(), Some(&raster));
        drop(initial);
        let second = selected_preview(&fixture.fixture, saved.identity, Some(32)).await;
        assert_captured_authoring(&second, theme, 32);
        let second_raster = second.raster().unwrap().clone();
        drop(second);
        originals.push((saved.identity, raster, second_raster));
    }
    assert_ne!(originals[0].1, originals[1].1);
    let calls = fixture.calls();
    assert_eq!(calls, b"day\nnight\n");
    let foreign = preview_original_teletext_design(
        &fixture.fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: 2,
            design_id: originals[0].0.design_id,
            revision: 1,
            owner: Some(channel_documents::address(123, 16)),
        },
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert_eq!(
        foreign.status,
        ersatzrs_web::host::teletext_authoring::TeletextAuthoringStatus::WrongChannel
    );
    edit_profile_and_remove_package(&fixture).await;
    fixture.fixture = fixture.fixture.reopen().await;
    let configuration = designs::config(&fixture.fixture).await;
    for ((identity, first, second), theme) in originals.iter().zip(["day", "night"]) {
        for (subcode, raster) in [(16, first), (32, second)] {
            let preview = selected_preview(&fixture.fixture, *identity, Some(subcode)).await;
            assert_captured_authoring(&preview, theme, subcode);
            assert_eq!(preview.raster(), Some(raster));
            assert!(!preview.publication().unwrap().can_publish);
            drop(preview);
        }
        assert!(fixture
            .fixture
            .state
            .application
            .teletext_publication_request(identity.channel_id)
            .await
            .unwrap()
            .is_none());
        assert!(fixture
            .fixture
            .state
            .application
            .accepted_teletext_generation(identity.channel_id)
            .await
            .unwrap()
            .is_none());
    }
    assert_eq!(fixture.calls(), calls);
    assert_eq!(designs::config(&fixture.fixture).await, configuration);
    assert!(fixture.fixture.playout.try_recv().is_err());
    fixture.fixture.state.pool.close().await;
}

pub(super) fn write(fixture: &Fixture, theme: &str, value: &Value) -> Vec<u8> {
    // Deliberately retain exact noncanonical whitespace in the saved original.
    let bytes = serde_json::to_vec_pretty(value).unwrap();
    write_bytes(fixture, theme, bytes)
}

pub(super) fn write_compact(fixture: &Fixture, theme: &str, value: &Value) -> Vec<u8> {
    write_bytes(fixture, theme, serde_json::to_vec(value).unwrap())
}

fn write_bytes(fixture: &Fixture, theme: &str, bytes: Vec<u8>) -> Vec<u8> {
    std::fs::write(
        fixture
            .root
            .path()
            .join(format!("authoring-response-{theme}.json")),
        &bytes,
    )
    .unwrap();
    bytes
}

pub(super) async fn install(fixture: &Fixture) {
    std::fs::write(
        fixture.root.path().join("package/fixture.sh"),
        AUTHORING_SCRIPT,
    )
    .unwrap();
    let mut current = manifest();
    current.capabilities = [
        "addon.check.v1",
        "teletext.pages.v1",
        "teletext.pages.v2",
        "teletext.pages.v3",
    ]
    .map(|id| AddonCapability {
        id: id.to_owned(),
        options: Vec::new(),
    })
    .to_vec();
    current.validate().unwrap();
    let hash = calculate_addon_content_sha256(&fixture.root.path().join("package"))
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ContentSha256"=?, "PackageSha256"=?, "ManifestJson"=? WHERE "AddonId"=?"#)
        .bind(hash).bind("a".repeat(64)).bind(serde_json::to_string(&current).unwrap()).bind(ADDON)
        .execute(&fixture.state.pool).await.unwrap();
}

pub(super) async fn candidate(
    fixture: &Fixture,
    channel: i64,
) -> EncodedChannelTeletextDocumentCandidate {
    prepare_channel_teletext_document_candidate(
        &fixture.state.application,
        channel,
        Some(ADDON),
        &guide(&[]),
        channel_documents::policy(),
    )
    .await
    .unwrap()
}

fn assert_source(candidate: &EncodedChannelTeletextDocumentCandidate, bytes: &[u8], value: &Value) {
    assert_eq!(candidate.sources().len(), 1);
    let source = &candidate.sources()[0];
    assert_eq!(source.contract(), TeletextPageContract::V3);
    assert_eq!(source.generation(), GENERATION);
    assert_eq!(source.bytes(), bytes);
    assert_eq!(
        serde_json::to_value(source.authoring().unwrap()).unwrap(),
        value["authoring"]
    );
    assert_eq!(
        candidate.selection().bindings[0].contract,
        TeletextPageContract::V3
    );
    assert_eq!(
        candidate
            .requests()
            .request(TeletextPageContract::V3)
            .unwrap(),
        ersatzrs_app::teletext_guide::build_request_for_schema(&guide(&[]), 3).unwrap()
    );
}

type OriginalTransport = (Vec<Vec<[u8; 46]>>, Vec<[u8; 46]>);

fn transport(service: &EncodedTeletextAuthoredService) -> OriginalTransport {
    (designs::units(service), service.magazine_units().to_vec())
}

fn assert_theme(candidate: &EncodedChannelTeletextDocumentCandidate, rgb: [u8; 3]) {
    let preview = generations::preview(candidate.encoded().unwrap());
    assert!(preview.raster().diagnostics().is_empty());
    // The same original Level 2.5 colour-18 G0 cell uses each profile's CLUT2.
    assert!((144..156).any(|x| (10..20).any(|y| matches!(
        preview.raster().pixel(x, y),
        Some(TeletextRasterPixel::Colour(TeletextColourPixel::Rgb(colour)))
            if colour.get() == rgb
    ))));
}

async fn assert_reopened_original(
    fixture: &Fixture,
    saved: &SavedOriginalTeletextDesign,
    bytes: &[u8],
    value: &Value,
    original: &OriginalTransport,
) {
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        saved.head().id,
        1,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        restored.edit_policy(),
        TeletextSavedEditPolicy::OriginalOnly
    );
    assert_eq!(restored.sources()[0].contract(), TeletextPageContract::V3);
    assert_eq!(restored.sources()[0].bytes(), bytes);
    assert_eq!(
        serde_json::to_value(restored.sources()[0].authoring().unwrap()).unwrap(),
        value["authoring"]
    );
    assert_eq!(&transport(restored.encoded().unwrap()), original);
    assert_eq!(
        restored
            .guide_inputs()
            .unwrap()
            .request(TeletextPageContract::V3),
        saved
            .candidate()
            .requests()
            .request(TeletextPageContract::V3)
    );
}

#[tokio::test]
async fn highest_v3_captures_two_profile_themes_and_reopens_originals_without_edit_authority() {
    let mut fixture = Fixture::new_persistent().await;
    install(&fixture).await;
    fixture
        .state
        .application
        .set_admin_setting(SetAdminSetting {
            key: MAX_CONCURRENT_SETTING,
            value: "2".to_owned(),
        })
        .await
        .unwrap();
    let values = [response("day", 124, 32), response("night", 124, 32)];
    let bytes = [
        write(&fixture, "day", &values[0]),
        write(&fixture, "night", &values[1]),
    ];
    let day = fixture.profile("Authoring day", "day", "good").await;
    let night = fixture.profile("Authoring night", "night", "good").await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    let configuration = designs::config(&fixture).await;
    let (first, second) = tokio::join!(candidate(&fixture, 1), candidate(&fixture, 2));
    for ((candidate, bytes), value) in [&first, &second].into_iter().zip(&bytes).zip(&values) {
        assert_source(candidate, bytes, value);
    }
    assert_eq!(first.selection().channel_id, 1);
    assert_eq!(second.selection().channel_id, 2);
    assert_eq!(first.selection().bindings[0].profile_id, day.id);
    assert_eq!(second.selection().bindings[0].profile_id, night.id);
    assert_theme(&first, [15, 0, 0]);
    assert_theme(&second, [0, 0, 15]);
    let originals = [
        transport(first.encoded().unwrap()),
        transport(second.encoded().unwrap()),
    ];
    assert_ne!(originals[0], originals[1]);
    let saved = [
        designs::save(&fixture, first, None).await,
        designs::save(&fixture, second, None).await,
    ];
    let mut calls = std::fs::read_to_string(fixture.root.path().join("authoring-calls"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    calls.sort();
    assert_eq!(calls, ["day:teletext.pages.v3", "night:teletext.pages.v3"]);
    for theme in ["day", "night"] {
        assert_eq!(
            std::fs::read(
                fixture
                    .root
                    .path()
                    .join(format!("authoring-input-{theme}.json"))
            )
            .unwrap(),
            ersatzrs_app::teletext_guide::build_request_for_schema(&guide(&[]), 3).unwrap()
        );
    }
    fixture = fixture.reopen().await;
    for (index, saved) in saved.iter().enumerate() {
        assert_reopened_original(
            &fixture,
            saved,
            &bytes[index],
            &values[index],
            &originals[index],
        )
        .await;
    }
    let mut retained_calls = std::fs::read_to_string(fixture.root.path().join("authoring-calls"))
        .unwrap()
        .lines()
        .map(str::to_owned)
        .collect::<Vec<_>>();
    retained_calls.sort();
    assert_eq!(retained_calls, calls);
    for channel in [1, 2] {
        assert!(fixture
            .state
            .application
            .teletext_publication_request(channel)
            .await
            .unwrap()
            .is_none());
        assert!(fixture
            .state
            .application
            .accepted_teletext_generation(channel)
            .await
            .unwrap()
            .is_none());
    }
    assert_eq!(designs::config(&fixture).await, configuration);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}

fn malformed_metadata(value: &Value) -> Vec<Value> {
    let mut overlap = value.clone();
    overlap["authoring"]["pages"][0]["regions"][1]["row"] = json!(2);
    let mut foreign_page = value.clone();
    foreign_page["authoring"]["pages"][0]["page"]["page"] = json!(125);
    // Wire-local side panels can span 16 columns; the selected original
    // presentation enables a four-column left panel, which the app must enforce.
    let mut excess_panel = value.clone();
    excess_panel["authoring"]["pages"][0]["regions"][0]["area"] = json!("left_panel");
    excess_panel["authoring"]["pages"][0]["regions"][0]["column"] = json!(0);
    excess_panel["authoring"]["pages"][0]["regions"][0]["width"] = json!(16);
    let mut authority = value.clone();
    authority["authoring"]["publish"] = json!(true);
    vec![overlap, foreign_page, excess_panel, authority]
}

async fn assert_metadata_rejected(fixture: &Fixture) {
    let rejected = prepare_channel_teletext_document_candidate(
        &fixture.state.application,
        1,
        Some(ADDON),
        &guide(&[]),
        channel_documents::policy(),
    )
    .await;
    let Err(ChannelTeletextDocumentEncodingError::Preparation(
        ChannelTeletextDocumentError::Document { binding: 0, error },
    )) = rejected
    else {
        panic!("expected selected V3 document rejection");
    };
    assert!(matches!(
        *error,
        BoundTeletextDocumentError::AuthoredMetadataResponse(_)
    ));
}

#[tokio::test]
async fn invalid_highest_v3_metadata_never_downgrades_or_changes_pending_and_accepted_originals() {
    let fixture = Fixture::new_persistent().await;
    install(&fixture).await;
    let value = response("day", 124, 32);
    write(&fixture, "day", &value);
    let profile = fixture.profile("Authoring original", "day", "good").await;
    fixture.bind(1, &profile, None).await;
    let prepared = candidate(&fixture, 1).await;
    let original_units = designs::units(prepared.encoded().unwrap());
    let saved = designs::save(&fixture, prepared, None).await;
    let pending = generations::request(&fixture, &saved, None).await;
    let accepted = generations::accepted(generations::accept(&fixture, &pending, 1, None).await);
    let configuration = designs::config(&fixture).await;
    let binding = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    let original = fixture
        .state
        .application
        .teletext_design_revision(saved.head().id, 1, designs::budget().max_serialized_bytes)
        .await
        .unwrap()
        .unwrap();
    let mut calls = 1;
    for changed in malformed_metadata(&value) {
        write(&fixture, "day", &changed);
        assert_metadata_rejected(&fixture).await;
        calls += 1;
        assert_eq!(
            std::fs::read_to_string(fixture.root.path().join("authoring-calls")).unwrap(),
            "day:teletext.pages.v3\n".repeat(calls)
        );
        assert_eq!(
            fixture
                .state
                .application
                .teletext_publication_request(1)
                .await
                .unwrap(),
            Some(pending.clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .accepted_teletext_generation(1)
                .await
                .unwrap(),
            Some(accepted.generation().clone())
        );
        assert_eq!(
            fixture
                .state
                .application
                .channel_teletext_bindings(1)
                .await
                .unwrap(),
            binding
        );
        assert_eq!(designs::config(&fixture).await, configuration);
        assert_eq!(
            fixture
                .state
                .application
                .teletext_design_revision(
                    saved.head().id,
                    1,
                    designs::budget().max_serialized_bytes
                )
                .await
                .unwrap()
                .unwrap(),
            original
        );
        let recovered = generations::recover(&fixture).await.unwrap();
        assert_eq!(recovered.generation(), accepted.generation());
        assert_eq!(designs::units(recovered.service()), original_units);
    }
    assert!(fixture.playout.is_empty());
    fixture.state.pool.close().await;
}

#[tokio::test]
async fn mixed_v1_v3_composition_retains_metadata_only_with_its_original_producer() {
    let fixture = Fixture::new_persistent().await;
    let first = channel_documents::response(DocumentFixture::Base, "day", 123, 16, "nld");
    let second = response("night", 124, 32);
    install_secondary(&fixture).await;
    // Existing fixture package pins are synthetic; durable originals need full hashes.
    for (addon, hash) in [(ADDON, "a".repeat(64)), (SECONDARY, "b".repeat(64))] {
        sqlx::query(r#"UPDATE "AddonInstallation" SET "PackageSha256"=? WHERE "AddonId"=?"#)
            .bind(hash)
            .bind(addon)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
    // Capture profile installation pins only after durable package identities
    // and selected capabilities are final, matching the production save fence.
    for (addon, label, schema, value) in [
        (ADDON, "first", 1, &first),
        (SECONDARY, "second", 3, &second),
    ] {
        channel_documents::install_contract(&fixture, addon, label, schema).await;
        channel_documents::write_response(&fixture, label, value);
        let profile = fixture.profile_for(addon, label, "day", "good").await;
        fixture.bind(1, &profile, None).await;
    }
    let prepared = channel_documents::candidate(&fixture, channel_documents::policy())
        .await
        .unwrap();
    channel_documents::assert_requests(&fixture, 1, 3);
    assert_eq!(prepared.sources()[0].contract(), TeletextPageContract::V1);
    assert!(prepared.sources()[0].authoring().is_none());
    assert_eq!(prepared.sources()[1].contract(), TeletextPageContract::V3);
    assert_eq!(
        serde_json::to_value(prepared.sources()[1].authoring().unwrap()).unwrap(),
        second["authoring"]
    );
    assert_eq!(
        prepared.sources()[1].bytes(),
        serde_json::to_vec(&second).unwrap()
    );
    assert_eq!(prepared.producers().len(), 2);
    let saved = designs::save(&fixture, prepared, None).await;
    assert_eq!(saved.head().revision, 1);
    super::authoring::setting(
        &fixture,
        "teletext.authoring.policy",
        &super::authoring::policy().to_string(),
    )
    .await;
    for (owner, has_metadata) in [
        (channel_documents::address(123, 16), false),
        (channel_documents::address(124, 32), true),
    ] {
        let preview = preview_original_teletext_design(
            &fixture.state.application,
            TeletextAuthoringPreviewRequest {
                channel_id: 1,
                design_id: saved.head().id,
                revision: 1,
                owner: Some(owner),
            },
            designs::deadline(),
        )
        .await
        .unwrap();
        assert_eq!(preview.authoring().is_empty(), !has_metadata);
        if has_metadata {
            let declaration = &preview.authoring()[0];
            assert_eq!(declaration.producer_index, 1);
            assert_eq!(declaration.binding_index, 1);
            assert_eq!(declaration.addon_id, SECONDARY);
            assert_eq!(
                serde_json::to_value(&declaration.regions).unwrap(),
                second["authoring"]["pages"][0]["regions"]
            );
        }
        drop(preview);
    }
    assert!(fixture
        .state
        .application
        .teletext_publication_request(1)
        .await
        .unwrap()
        .is_none());
    fixture.state.pool.close().await;
}
