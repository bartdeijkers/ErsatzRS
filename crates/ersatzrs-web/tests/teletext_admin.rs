//! Disposable native HTTP controls; no package execution or publication claim.

#![cfg(feature = "ssr")]

use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use ersatzrs_addon_contract::{AddonManifest, ADDON_MANIFEST_VERSION};
use ersatzrs_core::repositories::{ChannelRepository as _, FfmpegProfileRepository as _};
use ersatzrs_domain::{
    Channel, FFmpegProfile, TeletextProfile, TeletextProfileSetting, TeletextProfileValue,
};
use ersatzrs_ffmpeg::FfmpegRuntimeReport;
use ersatzrs_infra::db::{DatabaseBackend, DatabaseSchemaStatus};
use ersatzrs_web::api::teletext::MAX_TELETEXT_FORM_BYTES;
use serde_json::json;
use tokio::sync::mpsc;
use tower::ServiceExt as _;
use uuid::Uuid;

use ersatzrs_web::{
    admin::server_fns::teletext::{
        load_channel_teletext_for_application, TeletextInstallationData,
    },
    host::{
        router::build_app,
        state::{AppState, IptvAccessTokenVerifier, SystemStartup},
    },
};

type Pairs = Vec<(String, String)>;

const ADDON: &str = "org.example.teletext.admin-fixture";
const OTHER: &str = "org.example.teletext.admin-other";
const PROFILE_PATH: &str = "/admin-api/teletext/profile/save";
const BINDING_PATH: &str = "/admin-api/teletext/binding/save";
const OFF_PATH: &str = "/admin-api/teletext/binding/off";

struct Fixture {
    root: tempfile::TempDir,
    state: AppState,
    app: Router,
    builds: mpsc::UnboundedReceiver<ersatzrs_core::scheduling::PlayoutBuildRequest>,
}

