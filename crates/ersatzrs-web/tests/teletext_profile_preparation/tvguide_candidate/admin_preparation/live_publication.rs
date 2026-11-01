//! Exact production originals through explicit source-aware admin publication.
//! Native accepted roles are not ordinary-stream, transport or client evidence.

use super::*;
use ersatzrs_core::repositories::{
    TeletextDesignMutation, TeletextDesignRepository as _, TeletextDesignSave,
};
use ersatzrs_domain::{TeletextAcceptedGeneration, TELETEXT_DESIGN_DOCUMENT_VERSION};
use ersatzrs_infra::repositories::SqliteTeletextDesignRepository;
use ersatzrs_web::host::teletext_authoring::{
    publish_teletext_generation, request_teletext_publication,
    TeletextAuthoringPublicationObservation, TeletextAuthoringPublicationTarget,
    TeletextAuthoringPublishTarget,
};

const REQUEST: &str = "/admin-api/teletext/design/request-publication";
const PUBLISH: &str = "/admin-api/teletext/design/publish";

#[path = "live_publication/package_availability.rs"]
mod package_availability;

#[derive(Debug, PartialEq, Eq)]
struct Original {
    document: Vec<u8>,
    units: Vec<Vec<[u8; 46]>>,
    raster: TeletextRaster,
}

