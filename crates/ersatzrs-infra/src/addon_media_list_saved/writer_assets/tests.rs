use super::*;
use std::fmt::Write as _;

#[test]
fn provider_changes_accept_new_items_and_cleared_selection_without_asset_io() {
    let mut document = super::super::model::tests::fixture();
    let file = SavedFile {
        relative_path: "existing/replacement.mkv".into(),
        sha256: "a".repeat(64),
        byte_length: 123,
    };
    document.draft.items[0].replacement = Some(SavedReplacement {
        file: Some(file.clone()),
    });
    document.draft.files.push(file);
    let mut before = captured(&document);
    before.replacements.push(CapturedReplacement {
        provider_id: before.draft.items[0].content.provider_id.clone(),
        media_item_id: 20,
        media_version_id: Some(30),
    });
    before.draft.items[0].replacement = Some(SavedReplacement { file: None });
    let mut prepared = PreparedAssets::reuse(&document, &before).unwrap();
    let mut after = captured(&document);
    after.provider_id = "refreshed-list-identity".into();
    after.draft.items[0].replacement = None;
    let mut added = after.draft.items[0].clone();
    added.content.provider_id = "new-provider-item".into();
    after.draft.items.push(added);
    assert!(
        prepared.apply(&mut after).is_err(),
        "ordinary apply remains strict"
    );
    prepared.accept_provider_changes(&after).unwrap();
    prepared.apply(&mut after).unwrap();
    assert_eq!(after.draft.items.len(), 2);
    assert!(after
        .draft
        .items
        .iter()
        .all(|item| item.replacement.is_none()));
    assert!(
        after.draft.files.is_empty(),
        "cleared replacement bytes are not retained as referenced assets"
    );
}

#[test]
fn provider_changes_refuse_new_or_changed_local_replacement_and_artwork() {
    let document = super::super::model::tests::fixture();
    let before = captured(&document);
    let mut prepared = PreparedAssets::reuse(&document, &before).unwrap();
    let mut after = captured(&document);
    after.draft.items[0].replacement = Some(SavedReplacement { file: None });
    after.replacements.push(CapturedReplacement {
        provider_id: after.draft.items[0].content.provider_id.clone(),
        media_item_id: 20,
        media_version_id: Some(30),
    });
    assert!(prepared.accept_provider_changes(&after).is_err());
    let mut after = captured(&document);
    after.artwork.push(CapturedArtwork {
        upload_id: 42,
        scope_item_id: 0,
        role: 1,
        file: SavedFile {
            relative_path: ".ersatzrs-saved/artwork/new.png".into(),
            sha256: "a".repeat(64),
            byte_length: 3,
        },
    });
    assert!(prepared.accept_provider_changes(&after).is_err());
    // A refusal must not broaden the prepared state for a later ordinary apply.
    prepared.apply(&mut captured(&document)).unwrap();

    let mut document = document;
    document.draft.items[0].replacement = Some(SavedReplacement { file: None });
    let mut before = captured(&document);
    before.replacements.push(CapturedReplacement {
        provider_id: before.draft.items[0].content.provider_id.clone(),
        media_item_id: 20,
        media_version_id: Some(30),
    });
    let mut prepared = PreparedAssets::reuse(&document, &before).unwrap();
    before.replacements[0].media_version_id = Some(31);
    assert!(prepared.accept_provider_changes(&before).is_err());
}

#[test]
fn provider_changes_keep_addon_and_source_identity_fenced() {
    let document = super::super::model::tests::fixture();
    let mut prepared = PreparedAssets::reuse(&document, &captured(&document)).unwrap();
    let mut changed = captured(&document);
    changed.addon_id = "org.example.other".into();
    assert!(prepared.accept_provider_changes(&changed).is_err());
    let mut changed = captured(&document);
    changed.source_url = "https://example.invalid/other".into();
    assert!(prepared.accept_provider_changes(&changed).is_err());
    prepared.apply(&mut captured(&document)).unwrap();
}

fn captured(document: &SavedListDocument) -> CapturedDraft {
    CapturedDraft {
        addon_id: document.addon_id.clone(),
        provider_id: document.provider_id.clone(),
        source_url: document.source_url.clone(),
        draft: document.draft.clone(),
        artwork: Vec::new(),
        replacements: Vec::new(),
    }
}

fn save() -> AddonMediaListEditorSave {
    AddonMediaListEditorSave {
        list_id: 1,
        expected_revision: 1,
        metadata: None,
        settings: None,
        delivery_mode: None,
        disk_warning_gb: None,
        options: None,
        items: Vec::new(),
    }
}

#[tokio::test]
async fn unchanged_replacement_reuses_reference_without_opening_video_or_reading_media_rows() {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let root = tempfile::tempdir().unwrap();
    let tree = super::super::assembly::lock_managed_tree(root.path()).unwrap();
    let mut document = super::super::model::tests::fixture();
    let file = SavedFile {
        relative_path: "existing/replacement.mkv".into(),
        sha256: "a".repeat(64),
        byte_length: 123,
    };
    document.draft.items[0].replacement = Some(SavedReplacement {
        file: Some(file.clone()),
    });
    document.draft.files.push(file.clone());
    let mut before = captured(&document);
    before.replacements.push(CapturedReplacement {
        provider_id: document.draft.items[0].content.provider_id.clone(),
        media_item_id: 20,
        media_version_id: Some(30),
    });
    // A concrete DB capture carries local ownership, not an assembled file hash.
    before.draft.items[0].replacement = Some(SavedReplacement { file: None });
    before.draft.files.clear();
    let prepared = PreparedAssets::prepare(&pool, tree, &document, &before, &save())
        .await
        .unwrap();
    let mut after = captured(&document);
    after.replacements = before.replacements.clone();
    after.draft.items[0].replacement = Some(SavedReplacement { file: None });
    after.draft.files.clear();
    prepared.apply(&mut after).unwrap();
    assert_eq!(
        after.draft.items[0].replacement,
        document.draft.items[0].replacement
    );
    assert_eq!(after.draft.files, vec![file]);
    assert!(!root.path().join("existing/replacement.mkv").exists());
    after.replacements[0].media_version_id = Some(31);
    assert!(prepared.apply(&mut after).is_err());
}

