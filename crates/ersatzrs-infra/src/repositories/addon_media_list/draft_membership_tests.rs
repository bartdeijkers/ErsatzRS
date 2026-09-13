use super::*;
use sqlx::sqlite::SqlitePoolOptions;

async fn fixture() -> SqliteAddonMediaListRepository {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .expect("connect fixture");
    sqlx::migrate!("../../migrations")
        .run(&pool)
        .await
        .expect("migrate fixture");
    sqlx::raw_sql(
        r#"INSERT INTO "AddonInstallation" ("AddonId", "Version", "Enabled", "PackageSha256",
            "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt")
        VALUES ('fixture.addon', '1.0.0', 1, 'fixture', 'fixture', '{}', 'addons/fixture',
            '2026-09-13T00:00:00Z', '2026-09-13T00:00:00Z');
        INSERT INTO "AddonMediaList" ("Id", "AddonId", "ProviderId", "SourceUrl", "Name")
        VALUES (1, 'fixture.addon', 'list', 'https://media.example.test/list', 'List');
        INSERT INTO "AddonMediaListItem"
            ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Kind", "Rank", "Title")
        VALUES (10, 1, 'draft', 'Draft', 4, 1, 'Draft'),
               (20, 1, 'published', 'Published', 4, 0, 'Published');
        UPDATE "AddonMediaListItem" SET "InDraft" = 0, "Published" = 1 WHERE "Id" = 20;"#,
    )
    .execute(&pool)
    .await
    .expect("seed independent draft and publication");
    SqliteAddonMediaListRepository(pool)
}

#[tokio::test]
async fn draft_membership_filters_lists_counts_detail_and_metadata() {
    let repo = fixture().await;
    let items = repo.list_items(1).await.expect("draft list");
    assert_eq!(items.iter().map(|item| item.id).collect::<Vec<_>>(), [10]);
    let counts = repo.item_counts(1).await.expect("draft counts");
    assert_eq!(counts.available + counts.unavailable + counts.unknown, 1);
    let page = repo
        .list_items_page(1, items[0].availability, 0, Some(1))
        .await
        .expect("draft page");
    assert_eq!(page[0].id, 10);
    assert!(repo.get_item_detail(1, 20).await.expect("detail").is_none());
    assert!(matches!(
        load_item_metadata_form(&repo.0, 1, 20).await,
        Err(RepositoryError::NotFound { .. })
    ));
    assert!(load_item_metadata_form(&repo.0, 1, 10).await.is_ok());
    let published = repo
        .query_items(1, None, 0, None, false)
        .await
        .expect("verification rows");
    assert_eq!(published.len(), 2);
    assert!(published.iter().any(|item| item.id == 20 && item.published));
}

#[tokio::test]
async fn draft_membership_refuses_stale_direct_and_bulk_edits() {
    let repo = fixture().await;
    assert!(repo.set_item_included(1, 20, false).await.is_err());
    assert!(repo
        .update_item_content_kind(20, AddonMediaListContentKind::default())
        .await
        .is_err());
    assert!(repo.set_item_liveness_override(1, 20, None).await.is_err());
    assert!(repo
        .set_item_collection_role(1, 20, AddonMediaListItemRole::Primary)
        .await
        .is_err());
    assert!(repo.set_replacement(20, None).await.is_err());
    assert!(repo.reset_nfo(1, Some(20)).await.is_err());
    assert!(repo.update_item_chapters(20, "").await.is_err());
    assert!(repo
        .apply_list_liveness_override(1, None, &[10, 20])
        .await
        .is_err());
    let edit = AddonMediaListItemReviewEdit {
        item_id: 20,
        content_kind: AddonMediaListContentKind::default(),
        included: false,
        liveness_override: None,
        season: None,
        episode: None,
    };
    assert!(repo.apply_item_review_edits(1, &[edit]).await.is_err());
    let save = ersatzrs_core::repositories::AddonMediaListEditorSave {
        list_id: 1,
        expected_revision: 0,
        metadata: None,
        settings: None,
        delivery_mode: None,
        disk_warning_gb: None,
        options: None,
        items: vec![ersatzrs_core::repositories::AddonMediaListEditorItem {
            review: edit,
            expected_review: None,
            role: None,
            metadata: None,
            chapters: None,
            replacement: None,
        }],
    };
    assert!(matches!(
        editor::save(&repo.0, &save).await,
        Err(RepositoryError::NotFound { id: 20, .. })
    ));
    assert_eq!(
        repo.apply_list_content_kind(1, AddonMediaListContentKind::default())
            .await
            .expect("bulk draft edit"),
        1
    );
    assert_eq!(
        repo.apply_list_liveness_override(1, None, &[])
            .await
            .expect("bulk draft liveness"),
        1
    );
    repo.set_item_included(1, 10, false)
        .await
        .expect("default membership remains editable");
    let retained: (bool, bool, bool) = sqlx::query_as(
        r#"SELECT "InDraft", "Included", "Published" FROM "AddonMediaListItem" WHERE "Id" = 20"#,
    )
    .fetch_one(&repo.0)
    .await
    .expect("retained publication");
    assert_eq!(retained, (false, true, true));
}

