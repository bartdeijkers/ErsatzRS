//! Pending intent/CAS proof; these tests never accept or transmit a service.

use std::{collections::BTreeMap, path::Path, time::Duration};

use ersatzrs_addon_contract::{
    AddonCapability, AddonEntrypoint, AddonEntrypointKind, AddonManifest, AddonPermissions,
    AddonSettingDefinition, AddonSettingKind, ADDON_MANIFEST_VERSION,
};
use ersatzrs_core::repositories::{
    ChannelRepository as _, ChannelTeletextBindingSave, FfmpegProfileRepository as _,
    TeletextDesignMutation, TeletextDesignRepository as _, TeletextDesignSave,
    TeletextProfileMutation, TeletextProfileRepository as _, TeletextProfileSave,
};
use ersatzrs_domain::{Channel, FFmpegProfile, TeletextChannelSelectionSnapshot, TeletextProfile};
use semver::{Version, VersionReq};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};

use super::{
    super::{SqliteTeletextDesignRepository, SqliteTeletextProfileRepository},
    *,
};

const FIRST: &str = "org.example.publication.first";
const SECOND: &str = "org.example.publication.second";
const DOCUMENT: &[u8] = b"synthetic immutable originals";

fn manifest(id: &str) -> AddonManifest {
    AddonManifest {
        manifest_version: ADDON_MANIFEST_VERSION,
        id: id.to_owned(),
        version: Version::new(1, 0, 0),
        name: BTreeMap::from([("en-US".into(), "Synthetic producer".into())]),
        summary: BTreeMap::from([("en-US".into(), "Pending request fixture".into())]),
        icon: None,
        license: "GPL-3.0-or-later".into(),
        source_url: "https://example.test/source".into(),
        host_version: VersionReq::STAR,
        capabilities: ["addon.check.v1", "teletext.pages.v1"]
            .map(|id| AddonCapability {
                id: id.into(),
                options: Vec::new(),
            })
            .to_vec(),
        dependencies: Vec::new(),
        entrypoints: BTreeMap::from([(
            "any".into(),
            AddonEntrypoint {
                path: "fixture.bin".into(),
                kind: AddonEntrypointKind::Native,
            },
        )]),
        permissions: AddonPermissions::default(),
        settings: ["THEME", "ZONE"]
            .map(|key| AddonSettingDefinition {
                key: key.into(),
                label: BTreeMap::from([("en-US".into(), key.into())]),
                kind: AddonSettingKind::String,
                required: true,
                default: Some("Default".into()),
                options: Vec::new(),
                suggested_reference: None,
            })
            .to_vec(),
        media_list_storage: None,
        default_source_hosts: Vec::new(),
        panels: Vec::new(),
    }
}

pub(in crate::repositories) async fn open(path: &Path) -> DbPool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Wal)
                .busy_timeout(Duration::from_secs(10)),
        )
        .await
        .unwrap()
}

async fn seed_channels(pool: &DbPool) {
    sqlx::query(r#"INSERT INTO "Resolution" ("Id", "Name", "Width", "Height") VALUES (1, 'synthetic', 1920, 1080)"#)
        .execute(pool).await.unwrap();
    let repositories = super::super::make_repositories(pool.clone());
    repositories
        .ffmpeg_profile
        .upsert(&FFmpegProfile {
            id: 1,
            name: "Synthetic".into(),
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
                unique_id: uuid::Uuid::new_v4(),
                number: id.to_string(),
                name: "Synthetic".into(),
                ffmpeg_profile_id: 1,
                ..Channel::default()
            })
            .await
            .unwrap();
    }
}

fn profile_saved<T: std::fmt::Debug>(value: TeletextProfileMutation<T>) -> T {
    match value {
        TeletextProfileMutation::Saved(value) => value,
        TeletextProfileMutation::Rejected(reason) => panic!("unexpected {reason:?}"),
    }
}