async fn original(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
    scope: TeletextAuthoringScope,
    index: u16,
) -> Original {
    let revision = fixture
        .state
        .application
        .teletext_design_revision(
            identity.design_id,
            identity.revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    let units = match scope {
        TeletextAuthoringScope::Base => {
            let restored = reload_original_teletext_design_revision(
                &fixture.state.application,
                identity.design_id,
                identity.revision,
                designs::budget(),
                channel_documents::policy(),
                designs::deadline(),
            )
            .await
            .unwrap()
            .unwrap();
            designs::units(restored.encoded().unwrap())
        }
        TeletextAuthoringScope::Effective => {
            let restored = reload_effective_original_teletext_design_revision(
                &fixture.state.application,
                identity.design_id,
                identity.revision,
                designs::budget(),
                channel_documents::policy(),
                designs::deadline(),
            )
            .await
            .unwrap()
            .unwrap();
            designs::units(restored.encoded().unwrap())
        }
    };
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: identity.channel_id,
            design_id: identity.design_id,
            revision: identity.revision,
            owner: Some(channel_documents::address(index, 0)),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(preview.raster().unwrap().diagnostics().is_empty());
    Original {
        document: revision.document,
        units,
        raster: preview.raster().unwrap().clone(),
    }
}

async fn observe(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
    index: u16,
) -> TeletextAuthoringPublicationObservation {
    let preview = preview_original_teletext_design(
        &fixture.state.application,
        TeletextAuthoringPreviewRequest {
            channel_id: identity.channel_id,
            design_id: identity.design_id,
            revision: identity.revision,
            owner: Some(channel_documents::address(index, 0)),
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    preview.publication().unwrap().clone()
}

fn target(
    identity: TeletextAuthoringIdentity,
    observed: &TeletextAuthoringPublicationObservation,
) -> TeletextAuthoringPublicationTarget {
    TeletextAuthoringPublicationTarget {
        identity,
        digest: observed.digest.clone(),
        expected_design_head: observed.current_design_head,
        expected_request_id: observed.current_pending_id,
    }
}

async fn post(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
    index: u16,
    publish: bool,
) -> BTreeMap<String, String> {
    let observed = observe(fixture, identity, index).await;
    let mut fields = BTreeMap::from([
        ("channel_id", identity.channel_id.to_string()),
        ("design_id", identity.design_id.to_string()),
        ("revision", identity.revision.to_string()),
        ("digest", observed.digest),
        (
            "expected_design_head",
            observed.current_design_head.to_string(),
        ),
        (
            "expected_request_id",
            observed
                .current_pending_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        ("preview_page", index.to_string()),
        ("preview_subcode", "0000".to_owned()),
    ]);
    if publish {
        fields.insert(
            "expected_generation_id",
            observed
                .current_accepted_id
                .map_or_else(String::new, |id| id.to_string()),
        );
    }
    let response = editorial_http::post(
        fixture,
        if publish { PUBLISH } else { REQUEST },
        "application/x-www-form-urlencoded",
        serde_urlencoded::to_string(fields).unwrap().into_bytes(),
    )
    .await;
    editorial_http::redirect(&response, identity.channel_id)
}

#[derive(Debug, PartialEq, Eq)]
struct PublicationState {
    requests: Vec<(i64, i64, i64, i64, String)>,
    accepted: Vec<(i64, i64, i64, i64, String)>,
    pointers: Vec<Vec<(i64, i64)>>,
}

async fn state(fixture: &Fixture) -> PublicationState {
    let mut pointers = Vec::new();
    for query in [
        "SELECT ChannelId,RequestId FROM TeletextPublicationRequestCurrent ORDER BY ChannelId",
        "SELECT ChannelId,GenerationId FROM TeletextAcceptedGenerationCurrent ORDER BY ChannelId",
        "SELECT ChannelId,RequestId FROM TeletextEffectivePublicationRequestCurrent ORDER BY ChannelId",
        "SELECT ChannelId,GenerationId FROM TeletextEffectiveAcceptedGenerationCurrent ORDER BY ChannelId",
    ] {
        pointers.push(sqlx::query_as(query).fetch_all(&fixture.state.pool).await.unwrap());
    }
    PublicationState {
        requests:sqlx::query_as("SELECT Id,ChannelId,DesignId,Revision,Digest FROM TeletextPublicationRequest ORDER BY Id").fetch_all(&fixture.state.pool).await.unwrap(),
        accepted:sqlx::query_as("SELECT Id,ChannelId,RequestId,ArtifactVersion,ArtifactDigest FROM TeletextAcceptedGeneration ORDER BY Id").fetch_all(&fixture.state.pool).await.unwrap(),
        pointers,
    }
}

async fn accepted(fixture: &Fixture, effective: bool) -> TeletextAcceptedGeneration {
    if effective {
        fixture
            .state
            .application
            .effective_teletext_accepted_generation(1)
            .await
            .unwrap()
            .unwrap()
    } else {
        fixture
            .state
            .application
            .accepted_teletext_generation(1)
            .await
            .unwrap()
            .unwrap()
    }
}

async fn base(
    fixture: &Fixture,
) -> (
    TeletextAuthoringIdentity,
    Original,
    TeletextAcceptedGeneration,
) {
    let identity = http_prepare(fixture).await;
    let before = original(fixture, identity, TeletextAuthoringScope::Base, 200).await;
    let observed = observe(fixture, identity, 200).await;
    assert!(observed.can_request && !observed.can_publish);
    assert_eq!(
        post(fixture, identity, 200, false)
            .await
            .get("saved")
            .map(String::as_str),
        Some("publication-requested")
    );
    assert!(fixture
        .state
        .application
        .accepted_teletext_generation(1)
        .await
        .unwrap()
        .is_none());
    assert_eq!(
        post(fixture, identity, 200, true)
            .await
            .get("saved")
            .map(String::as_str),
        Some("publication-selected")
    );
    let selected = accepted(fixture, false).await;
    assert_eq!(selected.artifact_version, 5);
    assert_eq!(selected.request.design_id, identity.design_id);
    assert_eq!(selected.request.revision, identity.revision);
    assert_eq!(
        original(fixture, identity, TeletextAuthoringScope::Base, 200).await,
        before
    );
    drop(begin(&fixture.state.application, None).await.unwrap());
    (identity, before, selected)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the exact production addon bundle and isolated state/data directories"]
async fn production_current_guide_preview_waits_for_actual_worker_release_without_mutation() {
    let (mut fixture, _) = fixture().await;
    let mut runtime: Value = serde_json::from_str(include_str!(
        "../../../fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    runtime["version"] = json!(4);
    runtime["guide_limits"] = policy()["guide_limits"].clone();
    runtime["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    authoring::setting(&fixture, "teletext.output.policy", &runtime.to_string()).await;
    let (identity, before_original, selected) = base(&fixture).await;
    let before = state(&fixture).await;
    let refresh = fixture
        .state
        .application
        .observe_current_guide_refresh(
            1,
            ersatzrs_core::repositories::TeletextGuideRefreshRole::Base,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(refresh.seed.generation, selected);
    assert!(refresh.runtime.is_none());
    let guard = fixture
        .state
        .application
        .operations()
        .try_begin_teletext_work(1)
        .unwrap();
    let hold = guard.hold_work();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let worker = tokio::task::spawn_blocking(move || {
        let _hold = hold;
        let _ = started_tx.send(());
        let _ = release_rx.recv();
    });
    drop(guard);
    started_rx.await.unwrap();
    let mut request = Box::pin(editorial_http::rendered(
        &fixture,
        "/channels/1/teletext?guide_preview=1&guide_page=200&guide_subcode=0000",
    ));
    let while_occupied = tokio::time::timeout(Duration::from_millis(200), request.as_mut()).await;
    // Release and join the real worker even when the current behavior fails.
    release_tx.send(()).unwrap();
    worker.await.unwrap();
    assert!(
        while_occupied.is_err(),
        "current preview returned before the admitted worker released its only slot"
    );
    let html = request.await;
    assert!(html.contains("data-testid=\"teletext-current-guide-model\""));
    assert!(html.contains(&format!("data-generation=\"{}\"", selected.id)));
    assert_eq!(state(&fixture).await, before);
    assert_eq!(accepted(&fixture, false).await, selected);
    assert_eq!(
        fixture
            .state
            .application
            .observe_current_guide_refresh(
                1,
                ersatzrs_core::repositories::TeletextGuideRefreshRole::Base,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .unwrap(),
        refresh
    );
    assert_eq!(
        original(&fixture, identity, TeletextAuthoringScope::Base, 200).await,
        before_original
    );
    assert_eq!(
        fixture
            .state
            .application
            .operations()
            .entity_locker()
            .active_teletext_work(),
        0
    );
    assert!(fixture.playout.try_recv().is_err());
}

async fn forged_roles(
    fixture: &Fixture,
    identity: TeletextAuthoringIdentity,
) -> TeletextAuthoringIdentity {
    let revision = fixture
        .state
        .application
        .teletext_design_revision(
            identity.design_id,
            identity.revision,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    let mut document: Value = serde_json::from_slice(&revision.document).unwrap();
    let mut wire: Value =
        serde_json::from_str(document["guide_inputs"]["v3"].as_str().unwrap()).unwrap();
    assert!(wire["channels"][0]["context"]["current"].is_object());
    wire["channels"][0]["context"]["current"] = Value::Null;
    document["guide_inputs"]["v3"] = json!(wire.to_string());
    // Exercise the existing opaque storage boundary, not native authoring or
    // publication authority. The saved source fence remains byte-identical.
    let saved = SqliteTeletextDesignRepository(fixture.state.pool.clone())
        .save(
            &TeletextDesignSave {
                name: "Untrusted role mismatch".to_owned(),
                document_version: TELETEXT_DESIGN_DOCUMENT_VERSION,
                document: serde_json::to_vec(&document).unwrap(),
                existing: None,
            },
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap();
    let TeletextDesignMutation::Saved(head) = saved else {
        panic!("opaque storage must accept this structurally valid test document")
    };
    let forged = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: head.id,
        revision: head.revision,
    };
    let restored = reload_original_teletext_design_revision(
        &fixture.state.application,
        head.id,
        head.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        serde_json::to_value(restored.guide_source().unwrap()).unwrap(),
        document["guide_source"]
    );
    assert_eq!(
        designs::units(restored.encoded().unwrap()),
        original(fixture, identity, TeletextAuthoringScope::Base, 200)
            .await
            .units
    );
    forged
}

async fn refusals(fixture: &Fixture, identity: TeletextAuthoringIdentity) {
    let forged = forged_roles(fixture, identity).await;
    assert_eq!(
        post(fixture, forged, 200, false)
            .await
            .get("saved")
            .map(String::as_str),
        Some("publication-requested")
    );
    let before = state(fixture).await;
    assert_eq!(
        post(fixture, forged, 200, true)
            .await
            .get("error")
            .map(String::as_str),
        Some("unavailable")
    );
    assert_eq!(state(fixture).await, before);
    assert_eq!(
        post(fixture, identity, 200, false)
            .await
            .get("saved")
            .map(String::as_str),
        Some("publication-requested")
    );
    sqlx::query("UPDATE MovieMetadata SET Title='Changed live guide' WHERE Id=30")
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let before = state(fixture).await;
    assert_eq!(
        post(fixture, identity, 200, true)
            .await
            .get("error")
            .map(String::as_str),
        Some("unavailable")
    );
    assert_eq!(state(fixture).await, before);
    drop(begin(&fixture.state.application, None).await.unwrap());
}

async fn effective(
    fixture: &Fixture,
    night: &TeletextProfile,
    base: &TeletextAcceptedGeneration,
) -> TeletextAuthoringIdentity {
    let identity = effective_prepare(fixture, night).await;
    let before = original(fixture, identity, TeletextAuthoringScope::Effective, 410).await;
    let observed = observe(fixture, identity, 410).await;
    assert!(observed.can_request);
    request_teletext_publication(
        &fixture.state,
        target(identity, &observed),
        designs::deadline(),
    )
    .await
    .unwrap();
    let observed = observe(fixture, identity, 410).await;
    assert!(observed.can_publish);
    let selected = publish_teletext_generation(
        &fixture.state,
        TeletextAuthoringPublishTarget {
            publication: target(identity, &observed),
            expected_generation_id: observed.current_accepted_id,
        },
        designs::deadline(),
    )
    .await
    .unwrap();
    let scoped = accepted(fixture, true).await;
    assert_eq!(scoped.id, selected.generation_id);
    assert_eq!(scoped.artifact_version, 6);
    assert_eq!(accepted(fixture, false).await, *base);
    assert_eq!(
        original(fixture, identity, TeletextAuthoringScope::Effective, 410).await,
        before
    );
    drop(begin(&fixture.state.application, None).await.unwrap());
    identity
}

async fn current_effective_image(
    fixture: &Fixture,
    selected: &TeletextAcceptedGeneration,
) -> String {
    let html = editorial_http::rendered(
        fixture,
        "/channels/1/teletext?guide_preview=1&guide_page=410&guide_subcode=0000",
    )
    .await;
    let model = html
        .split("data-testid=\"teletext-current-guide-model\"")
        .nth(1)
        .expect("cold ordinary selection must recover the actual effective guide");
    assert!(model.contains(&format!("data-generation=\"{}\"", selected.id)));
    // Base and Effective IDs can coincide. The actual map/raster is the
    // distinguishing proof, rather than an ambiguous numeric generation ID.
    assert!(model.contains("alt=\"410/0000\""));
    assert!(model.contains("guide_page=410"));
    assert!(!model.contains("guide_page=200"));
    let image = model
        .split("src=\"")
        .nth(1)
        .unwrap()
        .split('"')
        .next()
        .unwrap()
        .to_owned();
    assert!(image.starts_with("data:image/svg+xml;base64,"));
    drop(begin(&fixture.state.application, None).await.unwrap());
    image
}

async fn live_guide_runtime(fixture: &Fixture) {
    let mut runtime: Value = serde_json::from_str(include_str!(
        "../../../fixtures/teletext-output-policy-v1.json"
    ))
    .unwrap();
    runtime["version"] = json!(4);
    runtime["guide_limits"] = policy()["guide_limits"].clone();
    runtime["live_updates"] = json!({"poll_interval_ms":25,"recovery_timeout_ms":2000});
    authoring::setting(fixture, "teletext.output.policy", &runtime.to_string()).await;
}

async fn effective_origin(
    fixture: &Fixture,
    night: &TeletextProfile,
    scoped: &TeletextAcceptedGeneration,
) -> (
    ersatzrs_domain::TeletextEffectiveChannelSelectionSnapshot,
    ersatzrs_core::repositories::TeletextGuideRefreshCurrentMetadata,
) {
    let selection = saved(
        fixture
            .state
            .application
            .observe_effective_channel_teletext_selection(1, Utc::now())
            .await
            .unwrap(),
    );
    assert!(selection.valid_until.unwrap() > Utc::now());
    let ersatzrs_domain::TeletextEffectiveBindingSnapshot::Scripted(interval) =
        &selection.bindings[0]
    else {
        panic!("the production guide must retain its published Lua provenance")
    };
    let ersatzrs_domain::TeletextOverrideState::Enabled(candidate) = &interval.interval.state
    else {
        panic!("the published interval must select the existing Night profile")
    };
    assert_eq!(candidate.profile_id, night.id);
    assert_eq!(candidate.profile_revision, night.revision);
    assert_eq!(candidate.installation, night.validated_installation);
    assert_eq!(selection.valid_until, Some(interval.interval.finish));
    let role = fixture
        .state
        .application
        .observe_current_guide_refresh(
            1,
            ersatzrs_core::repositories::TeletextGuideRefreshRole::Effective,
            designs::budget().max_serialized_bytes,
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(role.seed.generation, *scoped);
    assert_eq!(
        role.seed.role,
        ersatzrs_core::repositories::TeletextGuideRefreshRole::Effective
    );
    (selection, role)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "requires the exact production addon bundle and isolated state/data directories"]
async fn production_guide_explicit_http_base_and_real_lua_effective_publication_preserve_originals()
{
    let (mut fixture, night) = fixture().await;
    live_guide_runtime(&fixture).await;
    let configuration = designs::config(&fixture).await;
    let installation = fixture
        .state
        .application
        .teletext_installation_snapshot(TVGUIDE)
        .await
        .unwrap()
        .snapshot;
    assert_eq!(installation.as_ref().unwrap().version, "0.1.1");
    let (identity, before, base) = base(&fixture).await;
    refusals(&fixture, identity).await;
    assert_eq!(accepted(&fixture, false).await, base);
    assert_eq!(
        original(&fixture, identity, TeletextAuthoringScope::Base, 200).await,
        before
    );
    let effective_identity = effective(&fixture, &night, &base).await;
    let scoped = accepted(&fixture, true).await;
    let effective_before = original(
        &fixture,
        effective_identity,
        TeletextAuthoringScope::Effective,
        410,
    )
    .await;
    let (selection, role) = effective_origin(&fixture, &night, &scoped).await;
    let image = current_effective_image(&fixture, &scoped).await;
    let published = state(&fixture).await;
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_installation_snapshot(TVGUIDE)
            .await
            .unwrap()
            .snapshot,
        installation
    );
    assert!(fixture.playout.try_recv().is_err());
    fixture = fixture.reopen().await;
    assert_eq!(accepted(&fixture, false).await, base);
    assert_eq!(accepted(&fixture, true).await, scoped);
    assert_eq!(state(&fixture).await, published);
    assert_eq!(
        saved(
            fixture
                .state
                .application
                .observe_effective_channel_teletext_selection(1, Utc::now())
                .await
                .unwrap()
        ),
        selection
    );
    assert_eq!(current_effective_image(&fixture, &scoped).await, image);
    assert_eq!(
        fixture
            .state
            .application
            .observe_current_guide_refresh(
                1,
                ersatzrs_core::repositories::TeletextGuideRefreshRole::Effective,
                designs::budget().max_serialized_bytes,
            )
            .await
            .unwrap()
            .unwrap(),
        role
    );
    assert_eq!(
        original(
            &fixture,
            effective_identity,
            TeletextAuthoringScope::Effective,
            410,
        )
        .await,
        effective_before
    );
    assert_eq!(designs::config(&fixture).await, configuration);
    assert_eq!(
        fixture
            .state
            .application
            .teletext_installation_snapshot(TVGUIDE)
            .await
            .unwrap()
            .snapshot,
        installation
    );
    assert!(fixture.playout.try_recv().is_err());
    assert_eq!(
        original(&fixture, identity, TeletextAuthoringScope::Base, 200).await,
        before
    );
    drop(begin(&fixture.state.application, None).await.unwrap());
}

#[tokio::test]
#[ignore = "requires the exact production addon bundle and isolated state/data directories"]
async fn production_guide_role_expiry_rejects_publication_without_replacing_accepted_output() {
    let (fixture, _) = fixture().await;
    let (_, _, selected) = base(&fixture).await;
    let expiry = Utc::now() + chrono::Duration::seconds(12);
    // Match the repository's space-separated UTC storage shape. SQLx's direct
    // DateTime binding emits a different shape rejected by the guide reader.
    // Retain the exact expiry, including its fractional clock, without widening
    // the twelve-second window or changing any source offset.
    let stored_expiry = expiry.format("%Y-%m-%d %H:%M:%S%.f").to_string();
    sqlx::query("UPDATE PlayoutItem SET Finish=? WHERE Id=40")
        .bind(&stored_expiry)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    sqlx::query("UPDATE PlayoutItem SET Start=? WHERE Id=41")
        .bind(&stored_expiry)
        .execute(&fixture.state.pool)
        .await
        .unwrap();
    let captured = ersatzrs_web::host::teletext_guide::capture_teletext_guide(
        &fixture.state.pool,
        &[2],
        Utc::now(),
        &mut guide_capture::read_budget(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(
        captured.snapshot().channels()[0].occurrences()[0].segments[0].finish,
        expiry
    );
    let identity = http_prepare(&fixture).await;
    let before = original(&fixture, identity, TeletextAuthoringScope::Base, 200).await;
    assert_eq!(
        post(&fixture, identity, 200, false)
            .await
            .get("saved")
            .map(String::as_str),
        Some("publication-requested")
    );
    assert!(
        Utc::now() < expiry,
        "fixture must request the original while its role window is still current"
    );
    let publication = state(&fixture).await;
    let wait = (expiry - Utc::now()).to_std().unwrap_or_default() + Duration::from_millis(20);
    tokio::time::sleep(wait).await;
    assert!(Utc::now() >= expiry);
    assert_eq!(
        post(&fixture, identity, 200, true)
            .await
            .get("error")
            .map(String::as_str),
        Some("unavailable")
    );
    assert_eq!(state(&fixture).await, publication);
    assert_eq!(accepted(&fixture, false).await, selected);
    assert_eq!(
        original(&fixture, identity, TeletextAuthoringScope::Base, 200).await,
        before
    );
    drop(begin(&fixture.state.application, None).await.unwrap());
}
