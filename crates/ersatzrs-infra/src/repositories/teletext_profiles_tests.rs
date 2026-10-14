use std::collections::BTreeMap;

use ersatzrs_addon_contract::{
    AddonCapability, AddonDependency, AddonEntrypoint, AddonEntrypointKind, AddonManifest,
    AddonPermissions, AddonSettingDefinition, AddonSettingKind, ADDON_MANIFEST_VERSION,
};
use ersatzrs_core::repositories::{ChannelRepository, FfmpegProfileRepository};
use ersatzrs_domain::{
    Channel, FFmpegProfile, TeletextPageContract, MAX_TELETEXT_PROFILE_VALUE_BYTES,
};
use semver::{Version, VersionReq};
use sqlx::sqlite::SqlitePoolOptions;

use super::*;

const ADDON: &str = "org.example.teletext.profile-fixture";

fn definition(key: &str, kind: AddonSettingKind, default: Option<&str>) -> AddonSettingDefinition {
    AddonSettingDefinition {
        key: key.to_owned(),
        label: BTreeMap::from([("en-US".to_owned(), key.to_owned())]),
        kind,
        required: true,
        default: default.map(str::to_owned),
        options: if kind == AddonSettingKind::Enum {
            vec!["day".to_owned(), "night".to_owned()]
        } else {
            Vec::new()
        },
        suggested_reference: None,
    }
}

fn manifest() -> AddonManifest {
    AddonManifest {
        manifest_version: ADDON_MANIFEST_VERSION,
        id: ADDON.to_owned(),
        version: Version::new(1, 0, 0),
        name: BTreeMap::from([("en-US".to_owned(), "Synthetic profile".to_owned())]),
        summary: BTreeMap::from([("en-US".to_owned(), "Configuration fixture".to_owned())]),
        icon: None,
        license: "GPL-3.0-or-later".to_owned(),
        source_url: "https://example.test/source".to_owned(),
        host_version: VersionReq::STAR,
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
        settings: vec![
            definition("THEME", AddonSettingKind::Enum, Some("day")),
            definition("ZONE", AddonSettingKind::String, Some("Default")),
        ],
        media_list_storage: None,
        default_source_hosts: Vec::new(),
        panels: Vec::new(),
    }
}

fn literal(key: &str, value: &str) -> TeletextProfileSetting {
    TeletextProfileSetting {
        key: key.to_owned(),
        value: TeletextProfileValue::Literal(value.to_owned()),
    }
}

fn saved<T: std::fmt::Debug>(mutation: M<T>) -> T {
    match mutation {
        M::Saved(value) => value,
        M::Rejected(reason) => panic!("unexpected control rejection {reason:?}"),
    }
}

struct Fixture {
    repo: SqliteTeletextProfileRepository,
    manifest: AddonManifest,
}

impl Fixture {
    async fn new() -> Self {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        Self::from_pool(pool).await
    }

    async fn from_pool(pool: DbPool) -> Self {
        sqlx::query("PRAGMA foreign_keys = ON")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'fixture', 1920, 1080)"#).execute(&pool).await.unwrap();
        let repos = super::super::make_repositories(pool.clone());
        repos
            .ffmpeg_profile
            .upsert(&FFmpegProfile {
                id: 1,
                name: "Fixture".to_owned(),
                resolution_id: 1,
                ..FFmpegProfile::default()
            })
            .await
            .unwrap();
        for id in 1..=3 {
            repos
                .channel
                .upsert(&Channel {
                    id,
                    unique_id: uuid::Uuid::new_v4(),
                    number: id.to_string(),
                    sort_number: f64::from(i32::try_from(id).unwrap()),
                    name: format!("Fixture {id}"),
                    ffmpeg_profile_id: 1,
                    ..Channel::default()
                })
                .await
                .unwrap();
        }
        let manifest = manifest();
        insert_installation(&pool, &manifest).await;
        sqlx::query(r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'ZONE', 'Inherited')"#).bind(ADDON).execute(&pool).await.unwrap();
        Self {
            repo: SqliteTeletextProfileRepository(pool),
            manifest,
        }
    }

    async fn token(&self) -> TeletextInstallationSnapshot {
        self.repo
            .installation_snapshot(ADDON)
            .await
            .unwrap()
            .snapshot
            .unwrap()
    }

    async fn profile(&self, name: &str, theme: &str) -> TeletextProfile {
        saved(
            self.repo
                .save_profile(&self.save(name, vec![literal("THEME", theme)]).await)
                .await
                .unwrap(),
        )
        .profile
    }

    async fn save(&self, name: &str, settings: Vec<TeletextProfileSetting>) -> TeletextProfileSave {
        TeletextProfileSave {
            addon_id: ADDON.to_owned(),
            name: name.to_owned(),
            settings,
            expected_installation: self.token().await,
            existing: None,
        }
    }

    async fn bind(&self, channel: i64, profile: &TeletextProfile) -> ChannelTeletextBinding {
        saved(
            self.repo
                .save_binding(&Self::binding_save(channel, profile))
                .await
                .unwrap(),
        )
    }

    fn binding_save(channel: i64, profile: &TeletextProfile) -> ChannelTeletextBindingSave {
        ChannelTeletextBindingSave {
            channel_id: channel,
            addon_id: profile.addon_id.clone(),
            profile_id: profile.id,
            enabled: true,
            expected_installation: profile.validated_installation.clone(),
            expected_profile_revision: profile.revision,
            expected_binding_revision: None,
        }
    }

    async fn store_manifest(&self, manifest: &AddonManifest) {
        sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson" = ? WHERE "AddonId" = ?"#)
            .bind(serde_json::to_string(manifest).unwrap())
            .bind(&manifest.id)
            .execute(&self.repo.0)
            .await
            .unwrap();
    }
}

