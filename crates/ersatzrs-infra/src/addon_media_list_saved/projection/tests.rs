use super::*;
use crate::addon_media_list_saved::{
    capture::draft_in_tx,
    model::{SavedFile, SavedOptionSchema, SavedSourceObservation},
    receiving_artwork::ArtworkStaging,
};
use ersatzrs_domain::{AddonMediaListItemKind, AddonMediaListOptionValue, RemoteStreamLiveness};
use sha2::{Digest as _, Sha256};

fn sha256(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut value = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(value, "{byte:02x}").unwrap();
    }
    value
}

async fn fixture() -> (tempfile::TempDir, crate::db::DbPool, SavedListDocument) {
    let root = tempfile::tempdir().unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(root.path().join("receiving.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO "AddonInstallation"
           ("AddonId", "Version", "Enabled", "PackageSha256", "ContentSha256",
            "ManifestJson", "InstallPath", "InstalledAt", "UpdatedAt")
           VALUES ('org.example.media', '1', 1, 'fixture', 'fixture', '{}', 'addons/fixture',
                   '2026-09-13T00:00:00Z', '2026-09-13T00:00:00Z');
           INSERT INTO "AddonMediaList"
           ("Id", "AddonId", "ProviderId", "SourceUrl", "Name", "DraftRevision", "AdminHidden")
           VALUES (1, 'org.example.media', 'shared-list', 'https://example.invalid/list',
                   'Receiving list', 17, 1);"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut document = crate::addon_media_list_saved::model::tests::fixture();
    let item = &mut document.draft.items[0];
    item.content.kind = AddonMediaListItemKind::RemoteStream;
    item.content.provider_liveness = RemoteStreamLiveness::Finite;
    item.content.duration_seconds = Some(180);
    item.content.source_url = Some("https://example.invalid/item".into());
    item.content.guids = vec!["fixture:item".into()];
    item.source_observation = Some(SavedSourceObservation {
        missing_since: now(),
        last_observed_at: now(),
        observation_count: 3,
        confirmed_at: Some(now()),
        reappeared_at: None,
    });
    item.liveness_conflict = true;
    let definition: AddonCapability = serde_json::from_value(serde_json::json!({
        "id": "media-list.options.v1",
        "options": [{
            "key": "enabled", "label": {"en-US": "Enabled"},
            "description": {"en-US": "Fixture option"}, "scope": "both",
            "operation_targets": ["test_access", "preparation"],
            "applicability": "finite_only", "kind": "boolean", "default": false
        }]
    }))
    .unwrap();
    let definition_json = serde_json::to_string(&definition).unwrap();
    document.draft.options.schema = Some(SavedOptionSchema {
        identity: ersatzrs_core::repositories::AddonMediaListOptionSchemaRef {
            addon_id: document.addon_id.clone(),
            capability_id: definition.id.clone(),
            schema_digest: sha256(definition_json.as_bytes()),
        },
        definition: serde_json::to_value(definition).unwrap(),
    });
    document
        .draft
        .options
        .list_values
        .insert("enabled".into(), AddonMediaListOptionValue::Boolean(true));
    document.draft.options.item_values.insert(
        "unpublished".into(),
        BTreeMap::from([("enabled".into(), AddonMediaListOptionValue::Boolean(false))]),
    );
    (root, pool, document)
}

fn now() -> DateTime<Utc> {
    DateTime::parse_from_rfc3339("2026-09-13T12:00:00Z")
        .unwrap()
        .with_timezone(&Utc)
}

#[tokio::test]
async fn receiving_projection_removes_obsolete_owners_but_preserves_publication_and_pins() {
    let (_root, pool, document) = fixture().await;
    sqlx::raw_sql(
        r#"INSERT INTO "AddonMediaListItem"
           ("Id", "AddonMediaListId", "ProviderId", "DisplayTitle", "Title",
            "Kind", "Rank", "InDraft", "Published")
           VALUES (30, 1, 'obsolete-draft', 'Obsolete', 'Obsolete', 4, 0, 1, 0),
                  (31, 1, 'obsolete-owner', 'Obsolete', 'Obsolete', 4, 0, 0, 0),
                  (32, 1, 'local-publication', 'Local', 'Local', 4, 0, 1, 1),
                  (33, 1, 'published', 'Incoming publication', 'Incoming publication', 4, 0, 0, 0);
           INSERT INTO "AddonMediaListItemGuid" ("AddonMediaListItemId", "Guid")
           VALUES (30, 'fixture:obsolete');
           INSERT INTO "AddonMediaListArtifact"
           ("Id", "AddonMediaListId", "AddonMediaListItemId", "CacheKey", "Kind", "State",
            "CreatedAt", "UpdatedAt")
           VALUES (40, 1, 30, 'retained', 0, 2, '2026-09-13', '2026-09-13');
           INSERT INTO "AddonMediaListRevision" ("Id", "AddonMediaListId", "Revision")
           VALUES (50, 1, 1);
           INSERT INTO "AddonMediaListRevisionArtifact"
           ("AddonMediaListRevisionId", "AddonMediaListArtifactId") VALUES (50, 40);"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    for _ in 0..2 {
        let mut tx = staging.begin_projection().await.unwrap();
        let projected = project_draft_in_tx(&mut tx, 1, &document, 17, now())
            .await
            .unwrap();
        assert_eq!(projected.item_ids["published"], 33);
        let retained: Vec<(i64, bool, bool)> = sqlx::query_as(
            r#"SELECT "Id", "InDraft", "Published" FROM "AddonMediaListItem"
               WHERE "Id" BETWEEN 30 AND 33 ORDER BY "Id""#,
        )
        .fetch_all(&mut *tx)
        .await
        .unwrap();
        assert_eq!(retained, [(32, false, true), (33, false, false)]);
        let guids: i64 = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM "AddonMediaListItemGuid" WHERE "AddonMediaListItemId"=30"#,
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(guids, 0);
        let pinned: (Option<i64>, i64) = sqlx::query_as(
            r#"SELECT "artifact"."AddonMediaListItemId", "pin"."AddonMediaListRevisionId"
               FROM "AddonMediaListArtifact" AS "artifact"
               JOIN "AddonMediaListRevisionArtifact" AS "pin"
                 ON "pin"."AddonMediaListArtifactId"="artifact"."Id"
               WHERE "artifact"."Id"=40"#,
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!(pinned, (None, 50));
        tx.commit().await.unwrap();
    }
}

#[tokio::test]
async fn receiving_projection_roundtrips_draft_and_reuses_publication_only_owners() {
    let (_root, pool, document) = fixture().await;
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    let mut previous_ids = None;
    for _ in 0..2 {
        let mut tx = staging.begin_projection().await.unwrap();
        let projected = project_draft_in_tx(&mut tx, 1, &document, 17, now())
            .await
            .unwrap();
        if let Some(previous) = &previous_ids {
            assert_eq!(&projected.item_ids, previous);
        }
        previous_ids = Some(projected.item_ids);
        let captured = draft_in_tx(&mut tx, 1).await.unwrap();
        let mut expected = document.draft.clone();
        expected.revision = 17;
        assert_eq!(captured.draft, expected);
        let rows: Vec<(String, bool, bool)> = sqlx::query_as(
            r#"SELECT "ProviderId", "InDraft", "Published" FROM "AddonMediaListItem" ORDER BY "ProviderId""#,
        ).fetch_all(&mut *tx).await.unwrap();
        assert_eq!(
            rows,
            [
                ("published".into(), false, false),
                ("unpublished".into(), true, false),
            ]
        );
        let local: (bool, i32, i32, Option<String>) = sqlx::query_as(
            r#"SELECT "AdminHidden", "PublishedRevision", "DraftRevision", "NextRefreshAt"
               FROM "AddonMediaList" WHERE "Id"=1"#,
        )
        .fetch_one(&mut *tx)
        .await
        .unwrap();
        assert_eq!((local.0, local.1, local.2), (true, 0, 17));
        assert!(local.3.is_some());
        tx.commit().await.unwrap();
    }
}

#[tokio::test]
async fn receiving_projection_failure_rolls_back_owners_and_settings() {
    let (_root, pool, mut document) = fixture().await;
    // Fail during option binding, after owner and content writes.
    document
        .draft
        .options
        .item_values
        .insert("not-an-owner".into(), BTreeMap::new());
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    assert!(project_draft_in_tx(&mut tx, 1, &document, 17, now())
        .await
        .is_err());
    tx.rollback().await.unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItem""#)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let name: String = sqlx::query_scalar(r#"SELECT "Name" FROM "AddonMediaList" WHERE "Id"=1"#)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(name, "Receiving list");
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn receiving_projection_preserves_unresolved_replacement_choices() {
    let (_root, pool, mut document) = fixture().await;
    let file = SavedFile {
        relative_path: "replacement.mkv".into(),
        sha256: "c".repeat(64),
        byte_length: 200,
    };
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    for replacement in [
        SavedReplacement { file: None },
        SavedReplacement {
            file: Some(file.clone()),
        },
    ] {
        document.draft.items[0].replacement = Some(replacement.clone());
        document.draft.files = replacement.file.iter().cloned().collect();
        let mut tx = staging.begin_projection().await.unwrap();
        let projected = project_draft_in_tx(&mut tx, 1, &document, 17, now())
            .await
            .unwrap();
        assert_eq!(
            projected.replacement_choices["unpublished"],
            Some(replacement.clone())
        );
        let captured = draft_in_tx(&mut tx, 1).await.unwrap();
        assert_eq!(captured.draft.items[0].replacement, Some(replacement));
        assert_eq!(captured.draft.files, document.draft.files);
        let count: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListItemReplacement""#)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(count, 0);
        tx.commit().await.unwrap();
    }
}

#[tokio::test]
async fn receiving_projection_rebuilds_chapter_edges_without_draft_phantoms() {
    let (_root, pool, mut document) = fixture().await;
    let mut child = document.draft.items[0].clone();
    child.content.provider_id = "chapter".into();
    child.content.rank = 1;
    child.content.parent_provider_id = Some("unpublished".into());
    child.content.fragment_start_seconds = Some(10);
    child.content.fragment_end_seconds = Some(30);
    child.content.duration_seconds = Some(20);
    document.draft.items.push(child);
    document.draft.delivery.before_cutoff.push("chapter".into());
    document.draft.delivery.before_cutoff.sort();
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    let projected = project_draft_in_tx(&mut tx, 1, &document, 17, now())
        .await
        .unwrap();
    let chapter: (i64, i64, i64, i64) = sqlx::query_as(
        r#"SELECT "SourceItemId", "FragmentItemId", "StartSeconds", "EndSeconds" FROM "AddonMediaListChapter""#,
    ).fetch_one(&mut *tx).await.unwrap();
    assert_eq!(
        chapter,
        (
            projected.item_ids["unpublished"],
            projected.item_ids["chapter"],
            10,
            30
        )
    );
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn receiving_projection_binds_artwork_on_the_staged_connection_and_preserves_active_state() {
    let (root, pool, mut document) = fixture().await;
    let mut bytes = vec![0; 64];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    std::fs::write(root.path().join("poster.png"), &bytes).unwrap();
    let file = SavedFile {
        relative_path: "poster.png".into(),
        sha256: sha256(&bytes),
        byte_length: 64,
    };
    let artwork = serde_json::json!([{"url":"./poster.png","role":"poster"}]);
    document.draft.files.push(file);
    document
        .draft
        .metadata
        .provider
        .insert("artwork".into(), artwork.clone());
    document.draft.items[0]
        .metadata
        .overrides
        .insert("artwork".into(), artwork);
    sqlx::query(
        r#"UPDATE "AddonMediaList" SET "PublishedRevision"=4, "PresentationKind"=1 WHERE "Id"=1"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    staging
        .stage_document(root.path(), &document)
        .await
        .unwrap();
    for _ in 0..2 {
        let mut tx = staging.begin_projection().await.unwrap();
        project_draft_in_tx(&mut tx, 1, &document, 17, now())
            .await
            .unwrap();
        let uploads: i64 =
            sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtworkUpload""#)
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(uploads, 2, "list and item have distinct owned handles");
        let active: (i32, i32, bool) = sqlx::query_as(
            r#"SELECT "PresentationKind", "PublishedRevision", "AdminHidden" FROM "AddonMediaList" WHERE "Id"=1"#,
        ).fetch_one(&mut *tx).await.unwrap();
        assert_eq!(active, (1, 4, true));
        let metadata: String = sqlx::query_scalar(
            r#"SELECT "MetadataOverrideJson" FROM "AddonMediaListItem" WHERE "ProviderId"='unpublished'"#,
        ).fetch_one(&mut *tx).await.unwrap();
        assert!(metadata.contains(crate::addon_artwork::UPLOAD_URL_PREFIX));
        tx.commit().await.unwrap();
    }
}

#[tokio::test]
async fn receiving_projection_rejects_sender_revision_as_local_fence() {
    let (_root, pool, document) = fixture().await;
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    assert!(
        project_draft_in_tx(&mut tx, 1, &document, document.draft.revision, now())
            .await
            .is_err()
    );
    tx.rollback().await.unwrap();
}