async fn seed_producer(pool: &DbPool, id: &str, enabled: bool) -> TeletextProfile {
    let manifest = manifest(id);
    manifest.validate().unwrap();
    sqlx::query(r#"INSERT INTO "AddonInstallation" ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt", "StateRevision")
        VALUES (?, '1.0.0', 1, 'synthetic-package', 'synthetic-content', ?, 'synthetic-fixture', 'synthetic', 'synthetic', 'synthetic-state')"#)
        .bind(id).bind(serde_json::to_string(&manifest).unwrap()).execute(pool).await.unwrap();
    sqlx::query(
        r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'ZONE', 'Inherited')"#,
    )
    .bind(id)
    .execute(pool)
    .await
    .unwrap();
    let repository = SqliteTeletextProfileRepository(pool.clone());
    let token = repository
        .installation_snapshot(id)
        .await
        .unwrap()
        .snapshot
        .unwrap();
    let profile = profile_saved(
        repository
            .save_profile(&TeletextProfileSave {
                addon_id: id.into(),
                name: "Synthetic".into(),
                settings: Vec::new(),
                expected_installation: token,
                existing: None,
            })
            .await
            .unwrap(),
    )
    .profile;
    profile_saved(
        repository
            .save_binding(&ChannelTeletextBindingSave {
                channel_id: 1,
                addon_id: id.into(),
                profile_id: profile.id,
                enabled,
                expected_installation: profile.validated_installation.clone(),
                expected_profile_revision: profile.revision,
                expected_binding_revision: None,
            })
            .await
            .unwrap(),
    );
    profile
}

pub(in crate::repositories) struct Fixture {
    pub(in crate::repositories) root: tempfile::TempDir,
    pub(in crate::repositories) repo: SqliteTeletextPublicationRepository,
    pub(in crate::repositories) selection: TeletextChannelSelectionSnapshot,
    pub(in crate::repositories) design_id: i64,
    pub(in crate::repositories) digest: String,
}

impl Fixture {
    pub(in crate::repositories) async fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let pool = open(&root.path().join("requests.sqlite")).await;
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        seed_channels(&pool).await;
        seed_producer(&pool, FIRST, true).await;
        seed_producer(&pool, SECOND, false).await;
        let selection = profile_saved(
            SqliteTeletextProfileRepository(pool.clone())
                .channel_invocation_snapshot(1)
                .await
                .unwrap(),
        );
        let designs = SqliteTeletextDesignRepository(pool.clone());
        let TeletextDesignMutation::Saved(head) = designs
            .save(
                &TeletextDesignSave {
                    name: "Synthetic originals".into(),
                    document_version: 1,
                    document: DOCUMENT.to_vec(),
                    existing: None,
                },
                DOCUMENT.len(),
            )
            .await
            .unwrap()
        else {
            panic!("synthetic design rejected")
        };
        let digest = designs
            .revision(head.id, 1, DOCUMENT.len())
            .await
            .unwrap()
            .unwrap()
            .digest;
        Self {
            root,
            repo: SqliteTeletextPublicationRepository(pool),
            selection,
            design_id: head.id,
            digest,
        }
    }

    pub(in crate::repositories) fn save(
        &self,
        current: Option<i64>,
    ) -> TeletextPublicationRequestSave {
        TeletextPublicationRequestSave {
            design_id: self.design_id,
            revision: 1,
            digest: self.digest.clone(),
            expected_design_head: 1,
            expected_request_id: current,
            selection: self.selection.clone(),
        }
    }

    pub(in crate::repositories) async fn request(
        &self,
        current: Option<i64>,
    ) -> TeletextPublicationRequest {
        requested(
            self.repo
                .request(&self.save(current), DOCUMENT.len())
                .await
                .unwrap(),
        )
    }

    async fn count(&self) -> i64 {
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#)
            .fetch_one(&self.repo.0)
            .await
            .unwrap()
    }
}

fn requested(value: M<TeletextPublicationRequest>) -> TeletextPublicationRequest {
    match value {
        M::Requested(value) => value,
        M::Rejected(reason) => panic!("unexpected {reason:?}"),
    }
}

