//! End-to-end test for the local music-video + artist scanner. Builds a
//! minimal `Artist/artist.nfo` + `Artist/MV.mkv` + `MV.nfo` tree and asserts
//! the enriched metadata, child rows, and artwork all reach the database.
//!
//! Like [`local_scan.rs`] this is `#[ignore]`-gated on `ffprobe` + the Big
//! Buck Bunny fixture; run with `cargo test -p ersatzrs-scanner -- --ignored`.

#![cfg(not(feature = "mysql"))]

use std::env;
use std::path::{Path, PathBuf};

use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Executor, Row, SqlitePool};

#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; run cargo test -- --ignored to execute it"]
#[allow(clippy::too_many_lines)]
async fn local_music_video_scan_writes_full_metadata_and_child_rows() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let temp = TempDir::new("ersatzrs-local-music-video");
    let library_root = temp.path().join("library");
    let artist_dir = library_root.join("Talking Heads");
    std::fs::create_dir_all(&artist_dir).unwrap();

    std::fs::write(
        artist_dir.join("artist.nfo"),
        r"<artist>
            <name>Talking Heads</name>
            <sorttitle>Talking Heads</sorttitle>
            <disambiguation>New Wave band</disambiguation>
            <biography>American rock band formed in 1975.</biography>
            <genre>New Wave</genre>
            <genre>Post-Punk</genre>
            <style>Art Punk</style>
            <mood>Angular</mood>
         </artist>",
    )
    .unwrap();

    // Artist-folder artwork sidecars (empty payload is fine for path-only assertions).
    std::fs::write(artist_dir.join("thumb.jpg"), b"\x00").unwrap();
    std::fs::write(artist_dir.join("fanart.jpg"), b"\x00").unwrap();

    let video_path = artist_dir.join("Once in a Lifetime.mkv");
    std::fs::copy(fixture, &video_path).unwrap();
    std::fs::write(
        artist_dir.join("Once in a Lifetime.nfo"),
        r"<musicvideo>
            <title>Once in a Lifetime</title>
            <artist>Talking Heads</artist>
            <album>Remain in Light</album>
            <track>4</track>
            <plot>Iconic music video.</plot>
            <year>1980</year>
            <aired>1980-02-02</aired>
            <director>Toni Basil</director>
            <genre>New Wave</genre>
            <tag>iconic</tag>
            <studio>Sire</studio>
         </musicvideo>",
    )
    .unwrap();

    // Per-video artwork: `{base}-thumb.jpg` should bind to THUMBNAIL via the
    // music-video discovery order; folder `poster.jpg` binds to POSTER.
    std::fs::write(artist_dir.join("Once in a Lifetime-thumb.jpg"), b"\x00").unwrap();
    std::fs::write(artist_dir.join("poster.jpg"), b"\x00").unwrap();

    let pool = test_db().await;
    seed_local_library(
        &pool,
        library_root.as_path(),
        /* MediaKind = */ 3, /* MusicVideos */
    )
    .await;

    let report = scan_local_library(
        &pool,
        &LocalScanConfig {
            library_id: 1,
            ffprobe_path: ffprobe,
        },
    )
    .await
    .unwrap();

    assert_eq!(report.scanned_files, 1);
    assert_eq!(count(&pool, "Artist").await, 1);
    assert_eq!(count(&pool, "ArtistMetadata").await, 1);
    assert_eq!(count(&pool, "MusicVideo").await, 1);
    assert_eq!(count(&pool, "MusicVideoMetadata").await, 1);
    assert!(count(&pool, "MediaVersion").await >= 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);

    // Artist-scoped child rows.
    assert!(count_where(&pool, "Genre", r#""ArtistMetadataId""#).await >= 2);
    assert!(count_where(&pool, "Style", r#""ArtistMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Mood", r#""ArtistMetadataId""#).await >= 1);

    // Music-video-scoped child rows.
    assert!(count_where(&pool, "Genre", r#""MusicVideoMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Tag", r#""MusicVideoMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Studio", r#""MusicVideoMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Director", r#""MusicVideoMetadataId""#).await >= 1);
    assert!(count_where(&pool, "MusicVideoArtist", r#""MusicVideoMetadataId""#).await >= 1);

    // Artwork: artist thumb + fanart, music-video poster + thumbnail.
    let artist_artwork: i64 = sqlx::query(
        r#"SELECT COUNT(*) AS "Count" FROM "Artwork" WHERE "ArtistMetadataId" IS NOT NULL"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("Count");
    assert!(artist_artwork >= 2);

    let mv_artwork: i64 = sqlx::query(
        r#"SELECT COUNT(*) AS "Count" FROM "Artwork" WHERE "MusicVideoMetadataId" IS NOT NULL"#,
    )
    .fetch_one(&pool)
    .await
    .unwrap()
    .get("Count");
    assert!(mv_artwork >= 2);

    // Both NFOs present → MetadataKind = 1 (Sidecar) on both.
    let artist_kind: i32 = sqlx::query(r#"SELECT "MetadataKind" FROM "ArtistMetadata" LIMIT 1"#)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("MetadataKind");
    assert_eq!(artist_kind, 1);

    let mv_kind: i32 = sqlx::query(r#"SELECT "MetadataKind" FROM "MusicVideoMetadata" LIMIT 1"#)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("MetadataKind");
    assert_eq!(mv_kind, 1);
}

async fn test_db() -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys = ON").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    pool
}

async fn seed_local_library(pool: &SqlitePool, path: &Path, media_kind: i32) {
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (1, 'Local', ?, 1)"#,
    )
    .bind(media_kind)
    .execute(pool)
    .await
    .unwrap();
    pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
        .bind(path.display().to_string())
        .execute(pool)
        .await
        .unwrap();
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}""#);
    sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

async fn count_where(pool: &SqlitePool, table: &str, fk_column_quoted: &str) -> i64 {
    let sql = format!(
        r#"SELECT COUNT(*) AS "Count" FROM "{table}" WHERE {fk_column_quoted} IS NOT NULL"#
    );
    sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

#[allow(clippy::disallowed_methods)]
fn ffprobe_binary() -> Option<String> {
    env::var("ERSATZRS_FFPROBE")
        .ok()
        .or_else(|| find_on_path("ffprobe").map(|path| path.display().to_string()))
}

#[allow(clippy::disallowed_methods)]
fn find_on_path(binary: &str) -> Option<PathBuf> {
    env::var_os("PATH")?
        .to_string_lossy()
        .split(':')
        .find_map(|dir| {
            let path = Path::new(dir).join(binary);
            path.is_file().then_some(path)
        })
}

fn big_buck_bunny_fixture() -> Option<PathBuf> {
    const NAMES: &[&str] = &[
        "bbb_sunflower_1080p_30fps_normal.mp4",
        "bbb_sunflower_1080p_60fps_normal.mp4",
        "bbb_sunflower_2160p_30fps_normal.mp4",
        "bbb_sunflower_2160p_60fps_normal.mp4",
    ];
    const ROOTS: &[&str] = &["testdata", "../../testdata"];
    ROOTS
        .iter()
        .flat_map(|root| NAMES.iter().map(move |name| PathBuf::from(root).join(name)))
        .find(|path| path.is_file())
}

struct TempDir {
    path: PathBuf,
}

impl TempDir {
    #[allow(clippy::disallowed_methods)]
    fn new(prefix: &str) -> Self {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
