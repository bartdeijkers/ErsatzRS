//! Native host-to-draft proof with original synthetic pages, not publication.
//! POSIX shell fixture intentionally runs only on Unix; Windows acceptance is separate.
#![cfg(unix)]

use std::{collections::BTreeMap, path::Path, time::Duration};

use chrono::{TimeZone as _, Utc};
use ersatzrs_addon_contract::{
    AddonCapability, AddonEntrypoint, AddonEntrypointKind, AddonManifest, AddonPermissions,
    AddonSettingDefinition, AddonSettingKind, ADDON_MANIFEST_VERSION,
};
use ersatzrs_app::teletext_guide::{
    build_request, SuppliedTeletextGuideContext, TeletextGuideChannel,
};
use ersatzrs_app::teletext_profiles::BoundTeletextPagesError;
use ersatzrs_core::{
    locking::EntityLocker,
    repositories::{
        ChannelRepository as _, ChannelTeletextBindingSave, FfmpegProfileRepository as _,
        TeletextProfileMutation, TeletextProfileRejection, TeletextProfileSave,
    },
    scheduling::PlayoutBuildRequest,
};
use ersatzrs_domain::{
    Channel, ChannelTeletextBinding, FFmpegProfile, TeletextColour, TeletextProfile,
    TeletextProfileSetting, TeletextProfileValue, TeletextRunContent,
};
use ersatzrs_infra::{
    addons::calculate_addon_content_sha256,
    db::{DatabaseBackend, DatabaseSchemaStatus},
    repositories::make_repositories,
};
use ersatzrs_web::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
use sqlx::{sqlite::SqlitePoolOptions, SqlitePool};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use uuid::Uuid;

const ADDON: &str = "org.ersatzrs.teletext.native-profile-fixture";
const GENERATION: &str = "native-profile-generation";
const SCRIPT: &str = r#"case "$1" in
check) printf '%s' '{"status":"ready","code":"ready","message":"ok"}'; exit 0;;
pages) ;;
*) exit 9;;
esac
test "$ERSATZRS_ADDON_CAPABILITY" = teletext.pages.v1 || exit 10
theme="$ERSATZRS_ADDON_SETTING_THEME"
case "$theme" in day) colour=yellow;; night) colour=cyan;; *) exit 11;; esac
cat > "../input-$theme.json"
printf '1\n' >> "../calls-$theme"
: > "../ready-$theme"
count=0
while test ! -f ../ready-day || test ! -f ../ready-night; do
  count=$((count + 1))
  test "$count" -lt 500 || exit 12
  sleep 0.01
done
generation=native-profile-generation
test "$ERSATZRS_ADDON_SETTING_RESPONSE" != bad-generation || generation=wrong-generation
printf '{"schema_version":1,"generation":"%s","language":"nld","initial_page":123,"pages":[{"address":{"page":123,"subcode":0},"kind":"full_page","language":"nld","national_option":0,"rows":[{"row":1,"runs":[{"column":0,"style":{"foreground":"%s","background":"black","height":"normal","boxed":false,"flash":false,"conceal":false},"content":{"kind":"text","text":"Profile %s"}}]}],"navigation":[{"role":"index","target":{"page":123,"subcode":null}}]}]}' "$generation" "$colour" "$theme"
"#;

fn definition(key: &str, default: &str, options: &[&str]) -> AddonSettingDefinition {
    AddonSettingDefinition {
        key: key.to_owned(),
        label: BTreeMap::from([("en-US".to_owned(), key.to_owned())]),
        kind: AddonSettingKind::Enum,
        required: true,
        default: Some(default.to_owned()),
        options: options.iter().map(|value| (*value).to_owned()).collect(),
        suggested_reference: None,
    }
}

fn manifest() -> AddonManifest {
    AddonManifest {
        manifest_version: ADDON_MANIFEST_VERSION,
        id: ADDON.to_owned(),
        version: semver::Version::new(1, 0, 0),
        name: BTreeMap::from([("en-US".to_owned(), "Native profile fixture".to_owned())]),
        summary: BTreeMap::from([("en-US".to_owned(), "Original synthetic pages".to_owned())]),
        icon: None,
        license: "GPL-3.0-or-later".to_owned(),
        source_url: "https://example.test/source".to_owned(),
        host_version: semver::VersionReq::STAR,
        capabilities: ["addon.check.v1", "teletext.pages.v1"]
            .map(|id| AddonCapability {
                id: id.to_owned(),
                options: Vec::new(),
            })
            .to_vec(),
        dependencies: Vec::new(),
        entrypoints: BTreeMap::from([(
            "any".to_owned(),
            AddonEntrypoint {
                path: "fixture.sh".to_owned(),
                kind: AddonEntrypointKind::PosixShell,
            },
        )]),
        permissions: AddonPermissions::default(),
        settings: vec![
            definition("THEME", "day", &["day", "night"]),
            definition("RESPONSE", "good", &["good", "bad-generation"]),
        ],
        media_list_storage: None,
        default_source_hosts: Vec::new(),
        panels: Vec::new(),
    }
}