#[path = "teletext_profile_candidates_tests.rs"]
mod candidates;

#[path = "playout/teletext_intervals_tests.rs"]
mod intervals;

#[path = "teletext_effective_selection_tests.rs"]
mod effective;

async fn insert_installation(pool: &DbPool, manifest: &AddonManifest) {
    manifest.validate().unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision")
        VALUES (?, ?, 1, 'fixture-package', 'fixture-content', ?, 'unused-fixture', 'now', 'now', 'fixture-revision')"#)
        .bind(&manifest.id).bind(manifest.version.to_string()).bind(serde_json::to_string(manifest).unwrap()).execute(pool).await.unwrap();
}

#[tokio::test]
async fn profile_contract_uses_highest_supported_capability_in_same_snapshot() {
    for (capabilities, expected) in [
        (vec!["teletext.pages.v1"], TeletextPageContract::V1),
        (vec!["teletext.pages.v2"], TeletextPageContract::V2),
        (
            vec!["teletext.pages.v1", "teletext.pages.v2"],
            TeletextPageContract::V2,
        ),
    ] {
        let fixture = Fixture::new().await;
        let mut manifest = fixture.manifest.clone();
        manifest.capabilities = std::iter::once("addon.check.v1")
            .chain(capabilities)
            .map(|id| AddonCapability {
                id: id.to_owned(),
                options: Vec::new(),
            })
            .collect();
        fixture.store_manifest(&manifest).await;
        let profile = fixture.profile("Selected contract", "night").await;
        let binding = fixture.bind(1, &profile).await;
        let selection = saved(
            fixture
                .repo
                .invocation_snapshot(1, ADDON, binding.revision)
                .await
                .unwrap(),
        );
        assert_eq!(selection.contract, expected);
        assert_eq!(selection.installation, profile.validated_installation);
        assert_eq!(selection.effective_settings[0], literal("THEME", "night"));
        assert_eq!(
            selection.effective_settings[1],
            literal("ZONE", "Inherited")
        );
        assert_eq!(
            saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap()).bindings,
            vec![selection]
        );
        assert_eq!(
            fixture.repo.list_profiles(ADDON).await.unwrap()[0].availability,
            A::Configured
        );
    }
}

#[tokio::test]
async fn adding_v2_capability_requires_existing_installation_revalidation() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Legacy", "day").await;
    let binding = fixture.bind(1, &profile).await;
    let original = saved(
        fixture
            .repo
            .invocation_snapshot(1, ADDON, binding.revision)
            .await
            .unwrap(),
    );
    assert_eq!(original.contract, TeletextPageContract::V1);
    let mut manifest = fixture.manifest.clone();
    manifest.capabilities.push(AddonCapability {
        id: "teletext.pages.v2".to_owned(),
        options: Vec::new(),
    });
    fixture.store_manifest(&manifest).await;
    assert_eq!(
        fixture
            .repo
            .invocation_snapshot(1, ADDON, binding.revision)
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    assert_eq!(
        fixture.repo.list_profiles(ADDON).await.unwrap()[0].availability,
        A::StaleInstallation
    );
    let current = fixture.token().await;
    assert_eq!(current.state_revision, original.installation.state_revision);
    assert_eq!(current.content_sha256, original.installation.content_sha256);
    assert_ne!(
        current.manifest_sha256,
        original.installation.manifest_sha256
    );
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].binding,
        binding
    );
}

#[tokio::test]
async fn channel_profiles_and_shared_edits_isolate_exact_dependents_without_global_writes() {
    let fixture = Fixture::new().await;
    let original = fixture.token().await;
    let day = fixture.profile("Day", "day").await;
    let night = fixture.profile("Night", "night").await;
    fixture.bind(1, &day).await;
    fixture.bind(2, &night).await;
    fixture.bind(3, &day).await;
    let mut save = fixture
        .save(
            "Day",
            vec![
                literal("THEME", "night"),
                literal("ZONE", "Shared custom text"),
            ],
        )
        .await;
    save.existing = Some((day.id, day.revision));
    let changed = saved(fixture.repo.save_profile(&save).await.unwrap());
    assert_eq!(changed.dependent_bindings_changed, 2);
    assert_eq!(changed.profile.revision, 2);
    for (channel, revision, profile) in [(1, 2, day.id), (2, 1, night.id), (3, 2, day.id)] {
        let rows = fixture.repo.channel_bindings(channel).await.unwrap();
        assert_eq!(rows[0].binding.revision, revision);
        assert_eq!(rows[0].binding.profile_id, profile);
        assert_eq!(rows[0].availability, A::Configured);
    }
    assert_eq!(fixture.token().await, original);
    assert_eq!(
        crate::addons::load_addon_settings(&fixture.repo.0, ADDON)
            .await
            .unwrap()[0]
            .value
            .as_deref(),
        Some("Inherited")
    );
    let mut other = fixture.manifest.clone();
    other.id = "org.example.teletext.other-fixture".to_owned();
    insert_installation(&fixture.repo.0, &other).await;
    let token = fixture
        .repo
        .installation_snapshot(&other.id)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    let second = saved(
        fixture
            .repo
            .save_profile(&TeletextProfileSave {
                addon_id: other.id.clone(),
                name: "Other".to_owned(),
                settings: Vec::new(),
                expected_installation: token,
                existing: None,
            })
            .await
            .unwrap(),
    )
    .profile;
    fixture.bind(1, &second).await;
    assert_eq!(fixture.repo.channel_bindings(1).await.unwrap().len(), 2);
}