fn manifest(addon: &str) -> AddonManifest {
    let manifest: AddonManifest = serde_json::from_value(json!({
        "manifest_version":ADDON_MANIFEST_VERSION,"id":addon,"version":"1.0.0",
        "name":{"en-US":"Synthetic teletext","nl":"Synthetische teletekst"},
        "summary":{"en-US":"Configuration controls fixture"},"license":"GPL-3.0-or-later",
        "source_url":"https://example.test/teletext","host_version":"*",
        "capabilities":[{"id":"addon.check.v1"},{"id":"teletext.pages.v2"}],
        "entrypoints":{"any":{"path":"fixture.bin","kind":"native"}},
        "settings":[
            {"key":"THEME","label":{"en-US":"Theme"},"kind":"enum","required":true,
                "default":"day","options":["day","night"]},
            {"key":"LABEL","label":{"en-US":"Label"},"kind":"string"},
            {"key":"TOKEN","label":{"en-US":"Token reference"},"kind":"secret_reference"}
        ]
    }))
    .unwrap();
    manifest.validate().unwrap();
    manifest
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = ersatzrs_infra::db::init(&format!(
            "sqlite:{}",
            root.path().join("controls.db").display()
        ))
        .await
        .unwrap();
        sqlx::query(r#"INSERT INTO "Resolution" ("Id","Name","Width","Height") VALUES (1,'Fixture',640,360)"#).execute(&pool).await.unwrap();
        let (sender, builds) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            2,
            IptvAccessTokenVerifier::disabled(),
            sender,
            ersatzrs_core::locking::EntityLocker::default(),
        );
        state
            .repositories
            .ffmpeg_profile
            .upsert(&FFmpegProfile {
                id: 1,
                name: "Fixture".to_owned(),
                resolution_id: 1,
                ..FFmpegProfile::default()
            })
            .await
            .unwrap();
        for id in [1, 2] {
            state
                .repositories
                .channel
                .upsert(&Channel {
                    id,
                    unique_id: Uuid::from_u128(u128::try_from(id).unwrap()),
                    number: id.to_string(),
                    name: format!("Synthetic {id}"),
                    ffmpeg_profile_id: 1,
                    is_enabled: true,
                    ..Channel::default()
                })
                .await
                .unwrap();
        }
        let fixture = Self {
            app: build_app(state.clone()),
            root,
            state,
            builds,
        };
        fixture.install(ADDON).await;
        fixture
    }

    async fn install(&self, addon: &str) {
        let package = self.root.path().join(addon);
        std::fs::create_dir(&package).unwrap();
        std::fs::write(package.join("fixture.bin"), b"not an executable").unwrap();
        let hash = ersatzrs_infra::addons::calculate_addon_content_sha256(&package)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "AddonInstallation"
            ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt","StateRevision")
            VALUES (?,'1.0.0',1,'synthetic-package',?,?,?,'2026-10-12T00:00:00Z','2026-10-12T00:00:00Z','synthetic-state')"#)
            .bind(addon).bind(hash).bind(serde_json::to_string(&manifest(addon)).unwrap())
            .bind(package.to_string_lossy().as_ref()).execute(&self.state.pool).await.unwrap();
        sqlx::query(
            r#"INSERT INTO "AddonSetting" ("AddonId","Key","Value") VALUES (?,'THEME','day')"#,
        )
        .bind(addon)
        .execute(&self.state.pool)
        .await
        .unwrap();
    }

    async fn token(&self, addon: &str) -> String {
        let snapshot = self
            .state
            .application
            .teletext_installation_snapshot(addon)
            .await
            .unwrap()
            .snapshot
            .unwrap();
        serde_json::to_string(&TeletextInstallationData::from(snapshot)).unwrap()
    }

    async fn post(&self, path: &str, fields: Pairs) -> String {
        self.post_raw(
            path,
            serde_urlencoded::to_string(fields).unwrap(),
            StatusCode::SEE_OTHER,
        )
        .await
    }

    async fn post_raw(&self, path: &str, body: String, expected: StatusCode) -> String {
        let response = self
            .app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(path)
                    .header("content-type", "application/x-www-form-urlencoded")
                    .extension(axum::extract::ConnectInfo(
                        "127.0.0.1:8409".parse::<std::net::SocketAddr>().unwrap(),
                    ))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), expected);
        response
            .headers()
            .get("location")
            .map_or_else(String::new, |value| value.to_str().unwrap().to_owned())
    }

    async fn profiles(&self, addon: &str) -> Vec<TeletextProfile> {
        self.state
            .application
            .list_teletext_profiles(addon)
            .await
            .unwrap()
            .into_iter()
            .map(|read| read.profile)
            .collect()
    }
}

fn slot(fields: &mut Pairs, index: usize, key: &str, mode: &str, value: &str) {
    for (suffix, value) in [
        ("key", key),
        ("mode", mode),
        ("value", value),
        ("reference_kind", ""),
        ("reference", ""),
    ] {
        fields.push((format!("setting.{index}.{suffix}"), value.to_owned()));
    }
}

fn profile_form(
    token: &str,
    name: &str,
    channel: i64,
    existing: Option<&TeletextProfile>,
) -> Pairs {
    let mut fields = vec![
        ("channel_id".to_owned(), channel.to_string()),
        ("addon_id".to_owned(), ADDON.to_owned()),
        ("name".to_owned(), name.to_owned()),
        ("expected_installation".to_owned(), token.to_owned()),
        (
            "profile_id".to_owned(),
            existing.map_or_else(String::new, |profile| profile.id.to_string()),
        ),
        (
            "expected_profile_revision".to_owned(),
            existing.map_or_else(String::new, |profile| profile.revision.to_string()),
        ),
        ("setting_count".to_owned(), "3".to_owned()),
    ];
    slot(&mut fields, 0, "THEME", "inherit", "");
    slot(&mut fields, 1, "LABEL", "literal", "");
    slot(&mut fields, 2, "TOKEN", "inherit", "");
    fields
}

fn replace(fields: &mut Pairs, key: &str, value: &str) {
    value.clone_into(&mut fields.iter_mut().find(|(field, _)| field == key).unwrap().1);
}