#[tokio::test]
async fn independent_pools_request_cas_has_one_winner_and_retains_history() {
    let fixture = Fixture::new().await;
    let other = SqliteTeletextPublicationRepository(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    let save = fixture.save(None);
    let (a, b) = tokio::join!(
        fixture.repo.request(&save, DOCUMENT.len()),
        other.request(&save, DOCUMENT.len())
    );
    let results = [a.unwrap(), b.unwrap()];
    assert_eq!(
        results
            .iter()
            .filter(|value| matches!(value, M::Requested(_)))
            .count(),
        1
    );
    assert_eq!(
        results
            .iter()
            .filter(|value| matches!(value, M::Rejected(R::RequestConflict)))
            .count(),
        1
    );
    assert_eq!(fixture.count().await, 1);
    let first = fixture.repo.current(1).await.unwrap().unwrap();
    let second = fixture.request(Some(first.id)).await;
    assert!(second.id > first.id);
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(second));
    assert_eq!(fixture.count().await, 2);
}

#[tokio::test]
async fn changed_profiles_membership_installation_and_inherited_settings_reject_atomically() {
    for statement in [
        r#"UPDATE "TeletextProfile" SET "Revision"="Revision"+1 WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "TeletextProfile" SET "IsDeleted"=1 WHERE "AddonId"='org.example.publication.first'"#,
        r#"INSERT INTO "TeletextProfileSetting" ("ProfileId", "Key", "Kind", "Value") SELECT "Id", 'THEME', 0, 'Changed' FROM "TeletextProfile" WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "ChannelTeletextBinding" SET "Enabled"=1 WHERE "AddonId"='org.example.publication.second'"#,
        r#"UPDATE "ChannelTeletextBinding" SET "Enabled"=0 WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "AddonInstallation" SET "StateRevision"='changed' WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "AddonInstallation" SET "PackageSha256"='changed' WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "AddonInstallation" SET "ContentSha256"='changed' WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "AddonInstallation" SET "Enabled"=0 WHERE "AddonId"='org.example.publication.first'"#,
        r#"UPDATE "AddonSetting" SET "Value"='Changed' WHERE "AddonId"='org.example.publication.first' AND "Key"='ZONE'"#,
    ] {
        let fixture = Fixture::new().await;
        let first = fixture.request(None).await;
        sqlx::query(statement)
            .execute(&fixture.repo.0)
            .await
            .unwrap();
        let result = fixture
            .repo
            .request(&fixture.save(Some(first.id)), DOCUMENT.len())
            .await
            .unwrap();
        assert!(
            matches!(
                result,
                M::Rejected(R::SelectionChanged | R::SelectionUnavailable(_))
            ),
            "{result:?}"
        );
        assert_eq!(fixture.count().await, 1);
        assert_eq!(fixture.repo.current(1).await.unwrap(), Some(first));
    }
}

