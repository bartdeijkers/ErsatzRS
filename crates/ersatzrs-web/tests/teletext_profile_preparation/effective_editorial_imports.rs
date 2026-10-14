//! Actual Lua/installed-V3 scoped editor backend flow, without GUI/editor acceptance.
//! The parent intentionally restricts POSIX synthetic package fixtures to Unix.

use super::{
    authoring::AuthoringFixture, authoring_metadata, channel_documents, designs, editorial_imports,
    effective_authoring, scripted_controls, Fixture, ADDON,
};
use chrono::{DateTime, Timelike as _, Utc};
use ersatzrs_app::{
    teletext_design_documents::decode_teletext_effective_design_documents,
    teletext_export_project::{write_teletext_export_sidecar, TeletextExportEffectiveOrigin},
};
use ersatzrs_core::repositories::{TeletextDesignMutation, TeletextProfileSave};
use ersatzrs_domain::{TeletextDesignRevision, TeletextPageContract};
use ersatzrs_ffmpeg::teletext::TeletextRaster;
use ersatzrs_infra::addons::calculate_addon_content_sha256;
use ersatzrs_web::host::{
    teletext_authoring::{
        preview_original_teletext_design, publish_static_teletext_generation,
        request_static_teletext_publication, TeletextAuthoringIdentity,
        TeletextAuthoringPreviewRequest, TeletextAuthoringPublicationTarget,
        TeletextAuthoringPublishTarget,
    },
    teletext_design_exports::{
        export_original_teletext_design, ExportedOriginalTeletextDesign,
        NativeTeletextExportRequest,
    },
    teletext_editorial_designs::{
        import_teletext_editorial_design, NativeTeletextEditorialImportRequest,
        SavedEditorialTeletextDesign,
    },
    teletext_effective_designs::reload_effective_original_teletext_design_revision,
};
use serde_json::{json, Value};

#[path = "effective_editorial_imports/live_client.rs"]
mod live_client;
#[path = "effective_editorial_imports/live_updates.rs"]
mod live_updates;
#[path = "effective_editorial_imports/natural_expiry.rs"]
mod natural_expiry;

async fn v3_fixture() -> AuthoringFixture {
    let mut fixture = AuthoringFixture::new().await;
    let path = fixture.fixture.root.path().join("package/fixture.sh");
    // Reuse the original fixture's actual generation correlation, then install
    // the existing V3 metadata fixture. No Lua/package activation API is added.
    let script = std::fs::read_to_string(&path)
        .unwrap()
        .replace("teletext.pages.v2", "teletext.pages.v3");
    authoring_metadata::install(&fixture.fixture).await;
    std::fs::write(path, script).unwrap();
    let hash = calculate_addon_content_sha256(&fixture.fixture.root.path().join("package"))
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ContentSha256"=? WHERE "AddonId"=?"#)
        .bind(hash)
        .bind(ADDON)
        .execute(&fixture.fixture.state.pool)
        .await
        .unwrap();
    for theme in ["day", "night"] {
        authoring_metadata::write(
            &fixture.fixture,
            theme,
            &authoring_metadata::response(theme, 123, 16),
        );
    }
    refresh_profiles_and_bindings(&mut fixture).await;
    effective_authoring::base_selected(&fixture).await;
    for channel in [1, 2] {
        let binding = fixture
            .fixture
            .state
            .application
            .channel_teletext_bindings(channel)
            .await
            .unwrap()
            .into_iter()
            .find(|read| read.binding.addon_id == ADDON)
            .unwrap()
            .binding;
        assert!(
            !super::saved(
                fixture
                    .fixture
                    .state
                    .application
                    .disable_channel_teletext_binding(channel, ADDON, binding.revision,)
                    .await
                    .unwrap()
            )
            .enabled
        );
    }
    scripted_controls::seed_publication_inputs(&fixture.fixture).await;
    fixture
}

