use super::*;
use ersatzrs_core::repositories::{
    AddonMediaListArtworkRole, AddonMediaListEditorItem, AddonMediaListEditorSave,
};

async fn fixture() -> sqlx::SqlitePool {
    let pool = common::test_db().await;
    seed_addon(&pool).await;
    pool.execute(r#"INSERT INTO "AddonMediaList" (
        "Id", "AddonId", "ProviderId", "SourceUrl", "Name", "ItemCount"
    ) VALUES (1, 'org.example.lists', 'fixture/editor', 'https://example.invalid/editor', 'Editor fixture', 2)"#)
        .await.unwrap();
    seed_numbering_item(&pool, 10, 1, None, None, true, None, "First").await;
    seed_numbering_item(&pool, 11, 2, None, None, true, None, "Second").await;
    pool
}

fn command(revision: i32) -> AddonMediaListEditorSave {
    AddonMediaListEditorSave {
        list_id: 1,
        expected_revision: revision,
        playback_baseline: None,
        metadata: None,
        settings: None,
        delivery_mode: None,
        disk_warning_gb: Some(42),
        options: None,
        items: [10, 11]
            .into_iter()
            .map(|id| AddonMediaListEditorItem {
                review: AddonMediaListItemReviewEdit {
                    item_id: id,
                    content_kind: AddonMediaListContentKind::TelevisionEpisode,
                    included: true,
                    liveness_override: Some(RemoteStreamLiveness::Finite),
                    season: Some(3),
                    episode: Some(i32::try_from(id - 9).unwrap()),
                },
                expected_review: None,
                role: None,
                metadata: None,
                chapters: None,
                replacement: None,
            })
            .collect(),
    }
}

#[tokio::test]
async fn editor_artwork_staging_is_immutable_deduplicated_and_does_not_save() {
    let pool = fixture().await;
    let repositories = make_repositories(pool);
    let repo = &repositories.addon_media_lists;
    let before = repo.get_admin_form(1).await.unwrap().unwrap();
    let metadata = repo.get_show_nfo(1).await.unwrap();
    let bytes = b"\x89PNG\r\n\x1a\nfirst retained image";
    let first = repo
        .stage_editor_artwork(
            1,
            None,
            AddonMediaListArtworkRole::Poster,
            "image/png",
            bytes,
        )
        .await
        .unwrap();
    let repeated = repo
        .stage_editor_artwork(
            1,
            None,
            AddonMediaListArtworkRole::Poster,
            "image/png",
            bytes,
        )
        .await
        .unwrap();
    assert_eq!(first, repeated);
    let second = repo
        .stage_editor_artwork(
            1,
            None,
            AddonMediaListArtworkRole::Poster,
            "image/png",
            b"\x89PNG\r\n\x1a\nsecond retained image",
        )
        .await
        .unwrap();
    assert_ne!(first, second);
    let id = ersatzrs_infra::addon_artwork::upload_id(&first).unwrap();
    assert_eq!(
        repo.get_artwork_upload(id).await.unwrap().unwrap().bytes,
        bytes
    );
    assert_eq!(repo.get_show_nfo(1).await.unwrap().etag, metadata.etag);
    let after = repo.get_admin_form(1).await.unwrap().unwrap();
    assert_eq!(after.draft_revision, before.draft_revision);
    assert_eq!(after.review_state, before.review_state);
    assert!(repo
        .stage_editor_artwork(
            1,
            Some(999),
            AddonMediaListArtworkRole::Poster,
            "image/png",
            bytes
        )
        .await
        .is_err());
    assert!(repo
        .stage_editor_artwork(
            1,
            None,
            AddonMediaListArtworkRole::Poster,
            "image/png",
            b"invalid"
        )
        .await
        .is_err());
}