async fn seed_channels(pool: &SqlitePool) {
    sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, '1920x1080', 1920, 1080)"#)
        .execute(pool).await.unwrap();
    let repositories = make_repositories(pool.clone());
    repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Synthetic".to_owned(),
            resolution_id: 1,
            ..FFmpegProfile::default()
        })
        .await
        .unwrap();
    for id in [1, 2] {
        repositories
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
}

async fn seed_package(pool: &SqlitePool, root: &Path) {
    let package = root.join("package");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(package.join("fixture.sh"), SCRIPT).unwrap();
    let manifest = manifest();
    manifest.validate().unwrap();
    let hash = calculate_addon_content_sha256(&package).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonInstallation"
        ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson",
         "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision")
        VALUES (?, '1.0.0', 0, 'synthetic-package', ?, ?, ?, 'now', 'now', 'before-enable')"#,
    )
    .bind(ADDON)
    .bind(hash)
    .bind(serde_json::to_string(&manifest).unwrap())
    .bind(package.to_str().unwrap())
    .execute(pool)
    .await
    .unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'THEME', 'day')"#,
    )
    .bind(ADDON)
    .execute(pool)
    .await
    .unwrap();
}

fn saved<T: std::fmt::Debug>(outcome: TeletextProfileMutation<T>) -> T {
    match outcome {
        TeletextProfileMutation::Saved(value) => value,
        TeletextProfileMutation::Rejected(reason) => panic!("unexpected rejection: {reason:?}"),
    }
}

struct Fixture {
    root: tempfile::TempDir,
    state: AppState,
    playout: UnboundedReceiver<PlayoutBuildRequest>,
}

impl Fixture {
    async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::query("PRAGMA foreign_keys=ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        seed_channels(&pool).await;
        seed_package(&pool, root.path()).await;
        let (sender, playout) = mpsc::unbounded_channel();
        let state = AppState::new(
            pool,
            DatabaseBackend::Sqlite,
            SystemStartup {
                database_ready: true,
                schema_status: DatabaseSchemaStatus::default(),
            },
            ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
            Uuid::nil(),
            2,
            IptvAccessTokenVerifier::disabled(),
            sender,
            EntityLocker::new(),
        );
        state
            .application
            .enable_addon(ADDON.to_owned())
            .await
            .unwrap();
        Self {
            root,
            state,
            playout,
        }
    }

    async fn profile(&self, name: &str, theme: &str, response: &str) -> TeletextProfile {
        let installation = self
            .state
            .application
            .teletext_installation_snapshot(ADDON)
            .await
            .unwrap()
            .snapshot
            .unwrap();
        saved(
            self.state
                .application
                .save_teletext_profile(&TeletextProfileSave {
                    addon_id: ADDON.to_owned(),
                    name: name.to_owned(),
                    settings: [("THEME", theme), ("RESPONSE", response)]
                        .map(|(key, value)| TeletextProfileSetting {
                            key: key.to_owned(),
                            value: TeletextProfileValue::Literal(value.to_owned()),
                        })
                        .to_vec(),
                    expected_installation: installation,
                    existing: None,
                })
                .await
                .unwrap(),
        )
        .profile
    }

    async fn bind(
        &self,
        channel: i64,
        profile: &TeletextProfile,
        revision: Option<i64>,
    ) -> ChannelTeletextBinding {
        saved(
            self.state
                .application
                .save_channel_teletext_binding(&ChannelTeletextBindingSave {
                    channel_id: channel,
                    addon_id: ADDON.to_owned(),
                    profile_id: profile.id,
                    enabled: true,
                    expected_installation: profile.validated_installation.clone(),
                    expected_profile_revision: profile.revision,
                    expected_binding_revision: revision,
                })
                .await
                .unwrap(),
        )
    }
}

