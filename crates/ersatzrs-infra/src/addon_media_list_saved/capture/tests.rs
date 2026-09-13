use super::*;
use ersatzrs_domain::{
    AddonMediaListDeliveryMode, AddonMediaListDetailPolicy, AddonMediaListRefreshCadence,
};

async fn database() -> crate::db::DbPool {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation" ("AddonId", "Version", "Enabled", "PackageSha256",
            "ContentSha256", "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt")
        VALUES ('org.example.saved', '1.0.0', 1, 'fixture', 'fixture', '{}', 'addons/saved',
            '2026-09-13T00:00:00Z', '2026-09-13T00:00:00Z');
        INSERT INTO "AddonMediaList" ("Id", "AddonId", "ProviderId", "SourceUrl", "Name",
            "DraftRevision", "ProviderMetadataJson", "MetadataOverrideJson", "MetadataConflictJson",
            "DetailImportPolicy", "RefreshCadence", "DiskUsageWarningGb")
        VALUES (1, 'org.example.saved', 'list', 'https://example.invalid/list', 'Draft list', 8,
            '{"title":"Provider list","tags":["provider"]}', '{"title":"Operator list"}',
            '{"title":{"provider_before":"Original","provider_after":"Provider list","operator_value":"Operator list"}}',
            1, 0, 0),
            (2, 'org.example.saved', 'other', 'https://example.invalid/other', 'Other', 0, '{}', '{}', NULL, 0, 4, 25);
        INSERT INTO "AddonMediaListItem" ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle",
            "Kind", "Rank", "Title", "Included", "ProviderMetadataJson", "MetadataOverrideJson",
            "ProviderLiveness", "EffectiveLiveness", "ChapterInput", "ProviderChapterInput")
        VALUES (10, 1, 'retained', 'Excluded item', 4, 5, 'Excluded item', 0,
            '{"title":"Provider item"}', '{"title":"Operator item"}', 1, 1, '00:00 Start', '00:00 Provider'),
            (11, 1, 'new', 'Unpublished item', 4, 2, 'Unpublished item', 1, '{}', '{}', 1, 1, NULL, NULL);
        INSERT INTO "AddonMediaListItemGuid" ("AddonMediaListItemId", "Guid") VALUES (10, 'fixture:retained');
        INSERT INTO "AddonMediaListItemTombstone" ("AddonMediaListItemId", "MissingSince", "LastObservedAt",
            "ObservationCount", "LastReconcileGeneration", "ConfirmedAt")
        VALUES (10, '2026-09-10 00:00:00', '2026-09-11 00:00:00', 2, 700, '2026-09-11 00:00:00');
    "#).execute(&pool).await.unwrap();
    pool
}

