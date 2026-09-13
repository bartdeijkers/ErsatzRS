use super::*;
use std::fs;

async fn fixture() -> (tempfile::TempDir, DbPool, CapturedReplacement) {
    let root = tempfile::tempdir().unwrap();
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","MediaKind","MediaSourceId","Name") VALUES (1,3,1,'Fixture');
        INSERT INTO "LibraryPath" ("Id","LibraryId") VALUES (1,1);
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (10,1);
        INSERT INTO "OtherVideo" ("Id") VALUES (10);
        INSERT INTO "MediaVersion" ("Id","OtherVideoId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind")
        VALUES (20,10,'2026-09-13','2026-09-13','00:00:01',1,1,0),
               (21,10,'2026-09-13','2026-09-13','00:00:02',1,1,0);
        INSERT INTO "MediaStream" ("MediaVersionId","Index","MediaStreamKind","Channels","Default","Forced")
        VALUES (20,0,1,0,0,0),(21,0,1,0,0,0);
    "#).execute(&pool).await.unwrap();
    fs::write(root.path().join("selected.mkv"), b"selected exact bytes").unwrap();
    fs::write(root.path().join("other.mkv"), b"other version bytes").unwrap();
    for (id, version, path) in [(30, 20, "selected.mkv"), (31, 21, "other.mkv")] {
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash") VALUES (?,?,?,?)"#,
        )
        .bind(id)
        .bind(version)
        .bind(root.path().join(path).to_str().unwrap())
        .bind(path)
        .execute(&pool)
        .await
        .unwrap();
    }
    (
        root,
        pool,
        CapturedReplacement {
            provider_id: "selection".into(),
            media_item_id: 10,
            media_version_id: Some(20),
        },
    )
}

#[tokio::test]
async fn saved_replacement_source_preserves_exact_selected_version() {
    let (root, pool, mut captured) = fixture().await;
    let selected = inspect_owned_replacement(&pool, root.path(), &captured)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(selected.relative_path, "selected.mkv");
    assert_eq!(selected.byte_length, b"selected exact bytes".len() as u64);
    assert_eq!(
        crate::addon_media_list_saved::files::verify_file(root.path(), &selected)
            .await
            .unwrap(),
        crate::addon_media_list_saved::files::SavedFileState::Verified
    );
    captured.media_version_id = Some(21);
    let other = inspect_owned_replacement(&pool, root.path(), &captured)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(other.relative_path, "other.mkv");
    assert_ne!(other.sha256, selected.sha256);
    captured.media_version_id = None;
    assert!(inspect_owned_replacement(&pool, root.path(), &captured)
        .await
        .unwrap()
        .is_none());
    captured.media_version_id = Some(999);
    assert!(inspect_owned_replacement(&pool, root.path(), &captured)
        .await
        .unwrap()
        .is_none());
    captured.media_version_id = Some(20);
    captured.media_item_id = 999;
    assert!(inspect_owned_replacement(&pool, root.path(), &captured)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn saved_replacement_source_missing_outside_or_ambiguous_remains_unresolved() {
    let (root, pool, captured) = fixture().await;
    let outside = tempfile::tempdir().unwrap();
    let external = outside.path().join("outside.mkv");
    fs::write(&external, b"outside bytes").unwrap();
    for path in [external, root.path().join("missing.mkv")] {
        sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=30"#)
            .bind(path.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        assert!(inspect_owned_replacement(&pool, root.path(), &captured)
            .await
            .unwrap()
            .is_none());
    }
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=30"#)
        .bind(root.path().join("selected.mkv").to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("MediaVersionId","Path","PathHash") VALUES (20,?,'duplicate')"#,
    )
    .bind(root.path().join("other.mkv").to_str().unwrap())
    .execute(&pool)
    .await
    .unwrap();
    assert!(inspect_owned_replacement(&pool, root.path(), &captured)
        .await
        .unwrap()
        .is_none());
}

#[tokio::test]
async fn saved_replacement_source_unavailable_local_media_and_invalid_root_refuse_selection() {
    let (root, pool, captured) = fixture().await;
    sqlx::query(r#"UPDATE "MediaItem" SET "State"=1 WHERE "Id"=10"#)
        .execute(&pool)
        .await
        .unwrap();
    assert!(inspect_owned_replacement(&pool, root.path(), &captured)
        .await
        .unwrap()
        .is_none());
    assert!(
        inspect_owned_replacement(&pool, &root.path().join("selected.mkv"), &captured)
            .await
            .is_err()
    );
}

// Creating symlinks is privilege-free on Unix. Portable containment, missing
// sources and exact-version behavior are exercised above on all native targets.
#[cfg(unix)]
#[tokio::test]
async fn saved_replacement_source_rejects_symlinks_without_canonicalizing_them_away() {
    use std::os::unix::fs::symlink;
    let (root, pool, captured) = fixture().await;
    let link = root.path().join("linked.mkv");
    symlink(root.path().join("selected.mkv"), &link).unwrap();
    sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=30"#)
        .bind(link.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    assert!(inspect_owned_replacement(&pool, root.path(), &captured)
        .await
        .is_err());
    let root_link = root.path().join("root-link");
    symlink(root.path(), &root_link).unwrap();
    assert!(inspect_owned_replacement(&pool, &root_link, &captured)
        .await
        .is_err());
}

// These names are legal Unix files but deliberately excluded by the shared
// portable validator; Windows cannot create the equivalent colon-name fixture.
#[cfg(unix)]
#[tokio::test]
async fn saved_replacement_source_nonportable_local_names_remain_unresolved() {
    let (root, pool, captured) = fixture().await;
    for name in ["Movie: Part 1.mkv", "Movie.mkv."] {
        let local = root.path().join(name);
        fs::write(&local, b"local selected bytes").unwrap();
        sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=30"#)
            .bind(local.to_str().unwrap())
            .execute(&pool)
            .await
            .unwrap();
        assert!(inspect_owned_replacement(&pool, root.path(), &captured)
            .await
            .unwrap()
            .is_none());
        assert_eq!(fs::read(&local).unwrap(), b"local selected bytes");
    }
}