fn binding_form(
    token: &str,
    channel: i64,
    profile: &TeletextProfile,
    previous: Option<i64>,
) -> Pairs {
    vec![
        ("channel_id".to_owned(), channel.to_string()),
        ("addon_id".to_owned(), ADDON.to_owned()),
        (
            "profile_choice".to_owned(),
            format!("{}:{}", profile.id, profile.revision),
        ),
        (
            "expected_binding_revision".to_owned(),
            previous.map_or_else(String::new, |value| value.to_string()),
        ),
        ("expected_installation".to_owned(), token.to_owned()),
    ]
}

async fn config(
    fixture: &Fixture,
) -> (
    Vec<(Option<String>, Option<String>)>,
    Vec<(String, String, Option<String>)>,
) {
    let settings = sqlx::query_as(r#"SELECT "Key","Value" FROM "ConfigElement" ORDER BY "Key""#)
        .fetch_all(&fixture.state.pool)
        .await
        .unwrap();
    let globals = sqlx::query_as(
        r#"SELECT "AddonId","Key","Value" FROM "AddonSetting" ORDER BY "AddonId","Key""#,
    )
    .fetch_all(&fixture.state.pool)
    .await
    .unwrap();
    (settings, globals)
}

async fn selections(fixture: &Fixture) -> Vec<ersatzrs_domain::ChannelTeletextBindingRead> {
    let mut result = fixture
        .state
        .application
        .channel_teletext_bindings(1)
        .await
        .unwrap();
    result.extend(
        fixture
            .state
            .application
            .channel_teletext_bindings(2)
            .await
            .unwrap(),
    );
    result
}

#[tokio::test]
async fn native_forms_persist_complete_bags_and_isolate_channel_profiles() {
    let mut fixture = Fixture::new().await;
    let token = fixture.token(ADDON).await;
    let before = config(&fixture).await;
    let channel = fixture
        .state
        .repositories
        .channel
        .get_with_profile(1)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .post(PROFILE_PATH, profile_form(&token, "Day", 1, None))
            .await,
        "/channels/1/teletext?saved=profile"
    );
    let day = fixture.profiles(ADDON).await.remove(0);
    assert_eq!(
        day.settings,
        vec![TeletextProfileSetting {
            key: "LABEL".to_owned(),
            value: TeletextProfileValue::Literal(String::new())
        }]
    );
    let mut form = profile_form(&token, "Night", 2, None);
    replace(&mut form, "setting.0.mode", "literal");
    replace(&mut form, "setting.0.value", "night");
    assert_eq!(
        fixture.post(PROFILE_PATH, form).await,
        "/channels/2/teletext?saved=profile"
    );
    let night = fixture
        .profiles(ADDON)
        .await
        .into_iter()
        .find(|profile| profile.name == "Night")
        .unwrap();
    for (id, profile) in [(1, &day), (2, &night)] {
        assert_eq!(
            fixture
                .post(BINDING_PATH, binding_form(&token, id, profile, None))
                .await,
            format!("/channels/{id}/teletext?saved=binding")
        );
    }
    let old = selections(&fixture).await;
    let mut edit = profile_form(&token, "Day renamed", 1, Some(&day));
    replace(&mut edit, "setting.1.mode", "inherit");
    assert_eq!(
        fixture.post(PROFILE_PATH, edit).await,
        "/channels/1/teletext?saved=profile"
    );
    let after = selections(&fixture).await;
    assert_eq!(after[0].binding.profile_id, day.id);
    assert_eq!(after[0].binding.revision, old[0].binding.revision + 1);
    assert_eq!(after[1], old[1]);
    assert!(fixture
        .profiles(ADDON)
        .await
        .into_iter()
        .find(|profile| profile.id == day.id)
        .unwrap()
        .settings
        .is_empty());
    assert_eq!(config(&fixture).await, before);
    assert_eq!(
        fixture
            .state
            .repositories
            .channel
            .get_with_profile(1)
            .await
            .unwrap(),
        channel
    );
    assert!(fixture.builds.try_recv().is_err());
}