#[tokio::test]
async fn unchanged_effective_values_and_disabled_members_do_not_invent_conflicts() {
    let fixture = Fixture::new().await;
    sqlx::query(r#"UPDATE "AddonSetting" SET "Value"='Changed' WHERE "AddonId"=?"#)
        .bind(SECOND)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    let first = fixture.request(None).await;
    sqlx::query(
        r#"INSERT INTO "AddonSetting" ("AddonId", "Key", "Value") VALUES (?, 'THEME', 'Default')"#,
    )
    .bind(FIRST)
    .execute(&fixture.repo.0)
    .await
    .unwrap();
    let second = fixture.request(Some(first.id)).await;
    assert!(second.id > first.id);
    assert_eq!(fixture.count().await, 2);
}

#[tokio::test]
async fn historical_target_requires_fresh_head_and_exact_digest_but_not_latest_revision() {
    let fixture = Fixture::new().await;
    let designs = SqliteTeletextDesignRepository(fixture.repo.0.clone());
    let TeletextDesignMutation::Saved(_) = designs
        .save(
            &TeletextDesignSave {
                name: "Synthetic edited originals".into(),
                document_version: 1,
                document: b"synthetic newer original".to_vec(),
                existing: Some((fixture.design_id, 1)),
            },
            1024,
        )
        .await
        .unwrap()
    else {
        panic!("synthetic edit rejected")
    };
    assert_eq!(
        fixture
            .repo
            .request(&fixture.save(None), DOCUMENT.len())
            .await
            .unwrap(),
        M::Rejected(R::DesignConflict)
    );
    let mut save = fixture.save(None);
    save.expected_design_head = 2;
    let first = requested(fixture.repo.request(&save, DOCUMENT.len()).await.unwrap());
    assert_eq!(first.revision, 1);
    assert_eq!(first.digest, fixture.digest);
    assert_eq!(fixture.count().await, 1);
}

#[tokio::test]
async fn wrong_targets_bounds_and_tombstones_preserve_pending_pointer() {
    let fixture = Fixture::new().await;
    let first = fixture.request(None).await;
    let save = fixture.save(Some(first.id));
    let mut missing = save.clone();
    missing.design_id += 1;
    assert_eq!(
        fixture
            .repo
            .request(&missing, DOCUMENT.len())
            .await
            .unwrap(),
        M::Rejected(R::MissingDesign)
    );
    let mut missing = save.clone();
    missing.revision = 99;
    assert_eq!(
        fixture
            .repo
            .request(&missing, DOCUMENT.len())
            .await
            .unwrap(),
        M::Rejected(R::MissingRevision)
    );
    let mut wrong = save.clone();
    wrong.digest = "0".repeat(64);
    assert_eq!(
        fixture.repo.request(&wrong, DOCUMENT.len()).await.unwrap(),
        M::Rejected(R::DigestConflict)
    );
    let mut invalid = save.clone();
    invalid.selection.bindings.clear();
    assert_eq!(
        fixture
            .repo
            .request(&invalid, DOCUMENT.len())
            .await
            .unwrap(),
        M::Rejected(R::InvalidValues)
    );
    assert!(matches!(
        fixture.repo.request(&save, DOCUMENT.len() - 1).await,
        Err(RepositoryError::Decode { .. })
    ));
    SqliteTeletextDesignRepository(fixture.repo.0.clone())
        .tombstone(fixture.design_id, 1)
        .await
        .unwrap();
    assert_eq!(
        fixture.repo.request(&save, DOCUMENT.len()).await.unwrap(),
        M::Rejected(R::DeletedDesign)
    );
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(first));
    assert_eq!(fixture.count().await, 1);
}