#[test]
fn apply_refuses_changed_upload_owner_or_hash_without_io() {
    let document = super::super::model::tests::fixture();
    let mut draft = captured(&document);
    let upload = CapturedArtwork {
        upload_id: 1,
        scope_item_id: 10,
        role: 1,
        file: SavedFile {
            relative_path: format!(".ersatzrs-saved/artwork/{}.png", "a".repeat(64)),
            sha256: "a".repeat(64),
            byte_length: 12,
        },
    };
    let prepared = PreparedAssets {
        identity: (
            draft.addon_id.clone(),
            draft.provider_id.clone(),
            draft.source_url.clone(),
        ),
        artwork: vec![upload.clone()],
        files: vec![upload.file.clone()],
        replacements: draft
            .draft
            .items
            .iter()
            .map(|item| (item.content.provider_id.clone(), (None, None)))
            .collect(),
    };
    draft.artwork.push(upload);
    prepared.apply(&mut draft).unwrap();
    draft.artwork[0].scope_item_id = 11;
    assert!(prepared.apply(&mut draft).is_err());
    draft.artwork[0].scope_item_id = 10;
    draft.artwork[0].file.sha256 = "b".repeat(64);
    assert!(prepared.apply(&mut draft).is_err());
}

#[test]
fn unchanged_receiving_intent_preserves_exact_portable_reference_and_draft_inventory() {
    let mut document = super::super::model::tests::fixture();
    let file = SavedFile {
        relative_path: "existing.mkv".into(),
        sha256: "a".repeat(64),
        byte_length: 10,
    };
    document.draft.items[0].replacement = Some(SavedReplacement {
        file: Some(file.clone()),
    });
    let mut draft = captured(&document);
    let prepared = PreparedAssets {
        identity: (
            draft.addon_id.clone(),
            draft.provider_id.clone(),
            draft.source_url.clone(),
        ),
        artwork: Vec::new(),
        files: vec![file.clone()],
        replacements: draft
            .draft
            .items
            .iter()
            .map(|item| {
                (
                    item.content.provider_id.clone(),
                    (None, item.replacement.clone()),
                )
            })
            .collect(),
    };
    prepared.apply(&mut draft).unwrap();
    assert_eq!(draft.draft.files, vec![file]);
    draft.draft.items[0].replacement = None;
    assert!(prepared.apply(&mut draft).is_err());
}

#[tokio::test]
async fn newly_submitted_owned_upload_is_installed_before_post_edit_capture() {
    use ersatzrs_core::repositories::{AddonMediaListEditorMetadata, AddonMediaListNfoMetadata};
    use sha2::{Digest as _, Sha256};

    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
        VALUES ('org.example.media','1.0.0',1,'fixture','fixture','{}','addons/fixture','2026-09-13','2026-09-13');
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name")
        VALUES (1,'org.example.media','shared-list','https://example.invalid/list','Fixture');
    "#).execute(&pool).await.unwrap();
    let bytes = b"\x89PNG\r\n\x1a\nfixture image";
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(bytes) {
        write!(hash, "{byte:02x}").unwrap();
    }
    sqlx::query(r#"INSERT INTO "AddonMediaListArtworkUpload" ("Id","AddonMediaListId","ScopeItemId","Role","ContentType","Extension","Bytes","Sha256","CreatedAt","UpdatedAt") VALUES (41,1,0,1,'image/png','png',?,?,'2026-09-13','2026-09-13')"#)
        .bind(bytes.as_slice()).bind(&hash).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let before = super::super::capture::draft_in_tx(&mut tx, 1)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let document = SavedListDocument {
        version: super::super::model::SAVED_LIST_VERSION,
        addon_id: before.addon_id.clone(),
        provider_id: before.provider_id.clone(),
        source_url: before.source_url.clone(),
        draft: before.draft.clone(),
        published: None,
    };
    let mut save = save();
    let url = format!("{}41", crate::addon_artwork::UPLOAD_URL_PREFIX);
    save.metadata = Some(AddonMediaListEditorMetadata {
        metadata: AddonMediaListNfoMetadata {
            artwork_urls: vec![url.clone()],
            ..Default::default()
        },
        etag: String::new(),
    });
    let root = tempfile::tempdir().unwrap();
    let tree = super::super::assembly::lock_managed_tree(root.path()).unwrap();
    let prepared = PreparedAssets::prepare(&pool, tree, &document, &before, &save)
        .await
        .unwrap();
    let relative = format!(".ersatzrs-saved/artwork/{hash}.png");
    assert_eq!(std::fs::read(root.path().join(&relative)).unwrap(), bytes);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(r#"UPDATE "AddonMediaList" SET "MetadataOverrideJson"=? WHERE "Id"=1"#)
        .bind(serde_json::json!({"artwork_urls": [url]}).to_string())
        .execute(&mut *tx)
        .await
        .unwrap();
    let mut after = super::super::capture::draft_in_tx(&mut tx, 1)
        .await
        .unwrap();
    prepared.apply(&mut after).unwrap();
    assert_eq!(after.draft.files.len(), 1);
    assert_eq!(after.draft.files[0].relative_path, relative);
    tx.rollback().await.unwrap();
}