#[tokio::test]
async fn client_profile_installation_and_binding_snapshots_are_never_recaptured() {
    let fixture = Fixture::new().await;
    let token = fixture.token(ADDON).await;
    fixture
        .post(PROFILE_PATH, profile_form(&token, "Day", 1, None))
        .await;
    let profile = fixture.profiles(ADDON).await.remove(0);
    let binding = binding_form(&token, 1, &profile, None);
    assert!(fixture
        .post(BINDING_PATH, binding.clone())
        .await
        .ends_with("saved=binding"));
    let before = selections(&fixture).await;
    assert!(fixture
        .post(BINDING_PATH, binding)
        .await
        .ends_with("error=conflict"));
    assert_eq!(selections(&fixture).await, before);
    let mut edit = profile_form(&token, "Updated", 1, Some(&profile));
    replace(&mut edit, "setting.1.value", "new");
    assert!(fixture
        .post(PROFILE_PATH, edit.clone())
        .await
        .ends_with("saved=profile"));
    let updated = fixture.profiles(ADDON).await;
    assert!(fixture
        .post(PROFILE_PATH, edit)
        .await
        .ends_with("error=conflict"));
    assert_eq!(fixture.profiles(ADDON).await, updated);
    sqlx::query(
        r#"UPDATE "AddonInstallation" SET "StateRevision"='changed-state' WHERE "AddonId"=?"#,
    )
    .bind(ADDON)
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    let current = fixture.profiles(ADDON).await.remove(0);
    assert!(fixture
        .post(
            PROFILE_PATH,
            profile_form(&token, "Stale installation", 1, Some(&current))
        )
        .await
        .ends_with("error=conflict"));
    assert_eq!(fixture.profiles(ADDON).await, vec![current]);
}

#[tokio::test]
async fn off_retains_identity_when_installation_and_selected_profile_are_unavailable() {
    let mut fixture = Fixture::new().await;
    let token = fixture.token(ADDON).await;
    fixture
        .post(PROFILE_PATH, profile_form(&token, "Day", 1, None))
        .await;
    let profile = fixture.profiles(ADDON).await.remove(0);
    fixture
        .post(BINDING_PATH, binding_form(&token, 1, &profile, None))
        .await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"='invalid-private-fixture' WHERE "AddonId"=?"#).bind(ADDON).execute(&fixture.state.pool).await.unwrap();
    let before = config(&fixture).await;
    let read = load_channel_teletext_for_application(&fixture.state.application, 1)
        .await
        .unwrap()
        .unwrap();
    assert!(!read.catalog_available);
    let binding = read.addons[0].binding.as_ref().unwrap();
    assert!(binding.enabled);
    let off = vec![
        ("channel_id".to_owned(), "1".to_owned()),
        ("addon_id".to_owned(), ADDON.to_owned()),
        (
            "expected_binding_revision".to_owned(),
            binding.revision.to_string(),
        ),
    ];
    assert_eq!(
        fixture.post(OFF_PATH, off.clone()).await,
        "/channels/1/teletext?saved=off"
    );
    let after = selections(&fixture).await;
    assert!(!after[0].binding.enabled);
    assert_eq!(after[0].binding.profile_id, profile.id);
    assert!(fixture
        .post(OFF_PATH, off)
        .await
        .ends_with("error=conflict"));
    assert_eq!(config(&fixture).await, before);
    assert!(fixture.builds.try_recv().is_err());
}

