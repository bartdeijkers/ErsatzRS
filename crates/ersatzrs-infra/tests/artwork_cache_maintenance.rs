mod common;

use ersatzrs_infra::artwork_cache_maintenance::ArtworkCacheMaintenance;
use sqlx::{Executor, SqlitePool};
use std::{
    path::Path,
    time::{Duration, UNIX_EPOCH},
};

async fn artwork(pool: &SqlitePool, path: &str, source: Option<&Path>, kind: i64) {
    sqlx::query(
        r#"INSERT INTO "Artwork" ("ArtworkKind", "Path", "SourcePath", "DateAdded", "DateUpdated")
        VALUES (?, ?, ?, '2026-01-01', '2026-01-01')"#,
    )
    .bind(kind)
    .bind(path)
    .bind(source.map(|path| path.to_str().unwrap()))
    .execute(pool)
    .await
    .unwrap();
}

fn write(root: &Path, relative: &str) -> std::path::PathBuf {
    let path = root.join(relative);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(&path, b"synthetic-image").unwrap();
    path
}

#[tokio::test]
async fn cache_prunes_only_unreferenced_files_and_preserves_roots_originals_and_uploads() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    let original = write(root.path(), "artwork/original.jpg");
    let source = write(root.path(), "artwork/source.jpg");
    artwork(&pool, original.to_str().unwrap(), None, 0).await;
    artwork(&pool, "AA11", Some(&source), 0).await;
    pool.execute(r#"UPDATE "Artwork" SET "BlurHash43" = 'blur', "BlurHash54" = 'other', "BlurHash64" = 'third';
        INSERT INTO "ChannelWatermark" ("DurationSeconds", "FrequencyMinutes", "HorizontalMarginPercent",
        "Image", "ImageSource", "Location", "Mode", "Opacity", "PlaceWithinSourceContent", "Size",
        "VerticalMarginPercent", "WidthPercent", "ZIndex") VALUES (0,0,0,'watermark',0,0,0,100,0,0,0,10,0)"#)
        .await.unwrap();
    let keep = [
        "artwork/posters/AA/AA11",
        "artwork/posters/400/AA/AA11",
        "artwork/posters/400/Ym/Ymx1cg",
        "artwork/posters/b3RoZXI",
        "artwork/posters/dGhpcmQ",
        "artwork/watermarks/watermark",
        "watermarks/unregistered.png",
        "iptv/logos/unregistered.png",
    ];
    for file in keep {
        write(root.path(), file);
    }
    let stale = write(root.path(), "artwork/posters/dead/stale");
    let report = ArtworkCacheMaintenance::new(pool, root.path().to_owned())
        .run(0)
        .await
        .unwrap();
    assert_eq!(report.files_deleted, 1);
    assert_eq!(report.bytes_deleted, 15);
    assert_eq!(report.files_restored, 0);
    assert!(!stale.exists());
    assert!(!stale.parent().unwrap().exists());
    assert!(root.path().join("artwork/posters").is_dir());
    assert!(original.exists());
    assert!(source.exists());
    for file in keep {
        assert!(root.path().join(file).is_file(), "{file}");
    }
}

#[tokio::test]
async fn cache_restores_shared_owners_and_kinds_repoints_atomically_and_is_idempotent() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    let source = write(root.path(), "original/cover.PNG");
    std::fs::File::options()
        .write(true)
        .open(&source)
        .unwrap()
        .set_modified(UNIX_EPOCH + Duration::from_secs(1_700_000_000))
        .unwrap();
    for kind in [0, 0, 1, 2, 3, 4] {
        artwork(&pool, "DEADBEEF", Some(&source), kind).await;
    }
    let service = ArtworkCacheMaintenance::new(pool.clone(), root.path().to_owned());
    let (one, two) = tokio::join!(service.run(0), service.run(0));
    let one = one.unwrap();
    let two = two.unwrap();
    assert_eq!(one.files_restored + two.files_restored, 1);
    assert_eq!(one.rows_repointed + two.rows_repointed, 6);
    let paths: Vec<String> = sqlx::query_scalar(r#"SELECT "Path" FROM "Artwork""#)
        .fetch_all(&pool)
        .await
        .unwrap();
    let name = &paths[0];
    assert!(paths.iter().all(|path| path == name));
    assert_eq!(name.len(), 40);
    assert_ne!(name, "DEADBEEF");
    for kind in ["posters", "thumbnails", "fanart"] {
        let cached = root
            .path()
            .join("artwork")
            .join(kind)
            .join(&name[..2])
            .join(name);
        assert_eq!(std::fs::read(cached).unwrap(), b"synthetic-image");
    }
    for kind in ["iptv/logos", "watermarks"] {
        assert_eq!(
            std::fs::read(root.path().join(kind).join(name)).unwrap(),
            b"synthetic-image"
        );
    }
    let final_pass = service.run(0).await.unwrap();
    assert_eq!(
        final_pass.files_deleted + final_pass.files_restored + final_pass.file_failures,
        0
    );
    assert!(source.exists());
}