#[tokio::test]
async fn pending_history_survives_reopen_and_producer_removal_without_activation() {
    let fixture = Fixture::new().await;
    let before: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as(r#"SELECT "Key", "Value" FROM "ConfigElement" ORDER BY "Key""#)
            .fetch_all(&fixture.repo.0)
            .await
            .unwrap();
    let first = fixture.request(None).await;
    let after: Vec<(Option<String>, Option<String>)> =
        sqlx::query_as(r#"SELECT "Key", "Value" FROM "ConfigElement" ORDER BY "Key""#)
            .fetch_all(&fixture.repo.0)
            .await
            .unwrap();
    assert_eq!(after, before);
    for query in [
        r#"DELETE FROM "Channel""#,
        r#"DELETE FROM "TeletextProfile""#,
        r#"DELETE FROM "AddonInstallation""#,
    ] {
        sqlx::query(query).execute(&fixture.repo.0).await.unwrap();
    }
    fixture.repo.0.close().await;
    let reopened = SqliteTeletextPublicationRepository(
        open(&fixture.root.path().join("requests.sqlite")).await,
    );
    assert_eq!(reopened.current(1).await.unwrap(), Some(first.clone()));
    assert!(reopened.current(2).await.unwrap().is_none());
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "TeletextPublicationRequest""#)
        .fetch_one(&reopened.0)
        .await
        .unwrap();
    assert_eq!(count, 1);
    for query in [
        r#"UPDATE "TeletextPublicationRequest" SET "Digest"=printf('%064d',0)"#,
        r#"DELETE FROM "TeletextPublicationRequest""#,
    ] {
        assert!(sqlx::query(query).execute(&reopened.0).await.is_err());
    }
}

#[tokio::test]
async fn pointer_write_failure_rolls_back_new_request_and_current_pointer() {
    let fixture = Fixture::new().await;
    let first = fixture.request(None).await;
    sqlx::query(r#"CREATE TRIGGER "SyntheticPointerFailure" BEFORE UPDATE ON "TeletextPublicationRequestCurrent"
        BEGIN SELECT RAISE(ABORT,'synthetic pointer failure'); END"#).execute(&fixture.repo.0).await.unwrap();
    assert!(fixture
        .repo
        .request(&fixture.save(Some(first.id)), DOCUMENT.len())
        .await
        .is_err());
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(first));
    assert_eq!(fixture.count().await, 1);
}

#[tokio::test]
async fn malformed_current_metadata_is_bounded_error_not_absence_or_mutation() {
    let fixture = Fixture::new().await;
    let first = fixture.request(None).await;
    sqlx::query(r#"DROP TRIGGER "TR_TeletextPublicationRequest_ImmutableUpdate""#)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextPublicationRequest" SET "Digest"=?"#)
        .bind("private-corrupt-sentinel".repeat(4096))
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    let error = fixture.repo.current(1).await.unwrap_err();
    assert!(matches!(error, RepositoryError::Decode { .. }));
    assert!(!error.to_string().contains("private-corrupt-sentinel"));
    assert!(fixture
        .repo
        .request(&fixture.save(Some(first.id)), DOCUMENT.len())
        .await
        .is_err());
    assert_eq!(fixture.count().await, 1);
}

#[tokio::test]
async fn missing_or_wrong_channel_request_under_a_pointer_is_corruption_not_absence() {
    let fixture = Fixture::new().await;
    let first = fixture.request(None).await;
    sqlx::query("PRAGMA foreign_keys=OFF")
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextPublicationRequestCurrent" SET "RequestId"=?"#)
        .bind(first.id + 1)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert!(matches!(
        fixture.repo.current(1).await,
        Err(RepositoryError::Decode { .. })
    ));
    assert!(fixture
        .repo
        .request(&fixture.save(Some(first.id)), DOCUMENT.len())
        .await
        .is_err());
    assert_eq!(fixture.count().await, 1);
    sqlx::query(r#"UPDATE "TeletextPublicationRequestCurrent" SET "RequestId"=?"#)
        .bind(first.id)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"DROP TRIGGER "TR_TeletextPublicationRequest_ImmutableUpdate""#)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "TeletextPublicationRequest" SET "ChannelId"=2"#)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert!(matches!(
        fixture.repo.current(1).await,
        Err(RepositoryError::Decode { .. })
    ));
    assert!(fixture
        .repo
        .request(&fixture.save(Some(first.id)), DOCUMENT.len())
        .await
        .is_err());
    assert_eq!(fixture.count().await, 1);
}

#[tokio::test]
async fn channel_current_pointers_are_independent_and_manifest_bytes_are_fenced() {
    let fixture = Fixture::new().await;
    let first = fixture.request(None).await;
    let selection = &fixture.selection.bindings[0];
    let profiles = SqliteTeletextProfileRepository(fixture.repo.0.clone());
    profile_saved(
        profiles
            .save_binding(&ChannelTeletextBindingSave {
                channel_id: 2,
                addon_id: FIRST.into(),
                profile_id: selection.profile_id,
                enabled: true,
                expected_installation: selection.installation.clone(),
                expected_profile_revision: selection.profile_revision,
                expected_binding_revision: None,
            })
            .await
            .unwrap(),
    );
    let mut second = fixture.save(None);
    second.selection = profile_saved(profiles.channel_invocation_snapshot(2).await.unwrap());
    let second = requested(fixture.repo.request(&second, DOCUMENT.len()).await.unwrap());
    assert_ne!(first.id, second.id);
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(first.clone()));
    assert_eq!(fixture.repo.current(2).await.unwrap(), Some(second));
    let mut changed = manifest(FIRST);
    changed
        .summary
        .insert("en-US".into(), "Changed summary".into());
    sqlx::query(r#"UPDATE "AddonInstallation" SET "ManifestJson"=? WHERE "AddonId"=?"#)
        .bind(serde_json::to_string(&changed).unwrap())
        .bind(FIRST)
        .execute(&fixture.repo.0)
        .await
        .unwrap();
    assert!(matches!(
        fixture
            .repo
            .request(&fixture.save(Some(first.id)), DOCUMENT.len())
            .await
            .unwrap(),
        M::Rejected(R::SelectionUnavailable(_))
    ));
    assert_eq!(fixture.count().await, 2);
    assert_eq!(fixture.repo.current(1).await.unwrap(), Some(first));
}

#[path = "teletext_publications_source_tests.rs"]
mod source_tests;