#[tokio::test]
async fn draft_membership_cancellation_preserves_history_leases_and_revision_artifacts() {
    let repo = fixture().await;
    sqlx::raw_sql(
        r#"INSERT INTO "AddonMediaListArtifact"
            ("Id", "AddonMediaListId", "AddonMediaListItemId", "CacheKey", "Kind",
             "State", "CreatedAt", "UpdatedAt")
        VALUES (1, 1, 20, 'published-artifact', 0, 2,
                '2026-09-13T00:00:00Z', '2026-09-13T00:00:00Z');
        INSERT INTO "AddonMediaListRevision" ("Id", "AddonMediaListId", "Revision")
        VALUES (1, 1, 1);
        INSERT INTO "AddonMediaListRevisionArtifact"
            ("AddonMediaListRevisionId", "AddonMediaListArtifactId") VALUES (1, 1);"#,
    )
    .execute(&repo.0)
    .await
    .expect("pin published artifact");
    for state in 0..=5 {
        sqlx::query(
            r#"INSERT INTO "AddonMediaListItemOperationJob"
                ("Id", "AddonMediaListId", "AddonMediaListItemId", "Kind",
                 "SavedDraftRevision", "InputJson", "ContextFingerprint", "State",
                 "LeaseOwner", "ArtifactId", "CreatedAt", "UpdatedAt")
               VALUES (?, 1, 20, ?, 0, '{}', ?, ?, 'retained-lease', 1,
                       '2026-09-13T00:00:00Z', '2026-09-13T00:00:00Z')"#,
        )
        .bind(state + 1)
        .bind(state % 4)
        .bind(format!("state-{state}"))
        .bind(state)
        .execute(&repo.0)
        .await
        .expect("seed job history");
    }
    let mut tx = crate::db::begin_write(&repo.0).await.expect("write");
    crate::addon_media_list_item_jobs::cancel_item_jobs_in_tx(&mut tx, 1, 20, Utc::now())
        .await
        .expect("cancel active item work");
    tx.commit().await.expect("commit");
    let rows: Vec<(i32, String, i64)> = sqlx::query_as(
        r#"SELECT "State", "LeaseOwner", "ArtifactId"
           FROM "AddonMediaListItemOperationJob" ORDER BY "Id""#,
    )
    .fetch_all(&repo.0)
    .await
    .expect("retained history");
    assert_eq!(
        rows.iter().map(|row| row.0).collect::<Vec<_>>(),
        [4, 4, 2, 3, 4, 5]
    );
    assert!(rows
        .iter()
        .all(|row| row.1 == "retained-lease" && row.2 == 1));
    let pins: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "AddonMediaListRevisionArtifact"
           WHERE "AddonMediaListArtifactId"=1"#,
    )
    .fetch_one(&repo.0)
    .await
    .expect("revision pin");
    assert_eq!(pins, 1);
}
