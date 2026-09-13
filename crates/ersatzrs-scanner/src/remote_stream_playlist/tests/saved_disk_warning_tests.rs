use super::*;

#[tokio::test]
async fn saved_disk_warning_direct_save_preserves_revision_and_publication() {
    let (_receiver, pool, copy, list_id) = adopted_list().await;
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    let before = database_state(&pool, list_id).await;
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    let review: i32 =
        sqlx::query_scalar(r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();

    for warning in [42, 0] {
        repo.save_disk_warning_gb(Some(list_id), warning)
            .await
            .unwrap();
        let after = database_state(&pool, list_id).await;
        assert_eq!(after.0, i64::from(warning));
        assert_eq!(after.2, before.2);
        let saved: SavedListDocument =
            serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
        assert_eq!(saved.draft.disk_warning_gb, warning);
        assert_eq!(saved.published, original.published);
        assert_eq!(after.1, saved_sha256(&std::fs::read(&copy).unwrap()));
    }
    let after_review: i32 =
        sqlx::query_scalar(r#"SELECT "ReviewState" FROM "AddonMediaList" WHERE "Id"=?"#)
            .bind(list_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after_review, review);
    let bytes = std::fs::read(&copy).unwrap();
    let state = database_state(&pool, list_id).await;
    repo.save_disk_warning_gb(None, 450).await.unwrap();
    assert_eq!(repo.disk_warning_gb(None).await.unwrap(), 450);
    assert_eq!(std::fs::read(&copy).unwrap(), bytes);
    assert_eq!(database_state(&pool, list_id).await, state);
    assert!(matches!(
        repo.save_disk_warning_gb(Some(i64::MAX), 0).await,
        Err(ersatzrs_core::repositories::RepositoryError::NotFound { .. })
    ));
}

#[tokio::test]
async fn saved_disk_warning_direct_save_reports_file_first_failure() {
    let (_receiver, pool, copy, list_id) = adopted_list().await;
    let repo = ersatzrs_infra::repositories::SqliteAddonMediaListRepository(pool.clone());
    let before = database_state(&pool, list_id).await;
    let original: SavedListDocument =
        serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    sqlx::query(
        r#"CREATE TRIGGER "FailWarningDigest" BEFORE UPDATE OF "SavedDocumentDigest"
           ON "AddonMediaList" BEGIN SELECT RAISE(ABORT, 'synthetic digest failure'); END"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let error = repo
        .save_disk_warning_gb(Some(list_id), 42)
        .await
        .unwrap_err();
    assert!(error
        .to_string()
        .contains("saved to the list folder; database refresh is pending"));
    assert_eq!(database_state(&pool, list_id).await, before);
    let saved: SavedListDocument = serde_json::from_slice(&std::fs::read(&copy).unwrap()).unwrap();
    assert_eq!(saved.draft.disk_warning_gb, 42);
    assert_eq!(saved.published, original.published);
}
