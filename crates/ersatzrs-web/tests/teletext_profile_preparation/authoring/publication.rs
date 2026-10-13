//! Explicit HTTP publication uses exact rendered observations, never producer calls.

use super::*;
use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::Response,
};
use ersatzrs_web::host::{
    router::build_app,
    teletext_authoring::{
        publish_static_teletext_generation, request_static_teletext_publication,
        TeletextAuthoringPublicationObservation, TeletextAuthoringPublicationState,
        TeletextAuthoringPublicationTarget, TeletextAuthoringPublishTarget,
    },
    teletext_designs::reload_original_teletext_design_revision,
};
use tower::ServiceExt as _;

const REQUEST_ROUTE: &str = "/admin-api/teletext/design/request-publication";
const PUBLISH_ROUTE: &str = "/admin-api/teletext/design/publish";
type Fields = BTreeMap<String, String>;

async fn prepare(fixture: &AuthoringFixture) -> TeletextAuthoringIdentity {
    let saved = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        AuthoringFixture::request(1, fixture.observe(1).await),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(saved.preview.is_ok());
    let identity = saved.identity;
    drop(saved); // Release the actual preview lease before the next cap-one operation.
    identity
}

async fn observation(
    fixture: &AuthoringFixture,
    identity: TeletextAuthoringIdentity,
) -> TeletextAuthoringPublicationObservation {
    let preview = preview_original_teletext_design(
        &fixture.fixture.state.application,
        preview_request(identity),
        designs::deadline(),
    )
    .await
    .unwrap();
    preview.publication().unwrap().clone()
}

fn fields(
    identity: TeletextAuthoringIdentity,
    observed: &TeletextAuthoringPublicationObservation,
    publish: bool,
) -> Fields {
    let mut fields = BTreeMap::from([
        ("channel_id".to_owned(), identity.channel_id.to_string()),
        ("design_id".to_owned(), identity.design_id.to_string()),
        ("revision".to_owned(), identity.revision.to_string()),
        ("digest".to_owned(), observed.digest.clone()),
        (
            "expected_design_head".to_owned(),
            observed.current_design_head.to_string(),
        ),
        (
            "expected_request_id".to_owned(),
            observed
                .current_pending_id
                .map_or_else(String::new, |id| id.to_string()),
        ),
        ("preview_page".to_owned(), "123".to_owned()),
        ("preview_subcode".to_owned(), "0010".to_owned()),
    ]);
    if publish {
        fields.insert(
            "expected_generation_id".to_owned(),
            observed
                .current_accepted_id
                .map_or_else(String::new, |id| id.to_string()),
        );
    }
    fields
}

