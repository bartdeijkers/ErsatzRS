//! Existing bound membership grants eligibility checks, never base activation.

use super::*;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode};

async fn changes(repo: &SqliteTeletextProfileRepository) -> i64 {
    // All fixtures use one connection. This detects writes even if a read API
    // were to roll them back or restore identical values afterwards.
    sqlx::query_scalar("SELECT total_changes()")
        .fetch_one(&repo.0)
        .await
        .unwrap()
}

async fn candidate(
    fixture: &Fixture,
    channel: EntityId,
    profile: &TeletextProfile,
) -> TeletextProfileCandidateSnapshot {
    saved(
        fixture
            .repo
            .candidate_snapshot(channel, ADDON, profile.id)
            .await
            .unwrap(),
    )
}

#[tokio::test]
async fn two_channels_resolve_different_candidate_themes_with_base_off_without_writes() {
    let fixture = Fixture::new().await;
    let day = fixture.profile("Day", "day").await;
    let night = fixture.profile("Night", "night").await;
    let first = fixture.bind(1, &day).await;
    fixture.bind(2, &day).await;
    let off = saved(
        fixture
            .repo
            .disable_binding(1, ADDON, first.revision)
            .await
            .unwrap(),
    );
    let before = changes(&fixture.repo).await;
    let one = candidate(&fixture, 1, &night).await;
    let two = candidate(&fixture, 2, &day).await;
    assert_eq!((one.channel_id, two.channel_id), (1, 2));
    assert_eq!((one.profile_id, two.profile_id), (night.id, day.id));
    assert_eq!(one.addon_id, two.addon_id);
    assert_eq!(one.installation, two.installation);
    assert_eq!(
        one.effective_settings,
        vec![literal("THEME", "night"), literal("ZONE", "Inherited")]
    );
    assert_eq!(
        two.effective_settings,
        vec![literal("THEME", "day"), literal("ZONE", "Inherited")]
    );
    for snapshot in [&one, &two] {
        assert_eq!(
            saved(
                fixture
                    .repo
                    .recheck_candidate_snapshot(snapshot)
                    .await
                    .unwrap()
            ),
            *snapshot
        );
    }
    assert_eq!(changes(&fixture.repo).await, before);
    assert_eq!(
        fixture.repo.channel_bindings(1).await.unwrap()[0].binding,
        off
    );
    assert_eq!(
        fixture.repo.channel_bindings(2).await.unwrap()[0]
            .binding
            .profile_id,
        day.id
    );
    assert_eq!(fixture.token().await, one.installation);
    assert_eq!(
        fixture
            .repo
            .invocation_snapshot(1, ADDON, off.revision)
            .await
            .unwrap(),
        M::Rejected(R::DisabledBinding)
    );
}

#[tokio::test]
async fn admin_base_changes_between_candidate_and_recheck_do_not_invalidate_target() {
    let fixture = Fixture::new().await;
    let base = fixture.profile("Base", "day").await;
    let alternative = fixture.profile("Alternative", "night").await;
    let target = fixture.profile("Script candidate", "night").await;
    let original = fixture.bind(1, &base).await;
    let selected = candidate(&fixture, 1, &target).await;
    let mut replace = Fixture::binding_save(1, &alternative);
    replace.expected_binding_revision = Some(original.revision);
    let changed = saved(fixture.repo.save_binding(&replace).await.unwrap());
    let off = saved(
        fixture
            .repo
            .disable_binding(1, ADDON, changed.revision)
            .await
            .unwrap(),
    );
    // The retained base profile may itself become unavailable. The independently
    // chosen existing target still uses the same installation/settings policy.
    saved(
        fixture
            .repo
            .tombstone_profile(ADDON, alternative.id, alternative.revision)
            .await
            .unwrap(),
    );
    let before = changes(&fixture.repo).await;
    assert_eq!(
        saved(
            fixture
                .repo
                .recheck_candidate_snapshot(&selected)
                .await
                .unwrap()
        ),
        selected
    );
    assert_eq!(changes(&fixture.repo).await, before);
    let binding = &fixture.repo.channel_bindings(1).await.unwrap()[0].binding;
    assert!(!binding.enabled);
    assert_eq!(binding.profile_id, alternative.id);
    assert!(binding.revision > off.revision);
}

#[tokio::test]
async fn selected_profile_settings_and_package_changes_invalidate_candidate() {
    for (statement, expected) in [
        (
            r#"UPDATE "TeletextProfile" SET "Revision" = "Revision" + 1 WHERE "Name" = 'Target'"#,
            R::Conflict,
        ),
        (
            r#"UPDATE "TeletextProfileSetting" SET "Value" = 'invalid' WHERE "Key" = 'THEME'"#,
            R::InvalidSettings,
        ),
        (
            r#"UPDATE "TeletextProfileSetting" SET "Value" = 'day' WHERE "Key" = 'THEME'"#,
            R::Conflict,
        ),
        (
            r#"UPDATE "AddonInstallation" SET "StateRevision" = 'changed'"#,
            R::Conflict,
        ),
        (
            r#"UPDATE "AddonInstallation" SET "PackageSha256" = 'changed-package'"#,
            R::Conflict,
        ),
        (
            r#"UPDATE "AddonInstallation" SET "Enabled" = 0"#,
            R::InstallationUnavailable,
        ),
        (
            r#"UPDATE "AddonInstallation" SET "ManifestJson" = '{}'"#,
            R::InstallationUnavailable,
        ),
        (
            r#"UPDATE "AddonSetting" SET "Value" = 'Changed inherited zone' WHERE "Key" = 'ZONE'"#,
            R::Conflict,
        ),
    ] {
        let fixture = Fixture::new().await;
        let base = fixture.profile("Base", "day").await;
        let target = fixture.profile("Target", "night").await;
        let binding = fixture.bind(1, &base).await;
        let selected = candidate(&fixture, 1, &target).await;
        sqlx::query(statement)
            .execute(&fixture.repo.0)
            .await
            .unwrap();
        let before = changes(&fixture.repo).await;
        assert_eq!(
            fixture
                .repo
                .recheck_candidate_snapshot(&selected)
                .await
                .unwrap(),
            M::Rejected(expected)
        );
        assert_eq!(changes(&fixture.repo).await, before);
        assert_eq!(
            fixture.repo.channel_bindings(1).await.unwrap()[0].binding,
            binding
        );
    }
}