#[tokio::test]
async fn editor_artwork_save_rejects_wrong_scope_or_role_without_partial_writes() {
    use ersatzrs_core::repositories::{AddonMediaListArtwork, AddonMediaListEditorMetadata};
    let pool = fixture().await;
    let repositories = make_repositories(pool);
    let repo = &repositories.addon_media_lists;
    let revision = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .draft_revision;
    let list_url = repo
        .stage_editor_artwork(
            1,
            None,
            AddonMediaListArtworkRole::Poster,
            "image/png",
            b"\x89PNG\r\n\x1a\nlist image",
        )
        .await
        .unwrap();
    let item_url = repo
        .stage_editor_artwork(
            1,
            Some(10),
            AddonMediaListArtworkRole::Poster,
            "image/png",
            b"\x89PNG\r\n\x1a\nitem image",
        )
        .await
        .unwrap();
    let original = repo.get_show_nfo(1).await.unwrap();
    let mut save = command(revision);
    let mut metadata = original.metadata.clone();
    metadata.title = "Changed title".into();
    metadata.artwork = vec![AddonMediaListArtwork {
        url: item_url,
        role: AddonMediaListArtworkRole::Poster,
        width: None,
        height: None,
        language: None,
    }];
    save.metadata = Some(AddonMediaListEditorMetadata {
        metadata,
        etag: original.etag.clone(),
    });
    assert!(repo.save_editor_task(&save).await.is_err());
    assert_eq!(repo.get_show_nfo(1).await.unwrap().etag, original.etag);
    let metadata = &mut save.metadata.as_mut().unwrap().metadata;
    metadata.artwork[0].url = list_url;
    metadata.artwork[0].role = AddonMediaListArtworkRole::Fanart;
    assert!(repo.save_editor_task(&save).await.is_err());
    save.metadata.as_mut().unwrap().metadata.artwork[0].role = AddonMediaListArtworkRole::Poster;
    assert_eq!(repo.save_editor_task(&save).await.unwrap(), revision + 1);
    assert_eq!(
        repo.get_show_nfo(1).await.unwrap().metadata.title,
        "Changed title"
    );
    assert!(repo.save_editor_task(&save).await.is_err());
}

#[tokio::test]
async fn editor_task_save_is_one_revision_and_rejects_replay() {
    let pool = fixture().await;
    let repositories = make_repositories(pool.clone());
    let repo = &repositories.addon_media_lists;
    let revision = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .draft_revision;
    let save = command(revision);
    assert_eq!(repo.save_editor_task(&save).await.unwrap(), revision + 1);
    let rows = repo.list_items(1).await.unwrap();
    assert_eq!(
        rows.iter()
            .map(|row| (row.season, row.episode))
            .collect::<Vec<_>>(),
        [(Some(3), Some(1)), (Some(3), Some(2))]
    );
    assert!(matches!(
        repo.save_editor_task(&save).await,
        Err(RepositoryError::Conflict(_))
    ));
    assert_eq!(
        repo.get_admin_form(1)
            .await
            .unwrap()
            .unwrap()
            .draft_revision,
        revision + 1
    );
}