#[tokio::test]
async fn every_installation_identity_field_is_compared_before_profile_or_binding_mutation() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    let original = fixture.token().await;
    for field in 0..6 {
        let mut token = original.clone();
        match field {
            0 => token.addon_id.push('x'),
            1 => token.state_revision.push('x'),
            2 => token.version.push('x'),
            3 => token.package_sha256.push('x'),
            4 => token.content_sha256.push('x'),
            _ => token.manifest_sha256.push('x'),
        }
        let mut save = fixture.save("New", Vec::new()).await;
        save.expected_installation = token.clone();
        let expected = if field == 0 {
            R::WrongAddon
        } else {
            R::Conflict
        };
        assert_eq!(
            fixture.repo.save_profile(&save).await.unwrap(),
            M::Rejected(expected)
        );
        let mut binding = Fixture::binding_save(1, &profile);
        binding.expected_installation = token;
        assert_eq!(
            fixture.repo.save_binding(&binding).await.unwrap(),
            M::Rejected(expected)
        );
    }
    assert_eq!(fixture.repo.list_profiles(ADDON).await.unwrap().len(), 1);
    assert!(fixture.repo.channel_bindings(1).await.unwrap().is_empty());
    assert_eq!(fixture.token().await, original);
}

#[tokio::test]
async fn rejected_edits_leave_settings_selections_and_revisions_unchanged() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    let binding = fixture.bind(1, &profile).await;
    let baseline = fixture.repo.list_profiles(ADDON).await.unwrap();
    let mut save = fixture
        .save("Renamed", vec![literal("THEME", "invalid")])
        .await;
    save.existing = Some((profile.id, profile.revision));
    assert_eq!(
        fixture.repo.save_profile(&save).await.unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    save.settings = vec![literal("THEME", "night")];
    save.existing = Some((profile.id, 0));
    assert_eq!(
        fixture.repo.save_profile(&save).await.unwrap(),
        M::Rejected(R::Conflict)
    );
    let mut change = Fixture::binding_save(1, &profile);
    assert_eq!(
        fixture.repo.save_binding(&change).await.unwrap(),
        M::Rejected(R::Conflict)
    );
    change.expected_binding_revision = Some(binding.revision);
    change.expected_profile_revision = 0;
    assert_eq!(
        fixture.repo.save_binding(&change).await.unwrap(),
        M::Rejected(R::Conflict)
    );
    assert_eq!(fixture.repo.list_profiles(ADDON).await.unwrap(), baseline);
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].binding,
        binding
    );
    let duplicate = fixture.save("dAy", Vec::new()).await;
    assert_eq!(
        fixture.repo.save_profile(&duplicate).await.unwrap(),
        M::Rejected(R::NameConflict)
    );
}

#[tokio::test]
async fn ownership_missing_parents_tombstone_restrict_and_channel_cascade_are_explicit() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    let binding = fixture.bind(1, &profile).await;
    let mut change = Fixture::binding_save(2, &profile);
    change.channel_id = 99;
    assert_eq!(
        fixture.repo.save_binding(&change).await.unwrap(),
        M::Rejected(R::MissingChannel)
    );
    change.channel_id = 2;
    change.profile_id = 99;
    assert_eq!(
        fixture.repo.save_binding(&change).await.unwrap(),
        M::Rejected(R::MissingProfile)
    );
    assert_eq!(
        fixture
            .repo
            .tombstone_profile("org.example.wrong-addon", profile.id, 1)
            .await
            .unwrap(),
        M::Rejected(R::WrongAddon)
    );
    assert!(sqlx::query(r#"INSERT INTO "ChannelTeletextBinding" ("ChannelId", "AddonId", "ProfileId") VALUES (2, 'org.example.wrong-addon', ?)"#).bind(profile.id).execute(&fixture.repo.0).await.is_err());
    assert!(
        sqlx::query(r#"DELETE FROM "TeletextProfile" WHERE "Id" = ?"#)
            .bind(profile.id)
            .execute(&fixture.repo.0)
            .await
            .is_err()
    );
    let deleted = saved(
        fixture
            .repo
            .tombstone_profile(ADDON, profile.id, 1)
            .await
            .unwrap(),
    );
    assert_eq!(deleted.dependent_bindings_changed, 1);
    assert!(deleted.profile.deleted);
    assert_eq!(deleted.profile.settings, profile.settings);
    let row = &fixture.repo.channel_bindings(1).await.unwrap()[0];
    assert_eq!(row.availability, A::DeletedProfile);
    assert_eq!(row.binding.profile_id, binding.profile_id);
    assert_eq!(row.binding.revision, 2);
    assert_eq!(
        fixture
            .repo
            .save_binding(&Fixture::binding_save(2, &deleted.profile))
            .await
            .unwrap(),
        M::Rejected(R::DeletedProfile)
    );
    saved(fixture.repo.disable_binding(1, ADDON, 2).await.unwrap());
    sqlx::query(r#"DELETE FROM "Channel" WHERE "Id" = 1"#)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert!(fixture.repo.channel_bindings(1).await.unwrap().is_empty());
    assert!(
        fixture.repo.list_profiles(ADDON).await.unwrap()[0]
            .profile
            .deleted
    );
    assert!(
        !saved(
            fixture
                .repo
                .save_profile(&fixture.save("Day", Vec::new()).await)
                .await
                .unwrap()
        )
        .profile
        .deleted
    );
}

#[tokio::test]
async fn post_write_storage_failure_rolls_back_profile_settings_and_dependent_revisions() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    let binding = fixture.bind(1, &profile).await;
    let token = fixture.token().await;
    let globals = crate::addons::load_addon_settings(&fixture.repo.0, ADDON)
        .await
        .unwrap();
    sqlx::query(
        r#"CREATE TRIGGER "FixtureFailInvalidation" BEFORE UPDATE ON "ChannelTeletextBinding"
        BEGIN SELECT RAISE(ABORT, 'synthetic write failure'); END"#,
    )
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    let mut save = fixture
        .save(
            "Renamed",
            vec![literal("THEME", "night"), literal("ZONE", "Replaced")],
        )
        .await;
    save.existing = Some((profile.id, profile.revision));
    assert!(fixture.repo.save_profile(&save).await.is_err());
    assert_eq!(
        fixture.repo.list_profiles(ADDON).await.unwrap()[0].profile,
        profile
    );
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].binding,
        binding
    );
    assert_eq!(fixture.token().await, token);
    assert_eq!(
        crate::addons::load_addon_settings(&fixture.repo.0, ADDON)
            .await
            .unwrap(),
        globals
    );
}

