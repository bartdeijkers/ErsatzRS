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
use ersatzrs_ffmpeg::teletext::source_header::{
    observe_dvb_teletext_header, TeletextHeaderObservation,
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
const SECONDARY: &str = "org.ersatzrs.teletext.native-profile-fixture.secondary";
const GENERATION: &str = "native-profile-generation";

#[path = "teletext_profile_preparation/authoring.rs"]
mod authoring;
#[path = "teletext_profile_preparation/authoring_metadata.rs"]
mod authoring_metadata;
#[path = "teletext_profile_preparation/channel_documents.rs"]
mod channel_documents;
#[path = "teletext_profile_preparation/client_capture.rs"]
mod client_capture;
#[path = "teletext_profile_preparation/designs.rs"]
mod designs;
#[path = "teletext_profile_preparation/documents.rs"]
mod documents;
#[path = "teletext_profile_preparation/editorial_imports.rs"]
mod editorial_imports;
#[path = "teletext_profile_preparation/effective_channel_documents.rs"]
mod effective_channel_documents;
#[path = "teletext_profile_preparation/effective_designs.rs"]
mod effective_designs;
#[path = "teletext_profile_preparation/export_receipts.rs"]
mod export_receipts;
#[path = "teletext_profile_preparation/generations.rs"]
mod generations;
#[path = "teletext_profile_preparation/guide_capture.rs"]
mod guide_capture;
#[path = "teletext_profile_preparation/guide_publications.rs"]
mod guide_publications;
#[path = "teletext_profile_preparation/native_exports.rs"]
mod native_exports;
#[path = "teletext_profile_preparation/ordinary_stream_flow.rs"]
mod ordinary_stream_flow;
#[path = "teletext_profile_preparation/output_candidates.rs"]
mod output_candidates;
#[path = "teletext_profile_preparation/preview.rs"]
mod preview;
#[path = "teletext_profile_preparation/producer_admission.rs"]
mod producer_admission;
#[path = "teletext_profile_preparation/publications.rs"]
mod publications;
#[path = "teletext_profile_preparation/scripted_controls.rs"]
mod scripted_controls;
#[path = "teletext_profile_preparation/work_admission.rs"]
mod work_admission;
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
printf '{"schema_version":1,"generation":"%s","language":"nld","initial_page":123,"pages":[{"address":{"page":123,"subcode":0},"kind":"full_page","language":"nld","national_option":0,"rows":[{"row":1,"runs":[{"column":1,"style":{"foreground":"%s","background":"black","height":"normal","boxed":false,"flash":false,"conceal":false},"content":{"kind":"text","text":"Profile %s"}}]}],"navigation":[{"role":"index","target":{"page":123,"subcode":null}}]}]}' "$generation" "$colour" "$theme"
"#;

const SECONDARY_SCRIPT: &str = r#"case "$1" in
check) printf '%s' '{"status":"ready","code":"ready","message":"ok"}'; exit 0;;
pages) ;;
*) exit 9;;
esac
test "$ERSATZRS_ADDON_CAPABILITY" = teletext.pages.v1 || exit 10
theme="$ERSATZRS_ADDON_SETTING_THEME"
case "$theme" in day) colour=red;; night) colour=green;; *) exit 11;; esac
cat > "../secondary-input-$theme.json"
printf '1\n' >> "../secondary-calls-$theme"
page=223
text="Second $theme"
case "$ERSATZRS_ADDON_SETTING_RESPONSE" in
good) ;;
unencodable) text='🚀';;
collision) page=123;;
*) exit 12;;
esac
printf '{"schema_version":1,"generation":"native-profile-generation","language":"fra","initial_page":%s,"pages":[{"address":{"page":%s,"subcode":0},"kind":"full_page","language":"fra","national_option":0,"rows":[{"row":1,"runs":[{"column":1,"style":{"foreground":"%s","background":"black","height":"normal","boxed":false,"flash":false,"conceal":false},"content":{"kind":"text","text":"%s"}}]}],"navigation":[{"role":"index","target":{"page":%s,"subcode":null}}]}]}' "$page" "$page" "$colour" "$text" "$page"
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

