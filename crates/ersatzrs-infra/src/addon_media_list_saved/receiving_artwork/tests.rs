use std::{
    fmt::Write as _,
    fs,
    path::Path,
    time::{Duration, Instant},
};

use ersatzrs_core::repositories::AddonMediaListArtworkRole;
use sha2::{Digest as _, Sha256};

use super::{bind_artwork_in_tx, ArtworkStaging};
use crate::{
    addon_media_list_saved::model::SavedFile,
    db::{Db, DbPool},
};

async fn fixture(connections: u32) -> (tempfile::TempDir, DbPool) {
    let root = tempfile::tempdir().unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(root.path().join("fixture.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(connections)
        .connect_with(options)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation" ("AddonId","Version","Enabled","PackageSha256","ContentSha256",
            "ManifestJson","InstallPath","InstalledAt","UpdatedAt")
        VALUES ('org.example.artwork','1.0.0',1,'fixture','fixture','{}','addons/fixture','2026-09-13','2026-09-13');
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name")
        VALUES (1,'org.example.artwork','list','https://example.invalid/list','List');
        INSERT INTO "AddonMediaListItem" ("Id","AddonMediaListId","ProviderId","DisplayTitle","Kind","Rank","Title")
        VALUES (10,1,'item','Item',4,0,'Item');
    "#).execute(&pool).await.unwrap();
    (root, pool)
}

fn image(root: &Path, index: usize, size: usize) -> SavedFile {
    let mut bytes = vec![b'x'; size];
    bytes[..8].copy_from_slice(b"\x89PNG\r\n\x1a\n");
    bytes[8..16].copy_from_slice(&(index as u64).to_le_bytes());
    let mut hash = String::new();
    for byte in Sha256::digest(&bytes) {
        write!(hash, "{byte:02x}").unwrap();
    }
    let relative_path = format!("image-{index}.png");
    fs::write(root.join(&relative_path), &bytes).unwrap();
    SavedFile {
        relative_path,
        sha256: hash,
        byte_length: bytes.len() as u64,
    }
}

async fn fresh_owner(tx: &mut sqlx::Transaction<'_, Db>) {
    sqlx::query(
        r#"INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name")
        VALUES (2,'org.example.artwork','fresh','https://example.invalid/fresh','Fresh')"#,
    )
    .execute(&mut **tx)
    .await
    .unwrap();
}

#[tokio::test]
async fn receiving_artwork_staging_does_not_hold_main_writer_lock() {
    let (root, pool) = fixture(2).await;
    let file = image(root.path(), 0, 128);
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    staging.stage_file(root.path(), &file).await.unwrap();
    tokio::time::timeout(Duration::from_secs(10), async {
        let mut tx = crate::db::begin_write(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "AddonMediaList" SET "Name"='Independent writer' WHERE "Id"=1"#)
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    })
    .await
    .expect("TEMP staging must leave the main database writer available");
}

#[tokio::test]
async fn receiving_artwork_owner_and_upload_rollback_atomically() {
    let (root, pool) = fixture(2).await;
    let file = image(root.path(), 0, 128);
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    staging.stage_file(root.path(), &file).await.unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    fresh_owner(&mut tx).await;
    let url = bind_artwork_in_tx(&mut tx, 2, None, AddonMediaListArtworkRole::Poster, &file)
        .await
        .unwrap();
    assert!(crate::addon_artwork::upload_id(&url).is_some());
    tx.rollback().await.unwrap();
    let owners: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaList" WHERE "Id"=2"#)
        .fetch_one(&pool)
        .await
        .unwrap();
    let uploads: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtworkUpload""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!((owners, uploads), (0, 0));
}

#[tokio::test]
async fn receiving_artwork_bind_is_owned_idempotent_and_needs_no_source_file() {
    let (root, pool) = fixture(2).await;
    let file = image(root.path(), 0, 128);
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    staging.stage_file(root.path(), &file).await.unwrap();
    fs::remove_file(root.path().join(&file.relative_path)).unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    fresh_owner(&mut tx).await;
    let mut urls = std::collections::BTreeSet::new();
    for (list, item, role) in [
        (1, None, AddonMediaListArtworkRole::Poster),
        (1, None, AddonMediaListArtworkRole::Fanart),
        (1, Some(10), AddonMediaListArtworkRole::Poster),
        (2, None, AddonMediaListArtworkRole::Poster),
    ] {
        let first = bind_artwork_in_tx(&mut tx, list, item, role, &file)
            .await
            .unwrap();
        assert_eq!(
            bind_artwork_in_tx(&mut tx, list, item, role, &file)
                .await
                .unwrap(),
            first
        );
        assert!(urls.insert(first));
    }
    tx.commit().await.unwrap();
    let owners: Vec<(i64, i64, i32)> = sqlx::query_as(
        r#"SELECT "AddonMediaListId","ScopeItemId","Role"
        FROM "AddonMediaListArtworkUpload" ORDER BY "AddonMediaListId","ScopeItemId","Role""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(owners, vec![(1, 0, 1), (1, 0, 2), (1, 10, 1), (2, 0, 1)]);
}

#[tokio::test]
async fn receiving_artwork_refuses_invalid_unstaged_and_changed_references() {
    let (root, pool) = fixture(2).await;
    let file = image(root.path(), 0, 128);
    let unstaged = image(root.path(), 1, 128);
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    let invalid_bytes = b"not an image";
    let mut hash = String::new();
    for byte in Sha256::digest(invalid_bytes) {
        write!(hash, "{byte:02x}").unwrap();
    }
    fs::write(root.path().join("bad.png"), invalid_bytes).unwrap();
    let bad = SavedFile {
        relative_path: "bad.png".into(),
        sha256: hash,
        byte_length: invalid_bytes.len() as u64,
    };
    assert!(staging.stage_file(root.path(), &bad).await.is_err());
    staging.stage_file(root.path(), &file).await.unwrap();
    let mut changed = file.clone();
    changed.byte_length += 1;
    let mut tx = staging.begin_projection().await.unwrap();
    for reference in [&unstaged, &changed, &bad] {
        assert!(bind_artwork_in_tx(
            &mut tx,
            1,
            None,
            AddonMediaListArtworkRole::Poster,
            reference
        )
        .await
        .is_err());
    }
    assert!(bind_artwork_in_tx(
        &mut tx,
        1,
        Some(999),
        AddonMediaListArtworkRole::Poster,
        &file
    )
    .await
    .is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn receiving_artwork_staging_connection_drop_discards_temp_table() {
    let (root, pool) = fixture(1).await;
    let file = image(root.path(), 0, 128);
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    staging.stage_file(root.path(), &file).await.unwrap();
    drop(staging);
    let mut connection = pool.acquire().await.unwrap();
    let tables: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_temp_master WHERE type='table' AND name='SavedListArtwork'",
    )
    .fetch_one(&mut *connection)
    .await
    .unwrap();
    assert_eq!(tables, 0);
}

#[tokio::test]
async fn receiving_artwork_bounded_batch_reports_projection_transaction_elapsed() {
    let (root, pool) = fixture(2).await;
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    let mut references = Vec::new();
    for index in 0..16 {
        let file = image(root.path(), index, 256 * 1024);
        staging.stage_file(root.path(), &file).await.unwrap();
        references.push(file);
    }
    let started = Instant::now();
    let mut tx = staging.begin_projection().await.unwrap();
    for reference in &references {
        bind_artwork_in_tx(
            &mut tx,
            1,
            None,
            AddonMediaListArtworkRole::Poster,
            reference,
        )
        .await
        .unwrap();
    }
    tx.commit().await.unwrap();
    eprintln!(
        "16 x 256 KiB receiving artwork projection transaction: {:?}",
        started.elapsed()
    );
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtworkUpload""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 16);
}

fn metadata_document(
    newer: &SavedFile,
    older: &SavedFile,
) -> crate::addon_media_list_saved::model::SavedListDocument {
    use crate::addon_media_list_saved::model::tests::fixture as document_fixture;
    use ersatzrs_core::repositories::AddonMediaListMetadataConflict;
    use serde_json::json;

    let newer_url = format!("./{}", newer.relative_path);
    let older_url = format!("./{}", older.relative_path);
    let mut document = document_fixture();
    document.draft.files = vec![newer.clone(), older.clone()];
    document.draft.metadata.provider = serde_json::from_value(json!({
        "artwork": [{"url": newer_url, "role": "poster", "language":"nl", "width":800}],
        "people": [{"name":"Literal", "thumb":"./not-owned.png"}],
        "unknown": {"url":"./not-owned.png"}
    }))
    .unwrap();
    document.draft.metadata.overrides = serde_json::from_value(json!({
        "artwork_urls": [newer_url, "https://example.invalid/remote.png"]
    }))
    .unwrap();
    document.draft.metadata.conflicts.insert(
        "artwork".into(),
        AddonMediaListMetadataConflict {
            provider_before: json!([{"url":older_url,"role":"poster"}]),
            provider_after: json!([{"url":newer_url,"role":"poster"}]),
            operator_value: json!([{"url":newer_url,"role":"fanart"}]),
        },
    );
    document.draft.items[0].metadata.provider = serde_json::from_value(json!({
        "artwork_urls": [newer_url]
    }))
    .unwrap();
    let published = document.published.as_mut().unwrap();
    published.files = vec![older.clone()];
    published.metadata = serde_json::from_value(json!({
        "artwork": [{"url":older_url,"role":"poster"}]
    }))
    .unwrap();
    published.items[0].metadata = serde_json::from_value(json!({
        "artwork_urls": [older_url]
    }))
    .unwrap();
    document
}

#[tokio::test]
async fn receiving_artwork_maps_draft_conflicts_and_old_publication_independently() {
    let (root, pool) = fixture(2).await;
    let newer = image(root.path(), 0, 128);
    let older = image(root.path(), 1, 128);
    let mut document = metadata_document(&newer, &older);
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    staging
        .stage_document(root.path(), &document)
        .await
        .unwrap();
    fs::remove_file(root.path().join(&newer.relative_path)).unwrap();
    fs::remove_file(root.path().join(&older.relative_path)).unwrap();
    let mut tx = staging.begin_projection().await.unwrap();
    let draft_files = document
        .draft
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file))
        .collect();
    super::bind_metadata_in_tx(&mut tx, 1, None, &mut document.draft.metadata, &draft_files)
        .await
        .unwrap();
    super::bind_metadata_in_tx(
        &mut tx,
        1,
        Some(10),
        &mut document.draft.items[0].metadata,
        &draft_files,
    )
    .await
    .unwrap();
    let published = document.published.as_mut().unwrap();
    let published_files = published
        .files
        .iter()
        .map(|file| (file.relative_path.as_str(), file))
        .collect();
    super::bind_map_in_tx(&mut tx, 1, None, &mut published.metadata, &published_files)
        .await
        .unwrap();
    super::bind_map_in_tx(
        &mut tx,
        1,
        Some(10),
        &mut published.items[0].metadata,
        &published_files,
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();

    let draft = &document.draft.metadata;
    let poster = draft.provider["artwork"][0]["url"].as_str().unwrap();
    let old_poster = published.metadata["artwork"][0]["url"].as_str().unwrap();
    assert!(crate::addon_artwork::upload_id(poster).is_some());
    assert_ne!(poster, old_poster);
    assert_eq!(
        draft.conflicts["artwork"].provider_before[0]["url"],
        old_poster
    );
    assert_eq!(draft.conflicts["artwork"].provider_after[0]["url"], poster);
    assert_ne!(draft.conflicts["artwork"].operator_value[0]["url"], poster);
    assert_ne!(
        draft.overrides["artwork_urls"][0],
        document.draft.items[0].metadata.provider["artwork_urls"][0]
    );
    assert_eq!(draft.provider["artwork"][0]["language"], "nl");
    assert_eq!(draft.provider["artwork"][0]["width"], 800);
    assert_eq!(draft.provider["people"][0]["thumb"], "./not-owned.png");
    assert_eq!(draft.provider["unknown"]["url"], "./not-owned.png");
    assert_eq!(
        draft.overrides["artwork_urls"][1],
        "https://example.invalid/remote.png"
    );
    let uploads: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtworkUpload""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(uploads, 6);
}

#[tokio::test]
async fn receiving_artwork_metadata_refuses_undeclared_files_and_unsupported_owned_roles() {
    use serde_json::json;
    let (root, pool) = fixture(2).await;
    let file = image(root.path(), 0, 128);
    let mut staging = ArtworkStaging::new(&pool).await.unwrap();
    staging.stage_file(root.path(), &file).await.unwrap();
    let files = std::collections::BTreeMap::from([(file.relative_path.as_str(), &file)]);
    let mut tx = staging.begin_projection().await.unwrap();
    for value in [
        json!({"artwork_urls":["./undeclared.png"]}),
        json!({"artwork":[{"url":format!("./{}",file.relative_path),"role":"other"}]}),
    ] {
        let mut metadata = serde_json::from_value(value).unwrap();
        assert!(
            super::bind_map_in_tx(&mut tx, 1, None, &mut metadata, &files)
                .await
                .is_err()
        );
    }
    tx.rollback().await.unwrap();
    let uploads: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListArtworkUpload""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(uploads, 0);
}