#[tokio::test]
async fn unavailable_binding_can_turn_off_but_rejected_disable_preserves_it() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    let binding = fixture.bind(1, &profile).await;
    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = ?"#)
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].availability,
        A::MissingInstallation
    );
    assert_eq!(
        fixture.repo.disable_binding(99, ADDON, 1).await.unwrap(),
        M::Rejected(R::MissingChannel)
    );
    assert_eq!(
        fixture.repo.disable_binding(2, ADDON, 1).await.unwrap(),
        M::Rejected(R::MissingBinding)
    );
    assert_eq!(
        fixture.repo.disable_binding(1, ADDON, 0).await.unwrap(),
        M::Rejected(R::Conflict)
    );
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].binding,
        binding
    );
    let off = saved(fixture.repo.disable_binding(1, ADDON, 1).await.unwrap());
    assert!(!off.enabled);
    assert_eq!(off.profile_id, profile.id);
    assert_eq!(off.revision, 2);
    assert_eq!(
        fixture
            .repo
            .save_binding(&Fixture::binding_save(2, &profile))
            .await
            .unwrap(),
        M::Rejected(R::InstallationUnavailable)
    );
}

#[tokio::test]
async fn revision_limits_reject_every_increment_without_partial_changes_or_real_coercion() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    fixture.bind(1, &profile).await;
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Revision" = ?"#)
        .bind(i64::MAX)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    let mut save = fixture
        .save("Rename", vec![literal("THEME", "night")])
        .await;
    save.existing = Some((profile.id, 1));
    assert_eq!(
        fixture.repo.save_profile(&save).await.unwrap(),
        M::Rejected(R::RevisionLimit)
    );
    assert_eq!(
        fixture
            .repo
            .tombstone_profile(ADDON, profile.id, 1)
            .await
            .unwrap(),
        M::Rejected(R::RevisionLimit)
    );
    assert_eq!(
        fixture.repo.list_profiles(ADDON).await.unwrap()[0].profile,
        profile
    );
    assert_eq!(
        fixture
            .repo
            .disable_binding(1, ADDON, i64::MAX)
            .await
            .unwrap(),
        M::Rejected(R::RevisionLimit)
    );
    let mut binding = Fixture::binding_save(1, &profile);
    binding.expected_binding_revision = Some(i64::MAX);
    assert_eq!(
        fixture.repo.save_binding(&binding).await.unwrap(),
        M::Rejected(R::RevisionLimit)
    );
    sqlx::query(r#"UPDATE "TeletextProfile" SET "Revision" = ? WHERE "Id" = ?"#)
        .bind(i64::MAX)
        .bind(profile.id)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    save.existing = Some((profile.id, i64::MAX));
    assert_eq!(
        fixture.repo.save_profile(&save).await.unwrap(),
        M::Rejected(R::RevisionLimit)
    );
    assert_eq!(
        fixture
            .repo
            .tombstone_profile(ADDON, profile.id, i64::MAX)
            .await
            .unwrap(),
        M::Rejected(R::RevisionLimit)
    );
    let kinds: (String, String) = sqlx::query_as(r#"SELECT TYPEOF("profile"."Revision"), TYPEOF("binding"."Revision") FROM "TeletextProfile" AS "profile" JOIN "ChannelTeletextBinding" AS "binding" ON "binding"."ProfileId" = "profile"."Id""#).fetch_one(&fixture.repo.0).await.unwrap();
    assert_eq!(kinds, ("integer".to_owned(), "integer".to_owned()));
}

#[tokio::test]
async fn profile_settings_validate_duplicates_unknown_types_defaults_and_inherited_values() {
    let fixture = Fixture::new().await;
    for (settings, rejection) in [
        (
            vec![literal("THEME", "day"), literal("THEME", "night")],
            R::InvalidValues,
        ),
        (vec![literal("UNKNOWN", "value")], R::InvalidSettings),
        (vec![literal("THEME", "invalid")], R::InvalidSettings),
        (vec![literal("ZONE", "")], R::InvalidSettings),
    ] {
        assert_eq!(
            fixture
                .repo
                .save_profile(&fixture.save("Rejected", settings).await)
                .await
                .unwrap(),
            M::Rejected(rejection)
        );
    }
    sqlx::query(
        r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'THEME', 'invalid')"#,
    )
    .bind(ADDON)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Rejected", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    let corrected = fixture.profile("Override", "night").await;
    assert_eq!(corrected.settings, vec![literal("THEME", "night")]);
    sqlx::query(r#"DELETE FROM "AddonSetting" WHERE "Key" = 'THEME'"#)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    let mut invalid_default = fixture.manifest.clone();
    invalid_default.settings[0].default = Some("invalid".to_owned());
    fixture.store_manifest(&invalid_default).await;
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Rejected", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    let mut missing = fixture.manifest.clone();
    missing.settings[0].default = None;
    fixture.store_manifest(&missing).await;
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Rejected", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
}

#[tokio::test]
async fn effective_and_persisted_budgets_include_defaults_inheritance_and_update_rows() {
    let fixture = Fixture::new().await;
    let too_large = "x".repeat(MAX_TELETEXT_PROFILE_VALUE_BYTES + 1);
    assert_eq!(
        fixture
            .repo
            .save_profile(
                &fixture
                    .save("Rejected", vec![literal("ZONE", &too_large)])
                    .await
            )
            .await
            .unwrap(),
        M::Rejected(R::InvalidValues)
    );
    sqlx::query(r#"UPDATE "AddonSetting" SET "Value" = ? WHERE "AddonId" = ?"#)
        .bind(&too_large)
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Rejected", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    sqlx::query(r#"DELETE FROM "AddonSetting" WHERE "AddonId" = ?"#)
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    let mut oversized = fixture.manifest.clone();
    oversized.settings[1].default = Some(too_large);
    fixture.store_manifest(&oversized).await;
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Rejected", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    let mut aggregate = fixture.manifest.clone();
    aggregate.settings = (0..4)
        .map(|i| {
            definition(
                &format!("K{i}"),
                AddonSettingKind::String,
                Some(&"x".repeat(MAX_TELETEXT_PROFILE_VALUE_BYTES - 2)),
            )
        })
        .collect();
    fixture.store_manifest(&aggregate).await;
    let profile = saved(
        fixture
            .repo
            .save_profile(&fixture.save("Exact", Vec::new()).await)
            .await
            .unwrap(),
    )
    .profile;
    aggregate.settings[0].default.as_mut().unwrap().push('x');
    fixture.store_manifest(&aggregate).await;
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Over", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    for i in 0..4 {
        sqlx::query(r#"INSERT INTO "TeletextProfileSetting" ("ProfileId", "Key", "Kind", "Value") VALUES (?, ?, 0, ?)"#).bind(profile.id).bind(format!("K{i}")).bind("x".repeat(MAX_TELETEXT_PROFILE_VALUE_BYTES - 2)).execute(&fixture.repo.0).await.unwrap();
    }
    assert!(sqlx::query(r#"UPDATE "TeletextProfileSetting" SET "Value" = "Value" || 'x' WHERE "ProfileId" = ? AND "Key" = 'K0'"#).bind(profile.id).execute(&fixture.repo.0).await.is_err());
    assert!(sqlx::query(r#"INSERT INTO "TeletextProfileSetting" ("ProfileId", "Key", "Kind", "Value") VALUES (?, 'EXTRA', 0, '')"#).bind(profile.id).execute(&fixture.repo.0).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "TeletextProfileSetting" WHERE "ProfileId" = ?"#
        )
        .bind(profile.id)
        .fetch_one(&fixture.repo.0)
        .await
        .unwrap(),
        4
    );
}

#[tokio::test]
async fn legacy_ordinary_null_rows_inherit_defaults_or_remain_absent_without_hidden_substitution() {
    let fixture = Fixture::new().await;
    crate::addons::save_addon_setting(
        &fixture.repo.0,
        ADDON,
        &crate::addons::AddonSettingValue {
            key: "ZONE".to_owned(),
            value: None,
            reference_kind: None,
            reference: None,
        },
    )
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 1 WHERE "AddonId" = ?"#)
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    fixture.profile("Default inherited", "day").await;
    let mut optional = fixture.manifest.clone();
    optional.settings[1].default = None;
    optional.settings[1].required = false;
    fixture.store_manifest(&optional).await;
    fixture.profile("Optional absent", "day").await;
    optional.settings[1].required = true;
    fixture.store_manifest(&optional).await;
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Missing required", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    fixture.store_manifest(&fixture.manifest).await;
    sqlx::query(r#"UPDATE "AddonSetting" SET "ReferenceKind" = 'environment', "Reference" = 'UNRESOLVED_FIXTURE' WHERE "AddonId" = ? AND "Key" = 'ZONE'"#).bind(ADDON).execute(&fixture.repo.0).await.unwrap();
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Malformed ordinary", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    sqlx::query(r#"DELETE FROM "AddonSetting" WHERE "AddonId" = ?"#)
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'THEME', '')"#,
    )
    .bind(ADDON)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    assert_eq!(
        fixture
            .repo
            .save_profile(&fixture.save("Explicit empty", Vec::new()).await)
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
}

#[tokio::test]
async fn malformed_unrelated_installation_is_isolated_but_required_dependencies_are_checked() {
    let fixture = Fixture::new().await;
    let mut other = fixture.manifest.clone();
    other.id = "org.example.invalid-manifest".to_owned();
    insert_installation(&fixture.repo.0, &other).await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson" = '{}' WHERE "AddonId" = ?"#)
        .bind(&other.id)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    fixture.profile("Unaffected", "day").await;
    let mut dependent = fixture.manifest.clone();
    dependent.dependencies = vec![AddonDependency {
        id: other.id.clone(),
        version: VersionReq::STAR,
        optional: false,
    }];
    fixture.store_manifest(&dependent).await;
    assert_eq!(
        fixture
            .repo
            .installation_snapshot(ADDON)
            .await
            .unwrap()
            .availability,
        A::IncompatibleInstallation
    );
    dependent.dependencies[0].optional = true;
    fixture.store_manifest(&dependent).await;
    assert_eq!(
        fixture
            .repo
            .installation_snapshot(ADDON)
            .await
            .unwrap()
            .availability,
        A::Configured
    );
    fixture.profile("Optional", "day").await;
}

#[tokio::test]
async fn installation_disable_identity_drift_and_manifest_failure_remain_visible() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    fixture.bind(1, &profile).await;
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 0 WHERE "AddonId" = ?"#)
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].availability,
        A::DisabledInstallation
    );
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled" = 1, "StateRevision" = 'changed' WHERE "AddonId" = ?"#).bind(ADDON).execute(&fixture.repo.0).await.unwrap();
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].availability,
        A::StaleInstallation
    );
    let mut save = fixture.save("Day", profile.settings.clone()).await;
    save.existing = Some((profile.id, 1));
    saved(fixture.repo.save_profile(&save).await.unwrap());
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].availability,
        A::Configured
    );
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson" = '{}' WHERE "AddonId" = ?"#)
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].availability,
        A::IncompatibleInstallation
    );
    assert!(!saved(fixture.repo.disable_binding(1, ADDON, 2).await.unwrap()).enabled);
}