#[tokio::test]
async fn malformed_duplicate_unknown_missing_and_foreign_profile_inputs_write_nothing() {
    let fixture = Fixture::new().await;
    let token = fixture.token(ADDON).await;
    fixture
        .post(PROFILE_PATH, profile_form(&token, "Day", 1, None))
        .await;
    let profile = fixture.profiles(ADDON).await.remove(0);
    let before = fixture.profiles(ADDON).await;
    for (key, value) in [
        ("setting_count", "257"),
        ("setting.0.mode", "unknown"),
        ("expected_installation", "{private-invalid-json"),
        ("profile_id", "1"),
        ("setting.1.key", "THEME"),
        ("channel_id", "999999"),
    ] {
        let mut input = profile_form(&token, "Invalid", 1, None);
        replace(&mut input, key, value);
        assert!(fixture.post(PROFILE_PATH, input).await.contains("error="));
        assert_eq!(fixture.profiles(ADDON).await, before);
    }
    for duplicate in [true, false] {
        let mut input = profile_form(&token, "Ambiguous", 1, None);
        input.push((
            if duplicate { "name" } else { "unknown" }.to_owned(),
            "private-marker".to_owned(),
        ));
        assert!(fixture
            .post(PROFILE_PATH, input)
            .await
            .ends_with("error=invalid"));
    }
    let mut missing = binding_form(&token, 1, &profile, None);
    replace(&mut missing, "profile_choice", "999999:1");
    assert!(fixture
        .post(BINDING_PATH, missing)
        .await
        .ends_with("error=missing-profile"));
    fixture.install(OTHER).await;
    let mut other = profile_form(&fixture.token(OTHER).await, "Other", 1, None);
    replace(&mut other, "addon_id", OTHER);
    assert!(fixture
        .post(PROFILE_PATH, other)
        .await
        .ends_with("saved=profile"));
    let foreign = fixture.profiles(OTHER).await.remove(0);
    assert!(fixture
        .post(BINDING_PATH, binding_form(&token, 1, &foreign, None))
        .await
        .ends_with("error=wrong-addon"));
    let mut wrong_edit = profile_form(&token, "Foreign", 1, Some(&foreign));
    replace(&mut wrong_edit, "setting.1.mode", "inherit");
    assert!(fixture
        .post(PROFILE_PATH, wrong_edit)
        .await
        .ends_with("error=wrong-addon"));
    assert_eq!(fixture.profiles(ADDON).await, before);
    assert!(selections(&fixture).await.is_empty());
}

#[tokio::test]
async fn unsupported_and_redacted_old_overrides_cannot_be_silently_removed() {
    let fixture = Fixture::new().await;
    let token = fixture.token(ADDON).await;
    fixture
        .post(PROFILE_PATH, profile_form(&token, "Retained", 1, None))
        .await;
    let profile = fixture.profiles(ADDON).await.remove(0);
    for key in ["REMOVED", "TOKEN"] {
        sqlx::query(r#"INSERT INTO "TeletextProfileSetting" ("ProfileId","Key","Kind","Value") VALUES (?,?,0,'private-retained-marker')"#)
            .bind(profile.id).bind(key).execute(&fixture.state.pool).await.unwrap();
        let before = fixture.profiles(ADDON).await;
        let read = load_channel_teletext_for_application(&fixture.state.application, 1)
            .await
            .unwrap()
            .unwrap();
        let bytes = serde_json::to_string(&read).unwrap();
        assert!(!bytes.contains("private-retained-marker"));
        assert!(read.addons[0].profiles[0]
            .settings
            .iter()
            .any(|setting| setting.key == key && setting.redacted));
        assert!(fixture
            .post(
                PROFILE_PATH,
                profile_form(&token, "Erase", 1, Some(&profile))
            )
            .await
            .ends_with("error=invalid"));
        assert_eq!(fixture.profiles(ADDON).await, before);
        sqlx::query(r#"DELETE FROM "TeletextProfileSetting" WHERE "ProfileId"=? AND "Key"=?"#)
            .bind(profile.id)
            .bind(key)
            .execute(&fixture.state.pool)
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn body_limits_reject_oversized_forms_before_mutation() {
    let fixture = Fixture::new().await;
    let before = config(&fixture).await;
    fixture
        .post_raw(
            PROFILE_PATH,
            "x".repeat(MAX_TELETEXT_FORM_BYTES + 1),
            StatusCode::PAYLOAD_TOO_LARGE,
        )
        .await;
    assert!(fixture.profiles(ADDON).await.is_empty());
    assert_eq!(config(&fixture).await, before);
}