#[tokio::test]
async fn editor_task_write_failure_rolls_back_earlier_rows_and_settings() {
    let pool = fixture().await;
    let repositories = make_repositories(pool.clone());
    let repo = &repositories.addon_media_lists;
    let revision = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .draft_revision;
    let warning: i64 =
        sqlx::query_scalar(r#"SELECT "DiskUsageWarningGb" FROM "AddonMediaList" WHERE "Id"=1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    pool.execute(
        r#"CREATE TRIGGER fail_editor_second_row BEFORE UPDATE ON "AddonMediaListItem"
        WHEN NEW."Id"=11 BEGIN SELECT RAISE(ABORT, 'injected editor write failure'); END"#,
    )
    .await
    .unwrap();
    assert!(repo.save_editor_task(&command(revision)).await.is_err());
    assert!(repo
        .list_items(1)
        .await
        .unwrap()
        .iter()
        .all(|row| row.season.is_none() && row.episode.is_none()));
    assert_eq!(
        repo.get_admin_form(1)
            .await
            .unwrap()
            .unwrap()
            .draft_revision,
        revision
    );
    let after: i64 =
        sqlx::query_scalar(r#"SELECT "DiskUsageWarningGb" FROM "AddonMediaList" WHERE "Id"=1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, warning);
}

#[tokio::test]
async fn editor_task_invalid_pair_or_foreign_item_rolls_back_all_changes() {
    let pool = fixture().await;
    let repositories = make_repositories(pool);
    let repo = &repositories.addon_media_lists;
    let revision = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .draft_revision;
    for malformed_id in [false, true] {
        let mut save = command(revision);
        if malformed_id {
            save.items[1].review.item_id = 999;
        } else {
            save.items[1].review.episode = None;
        }
        assert!(repo.save_editor_task(&save).await.is_err());
        assert!(repo
            .list_items(1)
            .await
            .unwrap()
            .iter()
            .all(|row| row.season.is_none()));
        assert_eq!(
            repo.get_admin_form(1)
                .await
                .unwrap()
                .unwrap()
                .draft_revision,
            revision
        );
    }
}

#[tokio::test]
async fn editor_task_chapters_and_reconciliation_admission_commit_together() {
    let pool = fixture().await;
    seed_local_movie(&pool, 100, "Replacement", 1).await;
    pool.execute(r#"INSERT INTO "AddonMediaListLibrary" ("AddonMediaListId","LibraryId","FolderName","ManifestPath") VALUES (1,1,'editor','managed/editor/playlist.json')"#).await.unwrap();
    let repositories = make_repositories(pool.clone());
    let repo = &repositories.addon_media_lists;
    let revision = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .draft_revision;
    let mut save = command(revision);
    save.items[0].chapters = Some("00:00 Introduction\n00:30 Topic".into());
    repo.save_editor_task(&save).await.unwrap();
    let stored:(String,i32)=sqlx::query_as(r#"SELECT "item"."ChapterInput","job"."SavedDraftRevision" FROM "AddonMediaListItem" AS "item" JOIN "AddonMediaListLifecycleJob" AS "job" ON "job"."AddonMediaListId"="item"."AddonMediaListId" WHERE "item"."Id"=10"#).fetch_one(&pool).await.unwrap();
    assert_eq!(stored.0, save.items[0].chapters.clone().unwrap());
    assert_eq!(stored.1, revision + 1);
    assert!(repo.save_editor_task(&save).await.is_err());
    let jobs: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListLifecycleJob" WHERE "AddonMediaListId"=1"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(jobs, 1);
    let next = command(revision + 1);
    repo.save_editor_task(&next).await.unwrap();
    let retained: i32 = sqlx::query_scalar(r#"SELECT "SavedDraftRevision" FROM "AddonMediaListLifecycleJob" WHERE "AddonMediaListId"=1"#).fetch_one(&pool).await.unwrap();
    assert_eq!(
        retained,
        revision + 2,
        "later task saves retain queued chapter work"
    );
    pool.execute(r#"UPDATE "ManagedLibraryOperation" SET "State"=1"#)
        .await
        .unwrap();
    let mut running = command(revision + 2);
    running.items[0].chapters = Some("00:00 Changed while running".into());
    assert!(repo.save_editor_task(&running).await.is_err());
    let after:(String,i32)=sqlx::query_as(r#"SELECT "item"."ChapterInput","list"."DraftRevision" FROM "AddonMediaListItem" AS "item" JOIN "AddonMediaList" AS "list" ON "list"."Id"="item"."AddonMediaListId" WHERE "item"."Id"=10"#).fetch_one(&pool).await.unwrap();
    assert_eq!(after.0, save.items[0].chapters.clone().unwrap());
    assert_eq!(after.1, revision + 2);
}

#[tokio::test]
async fn editor_task_failed_chapter_admission_keeps_original_metadata() {
    let pool = fixture().await;
    let repositories = make_repositories(pool.clone());
    let repo = &repositories.addon_media_lists;
    let revision = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .draft_revision;
    let mut save = command(revision);
    save.items[0].chapters = Some("00:00 Introduction\n00:30 Topic".into());
    // No managed-library binding: the durable coordinator cannot accept work.
    assert!(repo.save_editor_task(&save).await.is_err());
    assert!(repo
        .list_items(1)
        .await
        .unwrap()
        .iter()
        .all(|r| r.chapter_input.is_empty() && r.season.is_none()));
    assert_eq!(
        repo.get_admin_form(1)
            .await
            .unwrap()
            .unwrap()
            .draft_revision,
        revision
    );
}

#[tokio::test]
async fn editor_task_metadata_etag_and_replacement_validity_are_transactional() {
    let pool = fixture().await;
    let repositories = make_repositories(pool);
    let repo = &repositories.addon_media_lists;
    let revision = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .draft_revision;
    let mut save = command(revision);
    save.items[1].replacement = Some(Some((999, 998)));
    assert!(repo.save_editor_task(&save).await.is_err());
    save.items[1].replacement = None;
    let nfo = repo.get_item_detail(1, 10).await.unwrap().unwrap().nfo;
    save.items[0].metadata = Some(ersatzrs_core::repositories::AddonMediaListEditorMetadata {
        metadata: nfo.metadata,
        etag: "stale".into(),
    });
    assert!(repo.save_editor_task(&save).await.is_err());
    assert_eq!(
        repo.get_admin_form(1)
            .await
            .unwrap()
            .unwrap()
            .draft_revision,
        revision
    );
    assert!(repo
        .list_items(1)
        .await
        .unwrap()
        .iter()
        .all(|r| r.season.is_none()));
}

#[tokio::test]
async fn editor_task_overview_all_and_page_clamping_are_exact() {
    let pool = fixture().await;
    let repositories = make_repositories(pool);
    let repo = &repositories.addon_media_lists;
    repo.set_page_size(0).await.unwrap();
    assert_eq!(repo.get_page_size().await.unwrap(), 0);
    for size in [0, 10, 25, 50, 100] {
        let (rows, total) = repo
            .list_admin(
                u32::MAX,
                size,
                AddonMediaListAdminSortField::Name,
                AddonMediaListAdminSortDirection::Ascending,
            )
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(total, 1);
    }
}

#[tokio::test]
async fn editor_retained_row_fence_rejects_changes_from_another_saved_task() {
    let pool = fixture().await;
    let repositories = make_repositories(pool.clone());
    let repo = &repositories.addon_media_lists;
    let revision = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .draft_revision;
    let before = repo.list_items(1).await.unwrap().remove(0);
    let expected = AddonMediaListItemReviewEdit {
        item_id: before.id,
        content_kind: before.content_kind,
        included: before.included,
        liveness_override: before.liveness_override,
        season: before.season,
        episode: before.episode,
    };
    let mut first = command(revision);
    first.items.truncate(1);
    let saved = repo.save_editor_task(&first).await.unwrap();
    let mut stale = command(saved);
    stale.disk_warning_gb = Some(99);
    stale.items[0].expected_review = Some(expected);
    assert!(matches!(
        repo.save_editor_task(&stale).await,
        Err(RepositoryError::Conflict(_))
    ));
    assert_eq!(
        repo.get_admin_form(1)
            .await
            .unwrap()
            .unwrap()
            .draft_revision,
        saved
    );
    let warning: i64 =
        sqlx::query_scalar(r#"SELECT "DiskUsageWarningGb" FROM "AddonMediaList" WHERE "Id"=1"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(warning, 42);
}

#[tokio::test]
async fn editor_advisory_warning_does_not_dirty_the_publication_revision() {
    let pool = fixture().await;
    let repositories = make_repositories(pool.clone());
    let repo = &repositories.addon_media_lists;
    let before = repo.get_admin_form(1).await.unwrap().unwrap();
    let mut save = command(before.draft_revision);
    save.items.clear();
    save.delivery_mode = Some(before.delivery_mode);
    assert_eq!(
        repo.save_editor_task(&save).await.unwrap(),
        before.draft_revision
    );
    let after = repo.get_admin_form(1).await.unwrap().unwrap();
    assert_eq!(after.draft_revision, before.draft_revision);
    assert_eq!(after.review_state, before.review_state);
}

#[tokio::test]
async fn playback_baseline_allows_metadata_progress_but_preserves_real_revision() {
    let pool = fixture().await;
    let repositories = make_repositories(pool.clone());
    let repo = &repositories.addon_media_lists;
    let (revision, baseline) = repo.playback_baseline(1).await.unwrap();
    let published = repo
        .get_admin_form(1)
        .await
        .unwrap()
        .unwrap()
        .published_revision;
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "DraftRevision" = "DraftRevision" + 1,
        "ProviderMetadataJson" = '{"title":"Updated provider title"}' WHERE "Id" = 1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut save = command(revision);
    save.items.clear();
    save.delivery_mode = Some(baseline.delivery_mode);
    save.disk_warning_gb = Some(42);
    save.playback_baseline = Some(baseline);
    assert_eq!(repo.save_editor_task(&save).await.unwrap(), revision + 1);
    assert_eq!(
        repo.playback_baseline(1).await.unwrap().1.disk_warning_gb,
        Some(42)
    );
    assert_eq!(
        repo.get_admin_form(1)
            .await
            .unwrap()
            .unwrap()
            .published_revision,
        published
    );
    assert!(repo.save_editor_task(&save).await.is_err());
}

#[tokio::test]
async fn playback_baseline_rejects_schema_identity_cutoff_and_ownership_changes() {
    for mutation in [
        r#"UPDATE "AddonInstallation" SET "ManifestJson" = '{"capabilities":[]}'"#,
        r#"UPDATE "AddonInstallation" SET "Enabled" = 0"#,
        r#"UPDATE "AddonMediaList" SET "DeliveryMode" = 2, "DeliveryCutoffAt" = '2026-09-01 00:00:00', "DeliveryCutoffIdentityId" = 99"#,
        r#"UPDATE "AddonMediaList" SET "DiskUsageWarningGb" = 99"#,
    ] {
        let pool = fixture().await;
        let repositories = make_repositories(pool.clone());
        let repo = &repositories.addon_media_lists;
        let (revision, baseline) = repo.playback_baseline(1).await.unwrap();
        let mut save = command(revision);
        save.items.clear();
        save.delivery_mode = Some(baseline.delivery_mode);
        save.disk_warning_gb = Some(42);
        save.playback_baseline = Some(baseline);
        pool.execute(mutation).await.unwrap();
        assert!(repo.save_editor_task(&save).await.is_err(), "{mutation}");
        let warning: i64 = sqlx::query_scalar(
            r#"SELECT "DiskUsageWarningGb" FROM "AddonMediaList" WHERE "Id" = 1"#,
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_ne!(warning, 42);
    }
}

#[tokio::test]
async fn playback_baseline_cannot_relax_other_tasks_or_legacy_forms() {
    let pool = fixture().await;
    let repositories = make_repositories(pool.clone());
    let repo = &repositories.addon_media_lists;
    let (revision, baseline) = repo.playback_baseline(1).await.unwrap();
    let mut mixed = command(revision);
    mixed.playback_baseline = Some(baseline.clone());
    mixed.delivery_mode = Some(baseline.delivery_mode);
    assert!(repo.save_editor_task(&mixed).await.is_err());
    pool.execute(r#"UPDATE "AddonMediaList" SET "DraftRevision" = "DraftRevision" + 1"#)
        .await
        .unwrap();
    let mut legacy = command(revision);
    legacy.items.clear();
    legacy.delivery_mode = Some(baseline.delivery_mode);
    assert!(repo.save_editor_task(&legacy).await.is_err());
}