#[tokio::test]
async fn secret_reference_values_are_unresolved_redacted_and_never_written_as_literals() {
    let fixture = Fixture::new().await;
    let mut secret = fixture.manifest.clone();
    secret
        .settings
        .push(definition("TOKEN", AddonSettingKind::SecretReference, None));
    fixture.store_manifest(&secret).await;
    assert_eq!(
        fixture
            .repo
            .save_profile(
                &fixture
                    .save("Rejected", vec![literal("TOKEN", "private literal")])
                    .await
            )
            .await
            .unwrap(),
        M::Rejected(R::InvalidSettings)
    );
    let setting = TeletextProfileSetting {
        key: "TOKEN".to_owned(),
        value: TeletextProfileValue::SecretReference {
            kind: TeletextSecretReferenceKind::Environment,
            reference: "ERSATZRS_SYNTHETIC_UNRESOLVED_TOKEN".to_owned(),
        },
    };
    let profile = saved(
        fixture
            .repo
            .save_profile(&fixture.save("Reference", vec![setting.clone()]).await)
            .await
            .unwrap(),
    )
    .profile;
    assert_eq!(profile.settings, vec![setting]);
    assert!(!format!("{profile:?}").contains("ERSATZRS_SYNTHETIC_UNRESOLVED_TOKEN"));
    let value: Option<String> =
        sqlx::query_scalar(r#"SELECT "Value" FROM "TeletextProfileSetting" WHERE "ProfileId" = ?"#)
            .bind(profile.id)
            .fetch_one(&fixture.repo.0)
            .await
            .unwrap();
    assert_eq!(value, None);
}

#[tokio::test]
async fn existing_global_setting_save_keeps_its_disable_and_revision_semantics() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    fixture.bind(1, &profile).await;
    let before = fixture.token().await;
    crate::addons::save_addon_setting(
        &fixture.repo.0,
        ADDON,
        &crate::addons::AddonSettingValue {
            key: "ZONE".to_owned(),
            value: Some("Changed global".to_owned()),
            reference_kind: None,
            reference: None,
        },
    )
    .await
    .unwrap();
    let installation = fixture.repo.installation_snapshot(ADDON).await.unwrap();
    assert_eq!(installation.availability, A::DisabledInstallation);
    assert_ne!(
        installation.snapshot.unwrap().state_revision,
        before.state_revision
    );
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].availability,
        A::DisabledInstallation
    );
    assert_eq!(
        fixture.repo.list_profiles(ADDON).await.unwrap()[0]
            .profile
            .settings,
        profile.settings
    );
}

