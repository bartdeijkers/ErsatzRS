//! Installed synthetic original-only controller flow; no runtime playback claim.

use super::{channel_documents, designs, documents::DocumentFixture, *};
use ersatzrs_app::commands::SetAdminSetting;
use ersatzrs_web::host::teletext_authoring::{
    observe_teletext_authoring_selection, prepare_static_teletext_design,
    preview_original_teletext_design, TeletextAuthoringIdentity, TeletextAuthoringPrepare,
    TeletextAuthoringPreviewRequest, TeletextAuthoringScope, TeletextAuthoringStatus,
};
use ersatzrs_web::host::teletext_channel_documents::prepare_channel_teletext_document_candidate;
use ersatzrs_web::host::teletext_work::{begin, MAX_CONCURRENT_SETTING};
use serde_json::{json, Value};
use std::time::Instant;

#[path = "authoring/publication.rs"]
mod publication;

const POLICY_KEY: &str = "teletext.authoring.policy";
const SCRIPT: &str = r#"test "$1" = pages || exit 9
 test "$ERSATZRS_ADDON_CAPABILITY" = teletext.pages.v2 || exit 10
 theme="$ERSATZRS_ADDON_SETTING_THEME"
 case "$theme" in day|night) ;; *) exit 11;; esac
 IFS= read -r input || test -n "$input" || exit 12
 printf '%s' "$input" > "../authoring-input-$theme.json"
 printf '%s\n' "$theme" >> ../authoring-calls
 generation="${input#*\"generation\":\"}"
 generation="${generation%%\"*}"
 sed "s/native-profile-generation/$generation/" "../authoring-response-$theme.json"
"#;

pub(super) fn policy() -> Value {
    let mut value: Value =
        serde_json::from_str(include_str!("../fixtures/teletext-output-policy-v1.json")).unwrap();
    value.as_object_mut().unwrap().remove("source");
    value.as_object_mut().unwrap().remove("recovery_timeout_ms");
    value["operation_timeout_ms"] = json!(30_000);
    value["preview"] = json!({"max_pages":16,"max_model_records":32_768,
        "max_pixels":168_000,"max_diagnostics":1400,"max_work":50_000_000});
    value
}

pub(super) async fn setting(fixture: &Fixture, key: &'static str, value: &str) {
    fixture
        .state
        .application
        .set_admin_setting(SetAdminSetting {
            key,
            value: value.to_owned(),
        })
        .await
        .unwrap();
}

pub(super) struct AuthoringFixture {
    pub(super) fixture: Fixture,
    pub(super) profiles: [TeletextProfile; 2],
}
impl AuthoringFixture {
    pub(super) async fn new() -> Self {
        let fixture = Fixture::new_persistent().await;
        channel_documents::install_contract(&fixture, ADDON, "first", 2).await;
        std::fs::write(fixture.root.path().join("package/fixture.sh"), SCRIPT).unwrap();
        let hash = calculate_addon_content_sha256(&fixture.root.path().join("package"))
            .await
            .unwrap();
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ContentSha256"=?,"PackageSha256"=? WHERE "AddonId"=?"#)
            .bind(hash).bind("a".repeat(64)).bind(ADDON).execute(&fixture.state.pool).await.unwrap();
        for theme in ["day", "night"] {
            std::fs::write(
                fixture
                    .root
                    .path()
                    .join(format!("authoring-response-{theme}.json")),
                serde_json::to_vec(&super::documents::document(
                    DocumentFixture::Authored,
                    theme,
                ))
                .unwrap(),
            )
            .unwrap();
        }
        let profiles = [
            fixture.profile("Day", "day", "good").await,
            fixture.profile("Night", "night", "good").await,
        ];
        for (channel, profile) in [1, 2].into_iter().zip(&profiles) {
            fixture.bind(channel, profile, None).await;
        }
        setting(&fixture, POLICY_KEY, &policy().to_string()).await;
        setting(&fixture, "pages.language", "en-us").await;
        setting(&fixture, MAX_CONCURRENT_SETTING, "1").await;
        Self { fixture, profiles }
    }
    pub(super) fn calls(&self) -> Vec<u8> {
        std::fs::read(self.fixture.root.path().join("authoring-calls")).unwrap_or_default()
    }
    pub(super) async fn observe(&self, channel: i64) -> String {
        observe_teletext_authoring_selection(
            &self.fixture.state.application,
            channel,
            designs::deadline(),
        )
        .await
        .unwrap()
    }
    pub(super) fn request(channel: i64, digest: String) -> TeletextAuthoringPrepare {
        TeletextAuthoringPrepare {
            scope: TeletextAuthoringScope::Base,
            channel_id: channel,
            selection_digest: digest,
            name: "Original synthetic authoring".to_owned(),
            input_json: json!({
                "input_version":1,"channel_id":channel,"entry_addon_id":ADDON,
                "entry":{"page":123,"subcode":16},"v1_region":"western_latin",
                "resource_transmissions":[]})
            .to_string(),
        }
    }
    pub(super) async fn design_count(&self) -> i64 {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextDesignRevision""#)
            .fetch_one(&self.fixture.state.pool)
            .await
            .unwrap()
    }
}
pub(super) fn preview_request(
    identity: TeletextAuthoringIdentity,
) -> TeletextAuthoringPreviewRequest {
    TeletextAuthoringPreviewRequest {
        channel_id: identity.channel_id,
        design_id: identity.design_id,
        revision: identity.revision,
        owner: Some(channel_documents::address(123, 16)),
    }
}