#[tokio::test]
async fn cache_skips_remote_embedded_and_direct_sources_but_retries_missing_local_images() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    let media = write(root.path(), "original/movie.mkv");
    let missing = root.path().join("original/late.webp");
    artwork(
        &pool,
        "AAAA",
        Some(Path::new("https://invalid.example/image.png")),
        0,
    )
    .await;
    artwork(&pool, "BBBB", Some(&media), 0).await;
    artwork(&pool, "CCCC", Some(&missing), 0).await;
    artwork(
        &pool,
        "jellyfin://Items/fixture/Images/Primary",
        Some(&media),
        0,
    )
    .await;
    artwork(&pool, media.to_str().unwrap(), None, 0).await;
    let service = ArtworkCacheMaintenance::new(pool.clone(), root.path().to_owned());
    let report = service.run(0).await.unwrap();
    assert_eq!(report.files_restored, 0);
    assert_eq!(report.unrestorable, 2);
    assert_eq!(report.file_failures, 1);
    std::fs::write(&missing, b"recovered").unwrap();
    let report = service.run(0).await.unwrap();
    assert_eq!(report.files_restored, 1);
    assert_eq!(report.rows_repointed, 1);
    let remaining: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "Artwork" WHERE "Path" IN ('AAAA','BBBB')"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(remaining, 2);
    assert_eq!(std::fs::read(media).unwrap(), b"synthetic-image");
}

#[tokio::test]
async fn cache_publication_failure_keeps_old_references_and_next_pass_recovers() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    let source = write(root.path(), "original/cover.jpg");
    artwork(&pool, "AABB", Some(&source), 0).await;
    pool.execute(
        r#"CREATE TRIGGER fail_repoint BEFORE UPDATE OF "Path" ON "Artwork"
        BEGIN SELECT RAISE(ABORT, 'synthetic publication failure'); END"#,
    )
    .await
    .unwrap();
    // A newly configured data directory need not exist before the first repair.
    let service = ArtworkCacheMaintenance::new(pool.clone(), root.path().join("new-data"));
    assert!(service.run(0).await.is_err());
    let old: String = sqlx::query_scalar(r#"SELECT "Path" FROM "Artwork""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old, "AABB");
    pool.execute("DROP TRIGGER fail_repoint").await.unwrap();
    let report = service.run(0).await.unwrap();
    assert_eq!(report.rows_repointed, 1);
    assert_eq!(report.files_restored, 1);
}

// Unix symlink construction; the production traversal uses portable file types.
#[cfg(unix)]
#[tokio::test]
async fn cache_never_traverses_symlinks_or_writes_through_linked_shards() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    let protected = write(outside.path(), "protected.jpg");
    std::fs::create_dir(root.path().join("artwork")).unwrap();
    std::os::unix::fs::symlink(outside.path(), root.path().join("artwork/posters")).unwrap();
    std::os::unix::fs::symlink(&protected, root.path().join("artwork/link")).unwrap();
    artwork(&pool, "ABCD", Some(&protected), 0).await;
    let report = ArtworkCacheMaintenance::new(pool, root.path().to_owned())
        .run(0)
        .await
        .unwrap();
    assert_eq!(report.files_deleted, 0);
    assert_eq!(report.files_restored, 0);
    assert_eq!(report.file_failures, 1);
    assert_eq!(std::fs::read(protected).unwrap(), b"synthetic-image");
    assert_eq!(std::fs::read_dir(outside.path()).unwrap().count(), 1);
}

#[tokio::test]
async fn cache_pages_references_and_repairs_past_first_thousand_rows() {
    let pool = common::test_db().await;
    let root = tempfile::tempdir().unwrap();
    pool.execute(
        r#"WITH RECURSIVE n(i) AS (VALUES(1) UNION ALL SELECT i+1 FROM n WHERE i<1000)
        INSERT INTO "Artwork" ("ArtworkKind", "Path", "DateAdded", "DateUpdated")
        SELECT 0, NULL, '2026-01-01', '2026-01-01' FROM n"#,
    )
    .await
    .unwrap();
    let source = write(root.path(), "original/late.jpg");
    artwork(&pool, "FFFF", Some(&source), 0).await;
    artwork(&pool, "AAAA", None, 0).await;
    let retained = write(root.path(), "artwork/posters/AA/AAAA");
    let report = ArtworkCacheMaintenance::new(pool, root.path().to_owned())
        .run(0)
        .await
        .unwrap();
    assert_eq!(report.files_restored, 1);
    assert_eq!(report.rows_repointed, 1);
    assert_eq!(report.file_failures, 0);
    assert!(retained.exists());
}