fn guide<'a>(channels: &'a [TeletextGuideChannel<'a>]) -> SuppliedTeletextGuideContext<'a> {
    SuppliedTeletextGuideContext {
        generation: GENERATION,
        locale: "nl",
        supported_locales: &["nl"],
        timezone: "Europe/Amsterdam",
        captured_at: Utc.with_ymd_and_hms(2026, 3, 28, 12, 0, 0).unwrap(),
        channels,
    }
}

async fn prove_concurrent_profiles(fixture: &Fixture, context: &SuppliedTeletextGuideContext<'_>) {
    let day = fixture.profile("Day", "day", "good").await;
    let night = fixture.profile("Night", "night", "good").await;
    fixture.bind(1, &day, None).await;
    fixture.bind(2, &night, None).await;
    let application = &fixture.state.application;
    let (first, second) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            application.prepare_bound_teletext_pages(1, ADDON, 1, context),
            application.prepare_bound_teletext_pages(2, ADDON, 1, context)
        )
    })
    .await
    .unwrap();
    // Each child waits for both markers before returning. Success proves overlap.
    for (prepared, channel, profile, theme, colour) in [
        (first.unwrap(), 1, day, "day", TeletextColour::Yellow),
        (second.unwrap(), 2, night, "night", TeletextColour::Cyan),
    ] {
        assert_eq!(prepared.selection.channel_id, channel);
        assert_eq!(prepared.selection.profile_id, profile.id);
        assert_eq!(prepared.generation, GENERATION);
        let run = &prepared.draft.pages[0].rows[0].runs[0];
        assert_eq!(run.style.foreground, colour);
        assert_eq!(
            run.content,
            TeletextRunContent::Text {
                text: format!("Profile {theme}")
            }
        );
        assert_eq!(
            std::fs::read(fixture.root.path().join(format!("input-{theme}.json"))).unwrap(),
            build_request(context).unwrap()
        );
        assert_eq!(
            std::fs::read(fixture.root.path().join(format!("calls-{theme}"))).unwrap(),
            b"1\n"
        );
    }
}

async fn prove_off_and_rejected_response(
    fixture: &Fixture,
    context: &SuppliedTeletextGuideContext<'_>,
) {
    let application = &fixture.state.application;
    let second_before = application.channel_teletext_bindings(2).await.unwrap();
    let off = saved(
        application
            .disable_channel_teletext_binding(1, ADDON, 1)
            .await
            .unwrap(),
    );
    assert!(!off.enabled);
    assert!(matches!(
        application
            .prepare_bound_teletext_pages(1, ADDON, off.revision, context)
            .await,
        Err(BoundTeletextPagesError::SelectionRejected(
            TeletextProfileRejection::DisabledBinding
        ))
    ));
    assert_eq!(
        std::fs::read(fixture.root.path().join("calls-day")).unwrap(),
        b"1\n"
    );
    application
        .prepare_bound_teletext_pages(2, ADDON, 1, context)
        .await
        .unwrap();
    assert_eq!(
        application.channel_teletext_bindings(2).await.unwrap(),
        second_before
    );
    let bad = fixture
        .profile("Bad response", "day", "bad-generation")
        .await;
    let binding = fixture.bind(1, &bad, Some(off.revision)).await;
    assert!(matches!(
        application
            .prepare_bound_teletext_pages(1, ADDON, binding.revision, context)
            .await,
        Err(BoundTeletextPagesError::Response(
            ersatzrs_app::teletext_pages::TeletextPagesError::Contract(
                ersatzrs_addon_contract::TeletextPagesError::GenerationMismatch
            )
        ))
    ));
    assert_eq!(
        std::fs::read(fixture.root.path().join("calls-day")).unwrap(),
        b"1\n1\n"
    );
    // An invalid profile selection never replaces the existing binding.
    assert_eq!(
        application
            .save_channel_teletext_binding(&ChannelTeletextBindingSave {
                channel_id: 1,
                addon_id: ADDON.to_owned(),
                profile_id: i64::MAX,
                enabled: true,
                expected_installation: bad.validated_installation,
                expected_profile_revision: 1,
                expected_binding_revision: Some(binding.revision),
            })
            .await
            .unwrap(),
        TeletextProfileMutation::Rejected(TeletextProfileRejection::MissingProfile)
    );
    assert_eq!(
        application.channel_teletext_bindings(1).await.unwrap()[0].binding,
        binding
    );
}

#[tokio::test]
async fn native_profile_preparation_keeps_simultaneous_channels_isolated() {
    let mut fixture = Fixture::new().await;
    let installation = fixture
        .state
        .application
        .teletext_installation_snapshot(ADDON)
        .await
        .unwrap();
    let channels = [
        TeletextGuideChannel {
            id: 1,
            number: "1",
            name: "Synthetic one",
            programmes: &[],
        },
        TeletextGuideChannel {
            id: 2,
            number: "2",
            name: "Synthetic two",
            programmes: &[],
        },
    ];
    let context = guide(&channels);
    prove_concurrent_profiles(&fixture, &context).await;
    prove_off_and_rejected_response(&fixture, &context).await;
    assert_eq!(
        fixture
            .state
            .application
            .teletext_installation_snapshot(ADDON)
            .await
            .unwrap(),
        installation
    );
    let settings: Vec<(String, Option<String>)> =
        sqlx::query_as(r#"SELECT "Key", "Value" FROM "AddonSetting" WHERE "AddonId" = ?"#)
            .bind(ADDON)
            .fetch_all(&fixture.state.pool)
            .await
            .unwrap();
    assert_eq!(settings, vec![("THEME".to_owned(), Some("day".to_owned()))]);
    assert!(fixture.playout.try_recv().is_err());
    fixture.state.pool.close().await;
}