#[tokio::test]
async fn candidate_requires_existing_channel_addon_membership_and_same_addon_profile() {
    let fixture = Fixture::new().await;
    let base = fixture.profile("Base", "day").await;
    let target = fixture.profile("Target", "night").await;
    fixture.bind(1, &base).await;
    let foreign = add_channel_producer(&fixture, "org.example.foreign").await;
    let before = changes(&fixture.repo).await;
    for (channel, addon, id, expected) in [
        (99, ADDON, target.id, R::MissingChannel),
        (2, ADDON, target.id, R::MissingBinding),
        (1, "org.example.unbound", target.id, R::MissingBinding),
        (1, ADDON, 99, R::MissingProfile),
        (1, ADDON, foreign.profile_id, R::WrongAddon),
    ] {
        assert_eq!(
            fixture
                .repo
                .candidate_snapshot(channel, addon, id)
                .await
                .unwrap(),
            M::Rejected(expected)
        );
    }
    let selected = candidate(&fixture, 1, &target).await;
    for (channel_id, expected) in [(2, R::MissingBinding), (99, R::MissingChannel)] {
        let changed_channel = TeletextProfileCandidateSnapshot {
            channel_id,
            ..selected.clone()
        };
        assert_eq!(
            fixture
                .repo
                .recheck_candidate_snapshot(&changed_channel)
                .await
                .unwrap(),
            M::Rejected(expected)
        );
    }
    assert_eq!(changes(&fixture.repo).await, before);
}

#[tokio::test]
async fn deletion_tombstone_and_removed_membership_reject_without_restoring_base() {
    for (statement, expected) in [
        (
            r#"UPDATE "TeletextProfile" SET "IsDeleted" = 1 WHERE "Name" = 'Target'"#,
            R::DeletedProfile,
        ),
        (
            r#"DELETE FROM "TeletextProfile" WHERE "Name" = 'Target'"#,
            R::MissingProfile,
        ),
        (
            r#"DELETE FROM "ChannelTeletextBinding" WHERE "ChannelId" = 1"#,
            R::MissingBinding,
        ),
        (r#"DELETE FROM "Channel" WHERE "Id" = 1"#, R::MissingChannel),
        (
            r#"DELETE FROM "AddonInstallation""#,
            R::InstallationUnavailable,
        ),
    ] {
        let fixture = Fixture::new().await;
        let base = fixture.profile("Base", "day").await;
        let target = fixture.profile("Target", "night").await;
        fixture.bind(1, &base).await;
        let selected = candidate(&fixture, 1, &target).await;
        sqlx::query(statement)
            .execute(&fixture.repo.0)
            .await
            .unwrap();
        let before = changes(&fixture.repo).await;
        assert_eq!(
            fixture
                .repo
                .recheck_candidate_snapshot(&selected)
                .await
                .unwrap(),
            M::Rejected(expected)
        );
        assert_eq!(
            fixture
                .repo
                .candidate_snapshot(1, ADDON, target.id)
                .await
                .unwrap(),
            M::Rejected(expected)
        );
        assert_eq!(changes(&fixture.repo).await, before);
    }
}

async fn open(path: &std::path::Path) -> DbPool {
    SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(true)
                .foreign_keys(true)
                .journal_mode(SqliteJournalMode::Wal),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn candidate_rechecks_survive_database_reopen_without_activation() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("candidates.sqlite");
    let fixture = Fixture::from_pool(open(&path).await).await;
    let base = fixture.profile("Base", "day").await;
    let target = fixture.profile("Target", "night").await;
    let binding = fixture.bind(1, &base).await;
    let off = saved(
        fixture
            .repo
            .disable_binding(1, ADDON, binding.revision)
            .await
            .unwrap(),
    );
    let selected = candidate(&fixture, 1, &target).await;
    fixture.repo.0.close().await;
    let reopened = SqliteTeletextProfileRepository(open(&path).await);
    let before = changes(&reopened).await;
    assert_eq!(
        saved(
            reopened
                .recheck_candidate_snapshot(&selected)
                .await
                .unwrap()
        ),
        selected
    );
    assert_eq!(
        saved(
            reopened
                .candidate_snapshot(1, ADDON, target.id)
                .await
                .unwrap()
        ),
        selected
    );
    assert_eq!(changes(&reopened).await, before);
    assert_eq!(reopened.channel_bindings(1).await.unwrap()[0].binding, off);
    assert_eq!(reopened.list_profiles(ADDON).await.unwrap().len(), 2);
    assert_eq!(
        reopened
            .installation_snapshot(ADDON)
            .await
            .unwrap()
            .snapshot,
        Some(selected.installation)
    );
    reopened.0.close().await;
}