#[tokio::test]
async fn existing_uninstall_retains_unavailable_profiles_and_bindings_in_isolated_state() {
    const HELPER: &str = "ERSATZRS_TELETEXT_PROFILE_UNINSTALL_FIXTURE";
    if crate::startup_config::environment_variable(HELPER).is_none() {
        let temp = tempfile::tempdir().unwrap();
        let output = tokio::time::timeout(std::time::Duration::from_secs(20), tokio::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "repositories::teletext_profiles::tests::existing_uninstall_retains_unavailable_profiles_and_bindings_in_isolated_state", "--nocapture"])
            .env(HELPER, "1").env(crate::file_system_layout::APP_STATE_DIR_ENV_VAR, temp.path()).env(crate::file_system_layout::APP_DATA_DIR_ENV_VAR, temp.path()).kill_on_drop(true).output()).await.unwrap().unwrap();
        assert!(
            output.status.success(),
            "isolated uninstall fixture failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    let fixture = Fixture::new().await;
    let directory = crate::file_system_layout::addon_packages_folder().join("fixture");
    std::fs::create_dir_all(&directory).unwrap();
    sqlx::query(r#"UPDATE "AddonInstallation" SET "InstallPath" = ? WHERE "AddonId" = ?"#)
        .bind(directory.to_string_lossy().as_ref())
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    let profile = fixture.profile("Day", "day").await;
    fixture.bind(1, &profile).await;
    crate::addons::uninstall_addon(&fixture.repo.0, ADDON, false)
        .await
        .unwrap();
    assert!(!directory.exists());
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].availability,
        A::MissingInstallation
    );
    assert_eq!(
        fixture.repo.list_profiles(ADDON).await.unwrap()[0].profile,
        profile
    );
    assert!(!saved(fixture.repo.disable_binding(1, ADDON, 1).await.unwrap()).enabled);
}

