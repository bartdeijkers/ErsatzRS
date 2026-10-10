//! Public persistence proof only; no invocation, rendering or playback claim.

mod common;

use std::{collections::BTreeMap, path::Path};

use ersatzrs_addon_contract::{
    AddonCapability, AddonEntrypoint, AddonEntrypointKind, AddonManifest, AddonPermissions,
    AddonSettingDefinition, AddonSettingKind, ADDON_MANIFEST_VERSION,
};
use ersatzrs_core::repositories::{
    ChannelTeletextBindingSave, TeletextProfileMutation, TeletextProfileRepository,
    TeletextProfileSave,
};
use ersatzrs_domain::{
    ChannelTeletextBinding, TeletextProfile, TeletextProfileAvailability, TeletextProfileSetting,
    TeletextProfileValue,
};
use ersatzrs_infra::repositories::{make_repositories, SqliteTeletextProfileRepository};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};

const ADDON_ID: &str = "org.ersatzrs.teletext.restart-fixture";

async fn open_database(path: &Path) -> sqlx::SqlitePool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true),
        )
        .await
        .unwrap()
}

fn manifest() -> AddonManifest {
    AddonManifest {
        manifest_version: ADDON_MANIFEST_VERSION,
        id: ADDON_ID.to_owned(),
        version: semver::Version::new(1, 0, 0),
        name: BTreeMap::from([("en-US".to_owned(), "Synthetic profiles".to_owned())]),
        summary: BTreeMap::from([("en-US".to_owned(), "Persistence fixture".to_owned())]),
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
                path: "fixture.bin".to_owned(),
                kind: AddonEntrypointKind::Native,
            },
        )]),
        permissions: AddonPermissions::default(),
        settings: vec![AddonSettingDefinition {
            key: "THEME".to_owned(),
            label: BTreeMap::from([("en-US".to_owned(), "Theme".to_owned())]),
            kind: AddonSettingKind::Enum,
            required: true,
            default: Some("day".to_owned()),
            options: vec!["day".to_owned(), "night".to_owned()],
            suggested_reference: None,
        }],
        media_list_storage: None,
        default_source_hosts: Vec::new(),
        panels: Vec::new(),
    }
}

fn saved<T: std::fmt::Debug>(result: TeletextProfileMutation<T>) -> T {
    match result {
        TeletextProfileMutation::Saved(value) => value,
        TeletextProfileMutation::Rejected(reason) => panic!("unexpected rejection: {reason:?}"),
    }
}

async fn seed(pool: &sqlx::SqlitePool) {
    sqlx::migrate!("../../migrations").run(pool).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height")
           VALUES (1, '1920x1080', 1920, 1080)"#,
    )
    .execute(pool)
    .await
    .unwrap();
    let repos = make_repositories(pool.clone());
    common::seed_profile_and_channel(&repos, 1).await;
    common::seed_profile_and_channel(&repos, 2).await;
    let manifest = manifest();
    manifest.validate().unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonInstallation"
        ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
         "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision")
        VALUES (?, '1.0.0', 1, 'synthetic-package', 'synthetic-content', ?,
                'unused-fixture', 'now', 'now', 'fixture-revision')"#,
    )
    .bind(ADDON_ID)
    .bind(serde_json::to_string(&manifest).unwrap())
    .execute(pool)
    .await
    .unwrap();
}

async fn seed_bindings(
    repository: &SqliteTeletextProfileRepository,
) -> Vec<(TeletextProfile, ChannelTeletextBinding)> {
    let installation = repository.installation_snapshot(ADDON_ID).await.unwrap();
    assert_eq!(
        installation.availability,
        TeletextProfileAvailability::Configured
    );
    let identity = installation.snapshot.unwrap();
    let mut expected = Vec::new();
    for (channel_id, theme) in [(1, "day"), (2, "night")] {
        let result = saved(
            repository
                .save_profile(&TeletextProfileSave {
                    addon_id: ADDON_ID.to_owned(),
                    name: theme.to_owned(),
                    settings: vec![TeletextProfileSetting {
                        key: "THEME".to_owned(),
                        value: TeletextProfileValue::Literal(theme.to_owned()),
                    }],
                    expected_installation: identity.clone(),
                    existing: None,
                })
                .await
                .unwrap(),
        );
        let binding = saved(
            repository
                .save_binding(&ChannelTeletextBindingSave {
                    channel_id,
                    addon_id: ADDON_ID.to_owned(),
                    profile_id: result.profile.id,
                    enabled: true,
                    expected_installation: identity.clone(),
                    expected_profile_revision: result.profile.revision,
                    expected_binding_revision: None,
                })
                .await
                .unwrap(),
        );
        expected.push((result.profile, binding));
    }
    assert_eq!(
        repository
            .installation_snapshot(ADDON_ID)
            .await
            .unwrap()
            .snapshot,
        Some(identity)
    );
    expected
}

#[tokio::test]
async fn teletext_profiles_reopen_retains_distinct_channels_and_unavailable_off() {
    let temporary = tempfile::tempdir().unwrap();
    let path = temporary.path().join("profiles.db");
    let pool = open_database(&path).await;
    seed(&pool).await;
    let repository = SqliteTeletextProfileRepository(pool.clone());
    let expected = seed_bindings(&repository).await;
    drop(repository);
    pool.close().await;

    let pool = open_database(&path).await;
    let repository = SqliteTeletextProfileRepository(pool.clone());
    let profiles = repository.list_profiles(ADDON_ID).await.unwrap();
    assert_eq!(profiles.len(), 2);
    for (profile, binding) in &expected {
        let read = profiles
            .iter()
            .find(|read| read.profile.id == profile.id)
            .unwrap();
        assert_eq!(&read.profile, profile);
        assert_eq!(read.availability, TeletextProfileAvailability::Configured);
        let bindings = repository
            .channel_bindings(binding.channel_id)
            .await
            .unwrap();
        assert_eq!(bindings.len(), 1);
        assert_eq!(&bindings[0].binding, binding);
    }
    // Simulate a now-absent installation. The unit suite separately exercises
    // the real uninstall API; this test owns persistence across connection loss.
    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = ?"#)
        .bind(ADDON_ID)
        .execute(&pool)
        .await
        .unwrap();
    let off = saved(
        repository
            .disable_binding(1, ADDON_ID, expected[0].1.revision)
            .await
            .unwrap(),
    );
    assert!(!off.enabled);
    assert_eq!(
        repository.channel_bindings(2).await.unwrap()[0].binding,
        expected[1].1
    );
    drop(repository);
    pool.close().await;

    let pool = open_database(&path).await;
    let repository = SqliteTeletextProfileRepository(pool.clone());
    let first = repository.channel_bindings(1).await.unwrap();
    let second = repository.channel_bindings(2).await.unwrap();
    assert_eq!(first[0].binding, off);
    assert_eq!(second[0].binding, expected[1].1);
    assert_eq!(
        first[0].availability,
        TeletextProfileAvailability::MissingInstallation
    );
    assert_eq!(
        second[0].availability,
        TeletextProfileAvailability::MissingInstallation
    );
    assert_eq!(repository.list_profiles(ADDON_ID).await.unwrap().len(), 2);
    drop(repository);
    pool.close().await;
}