#[tokio::test]
async fn saved_list_capture_retains_operator_provider_and_excluded_state_without_mutation() {
    let pool = database().await;
    let mut tx = pool.begin().await.unwrap();
    let captured = draft_in_tx(&mut tx, 1).await.unwrap();
    assert_eq!(captured.addon_id, "org.example.saved");
    assert_eq!(captured.provider_id, "list");
    assert_eq!(captured.draft.revision, 8);
    assert_eq!(captured.draft.metadata.provider["title"], "Provider list");
    assert_eq!(captured.draft.metadata.overrides["title"], "Operator list");
    assert_eq!(
        captured.draft.metadata.conflicts["title"].provider_before,
        "Original"
    );
    assert_eq!(
        captured.draft.detail_import_policy,
        AddonMediaListDetailPolicy::ManualOnly
    );
    assert_eq!(
        captured.draft.refresh_cadence,
        AddonMediaListRefreshCadence::Daily
    );
    assert_eq!(captured.draft.disk_warning_gb, 0);
    assert_eq!(
        captured.draft.delivery.mode,
        AddonMediaListDeliveryMode::Stream
    );
    assert_eq!(captured.draft.items.len(), 2);
    assert_eq!(captured.draft.items[0].content.provider_id, "new");
    let excluded = &captured.draft.items[1];
    assert!(!excluded.content.included);
    assert_eq!(excluded.content.guids, ["fixture:retained"]);
    assert_eq!(excluded.content.chapter_input, "00:00 Start");
    assert_eq!(excluded.content.provider_chapter_input, "00:00 Provider");
    assert_eq!(excluded.metadata.provider["title"], "Provider item");
    assert_eq!(excluded.metadata.overrides["title"], "Operator item");
    assert_eq!(
        excluded
            .source_observation
            .as_ref()
            .unwrap()
            .observation_count,
        2
    );
    assert!(captured.artwork.is_empty());
    let json = serde_json::to_string(&captured.draft).unwrap();
    assert!(!json.contains("last_reconcile_generation"));
    assert!(!json.contains("media_item_id"));
    let stored: (i32, String, i32) = sqlx::query_as(r#"SELECT "DraftRevision", "MetadataOverrideJson", "PublishedRevision" FROM "AddonMediaList" WHERE "Id"=1"#)
        .fetch_one(&mut *tx).await.unwrap();
    assert_eq!(stored, (8, "{\"title\":\"Operator list\"}".to_owned(), 0));
    tx.rollback().await.unwrap();
}

async fn upload(pool: &crate::db::DbPool, id: i64, list_id: i64, hash: &str) {
    sqlx::query(r#"INSERT INTO "AddonMediaListArtworkUpload" ("Id", "AddonMediaListId", "ScopeItemId", "Role", "ContentType", "Extension", "Bytes", "Sha256", "CreatedAt", "UpdatedAt")
        VALUES (?, ?, 0, 1, 'image/png', 'png', X'010203', ?, '2026-09-13 00:00:00', '2026-09-13 00:00:00')"#)
        .bind(id).bind(list_id).bind(hash).execute(pool).await.unwrap();
}

#[tokio::test]
async fn saved_list_capture_maps_only_referenced_owned_artwork_without_loading_blobs() {
    let pool = database().await;
    upload(&pool, 41, 1, &"a".repeat(64)).await;
    upload(&pool, 42, 1, &"b".repeat(64)).await;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson" = '{"artwork":[{"url":"ersatzrs-upload://41","role":"poster"}],"artwork_urls":["ersatzrs-upload://41"]}' WHERE "Id"=1"#)
        .execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let captured = draft_in_tx(&mut tx, 1).await.unwrap();
    assert_eq!(captured.artwork.len(), 1);
    assert_eq!(captured.artwork[0].upload_id, 41);
    assert_eq!(captured.draft.files.len(), 1);
    assert_eq!(captured.draft.files[0].byte_length, 3);
    let url = format!("./.ersatzrs-saved/artwork/{}.png", "a".repeat(64));
    assert_eq!(captured.draft.metadata.overrides["artwork"][0]["url"], url);
    assert_eq!(captured.draft.metadata.overrides["artwork_urls"][0], url);
    let json = serde_json::to_string(&captured.draft).unwrap();
    assert!(!json.contains("ersatzrs-upload://"));
    assert!(!json.contains("upload_id"));
    // References only: image validation/materialization is a separate required step.
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtworkUpload""#)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(count, 2);
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn saved_list_capture_rejects_artwork_owned_by_another_list() {
    let pool = database().await;
    upload(&pool, 41, 2, &"a".repeat(64)).await;
    sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson" = '{"artwork_urls":["ersatzrs-upload://41"]}' WHERE "Id"=1"#)
        .execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(draft_in_tx(&mut tx, 1).await.is_err());
    assert!(draft_in_tx(&mut tx, 999).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn saved_list_capture_rejects_wrong_artwork_scope_role_and_stray_handles() {
    let pool = database().await;
    upload(&pool, 41, 1, &"a".repeat(64)).await;
    for metadata in [
        r#"{"artwork":[{"url":"ersatzrs-upload://41","role":"fanart"}]}"#,
        r#"{"plot":"ersatzrs-upload://41"}"#,
        r#"{"people":[{"name":"Example","thumb":"ersatzrs-upload://41"}]}"#,
    ] {
        sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson"=? WHERE "Id"=1"#)
            .bind(metadata)
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(draft_in_tx(&mut tx, 1).await.is_err());
        tx.rollback().await.unwrap();
    }
    sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson"='{}' WHERE "Id"=1"#)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"UPDATE "AddonMediaListItem" SET "MetadataOverrideJson"='{"artwork_urls":["ersatzrs-upload://41"]}' WHERE "Id"=10"#)
        .execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(draft_in_tx(&mut tx, 1).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn saved_list_capture_preserves_literal_person_thumbnails() {
    let pool = database().await;
    let people = serde_json::json!([
        {"name":"Remote", "thumb":"https://example.invalid/person.png"},
        {"name":"Literal", "thumb":"./person.png"}
    ]);
    sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson"=? WHERE "Id"=1"#)
        .bind(serde_json::json!({"people": people}).to_string())
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let captured = draft_in_tx(&mut tx, 1).await.unwrap();
    assert_eq!(captured.draft.metadata.overrides["people"], people);
    assert!(captured.artwork.is_empty());
    tx.rollback().await.unwrap();
}