#[tokio::test]
async fn invocation_snapshot_captures_complete_effective_values_without_writes() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Night", "night").await;
    let binding = fixture.bind(1, &profile).await;
    let initial = saved(
        fixture
            .repo
            .invocation_snapshot(1, ADDON, binding.revision)
            .await
            .unwrap(),
    );
    assert_eq!(initial.channel_id, 1);
    assert_eq!(initial.profile_id, profile.id);
    assert_eq!(initial.profile_revision, profile.revision);
    assert_eq!(initial.installation, profile.validated_installation);
    assert_eq!(
        initial.effective_settings,
        vec![literal("THEME", "night"), literal("ZONE", "Inherited")]
    );
    // A legacy/global change need not advance installation StateRevision. Its
    // actual effective values are nevertheless part of the invocation fence.
    sqlx::query(
        r#"UPDATE "AddonSetting" SET "Value" = 'Changed' WHERE "AddonId" = ? AND "Key" = 'ZONE'"#,
    )
    .bind(ADDON)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    let changed = saved(
        fixture
            .repo
            .invocation_snapshot(1, ADDON, binding.revision)
            .await
            .unwrap(),
    );
    assert_ne!(initial, changed);
    assert_eq!(initial.installation, changed.installation);
    assert_eq!(
        changed.effective_settings,
        vec![literal("THEME", "night"), literal("ZONE", "Changed")]
    );
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].binding,
        binding
    );
    assert_eq!(
        fixture.repo.list_profiles(ADDON).await.unwrap()[0].profile,
        profile
    );
    assert_eq!(fixture.token().await, initial.installation);
}

