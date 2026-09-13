use super::*;
use crate::db::DbPool;

async fn fixture() -> (tempfile::TempDir, DbPool, PathBuf) {
    let directory = tempfile::tempdir().unwrap();
    let root = dunce::canonicalize(directory.path()).unwrap();
    let folder = root.join("Copied original");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("list.playlist.yml"), b"retained manifest bytes").unwrap();
    std::fs::write(
        folder.join("replacement.mkv"),
        b"retained replacement bytes",
    )
    .unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    assert!(sqlx::query_scalar::<_, bool>("PRAGMA foreign_keys")
        .fetch_one(&pool)
        .await
        .unwrap());
    sqlx::raw_sql(r#"INSERT INTO "AddonInstallation"
        ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
        VALUES ('org.example.storage','1',1,'fixture','fixture','{}','addons/fixture','2026-09-13','2026-09-13');
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
        INSERT INTO "AddonMediaList" ("Id","AddonId","ProviderId","SourceUrl","Name") VALUES
            (10,'org.example.storage','one','https://example.invalid/one','One'),
            (11,'org.example.storage','two','https://example.invalid/two','Two');"#)
        .execute(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "AddonMediaSource" ("AddonId","MediaSourceId","Name","RootPath") VALUES ('org.example.storage',1,'Fixture',?)"#)
        .bind(root.to_str().unwrap()).execute(&pool).await.unwrap();
    (directory, pool, folder)
}

#[tokio::test]
async fn existing_folder_registration_preserves_files_and_is_idempotent() {
    let (_directory, pool, folder) = fixture().await;
    let mut tx = pool.begin().await.unwrap();
    let first =
        register_existing_list_library_in_transaction(&mut tx, 10, "org.example.storage", &folder)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let second =
        register_existing_list_library_in_transaction(&mut tx, 10, "org.example.storage", &folder)
            .await
            .unwrap();
    tx.commit().await.unwrap();
    assert_eq!(first, second);
    let counts: (i64,i64,i64,i64) = sqlx::query_as(r#"SELECT (SELECT COUNT(*) FROM "Library"), (SELECT COUNT(*) FROM "LocalLibrary"), (SELECT COUNT(*) FROM "LibraryPath"), (SELECT COUNT(*) FROM "AddonMediaListLibrary")"#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1, 1));
    let paths: (String,String,String) = sqlx::query_as(r#"SELECT "managed"."FolderName", "managed"."ManifestPath", "path"."Path" FROM "AddonMediaListLibrary" AS "managed" JOIN "LibraryPath" AS "path" ON "path"."LibraryId"="managed"."LibraryId" WHERE "managed"."AddonMediaListId"=10"#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(
        paths,
        (
            "Copied original".into(),
            folder.join("list.playlist.yml").to_str().unwrap().into(),
            folder.to_str().unwrap().into()
        )
    );
    assert_eq!(
        std::fs::read(folder.join("list.playlist.yml")).unwrap(),
        b"retained manifest bytes"
    );
    assert_eq!(
        std::fs::read(folder.join("replacement.mkv")).unwrap(),
        b"retained replacement bytes"
    );
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 2);
}

#[tokio::test]
async fn existing_folder_registration_refuses_other_owners_and_relocation() {
    let (_directory, pool, folder) = fixture().await;
    let mut tx = pool.begin().await.unwrap();
    register_existing_list_library_in_transaction(&mut tx, 10, "org.example.storage", &folder)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    for (list_id, path) in [
        (11, folder.clone()),
        (10, folder.with_file_name("Different")),
    ] {
        let mut tx = pool.begin().await.unwrap();
        assert!(register_existing_list_library_in_transaction(
            &mut tx,
            list_id,
            "org.example.storage",
            &path
        )
        .await
        .is_err());
        tx.rollback().await.unwrap();
    }
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "AddonMediaListLibrary""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    assert!(!folder.with_file_name("Different").exists());
    assert_eq!(
        std::fs::read(folder.join("list.playlist.yml")).unwrap(),
        b"retained manifest bytes"
    );
}

#[tokio::test]
async fn existing_folder_registration_requires_enabled_owner_and_direct_containment() {
    let (_directory, pool, folder) = fixture().await;
    for path in [
        folder.join("nested"),
        folder.parent().unwrap().to_path_buf(),
        folder.parent().unwrap().join("..").join("outside"),
    ] {
        let mut tx = pool.begin().await.unwrap();
        assert!(register_existing_list_library_in_transaction(
            &mut tx,
            10,
            "org.example.storage",
            &path
        )
        .await
        .is_err());
        tx.rollback().await.unwrap();
    }
    sqlx::query(r#"UPDATE "AddonInstallation" SET "Enabled"=0"#)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(register_existing_list_library_in_transaction(
        &mut tx,
        10,
        "org.example.storage",
        &folder
    )
    .await
    .is_err());
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Library""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
