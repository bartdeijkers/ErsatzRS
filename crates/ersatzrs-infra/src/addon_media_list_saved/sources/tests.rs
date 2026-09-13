use super::*;
use crate::addon_media_list_saved::model::SavedFile;

fn digest(bytes: &[u8]) -> String {
    let mut result = String::new();
    for byte in Sha256::digest(bytes) {
        write!(result, "{byte:02x}").unwrap();
    }
    result
}

async fn artwork_fixture() -> (DbPool, CapturedArtwork, Vec<u8>) {
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    // Only columns consumed by this read-only adapter; no application state is saved.
    sqlx::query(
        r#"CREATE TABLE "AddonMediaListArtworkUpload" (
        "Id" INTEGER, "AddonMediaListId" INTEGER, "ScopeItemId" INTEGER, "Role" INTEGER,
        "ContentType" TEXT, "Extension" TEXT, "Bytes" BLOB, "Sha256" TEXT)"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let bytes = b"\x89PNG\r\n\x1a\nsynthetic image".to_vec();
    let hash = digest(&bytes);
    sqlx::query(
        r#"INSERT INTO "AddonMediaListArtworkUpload" VALUES (41,1,10,1,'image/png','png',?,?)"#,
    )
    .bind(&bytes)
    .bind(&hash)
    .execute(&pool)
    .await
    .unwrap();
    let captured = CapturedArtwork {
        upload_id: 41,
        scope_item_id: 10,
        role: 1,
        file: SavedFile {
            relative_path: format!(".ersatzrs-saved/artwork/{hash}.png"),
            sha256: hash,
            byte_length: bytes.len() as u64,
        },
    };
    (pool, captured, bytes)
}

#[tokio::test]
async fn saved_source_artwork_revalidates_upload_list_scope_and_role() {
    let (pool, mut captured, bytes) = artwork_fixture().await;
    assert_eq!(
        load_owned_artwork(&pool, 1, &captured).await.unwrap(),
        bytes
    );
    assert!(load_owned_artwork(&pool, 2, &captured).await.is_err());
    captured.upload_id = 42;
    assert!(load_owned_artwork(&pool, 1, &captured).await.is_err());
    captured.upload_id = 41;
    captured.scope_item_id = 11;
    assert!(load_owned_artwork(&pool, 1, &captured).await.is_err());
    captured.scope_item_id = 10;
    captured.role = 2;
    assert!(load_owned_artwork(&pool, 1, &captured).await.is_err());
}

#[tokio::test]
async fn saved_source_artwork_rejects_changed_bytes_mime_extension_and_oversize() {
    let (pool, mut captured, mut bytes) = artwork_fixture().await;
    let original = bytes.clone();
    *bytes.last_mut().unwrap() = b'X';
    sqlx::query(r#"UPDATE "AddonMediaListArtworkUpload" SET "Bytes"=?"#)
        .bind(&bytes)
        .execute(&pool)
        .await
        .unwrap();
    assert!(load_owned_artwork(&pool, 1, &captured).await.is_err());
    sqlx::query(r#"UPDATE "AddonMediaListArtworkUpload" SET "Bytes"=?,"ContentType"='image/jpeg'"#)
        .bind(&original)
        .execute(&pool)
        .await
        .unwrap();
    assert!(load_owned_artwork(&pool, 1, &captured).await.is_err());
    sqlx::query(
        r#"UPDATE "AddonMediaListArtworkUpload" SET "ContentType"='image/png',"Extension"='jpg'"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(load_owned_artwork(&pool, 1, &captured).await.is_err());
    sqlx::query(
        r#"UPDATE "AddonMediaListArtworkUpload" SET "Extension"='png',"Bytes"=zeroblob(?)"#,
    )
    .bind(i64::try_from(MAX_ARTWORK_BYTES + 1).unwrap())
    .execute(&pool)
    .await
    .unwrap();
    assert!(load_owned_artwork(&pool, 1, &captured).await.is_err());
    captured.file.byte_length = (MAX_ARTWORK_BYTES + 1) as u64;
    assert!(load_owned_artwork(&pool, 1, &captured).await.is_err());
}

fn retained_fixture(storage: &Path, bytes: &[u8]) -> AddonMediaListRevisionFile {
    let hash = digest(bytes);
    let path = format!(
        "{REVISION_BLOB_STORE_DIRECTORY}/{}/1/{}/{hash}",
        "a".repeat(32),
        &hash[..2]
    );
    let source = storage.join(&path);
    fs::create_dir_all(source.parent().unwrap()).unwrap();
    fs::write(source, bytes).unwrap();
    AddonMediaListRevisionFile {
        blob_id: 1,
        provider_id: None,
        published_relative_path: "poster.png".into(),
        storage_relative_path: path,
        sha256: hash,
        byte_length: bytes.len() as u64,
    }
}

#[test]
fn saved_source_retained_enforces_list_storage_owner_and_confined_path() {
    let storage = tempfile::tempdir().unwrap();
    let managed = tempfile::tempdir().unwrap();
    let mut file = retained_fixture(storage.path(), b"retained A");
    let owner = "a".repeat(32);
    assert!(install_retained_file(managed.path(), storage.path(), &owner, 2, &file).is_err());
    assert!(
        install_retained_file(managed.path(), storage.path(), &"b".repeat(32), 1, &file).is_err()
    );
    file.storage_relative_path = "../outside".into();
    assert!(install_retained_file(managed.path(), storage.path(), &owner, 1, &file).is_err());
    assert_eq!(fs::read_dir(managed.path()).unwrap().count(), 0);
}

#[test]
fn saved_source_retained_installs_and_reuses_a_independently_of_mutable_b() {
    let storage = tempfile::tempdir().unwrap();
    let managed = tempfile::tempdir().unwrap();
    let file = retained_fixture(storage.path(), b"retained A");
    let active = managed.path().join("poster.png");
    fs::write(&active, b"mutable B").unwrap();
    for _ in 0..2 {
        install_retained_file(managed.path(), storage.path(), &"a".repeat(32), 1, &file).unwrap();
    }
    let saved = super::super::published::saved_file(&file);
    assert_eq!(
        fs::read(managed.path().join(saved.relative_path)).unwrap(),
        b"retained A"
    );
    assert_eq!(fs::read(active).unwrap(), b"mutable B");
}

#[test]
fn saved_source_retained_rejects_corrupt_or_nonregular_source() {
    let storage = tempfile::tempdir().unwrap();
    let managed = tempfile::tempdir().unwrap();
    let file = retained_fixture(storage.path(), b"retained A");
    let source = storage.path().join(&file.storage_relative_path);
    fs::write(&source, b"corrupt B").unwrap();
    assert!(
        install_retained_file(managed.path(), storage.path(), &"a".repeat(32), 1, &file).is_err()
    );
    fs::remove_file(&source).unwrap();
    fs::create_dir(source).unwrap();
    assert!(
        install_retained_file(managed.path(), storage.path(), &"a".repeat(32), 1, &file).is_err()
    );
    assert!(!managed
        .path()
        .join(super::super::published::saved_file(&file).relative_path)
        .exists());
}