async fn refresh_profiles_and_bindings(fixture: &mut AuthoringFixture) {
    // Package upgrades intentionally invalidate the old exact profile token.
    // Revalidate through the application before using the upgraded capability;
    // no production scope or health fence is bypassed by the fixture.
    let application = &fixture.fixture.state.application;
    let installation = application
        .teletext_installation_snapshot(ADDON)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    for (index, channel) in [1, 2].into_iter().enumerate() {
        let profile = &fixture.profiles[index];
        let profile = super::saved(
            application
                .save_teletext_profile(&TeletextProfileSave {
                    addon_id: profile.addon_id.clone(),
                    name: profile.name.clone(),
                    settings: profile.settings.clone(),
                    expected_installation: installation.clone(),
                    existing: Some((profile.id, profile.revision)),
                })
                .await
                .unwrap(),
        )
        .profile;
        // Profile revalidation also invalidates dependent binding revisions.
        let binding = application
            .channel_teletext_bindings(channel)
            .await
            .unwrap()
            .into_iter()
            .find(|read| read.binding.addon_id == ADDON)
            .unwrap()
            .binding;
        fixture
            .fixture
            .bind(channel, &profile, Some(binding.revision))
            .await;
        fixture.profiles[index] = profile;
    }
}

pub(super) async fn original(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
) -> TeletextDesignRevision {
    fixture
        .state
        .application
        .teletext_design_revision(
            identity.design_id,
            identity.revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap()
}

pub(super) async fn export(
    fixture: &Fixture,
    original: &TeletextDesignRevision,
) -> ExportedOriginalTeletextDesign {
    let TeletextDesignMutation::Saved(exported) = export_original_teletext_design(
        &fixture.state.application,
        NativeTeletextExportRequest {
            design_id: original.design_id,
            revision: original.revision,
            design_digest: original.digest.clone(),
            expected_design_head: original.revision,
            pages: None,
        },
        editorial_imports::policy(),
        designs::deadline(),
    )
    .await
    .unwrap() else {
        panic!("actual scoped original must export")
    };
    exported
}

pub(super) struct Flow {
    pub(super) fixture: AuthoringFixture,
    pub(super) identities: [TeletextAuthoringIdentity; 2],
    pub(super) originals: [TeletextDesignRevision; 2],
    pub(super) exported: [ExportedOriginalTeletextDesign; 2],
    pub(super) pixels: [TeletextRaster; 2],
}
pub(super) async fn setup() -> Flow {
    let fixture = v3_fixture().await;
    for (channel, profile) in [1, 2].into_iter().zip(&fixture.profiles) {
        effective_authoring::lua(&fixture, channel, Some(profile)).await;
    }
    let (first, day, observed) = effective_authoring::prepare(&fixture, 1).await;
    effective_authoring::select(&fixture, first, &observed, false).await;
    let (second, night, observed) = effective_authoring::prepare(&fixture, 2).await;
    effective_authoring::select(&fixture, second, &observed, false).await;
    let originals = [
        original(&fixture.fixture, first).await,
        original(&fixture.fixture, second).await,
    ];
    let exported = [
        export(&fixture.fixture, &originals[0]).await,
        export(&fixture.fixture, &originals[1]).await,
    ];
    Flow {
        fixture,
        identities: [first, second],
        originals,
        exported,
        pixels: [day.unwrap(), night.unwrap()],
    }
}

pub(super) async fn stable_state(fixture: &Fixture) -> Vec<Vec<String>> {
    let mut result = Vec::new();
    for statement in [
        r#"SELECT json_array("ChannelId","RequestId") FROM "TeletextPublicationRequestCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextAcceptedGenerationCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","RequestId") FROM "TeletextEffectivePublicationRequestCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("ChannelId","GenerationId") FROM "TeletextEffectiveAcceptedGenerationCurrent" ORDER BY "ChannelId""#,
        r#"SELECT json_array("Id","ChannelId","DesignId","Revision","Digest") FROM "TeletextPublicationRequest" ORDER BY "Id""#,
        r#"SELECT json_array("Id","ChannelId","RequestId","ArtifactVersion","ArtifactDigest") FROM "TeletextAcceptedGeneration" ORDER BY "Id""#,
        r#"SELECT json_array("ChannelId","AddonId","ProfileId","Enabled","Revision") FROM "ChannelTeletextBinding" ORDER BY "ChannelId","AddonId""#,
        r#"SELECT json_array("AddonId","Enabled","StateRevision","PackageSha256","ContentSha256","ManifestJson") FROM "AddonInstallation" ORDER BY "AddonId""#,
        r#"SELECT json_array("PlayoutId","AddonId","Start","Finish","State","CandidateSnapshot") FROM "PlayoutTeletextInterval" ORDER BY "PlayoutId","AddonId","Start""#,
        r#"SELECT json_array("PlayoutId","CheckpointDate","Context") FROM "PlayoutScriptedCheckpoint" ORDER BY "PlayoutId","CheckpointDate""#,
        r#"SELECT json_array("Id","PlayoutId","MediaItemId","Start","Finish") FROM "PlayoutItem" ORDER BY "Id""#,
    ] {
        result.push(
            sqlx::query_scalar(statement)
                .fetch_all(&fixture.state.pool)
                .await
                .unwrap(),
        );
    }
    result
}

fn edit(
    exported: &ExportedOriginalTeletextDesign,
    head: i64,
    row: u8,
) -> NativeTeletextEditorialImportRequest {
    let mut files = exported.files().to_vec();
    editorial_imports::edit_cell(
        &mut files,
        &exported.sidecar().project.pages[0].name,
        row,
        8,
        b'Q',
    );
    NativeTeletextEditorialImportRequest {
        name: "Scoped editor concept".to_owned(),
        receipt_id: exported.metadata().id,
        receipt_digest: exported.metadata().digest.clone(),
        expected_design_head: head,
        sidecar: write_teletext_export_sidecar(
            exported.sidecar(),
            editorial_imports::policy().project,
        )
        .unwrap(),
        files,
    }
}

pub(super) async fn import(
    fixture: &Fixture,
    request: NativeTeletextEditorialImportRequest,
) -> SavedEditorialTeletextDesign {
    let TeletextDesignMutation::Saved(concept) = import_teletext_editorial_design(
        &fixture.state.application,
        request,
        editorial_imports::policy(),
        designs::deadline(),
    )
    .await
    .unwrap() else {
        panic!("permitted scoped edit must save a concept")
    };
    concept
}

fn assert_project(
    exported: &ExportedOriginalTeletextDesign,
    original: &TeletextDesignRevision,
    theme: &str,
) {
    let project = &exported.sidecar().project;
    assert_eq!(project.schema_version, 2);
    assert_eq!(project.original.document_version, 2);
    assert_eq!(project.original.digest, original.digest);
    assert_eq!(project.producers.len(), 1);
    let producer = &project.producers[0];
    assert_eq!(producer.addon_id, ADDON);
    assert!(producer.binding_revision.is_none());
    assert_eq!(producer.authoring.as_ref().unwrap().theme.id, theme);
    let TeletextExportEffectiveOrigin::Scripted {
        binding_index,
        playout_id,
        start,
        finish,
    } = producer.effective_origin.as_ref().unwrap()
    else {
        panic!("actual Lua origin must remain Scripted")
    };
    assert_eq!(*binding_index, 0);
    assert_eq!(*playout_id, 92 + project.channel_id);
    let original =
        decode_teletext_effective_design_documents(&original.document, designs::budget()).unwrap();
    assert_eq!(
        project.effective_scope.as_ref().unwrap().binding_count,
        original.selection().bindings.len()
    );
    assert_eq!(
        project
            .effective_scope
            .as_ref()
            .unwrap()
            .valid_until
            .as_ref()
            .unwrap()
            .parse::<DateTime<Utc>>()
            .unwrap(),
        original.selection().valid_until.unwrap()
    );
    let ersatzrs_domain::TeletextEffectiveBindingSnapshot::Scripted(published) =
        &original.selection().bindings[0]
    else {
        panic!("stored Scripted origin")
    };
    assert_eq!(
        start.parse::<DateTime<Utc>>().unwrap(),
        published.interval.start
    );
    assert_eq!(
        finish.parse::<DateTime<Utc>>().unwrap(),
        published.interval.finish
    );
}

pub(super) async fn assert_concept(
    fixture: &Fixture,
    original: &TeletextDesignRevision,
    concept: &SavedEditorialTeletextDesign,
) -> TeletextRaster {
    let stored = fixture
        .state
        .application
        .teletext_design_revision(
            concept.head().id,
            concept.head().revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(stored.document_version, 2);
    assert_eq!(stored.design_id, original.design_id);
    let before =
        decode_teletext_effective_design_documents(&original.document, designs::budget()).unwrap();
    let after =
        decode_teletext_effective_design_documents(&stored.document, designs::budget()).unwrap();
    assert_eq!(before.selection(), after.selection());
    assert_eq!(before.producer_bindings(), after.producer_bindings());
    assert_eq!(before.entry(), after.entry());
    assert_eq!(
        before.resource_transmissions(),
        after.resource_transmissions()
    );
    assert_eq!(before.guide_source(), after.guide_source());
    assert_eq!(before.sources()[0].bytes(), after.sources()[0].bytes());
    assert_eq!(
        before.sources()[0].authoring(),
        after.sources()[0].authoring()
    );
    assert_eq!(
        before
            .guide_inputs()
            .unwrap()
            .request(TeletextPageContract::V3),
        after
            .guide_inputs()
            .unwrap()
            .request(TeletextPageContract::V3)
    );
    assert!(after.editorial_overlay().is_some());
    let restored = reload_effective_original_teletext_design_revision(
        &fixture.state.application,
        concept.head().id,
        concept.head().revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(restored.selection(), before.selection());
    assert_eq!(restored.producer_bindings(), before.producer_bindings());
    drop(restored);
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: before.selection().channel_id,
            design_id: concept.head().id,
            revision: concept.head().revision,
            owner: before.entry(),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(preview.valid_until(), before.selection().valid_until);
    preview.raster().unwrap().clone()
}

#[tokio::test]
async fn actual_lua_scoped_v3_editor_free_and_custom_zone_concepts_preserve_isolation_and_pointers()
{
    let flow = setup().await;
    assert_project(&flow.exported[0], &flow.originals[0], "day");
    assert_project(&flow.exported[1], &flow.originals[1], "night");
    assert_ne!(flow.pixels[0], flow.pixels[1]);
    assert_ne!(
        flow.exported[0].sidecar().project.producers[0].profile_id,
        flow.exported[1].sidecar().project.producers[0].profile_id
    );
    let before = stable_state(&flow.fixture.fixture).await;
    let calls = flow.fixture.calls();
    let configuration = designs::config(&flow.fixture.fixture).await;
    let free = import(&flow.fixture.fixture, edit(&flow.exported[0], 1, 2)).await;
    let free_pixels = assert_concept(&flow.fixture.fixture, &flow.originals[0], &free).await;
    assert_ne!(free_pixels, flow.pixels[0]);
    let zone = import(&flow.fixture.fixture, edit(&flow.exported[0], 2, 3)).await;
    let zone_pixels = assert_concept(&flow.fixture.fixture, &flow.originals[0], &zone).await;
    assert_ne!(zone_pixels, flow.pixels[0]);
    assert_ne!(zone_pixels, free_pixels);
    assert_eq!(stable_state(&flow.fixture.fixture).await, before);
    assert_eq!(flow.fixture.calls(), calls);
    assert_eq!(designs::config(&flow.fixture.fixture).await, configuration);
    for index in 0..2 {
        assert_eq!(
            original(&flow.fixture.fixture, flow.identities[index]).await,
            flow.originals[index]
        );
    }
    let other = effective_authoring::preview(&flow.fixture, flow.identities[1], false).await;
    assert_eq!(other.raster(), Some(&flow.pixels[1]));
    drop(other);
    explicit_select_concept(&flow.fixture, &zone).await;
}

pub(super) async fn explicit_select_concept(
    fixture: &AuthoringFixture,
    concept: &SavedEditorialTeletextDesign,
) {
    let identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: concept.head().id,
        revision: concept.head().revision,
    };
    let preview = effective_authoring::preview(fixture, identity, false).await;
    let observed = preview.publication().unwrap().clone();
    drop(preview);
    let accepted = fixture
        .fixture
        .state
        .application
        .effective_teletext_accepted_generation(1)
        .await
        .unwrap()
        .unwrap();
    let pending = request_static_teletext_publication(
        &fixture.fixture.state.application,
        effective_authoring::target(identity, &observed),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(
        fixture
            .fixture
            .state
            .application
            .effective_teletext_accepted_generation(1)
            .await
            .unwrap(),
        Some(accepted.clone())
    );
    let mut target = effective_authoring::target(identity, &observed);
    target.expected_request_id = Some(pending.request_id);
    let selected = publish_static_teletext_generation(
        &fixture.fixture.state.application,
        TeletextAuthoringPublishTarget {
            publication: target,
            expected_generation_id: Some(accepted.id),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_ne!(selected.generation_id, accepted.id);
}

#[derive(Clone, Copy)]
enum Tamper {
    Role,
    Origin,
    Expiry,
    Theme,
    Template,
    Zone,
    Generated,
    Resource,
}
fn tampered(
    exported: &ExportedOriginalTeletextDesign,
    kind: Tamper,
) -> NativeTeletextEditorialImportRequest {
    let mut request = edit(exported, 1, 2);
    let mut sidecar: Value = serde_json::from_slice(&request.sidecar).unwrap();
    match kind {
        Tamper::Role => sidecar["project"]["original"]["document_version"] = json!(1),
        Tamper::Origin => {
            sidecar["project"]["producers"][0]["effective_origin"]["playout_id"] = json!(999);
        }
        Tamper::Expiry => {
            sidecar["project"]["effective_scope"]["valid_until"] = json!("2030-01-01T00:00:00Z");
        }
        Tamper::Theme => {
            sidecar["project"]["producers"][0]["authoring"]["theme"]["id"] = json!("forged");
        }
        Tamper::Template => {
            sidecar["project"]["producers"][0]["authoring"]["template"]["revision"] =
                json!("forged");
        }
        Tamper::Zone => {
            sidecar["project"]["producers"][0]["authoring"]["pages"][0]["regions"][0]["width"] =
                json!(30);
        }
        Tamper::Generated => editorial_imports::edit_cell(
            &mut request.files,
            &exported.sidecar().project.pages[0].name,
            4,
            8,
            b'Q',
        ),
        Tamper::Resource => {
            let name = &exported.sidecar().project.assets[0].name;
            request
                .files
                .iter_mut()
                .find(|file| &file.name == name)
                .unwrap()
                .bytes[0] ^= 1;
        }
    }
    request.sidecar = serde_json::to_vec(&sidecar).unwrap();
    request
}

#[tokio::test]
async fn scoped_editor_role_origin_and_protected_content_tampering_never_mutate_state() {
    let flow = setup().await;
    let before = stable_state(&flow.fixture.fixture).await;
    let count = flow.fixture.design_count().await;
    let calls = flow.fixture.calls();
    for kind in [
        Tamper::Role,
        Tamper::Origin,
        Tamper::Expiry,
        Tamper::Theme,
        Tamper::Template,
        Tamper::Zone,
        Tamper::Generated,
        Tamper::Resource,
    ] {
        assert!(import_teletext_editorial_design(
            &flow.fixture.fixture.state.application,
            tampered(&flow.exported[0], kind),
            editorial_imports::policy(),
            designs::deadline(),
        )
        .await
        .is_err());
        assert_eq!(flow.fixture.design_count().await, count);
        assert_eq!(stable_state(&flow.fixture.fixture).await, before);
        assert_eq!(flow.fixture.calls(), calls);
    }
}

#[tokio::test]
async fn actual_lua_off_has_no_fabricated_editor_export() {
    let fixture = v3_fixture().await;
    effective_authoring::lua(&fixture, 1, None).await;
    let (identity, pixels, _) = effective_authoring::prepare(&fixture, 1).await;
    assert!(pixels.is_none());
    let stored = original(&fixture.fixture, identity).await;
    let before = stable_state(&fixture.fixture).await;
    let calls = fixture.calls();
    assert!(export_original_teletext_design(
        &fixture.fixture.state.application,
        NativeTeletextExportRequest {
            design_id: identity.design_id,
            revision: identity.revision,
            design_digest: stored.digest,
            expected_design_head: identity.revision,
            pages: None,
        },
        editorial_imports::policy(),
        designs::deadline()
    )
    .await
    .is_err());
    assert_eq!(stable_state(&fixture.fixture).await, before);
    assert_eq!(fixture.calls(), calls);
}

async fn short_lua(fixture: &AuthoringFixture) -> DateTime<Utc> {
    let script = format!("assert(teletext_profile('{ADDON}',{}).ok)\nassert(teletext_enabled('{ADDON}',true).ok)\nadd_duration('clip','00:00:12',{{trim=true,stop_before_end=true}})", fixture.profiles[0].id);
    let result = scripted_controls::build_at(
        &fixture.fixture,
        &script,
        1,
        true,
        (Utc::now() - chrono::Duration::seconds(1))
            .with_nanosecond(0)
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(result.teletext_intervals.len(), 1);
    assert_eq!(
        result.teletext_intervals[0].finish,
        result.added_items.last().unwrap().finish
    );
    assert_eq!(
        result.added_items[0].finish - result.added_items[0].start,
        chrono::Duration::seconds(12)
    );
    scripted_controls::publish_result(&fixture.fixture, &result).await;
    result.teletext_intervals[0].finish
}

#[tokio::test]
async fn naturally_expired_lua_scope_exports_imports_and_previews_without_live_publication() {
    let fixture = v3_fixture().await;
    let finish = short_lua(&fixture).await;
    let (identity, pixels, observed) = effective_authoring::prepare(&fixture, 1).await;
    let pending = request_static_teletext_publication(
        &fixture.fixture.state.application,
        effective_authoring::target(identity, &observed),
        designs::deadline(),
    )
    .await
    .unwrap();
    let stored = original(&fixture.fixture, identity).await;
    tokio::time::sleep(
        (finish - Utc::now()).to_std().unwrap_or_default() + std::time::Duration::from_millis(20),
    )
    .await;
    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId"=?"#)
        .bind(ADDON)
        .execute(&fixture.fixture.state.pool)
        .await
        .unwrap();
    let before = stable_state(&fixture.fixture).await;
    let calls = fixture.calls();
    let exported = export(&fixture.fixture, &stored).await;
    let concept = import(&fixture.fixture, edit(&exported, 1, 2)).await;
    let edited = assert_concept(&fixture.fixture, &stored, &concept).await;
    assert_ne!(Some(edited), pixels);
    let identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: concept.head().id,
        revision: concept.head().revision,
    };
    let preview = effective_authoring::preview(&fixture, identity, false).await;
    assert_eq!(preview.valid_until(), Some(finish));
    assert!(
        !preview.publication().unwrap().can_request && !preview.publication().unwrap().can_publish
    );
    drop(preview);
    let target = TeletextAuthoringPublicationTarget {
        identity,
        digest: concept.digest().to_owned(),
        expected_design_head: concept.head().revision,
        expected_request_id: Some(pending.request_id),
    };
    assert!(request_static_teletext_publication(
        &fixture.fixture.state.application,
        target.clone(),
        designs::deadline()
    )
    .await
    .is_err());
    assert!(publish_static_teletext_generation(
        &fixture.fixture.state.application,
        TeletextAuthoringPublishTarget {
            publication: target,
            expected_generation_id: None
        },
        designs::deadline(),
    )
    .await
    .is_err());
    assert_eq!(stable_state(&fixture.fixture).await, before);
    assert_eq!(fixture.calls(), calls);
}