#[tokio::test]
async fn invocation_snapshot_rejects_disabled_stale_missing_and_unavailable_selection() {
    let fixture = Fixture::new().await;
    let profile = fixture.profile("Day", "day").await;
    let binding = fixture.bind(1, &profile).await;
    for (channel, revision, expected) in [
        (99, binding.revision, R::MissingChannel),
        (2, binding.revision, R::MissingBinding),
        (1, binding.revision + 1, R::Conflict),
    ] {
        assert_eq!(
            fixture
                .repo
                .invocation_snapshot(channel, ADDON, revision)
                .await
                .unwrap(),
            M::Rejected(expected)
        );
    }
    let disabled = saved(
        fixture
            .repo
            .disable_binding(1, ADDON, binding.revision)
            .await
            .unwrap(),
    );
    assert_eq!(
        fixture
            .repo
            .invocation_snapshot(1, ADDON, disabled.revision)
            .await
            .unwrap(),
        M::Rejected(R::DisabledBinding)
    );
    let binding = fixture.bind(2, &profile).await;
    sqlx::query(
        r#"UPDATE "AddonInstallation" SET "StateRevision" = 'changed' WHERE "AddonId" = ?"#,
    )
    .bind(ADDON)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    assert_eq!(
        fixture
            .repo
            .invocation_snapshot(2, ADDON, binding.revision)
            .await
            .unwrap(),
        M::Rejected(R::Conflict)
    );
    sqlx::query(r#"DELETE FROM "AddonInstallation" WHERE "AddonId" = ?"#)
        .bind(ADDON)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert_eq!(
        fixture
            .repo
            .invocation_snapshot(2, ADDON, binding.revision)
            .await
            .unwrap(),
        M::Rejected(R::InstallationUnavailable)
    );
}

async fn add_channel_producer(fixture: &Fixture, id: &str) -> ChannelTeletextBinding {
    let mut manifest = fixture.manifest.clone();
    manifest.id = id.to_owned();
    insert_installation(&fixture.repo.0, &manifest).await;
    let token = fixture
        .repo
        .installation_snapshot(id)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    let profile = saved(
        fixture
            .repo
            .save_profile(&TeletextProfileSave {
                addon_id: id.to_owned(),
                name: "Profile".to_owned(),
                settings: vec![literal("THEME", "day")],
                expected_installation: token,
                existing: None,
            })
            .await
            .unwrap(),
    )
    .profile;
    fixture.bind(1, &profile).await
}

#[tokio::test]
async fn channel_invocation_snapshot_canonical_membership_and_effective_changes() {
    let fixture = Fixture::new().await;
    let last = add_channel_producer(&fixture, "org.example.z-last").await;
    let first = add_channel_producer(&fixture, "org.example.a-first").await;
    let initial = saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap());
    assert_eq!(initial.channel_id, 1);
    assert_eq!(
        initial
            .bindings
            .iter()
            .map(|value| value.addon_id.as_str())
            .collect::<Vec<_>>(),
        [first.addon_id.as_str(), last.addon_id.as_str()]
    );
    for (selection, binding) in initial.bindings.iter().zip([&first, &last]) {
        assert_eq!(
            *selection,
            saved(
                fixture
                    .repo
                    .invocation_snapshot(1, &binding.addon_id, binding.revision)
                    .await
                    .unwrap()
            )
        );
    }
    sqlx::query(
        r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'ZONE', 'Changed')"#,
    )
    .bind(&first.addon_id)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    let inherited = saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap());
    assert_ne!(inherited.bindings[0], initial.bindings[0]);
    assert_eq!(inherited.bindings[1], initial.bindings[1]);
    assert_eq!(
        inherited.bindings[0].installation,
        initial.bindings[0].installation
    );
    let selected = &inherited.bindings[0];
    saved(
        fixture
            .repo
            .save_profile(&TeletextProfileSave {
                addon_id: selected.addon_id.clone(),
                name: "Changed profile".to_owned(),
                settings: vec![literal("THEME", "night")],
                expected_installation: selected.installation.clone(),
                existing: Some((selected.profile_id, selected.profile_revision)),
            })
            .await
            .unwrap(),
    );
    let edited = saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap());
    assert_eq!(edited.bindings[0].binding_revision, first.revision + 1);
    assert_eq!(
        edited.bindings[0].profile_revision,
        selected.profile_revision + 1
    );
    assert_eq!(edited.bindings[1], initial.bindings[1]);
    saved(
        fixture
            .repo
            .disable_binding(1, &last.addon_id, last.revision)
            .await
            .unwrap(),
    );
    let removed = saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap());
    assert_eq!(removed.bindings, vec![edited.bindings[0].clone()]);
    assert_ne!(removed, edited);
    assert_eq!(initial.bindings.len(), 2);
}

#[tokio::test]
async fn channel_invocation_snapshot_ignores_disabled_invalid_but_rejects_enabled_invalid() {
    let fixture = Fixture::new().await;
    assert_eq!(
        fixture.repo.channel_invocation_snapshot(99).await.unwrap(),
        M::Rejected(R::MissingChannel)
    );
    assert!(
        saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap())
            .bindings
            .is_empty()
    );
    let valid = add_channel_producer(&fixture, "org.example.a-valid").await;
    let invalid = add_channel_producer(&fixture, "org.example.z-invalid").await;
    let disabled = saved(
        fixture
            .repo
            .disable_binding(1, &invalid.addon_id, invalid.revision)
            .await
            .unwrap(),
    );
    // A retained, malformed disabled profile must not be decoded/validated or
    // prevent healthy enabled producers from preparing their complete union.
    sqlx::query(
        r#"UPDATE "TeletextProfileSetting" SET "Value" = 'invalid-enum' WHERE "ProfileId" = ?"#,
    )
    .bind(invalid.profile_id)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    let snapshot = saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap());
    assert_eq!(snapshot.bindings.len(), 1);
    assert_eq!(snapshot.bindings[0].addon_id, valid.addon_id);
    sqlx::query(r#"UPDATE "ChannelTeletextBinding" SET "Enabled" = 1 WHERE "ChannelId" = 1 AND "AddonId" = ?"#)
        .bind(&disabled.addon_id).execute(&fixture.repo.0).await.unwrap();
    assert_eq!(
        fixture.repo.channel_invocation_snapshot(1).await.unwrap(),
        M::Rejected(R::InvalidSettings)
    );
}

#[tokio::test]
async fn channel_invocation_snapshot_binding_limit_precedes_effective_bag_resolution() {
    let fixture = Fixture::new().await;
    let mut first = None;
    for index in 0..MAX_TELETEXT_PAGE_RECORDS {
        let binding =
            add_channel_producer(&fixture, &format!("org.example.limit-{index:04}")).await;
        first.get_or_insert(binding);
    }
    let full = saved(fixture.repo.channel_invocation_snapshot(1).await.unwrap());
    assert_eq!(full.bindings.len(), MAX_TELETEXT_PAGE_RECORDS);
    // More than the page-record budget rejects before resolving even the first
    // now-invalid effective bag, never truncating to a partial enabled membership.
    sqlx::query(
        r#"UPDATE "TeletextProfileSetting" SET "Value" = 'invalid-enum' WHERE "ProfileId" = ?"#,
    )
    .bind(first.unwrap().profile_id)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    add_channel_producer(&fixture, "org.example.limit-extra").await;
    assert_eq!(
        fixture.repo.channel_invocation_snapshot(1).await.unwrap(),
        M::Rejected(R::BindingLimit)
    );
}