async fn pointers(fixture: &Fixture) -> (Vec<(i64, i64)>, Vec<(i64, i64)>) {
    let pending = sqlx::query_as(r#"SELECT "ChannelId","RequestId" FROM "TeletextPublicationRequestCurrent" ORDER BY "ChannelId""#)
        .fetch_all(&fixture.state.pool).await.unwrap();
    let accepted = sqlx::query_as(r#"SELECT "ChannelId","GenerationId" FROM "TeletextAcceptedGenerationCurrent" ORDER BY "ChannelId""#)
        .fetch_all(&fixture.state.pool).await.unwrap();
    (pending, accepted)
}

type PlaybackConfiguration = (Vec<(i64, i64, i64, i64, i64)>, i64);
async fn playback_configuration(fixture: &Fixture) -> PlaybackConfiguration {
    let channels = sqlx::query_as(r#"SELECT "Id","FFmpegProfileId","StreamingMode","TranscodeMode","SubtitleMode" FROM "Channel" ORDER BY "Id""#)
        .fetch_all(&fixture.state.pool).await.unwrap();
    let playouts = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Playout""#)
        .fetch_one(&fixture.state.pool)
        .await
        .unwrap();
    (channels, playouts)
}

#[tokio::test]
async fn capacity_one_static_chain_saves_reloads_projects_and_isolates_two_profile_themes() {
    let mut fixture = AuthoringFixture::new().await;
    let playback = playback_configuration(&fixture.fixture).await;
    let config = designs::config(&fixture.fixture).await;
    let before = pointers(&fixture.fixture).await;
    let mut rasters = Vec::new();
    for channel in [1, 2] {
        let observed = fixture.observe(channel).await;
        let saved = prepare_static_teletext_design(
            &fixture.fixture.state.application,
            AuthoringFixture::request(channel, observed),
            designs::deadline(),
        )
        .await
        .unwrap();
        assert_eq!(saved.identity.channel_id, channel);
        let preview = saved.preview.unwrap();
        assert_eq!(preview.pages(), &[channel_documents::address(123, 16)]);
        assert!(preview.raster().unwrap().diagnostics().is_empty());
        assert_eq!(preview.navigation()[0].target.page.get(), 123);
        let raster = preview
            .project(|preview| Ok(preview.raster().unwrap().clone()))
            .await
            .unwrap();
        let restored = preview_original_teletext_design(
            &fixture.fixture.state.application,
            preview_request(saved.identity),
            designs::deadline(),
        )
        .await
        .unwrap();
        assert_eq!(restored.raster(), Some(&raster));
        drop(restored);
        rasters.push(raster);
    }
    assert_ne!(rasters[0], rasters[1]);
    assert_eq!(fixture.design_count().await, 2);
    assert_eq!(fixture.calls(), b"day\nnight\n");
    for theme in ["day", "night"] {
        let request: Value = serde_json::from_slice(
            &std::fs::read(
                fixture
                    .fixture
                    .root
                    .path()
                    .join(format!("authoring-input-{theme}.json")),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(request["channels"], json!([]));
    }
    assert_eq!(pointers(&fixture.fixture).await, before);
    assert_eq!(designs::config(&fixture.fixture).await, config);
    assert_eq!(playback_configuration(&fixture.fixture).await, playback);
    assert!(fixture.fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn expired_invalid_and_missing_policy_fail_before_invocation_or_original_storage() {
    let mut fixture = AuthoringFixture::new().await;
    let playback = playback_configuration(&fixture.fixture).await;
    let digest = fixture.observe(1).await;
    for name in [" \t".to_owned(), "bad\0name".to_owned(), "x".repeat(257)] {
        let mut request = AuthoringFixture::request(1, digest.clone());
        request.name = name;
        let error = prepare_static_teletext_design(
            &fixture.fixture.state.application,
            request,
            designs::deadline(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.status, TeletextAuthoringStatus::InvalidInput);
    }
    let result = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        AuthoringFixture::request(1, digest.clone()),
        Instant::now(),
    )
    .await
    .unwrap_err();
    assert_eq!(result.status, TeletextAuthoringStatus::TimedOut);
    for raw in [Some("{"), None] {
        if let Some(raw) = raw {
            setting(&fixture.fixture, POLICY_KEY, raw).await;
        } else {
            sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key"=?"#)
                .bind(POLICY_KEY)
                .execute(&fixture.fixture.state.pool)
                .await
                .unwrap();
        }
        let config = designs::config(&fixture.fixture).await;
        let result = prepare_static_teletext_design(
            &fixture.fixture.state.application,
            AuthoringFixture::request(1, digest.clone()),
            designs::deadline(),
        )
        .await
        .unwrap_err();
        assert_eq!(
            result.status,
            if raw.is_some() {
                TeletextAuthoringStatus::InvalidPolicy
            } else {
                TeletextAuthoringStatus::MissingPolicy
            }
        );
        assert_eq!(designs::config(&fixture.fixture).await, config);
    }
    assert!(fixture.calls().is_empty());
    assert_eq!(fixture.design_count().await, 0);
    assert_eq!(pointers(&fixture.fixture).await, (vec![], vec![]));
    assert_eq!(playback_configuration(&fixture.fixture).await, playback);
    assert!(fixture.fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn observed_selection_change_and_wrong_input_channel_never_invoke_or_save() {
    let mut fixture = AuthoringFixture::new().await;
    let playback = playback_configuration(&fixture.fixture).await;
    let stale = fixture.observe(1).await;
    fixture.fixture.bind(1, &fixture.profiles[1], Some(1)).await;
    let config = designs::config(&fixture.fixture).await;
    let result = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        AuthoringFixture::request(1, stale),
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert_eq!(result.status, TeletextAuthoringStatus::SelectionChanged);
    let mut request = AuthoringFixture::request(1, fixture.observe(1).await);
    request.channel_id = 2;
    let result = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        request,
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert_eq!(result.status, TeletextAuthoringStatus::WrongChannel);
    assert_eq!(designs::config(&fixture.fixture).await, config);
    assert_eq!(fixture.design_count().await, 0);
    assert!(fixture.calls().is_empty());
    assert_eq!(pointers(&fixture.fixture).await, (vec![], vec![]));
    assert_eq!(playback_configuration(&fixture.fixture).await, playback);
    assert!(fixture.fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn historical_missing_foreign_revision_and_owner_fail_without_reinvocation_or_publication() {
    let mut fixture = AuthoringFixture::new().await;
    let playback = playback_configuration(&fixture.fixture).await;
    let mut limited = policy();
    limited["preview"]["max_pixels"] = json!(1);
    setting(&fixture.fixture, POLICY_KEY, &limited.to_string()).await;
    let saved = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        AuthoringFixture::request(1, fixture.observe(1).await),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(
        saved.preview.unwrap_err().status,
        TeletextAuthoringStatus::ResourceLimit
    );
    assert_eq!(saved.identity.revision, 1);
    setting(&fixture.fixture, POLICY_KEY, &policy().to_string()).await;
    let recovered = preview_original_teletext_design(
        &fixture.fixture.state.application,
        preview_request(saved.identity),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_eq!(recovered.identity, saved.identity);
    drop(recovered);
    let mut request = preview_request(saved.identity);
    let config = designs::config(&fixture.fixture).await;
    let calls = fixture.calls();
    for (mutated, expected) in [
        (
            TeletextAuthoringPreviewRequest {
                channel_id: 2,
                ..request
            },
            TeletextAuthoringStatus::WrongChannel,
        ),
        (
            TeletextAuthoringPreviewRequest {
                revision: 2,
                ..request
            },
            TeletextAuthoringStatus::MissingDesign,
        ),
        (
            TeletextAuthoringPreviewRequest {
                design_id: i64::MAX,
                ..request
            },
            TeletextAuthoringStatus::MissingDesign,
        ),
    ] {
        let error = preview_original_teletext_design(
            &fixture.fixture.state.application,
            mutated,
            designs::deadline(),
        )
        .await
        .unwrap_err();
        assert_eq!(error.status, expected);
    }
    request.owner = Some(channel_documents::address(123, 17));
    let error = preview_original_teletext_design(
        &fixture.fixture.state.application,
        request,
        designs::deadline(),
    )
    .await
    .unwrap_err();
    assert_eq!(error.status, TeletextAuthoringStatus::PreviewFailed);
    assert_eq!(error.location.unwrap().owner, request.owner);
    assert_eq!(fixture.calls(), calls);
    assert_eq!(fixture.design_count().await, 1);
    assert_eq!(designs::config(&fixture.fixture).await, config);
    assert_eq!(pointers(&fixture.fixture).await, (vec![], vec![]));
    assert_eq!(playback_configuration(&fixture.fixture).await, playback);
    assert!(fixture.fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn abandoned_projection_retains_the_actual_worker_lease_until_it_finishes() {
    let fixture = AuthoringFixture::new().await;
    let saved = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        AuthoringFixture::request(1, fixture.observe(1).await),
        designs::deadline(),
    )
    .await
    .unwrap();
    let preview = saved.preview.unwrap();
    let (started_tx, started_rx) = tokio::sync::oneshot::channel();
    let (finish_tx, finish_rx) = std::sync::mpsc::channel();
    let task = tokio::spawn(async move {
        preview
            .project(move |_| {
                started_tx.send(()).unwrap();
                finish_rx.recv().unwrap();
                Ok(())
            })
            .await
    });
    started_rx.await.unwrap();
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert!(matches!(
        begin(&fixture.fixture.state.application, None).await,
        Err(ersatzrs_web::host::teletext_work::TeletextWorkError::Saturated)
    ));
    finish_tx.send(()).unwrap();
    tokio::time::timeout(Duration::from_secs(5), async {
        loop {
            match begin(&fixture.fixture.state.application, None).await {
                Ok(guard) => {
                    drop(guard);
                    break;
                }
                Err(ersatzrs_web::host::teletext_work::TeletextWorkError::Saturated) => {
                    tokio::task::yield_now().await;
                }
                Err(error) => panic!("unexpected admission error: {error}"),
            }
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn original_authoring_preserves_nonempty_pending_and_accepted_generation_and_playback_configuration(
) {
    let mut fixture = AuthoringFixture::new().await;
    let playback = playback_configuration(&fixture.fixture).await;
    let candidate = prepare_channel_teletext_document_candidate(
        &fixture.fixture.state.application,
        1,
        Some(ADDON),
        &guide(&[]),
        channel_documents::policy(),
    )
    .await
    .unwrap();
    let saved = designs::save(&fixture.fixture, candidate, None).await;
    let pending = super::generations::request(&fixture.fixture, &saved, None).await;
    let original = super::generations::accepted(
        super::generations::accept(&fixture.fixture, &pending, saved.head().revision, None).await,
    );
    let before = pointers(&fixture.fixture).await;
    assert!(!before.0.is_empty());
    assert!(!before.1.is_empty());
    let config = designs::config(&fixture.fixture).await;
    let calls = fixture.calls();
    let authored = prepare_static_teletext_design(
        &fixture.fixture.state.application,
        AuthoringFixture::request(1, fixture.observe(1).await),
        designs::deadline(),
    )
    .await
    .unwrap();
    assert_ne!(authored.identity.design_id, saved.head().id);
    assert!(authored.preview.is_ok());
    drop(authored);
    let restored = super::generations::recover(&fixture.fixture).await.unwrap();
    assert_eq!(restored.generation(), original.generation());
    assert_eq!(restored.selection(), original.selection());
    assert_eq!(
        designs::units(restored.service()),
        designs::units(original.service())
    );
    assert_eq!(
        restored.service().magazine_units(),
        original.service().magazine_units()
    );
    assert_eq!(pointers(&fixture.fixture).await, before);
    assert_eq!(designs::config(&fixture.fixture).await, config);
    assert_eq!(fixture.calls(), [calls.as_slice(), b"day\n"].concat());
    assert_eq!(playback_configuration(&fixture.fixture).await, playback);
    assert!(fixture.fixture.playout.try_recv().is_err());
}

#[tokio::test]
async fn actual_native_prepare_post_preserves_real_unencodable_base_glyph_position_without_saving()
{
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use ersatzrs_web::host::router::build_app;
    use tower::ServiceExt as _;

    let mut fixture = AuthoringFixture::new().await;
    let mut response = super::documents::document(DocumentFixture::Authored, "day");
    response["pages"][0]["rows"][0]["runs"][0]["content"]["text"] = json!("📺ZZ");
    let bytes = serde_json::to_vec(&response).unwrap();
    let validation = channel_documents::policy().validation;
    assert!(
        ersatzrs_app::teletext_pages_v2::parse_declared_service(
            &bytes,
            GENERATION,
            validation.service,
            validation.declarations,
        )
        .is_ok(),
        "the rejected glyph must pass structural/semantic document validation"
    );
    std::fs::write(
        fixture
            .fixture
            .root
            .path()
            .join("authoring-response-day.json"),
        bytes,
    )
    .unwrap();
    let digest = fixture.observe(1).await;
    let config = designs::config(&fixture.fixture).await;
    let playback = playback_configuration(&fixture.fixture).await;
    let before = pointers(&fixture.fixture).await;
    let body = serde_urlencoded::to_string([
        ("channel_id", "1"),
        ("selection_digest", digest.as_str()),
        ("name", "Rejected original glyph"),
        ("entry_addon_id", ADDON),
        ("entry_page", "123"),
        ("entry_subcode", "0010"),
        ("v1_region", "western_latin"),
        ("resource_transmissions", "[]"),
    ])
    .unwrap();
    let returned = build_app(fixture.fixture.state.clone())
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/admin-api/teletext/design/prepare")
                .header("content-type", "application/x-www-form-urlencoded")
                .extension(axum::extract::ConnectInfo(
                    "127.0.0.1:8409".parse::<std::net::SocketAddr>().unwrap(),
                ))
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(returned.status(), StatusCode::SEE_OTHER);
    let location = returned
        .headers()
        .get("location")
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned();
    let (path, query) = location.split_once('?').unwrap();
    assert_eq!(path, "/channels/1/teletext");
    let fields: BTreeMap<String, String> = serde_urlencoded::from_str(query).unwrap();
    assert_eq!(
        fields,
        BTreeMap::from([
            ("error".to_owned(), "encoding".to_owned()),
            ("error_page".to_owned(), "123".to_owned()),
            ("error_subcode".to_owned(), "0010".to_owned()),
            ("error_row".to_owned(), "1".to_owned()),
            ("error_column".to_owned(), "8".to_owned()),
        ])
    );
    let response_body = axum::body::to_bytes(returned.into_body(), 4096)
        .await
        .unwrap();
    assert!(!String::from_utf8(response_body.to_vec())
        .unwrap()
        .contains("📺ZZ"));
    assert!(!location.contains("📺"));
    assert!(!location.contains("1F4FA"));
    assert_eq!(fixture.calls(), b"day\n");
    assert_eq!(fixture.design_count().await, 0);
    assert_eq!(pointers(&fixture.fixture).await, before);
    assert_eq!(designs::config(&fixture.fixture).await, config);
    assert_eq!(playback_configuration(&fixture.fixture).await, playback);
    assert!(fixture.fixture.playout.try_recv().is_err());
}