async fn install_secondary(fixture: &Fixture) {
    let package = fixture.root.path().join("secondary");
    std::fs::create_dir(&package).unwrap();
    std::fs::write(package.join("fixture.sh"), SECONDARY_SCRIPT).unwrap();
    let mut manifest = manifest();
    SECONDARY.clone_into(&mut manifest.id);
    manifest.settings[1] = definition("RESPONSE", "good", &["good", "unencodable", "collision"]);
    manifest.validate().unwrap();
    let hash = calculate_addon_content_sha256(&package).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonInstallation"
        ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson",
         "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision")
        VALUES (?, '1.0.0', 0, 'synthetic-secondary', ?, ?, ?, 'now', 'now', 'before-enable')"#,
    )
    .bind(SECONDARY)
    .bind(hash)
    .bind(serde_json::to_string(&manifest).unwrap())
    .bind(package.to_str().unwrap())
    .execute(&fixture.state.pool)
    .await
    .unwrap();
    fixture
        .state
        .application
        .enable_addon(SECONDARY.to_owned())
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
    async fn new_persistent() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = Self::open_persistent_pool(root.path()).await;
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        seed_channels(&pool).await;
        seed_package(&pool, root.path()).await;
        let (state, playout) = Self::state_from_pool(pool);
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

    async fn open_persistent_pool(root: &Path) -> SqlitePool {
        SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(
                sqlx::sqlite::SqliteConnectOptions::new()
                    .filename(root.join("designs.sqlite"))
                    .create_if_missing(true)
                    .foreign_keys(true),
            )
            .await
            .unwrap()
    }

    fn state_from_pool(pool: SqlitePool) -> (AppState, UnboundedReceiver<PlayoutBuildRequest>) {
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
        (state, playout)
    }

    async fn reopen(self) -> Self {
        let Self {
            root,
            state,
            playout,
        } = self;
        state.pool.close().await;
        drop(state);
        drop(playout);
        let pool = Self::open_persistent_pool(root.path()).await;
        let (state, playout) = Self::state_from_pool(pool);
        Self {
            root,
            state,
            playout,
        }
    }

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
        self.profile_for(ADDON, name, theme, response).await
    }

    async fn profile_for(
        &self,
        addon_id: &str,
        name: &str,
        theme: &str,
        response: &str,
    ) -> TeletextProfile {
        let installation = self
            .state
            .application
            .teletext_installation_snapshot(addon_id)
            .await
            .unwrap()
            .snapshot
            .unwrap();
        saved(
            self.state
                .application
                .save_teletext_profile(&TeletextProfileSave {
                    addon_id: addon_id.to_owned(),
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
                    addon_id: profile.addon_id.clone(),
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

type NativeCandidate = ersatzrs_web::host::teletext::EncodedChannelTeletextCandidate;

async fn configure_composed_profiles(fixture: &Fixture) {
    install_secondary(fixture).await;
    for (channel, theme) in [(1, "day"), (2, "night")] {
        for addon in [ADDON, SECONDARY] {
            let profile = fixture.profile_for(addon, theme, theme, "good").await;
            fixture.bind(channel, &profile, None).await;
        }
    }
}

async fn native_candidate(
    fixture: &Fixture,
    channel: i64,
    context: &SuppliedTeletextGuideContext<'_>,
) -> Result<NativeCandidate, ersatzrs_web::host::teletext::ChannelTeletextEncodingError> {
    ersatzrs_web::host::teletext::prepare_channel_teletext_candidate(
        &fixture.state.application,
        channel,
        Some(SECONDARY),
        context,
        ersatzrs_ffmpeg::teletext::TeletextEncodingRegion::WesternLatin,
    )
    .await
}

fn assert_native_pages(candidate: &NativeCandidate, channel: i64, theme: &str, colours: [u8; 2]) {
    assert_eq!(candidate.selection().channel_id, channel);
    assert_eq!(candidate.selection().bindings.len(), 2);
    assert_eq!(candidate.entry_addon_id(), Some(SECONDARY));
    assert_eq!(candidate.generation(), GENERATION);
    let draft = candidate.draft().unwrap();
    let encoded = candidate.encoded().unwrap();
    assert_eq!(draft.initial_page.get(), 223);
    assert_eq!(encoded.initial_page(), draft.initial_page);
    assert_eq!(encoded.language(), &draft.language);
    assert_eq!(draft.language.as_str(), "fra");
    assert_eq!(draft.pages.len(), 2);
    assert_eq!(encoded.pages().len(), 2);
    for (index, prefix) in ["Profile", "Second"].iter().enumerate() {
        let page = &draft.pages[index];
        let native = &encoded.pages()[index];
        assert_eq!(native.address(), page.address);
        assert_eq!(native.language(), &page.language);
        assert_eq!(page.language.as_str(), ["nld", "fra"][index]);
        let text = format!("{prefix} {theme}");
        assert_eq!(
            page.rows[0].runs[0].content,
            TeletextRunContent::Text { text: text.clone() }
        );
        // Independently inspect the physical row: odd parity, preceding alpha
        // colour-control cell and exact ASCII glyphs after DVB bit reversal.
        let unit = native
            .data_units()
            .iter()
            .find(|unit| {
                matches!(
                    observe_dvb_teletext_header(unit),
                    Ok(TeletextHeaderObservation::NotHeader(address))
                        if address.packet_number == 1
                            && address.wire_magazine == native.address().page.wire_magazine()
                )
            })
            .unwrap();
        let physical = &unit[6..];
        assert!(physical.iter().all(|byte| byte.count_ones() % 2 == 1));
        let cells: Vec<_> = physical
            .iter()
            .map(|byte| byte.reverse_bits() & 0x7f)
            .collect();
        assert_eq!(cells[0], colours[index]);
        assert_eq!(&cells[1..=text.len()], text.as_bytes());
    }
    // Exactly the explicit French initial page 223 is advertised as display.
    assert_eq!(
        candidate.descriptor().unwrap().bytes(),
        b"\x56\x05fra\x0a\x23"
    );
}

async fn prove_native_rejections(fixture: &Fixture, context: &SuppliedTeletextGuideContext<'_>) {
    use ersatzrs_app::teletext_channels::ChannelTeletextPagesError;
    use ersatzrs_ffmpeg::teletext::TeletextEncodingError;
    use ersatzrs_web::host::teletext::ChannelTeletextEncodingError as Error;

    let unencodable = fixture
        .profile_for(SECONDARY, "Unencodable", "day", "unencodable")
        .await;
    let binding = fixture.bind(1, &unencodable, Some(1)).await;
    assert!(matches!(
        native_candidate(fixture, 1, context).await,
        Err(Error::Encoding(TeletextEncodingError::Character {
            codepoint: 0x1f680,
            ..
        }))
    ));
    let collision = fixture
        .profile_for(SECONDARY, "Collision", "day", "collision")
        .await;
    fixture.bind(1, &collision, Some(binding.revision)).await;
    assert!(matches!(
        native_candidate(fixture, 1, context).await,
        Err(Error::Preparation(
            ChannelTeletextPagesError::AddressCollision
        ))
    ));
    // Both failures require actual secondary producer execution. No partial
    // native candidate is returned, and the other channel retains its own set.
    assert_eq!(
        std::fs::read(fixture.root.path().join("secondary-calls-day")).unwrap(),
        b"1\n1\n1\n"
    );
    assert_native_pages(
        &native_candidate(fixture, 2, context).await.unwrap(),
        2,
        "night",
        [6, 2],
    );
}

async fn prove_native_off(fixture: &Fixture, context: &SuppliedTeletextGuideContext<'_>) {
    let application = &fixture.state.application;
    for selected in application.channel_teletext_bindings(1).await.unwrap() {
        saved(
            application
                .disable_channel_teletext_binding(
                    1,
                    &selected.binding.addon_id,
                    selected.binding.revision,
                )
                .await
                .unwrap(),
        );
    }
    let calls = std::fs::read(fixture.root.path().join("secondary-calls-day")).unwrap();
    let disabled = native_candidate(fixture, 1, context).await.unwrap();
    assert!(disabled.selection().bindings.is_empty());
    assert!(disabled.entry_addon_id().is_none());
    assert!(disabled.draft().is_none());
    assert!(disabled.encoded().is_none());
    assert!(disabled.descriptor().is_none());
    assert_eq!(
        std::fs::read(fixture.root.path().join("secondary-calls-day")).unwrap(),
        calls
    );
}

#[tokio::test]
async fn native_channel_preparation_encodes_complete_multi_addon_candidates() {
    let mut fixture = Fixture::new().await;
    configure_composed_profiles(&fixture).await;
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
    let (left, right) = tokio::time::timeout(Duration::from_secs(10), async {
        tokio::join!(
            native_candidate(&fixture, 1, &context),
            native_candidate(&fixture, 2, &context)
        )
    })
    .await
    .unwrap();
    assert_native_pages(&left.unwrap(), 1, "day", [3, 1]);
    assert_native_pages(&right.unwrap(), 2, "night", [6, 2]);
    for theme in ["day", "night"] {
        assert_eq!(
            std::fs::read(
                fixture
                    .root
                    .path()
                    .join(format!("secondary-input-{theme}.json"))
            )
            .unwrap(),
            build_request(&context).unwrap()
        );
    }
    prove_native_rejections(&fixture, &context).await;
    prove_native_off(&fixture, &context).await;
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