async fn post_raw(fixture: &AuthoringFixture, route: &str, body: String) -> Response {
    build_app(fixture.fixture.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(route)
                .header("content-type", "application/x-www-form-urlencoded")
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:8409".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

async fn post(fixture: &AuthoringFixture, route: &str, fields: &Fields) -> (String, Fields) {
    let response = post_raw(fixture, route, serde_urlencoded::to_string(fields).unwrap()).await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    let location = response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap();
    let (path, query) = location.split_once('?').unwrap();
    let result = (path.to_owned(), serde_urlencoded::from_str(query).unwrap());
    let bytes = axum::body::to_bytes(response.into_body(), 4096)
        .await
        .unwrap();
    assert!(!String::from_utf8(bytes.to_vec())
        .unwrap()
        .contains(GENERATION));
    result
}

type PublicationPointers = (Vec<(i64, i64)>, Vec<(i64, i64)>);

#[derive(Debug, PartialEq, Eq)]
struct Safety {
    pointers: PublicationPointers,
    requests: Vec<(i64, i64, i64, i64, String)>,
    accepted: Vec<(i64, i64, i64, String)>,
    config: Vec<(Option<String>, Option<String>)>,
    playback: PlaybackConfiguration,
    designs: i64,
    calls: Vec<u8>,
}
impl Safety {
    async fn read(fixture: &AuthoringFixture) -> Self {
        Self {
            pointers: pointers(&fixture.fixture).await,
            requests: sqlx::query_as(r#"SELECT "Id","ChannelId","DesignId","Revision","Digest" FROM "TeletextPublicationRequest" ORDER BY "Id""#)
                .fetch_all(&fixture.fixture.state.pool).await.unwrap(),
            accepted: sqlx::query_as(r#"SELECT "Id","ChannelId","RequestId","ArtifactDigest" FROM "TeletextAcceptedGeneration" ORDER BY "Id""#)
                .fetch_all(&fixture.fixture.state.pool).await.unwrap(),
            config: designs::config(&fixture.fixture).await,
            playback: playback_configuration(&fixture.fixture).await,
            designs: fixture.design_count().await,
            calls: fixture.calls(),
        }
    }
    async fn unchanged(&self, fixture: &mut AuthoringFixture) {
        assert_eq!(&Self::read(fixture).await, self);
        assert!(fixture.fixture.playout.try_recv().is_err());
    }
}

async fn selected(fixture: &AuthoringFixture, identity: TeletextAuthoringIdentity) {
    let observed = observation(fixture, identity).await;
    let (_, reply) = post(fixture, REQUEST_ROUTE, &fields(identity, &observed, false)).await;
    assert_eq!(
        reply.get("saved").map(String::as_str),
        Some("publication-requested")
    );
    let observed = observation(fixture, identity).await;
    let (_, reply) = post(fixture, PUBLISH_ROUTE, &fields(identity, &observed, true)).await;
    assert_eq!(
        reply.get("saved").map(String::as_str),
        Some("publication-selected")
    );
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

#[tokio::test]
async fn capacity_one_http_request_then_explicit_publish_reloads_exact_output_without_reinvocation()
{
    let mut fixture = AuthoringFixture::new().await;
    let identity = prepare(&fixture).await;
    let before = Safety::read(&fixture).await;
    let observed = observation(&fixture, identity).await;
    assert_eq!(
        observed.state,
        TeletextAuthoringPublicationState::Unrequested
    );
    assert!(observed.can_request);
    assert!(!observed.can_publish);
    let (path, reply) = post(&fixture, REQUEST_ROUTE, &fields(identity, &observed, false)).await;
    assert_eq!(path, "/channels/1/teletext");
    assert_eq!(
        reply,
        BTreeMap::from([
            ("saved".to_owned(), "publication-requested".to_owned()),
            ("design_id".to_owned(), identity.design_id.to_string()),
            ("revision".to_owned(), identity.revision.to_string()),
            ("page".to_owned(), "123".to_owned()),
            ("subcode".to_owned(), "0010".to_owned()),
        ])
    );
    let pending = pointers(&fixture.fixture).await;
    assert_eq!(pending.0.len(), 1);
    assert!(pending.1.is_empty());
    let observed = observation(&fixture, identity).await;
    assert_eq!(observed.state, TeletextAuthoringPublicationState::Pending);
    assert!(!observed.can_request);
    assert!(observed.can_publish);
    let (_, reply) = post(&fixture, PUBLISH_ROUTE, &fields(identity, &observed, true)).await;
    assert_eq!(
        reply.get("saved").map(String::as_str),
        Some("publication-selected")
    );
    let after = Safety::read(&fixture).await;
    assert_eq!(after.pointers.0, pending.0);
    assert_eq!(after.pointers.1.len(), 1);
    assert_eq!(after.requests.len(), 1);
    assert_eq!(after.accepted.len(), 1);
    assert_eq!(after.calls, before.calls);
    assert_eq!(after.config, before.config);
    assert_eq!(after.playback, before.playback);
    assert_eq!(after.designs, before.designs);
    let observed = observation(&fixture, identity).await;
    assert_eq!(observed.state, TeletextAuthoringPublicationState::Selected);
    assert!(!observed.can_request && !observed.can_publish);
    let accepted = super::super::generations::recover(&fixture.fixture)
        .await
        .unwrap();
    let historical = reload_original_teletext_design_revision(
        &fixture.fixture.state.application,
        identity.design_id,
        identity.revision,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        designs::units(accepted.service()),
        designs::units(historical.encoded().unwrap())
    );
    assert_eq!(
        accepted.service().magazine_units(),
        historical.encoded().unwrap().magazine_units()
    );
    assert_eq!(accepted.selection(), historical.selection());
    assert_eq!(accepted.generation().request.design_id, identity.design_id);
    assert_eq!(accepted.generation().request.digest, observed.digest);
    assert_eq!(fixture.calls(), before.calls);
    assert!(fixture.fixture.playout.try_recv().is_err());
}

fn stale_cases(previous_pending: i64) -> Vec<(&'static str, &'static str, String, &'static str)> {
    vec![
        (
            REQUEST_ROUTE,
            "expected_request_id",
            String::new(),
            "publication-conflict",
        ),
        (
            REQUEST_ROUTE,
            "digest",
            "0".repeat(64),
            "publication-conflict",
        ),
        (
            REQUEST_ROUTE,
            "expected_design_head",
            "1".to_owned(),
            "publication-conflict",
        ),
        (REQUEST_ROUTE, "channel_id", "2".to_owned(), "invalid-input"),
        (
            PUBLISH_ROUTE,
            "expected_generation_id",
            String::new(),
            "publication-conflict",
        ),
        (
            PUBLISH_ROUTE,
            "expected_request_id",
            previous_pending.to_string(),
            "publication-conflict",
        ),
        (
            PUBLISH_ROUTE,
            "digest",
            "0".repeat(64),
            "publication-conflict",
        ),
        (
            PUBLISH_ROUTE,
            "expected_design_head",
            "1".to_owned(),
            "publication-conflict",
        ),
        (
            PUBLISH_ROUTE,
            "channel_id",
            "2".to_owned(),
            "publication-conflict",
        ),
    ]
}

#[tokio::test]
async fn stale_http_observations_and_foreign_owner_preserve_nonempty_history_and_exact_cas() {
    let mut fixture = AuthoringFixture::new().await;
    let first = prepare(&fixture).await;
    selected(&fixture, first).await;
    let identity = prepare(&fixture).await;
    let rendered = observation(&fixture, identity).await;
    assert!(rendered.replaces_pending);
    let good_request = fields(identity, &rendered, false);
    let (_, reply) = post(&fixture, REQUEST_ROUTE, &good_request).await;
    assert_eq!(
        reply.get("saved").map(String::as_str),
        Some("publication-requested")
    );
    let candidate = prepare_channel_teletext_document_candidate(
        &fixture.fixture.state.application,
        1,
        Some(ADDON),
        &guide(&[]),
        channel_documents::policy(),
    )
    .await
    .unwrap();
    let advanced = designs::save(
        &fixture.fixture,
        candidate,
        Some((identity.design_id, identity.revision)),
    )
    .await;
    assert_eq!(advanced.head().revision, identity.revision + 1);
    drop(advanced);
    let current = observation(&fixture, identity).await;
    assert_eq!(current.current_design_head, 2);
    let good_request = fields(identity, &current, false);
    let good_publish = fields(identity, &current, true);
    let before = Safety::read(&fixture).await;
    assert_eq!(before.requests.len(), 2);
    assert_eq!(before.accepted.len(), 1);
    for (route, key, value, code) in stale_cases(rendered.current_pending_id.unwrap()) {
        let mut submitted = if route == REQUEST_ROUTE {
            good_request.clone()
        } else {
            good_publish.clone()
        };
        submitted.insert(key.to_owned(), value);
        let (path, reply) = post(&fixture, route, &submitted).await;
        assert_eq!(
            path,
            if key == "channel_id" {
                "/channels/2/teletext"
            } else {
                "/channels/1/teletext"
            }
        );
        assert_eq!(reply.get("error").map(String::as_str), Some(code));
        before.unchanged(&mut fixture).await;
    }
    let (_, reply) = post(&fixture, PUBLISH_ROUTE, &good_publish).await;
    assert_eq!(
        reply.get("saved").map(String::as_str),
        Some("publication-selected")
    );
    let selected = observation(&fixture, identity).await;
    assert_eq!(
        selected.current_accepted_id.unwrap(),
        before.accepted[0].0 + 1
    );
    assert_eq!(fixture.calls(), before.calls);
    assert_positive_stale_accepted(&mut fixture, before.accepted[0].0).await;
}

async fn assert_positive_stale_accepted(fixture: &mut AuthoringFixture, old_generation: i64) {
    let identity = prepare(fixture).await;
    let observed = observation(fixture, identity).await;
    assert!(observed.current_accepted_id.unwrap() > old_generation);
    let (_, reply) = post(fixture, REQUEST_ROUTE, &fields(identity, &observed, false)).await;
    assert_eq!(
        reply.get("saved").map(String::as_str),
        Some("publication-requested")
    );
    let observed = observation(fixture, identity).await;
    let mut stale = fields(identity, &observed, true);
    stale.insert(
        "expected_generation_id".to_owned(),
        old_generation.to_string(),
    );
    let before = Safety::read(fixture).await;
    let (_, reply) = post(fixture, PUBLISH_ROUTE, &stale).await;
    assert_eq!(
        reply.get("error").map(String::as_str),
        Some("publication-conflict")
    );
    before.unchanged(fixture).await;
}

#[tokio::test]
async fn malformed_duplicate_unknown_and_missing_native_fields_never_mutate_publication() {
    let mut fixture = AuthoringFixture::new().await;
    let identity = prepare(&fixture).await;
    let observed = observation(&fixture, identity).await;
    let good = fields(identity, &observed, false);
    let before = Safety::read(&fixture).await;
    for key in [
        "channel_id",
        "design_id",
        "revision",
        "expected_design_head",
        "digest",
        "expected_request_id",
        "preview_page",
        "preview_subcode",
    ] {
        let mut body = good.clone();
        body.remove(key);
        let (_, reply) = post(&fixture, REQUEST_ROUTE, &body).await;
        assert!(matches!(
            reply.get("error").map(String::as_str),
            Some("invalid" | "invalid-input")
        ));
        before.unchanged(&mut fixture).await;
    }
    for (key, value) in [
        ("design_id", "0"),
        ("revision", "-1"),
        ("expected_request_id", "0"),
        ("expected_design_head", "9223372036854775808"),
        ("digest", "private-invalid-digest"),
        ("preview_page", "900"),
        ("preview_subcode", "10"),
        ("unknown", "private-form-marker"),
        ("expected_generation_id", ""),
    ] {
        let mut body = good.clone();
        body.insert(key.to_owned(), value.to_owned());
        let (_, reply) = post(&fixture, REQUEST_ROUTE, &body).await;
        assert!(matches!(
            reply.get("error").map(String::as_str),
            Some("invalid" | "invalid-input")
        ));
        assert!(!serde_json::to_string(&reply).unwrap().contains("private-"));
        before.unchanged(&mut fixture).await;
    }
    let encoded = serde_urlencoded::to_string(&good).unwrap();
    let response = post_raw(
        &fixture,
        REQUEST_ROUTE,
        format!("{encoded}&design_id={}", identity.design_id),
    )
    .await;
    assert_eq!(response.status(), StatusCode::SEE_OTHER);
    assert!(response
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .contains("error=invalid"));
    before.unchanged(&mut fixture).await;
    let publish = fields(identity, &observed, true);
    let (_, reply) = post(&fixture, PUBLISH_ROUTE, &publish).await;
    assert_eq!(
        reply.get("error").map(String::as_str),
        Some("invalid-input")
    );
    let (_, reply) = post(&fixture, REQUEST_ROUTE, &good).await;
    assert_eq!(
        reply.get("saved").map(String::as_str),
        Some("publication-requested")
    );
    let pending = observation(&fixture, identity).await;
    let good_publish = fields(identity, &pending, true);
    let before = Safety::read(&fixture).await;
    for value in [None, Some("0"), Some("-1"), Some("9223372036854775808")] {
        let mut malformed = good_publish.clone();
        if let Some(value) = value {
            malformed.insert("expected_generation_id".to_owned(), value.to_owned());
        } else {
            malformed.remove("expected_generation_id");
        }
        let (_, reply) = post(&fixture, PUBLISH_ROUTE, &malformed).await;
        assert_eq!(reply.get("error").map(String::as_str), Some("invalid"));
        before.unchanged(&mut fixture).await;
    }
    let response = post_raw(
        &fixture,
        PUBLISH_ROUTE,
        format!("name={}", "x".repeat(512 * 1024)),
    )
    .await;
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
    before.unchanged(&mut fixture).await;
}

#[tokio::test]
async fn invalid_policy_exhaustion_busy_and_original_deadline_leave_selected_output_unchanged() {
    let mut fixture = AuthoringFixture::new().await;
    let first = prepare(&fixture).await;
    selected(&fixture, first).await;
    let identity = prepare(&fixture).await;
    let observed = observation(&fixture, identity).await;
    let request = fields(identity, &observed, false);
    let mut exhausted = policy();
    exhausted["encoding"]["max_units"] = json!(0);
    for (raw, expected) in [
        (Some("{".to_owned()), "invalid-policy"),
        (Some(exhausted.to_string()), "limit"),
        (None, "missing-policy"),
    ] {
        if let Some(raw) = raw {
            setting(&fixture.fixture, POLICY_KEY, &raw).await;
        } else {
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
                .bind(POLICY_KEY)
                .execute(&fixture.fixture.state.pool)
                .await
                .unwrap();
        }
        let before = Safety::read(&fixture).await;
        let (_, reply) = post(&fixture, REQUEST_ROUTE, &request).await;
        assert_eq!(reply.get("error").map(String::as_str), Some(expected));
        before.unchanged(&mut fixture).await;
    }
    setting(&fixture.fixture, POLICY_KEY, &policy().to_string()).await;
    let before = Safety::read(&fixture).await;
    let guard = begin(&fixture.fixture.state.application, None)
        .await
        .unwrap();
    let (_, reply) = post(&fixture, REQUEST_ROUTE, &request).await;
    assert_eq!(reply.get("error").map(String::as_str), Some("busy"));
    drop(guard);
    before.unchanged(&mut fixture).await;
    let error = request_static_teletext_publication(
        &fixture.fixture.state.application,
        target(identity, &observed),
        Instant::now(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.status, TeletextAuthoringStatus::TimedOut);
    let (_, reply) = post(&fixture, REQUEST_ROUTE, &request).await;
    assert_eq!(
        reply.get("saved").map(String::as_str),
        Some("publication-requested")
    );
    let observed = observation(&fixture, identity).await;
    let before = Safety::read(&fixture).await;
    let error = publish_static_teletext_generation(
        &fixture.fixture.state.application,
        TeletextAuthoringPublishTarget {
            publication: target(identity, &observed),
            expected_generation_id: observed.current_accepted_id,
        },
        Instant::now(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.status, TeletextAuthoringStatus::TimedOut);
    before.unchanged(&mut fixture).await;
}

#[tokio::test]
async fn nonempty_guide_original_is_unavailable_to_static_http_without_changing_legacy_request_behavior(
) {
    let mut fixture = AuthoringFixture::new().await;
    let channels = [TeletextGuideChannel {
        id: 2,
        number: "2",
        name: "Synthetic guide",
        programmes: &[],
    }];
    let candidate = prepare_channel_teletext_document_candidate(
        &fixture.fixture.state.application,
        1,
        Some(ADDON),
        &guide(&channels),
        channel_documents::policy(),
    )
    .await
    .unwrap();
    assert!(candidate.guide_source().is_none());
    let saved = designs::save(&fixture.fixture, candidate, None).await;
    let identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: saved.head().id,
        revision: saved.head().revision,
    };
    let observed = observation(&fixture, identity).await;
    assert_eq!(
        observed.state,
        TeletextAuthoringPublicationState::Unavailable
    );
    assert!(!observed.can_request && !observed.can_publish);
    let before = Safety::read(&fixture).await;
    let (_, reply) = post(&fixture, REQUEST_ROUTE, &fields(identity, &observed, false)).await;
    assert_eq!(reply.get("error").map(String::as_str), Some("unavailable"));
    before.unchanged(&mut fixture).await;
    let legacy = super::super::generations::request(&fixture.fixture, &saved, None).await;
    assert_eq!(legacy.design_id, identity.design_id);
    assert_eq!(legacy.digest, saved.digest());
    assert!(pointers(&fixture.fixture).await.1.is_empty());
    let pending_observed = observation(&fixture, identity).await;
    assert!(!pending_observed.can_publish);
    let before_publish = Safety::read(&fixture).await;
    let (_, reply) = post(
        &fixture,
        PUBLISH_ROUTE,
        &fields(identity, &pending_observed, true),
    )
    .await;
    assert_eq!(reply.get("error").map(String::as_str), Some("unavailable"));
    before_publish.unchanged(&mut fixture).await;
    assert_eq!(fixture.calls(), before.calls);
    assert_eq!(designs::config(&fixture.fixture).await, before.config);
    assert_eq!(
        playback_configuration(&fixture.fixture).await,
        before.playback
    );
    assert!(fixture.fixture.playout.try_recv().is_err());
}

async fn source_bound_design(
    fixture: &AuthoringFixture,
) -> ersatzrs_web::host::teletext_designs::SavedOriginalTeletextDesign {
    use ersatzrs_infra::xmltv::GuideSnapshotDigestLimits;
    use ersatzrs_web::host::teletext_guide::capture_source_bound_teletext_guide;
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"='pages.language'"#)
        .execute(&fixture.fixture.state.pool)
        .await
        .unwrap();
    super::super::guide_capture::seed_guide(&fixture.fixture).await;
    let mut budget = super::super::guide_capture::read_budget();
    let captured = capture_source_bound_teletext_guide(
        &fixture.fixture.state.application,
        &fixture.fixture.state.pool,
        &[2],
        guide(&[]).captured_at,
        &mut budget,
        designs::deadline(),
        GuideSnapshotDigestLimits {
            max_fields: 100_000,
            max_bytes: 16 * 1024 * 1024,
        },
    )
    .await
    .unwrap();
    let candidate = captured
        .prepare_document_candidate(
            &fixture.fixture.state.application,
            1,
            Some(ADDON),
            GENERATION,
            channel_documents::policy(),
            &mut budget,
        )
        .await
        .unwrap();
    assert_eq!(candidate.guide_source(), captured.guide_source());
    assert_eq!(candidate.guide_source().unwrap().channel_ids, vec![2]);
    designs::save(&fixture.fixture, candidate, None).await
}

#[tokio::test]
async fn real_source_bound_guide_original_rejects_static_http_and_retains_legacy_source_fence() {
    use ersatzrs_core::repositories::{TeletextPublicationMutation, TeletextPublicationRejection};
    use ersatzrs_web::host::teletext_designs::request_original_teletext_design_publication;
    let mut fixture = AuthoringFixture::new().await;
    let saved = source_bound_design(&fixture).await;
    let identity = TeletextAuthoringIdentity {
        channel_id: 1,
        design_id: saved.head().id,
        revision: saved.head().revision,
    };
    let observed = observation(&fixture, identity).await;
    assert_eq!(
        observed.state,
        TeletextAuthoringPublicationState::Unavailable
    );
    assert!(!observed.can_request && !observed.can_publish);
    let before = Safety::read(&fixture).await;
    let (_, reply) = post(&fixture, REQUEST_ROUTE, &fields(identity, &observed, false)).await;
    assert_eq!(reply.get("error").map(String::as_str), Some("unavailable"));
    before.unchanged(&mut fixture).await;
    let legacy = request_original_teletext_design_publication(
        &fixture.fixture.state.application,
        identity.design_id,
        identity.revision,
        saved.digest().to_owned(),
        saved.head().revision,
        None,
        designs::budget(),
        channel_documents::policy(),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert!(matches!(
        legacy,
        TeletextPublicationMutation::Rejected(TeletextPublicationRejection::GuideFenceRequired)
    ));
    before.unchanged(&mut fixture).await;
}

async fn assert_unavailable_historical(
    fixture: &AuthoringFixture,
    identity: TeletextAuthoringIdentity,
    expected: &TeletextAuthoringPublicationObservation,
    raster: &ersatzrs_ffmpeg::teletext::TeletextRaster,
    navigation: &[ersatzrs_domain::TeletextNavigationLink],
) {
    let preview = preview_original_teletext_design(
        &fixture.fixture.state.application,
        preview_request(identity),
        designs::deadline(),
    )
    .await
    .unwrap();
    let observed = preview.publication().unwrap();
    assert_eq!(
        observed.state,
        TeletextAuthoringPublicationState::Unavailable
    );
    assert!(!observed.can_request && !observed.can_publish);
    assert_eq!(observed.current_pending_id, expected.current_pending_id);
    assert_eq!(observed.current_accepted_id, expected.current_accepted_id);
    assert_eq!(observed.digest, expected.digest);
    assert_eq!(preview.pages(), &[channel_documents::address(123, 16)]);
    assert_eq!(preview.navigation(), navigation);
    let projected = preview
        .project(|preview| Ok((preview.raster().clone(), preview.navigation().to_vec())))
        .await
        .unwrap();
    assert_eq!(&projected.0, raster);
    assert_eq!(projected.1, navigation);
}

#[tokio::test]
async fn edited_enabled_profile_and_off_keep_historical_preview_but_disable_publication_actions() {
    let mut fixture = AuthoringFixture::new().await;
    let identity = prepare(&fixture).await;
    selected(&fixture, identity).await;
    let preview = preview_original_teletext_design(
        &fixture.fixture.state.application,
        preview_request(identity),
        designs::deadline(),
    )
    .await
    .unwrap();
    let observed = preview.publication().unwrap().clone();
    assert_eq!(observed.state, TeletextAuthoringPublicationState::Selected);
    assert!(observed.current_pending_id.is_some());
    assert!(observed.current_accepted_id.is_some());
    let (raster, navigation) = preview
        .project(|preview| Ok((preview.raster().clone(), preview.navigation().to_vec())))
        .await
        .unwrap();
    let before = Safety::read(&fixture).await;
    let original = &fixture.profiles[0];
    let edited = saved(
        fixture
            .fixture
            .state
            .application
            .save_teletext_profile(&TeletextProfileSave {
                addon_id: ADDON.to_owned(),
                name: "Edited synthetic profile".to_owned(),
                settings: [("THEME", "night"), ("RESPONSE", "good")]
                    .map(|(key, value)| TeletextProfileSetting {
                        key: key.to_owned(),
                        value: TeletextProfileValue::Literal(value.to_owned()),
                    })
                    .to_vec(),
                expected_installation: original.validated_installation.clone(),
                existing: Some((original.id, original.revision)),
            })
            .await
            .unwrap(),
    );
    assert_eq!(edited.profile.id, original.id);
    assert_eq!(edited.profile.revision, original.revision + 1);
    let binding = fixture
        .fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    assert_eq!(binding.len(), 1);
    assert!(binding[0].binding.enabled);
    assert_unavailable_historical(&fixture, identity, &observed, &raster, &navigation).await;
    before.unchanged(&mut fixture).await;
    let disabled = saved(
        fixture
            .fixture
            .state
            .application
            .disable_channel_teletext_binding(1, ADDON, binding[0].binding.revision)
            .await
            .unwrap(),
    );
    assert!(!disabled.enabled);
    assert_unavailable_historical(&fixture, identity, &observed, &raster, &navigation).await;
    before.unchanged(&mut fixture).await;
}
