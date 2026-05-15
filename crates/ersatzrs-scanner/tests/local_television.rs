//! End-to-end test for the local TV scanner. Builds a minimal show tree with a
//! `tvshow.nfo`, one season folder, and one episode (video + per-episode NFO),
//! then asserts that every metadata table the scanner is supposed to write was
//! populated.
//!
//! Like [`local_scan.rs`] this is `#[ignore]`-gated on `ffprobe` and the Big
//! Buck Bunny fixture; run with `cargo test -p ersatzrs-scanner -- --ignored`.

#![cfg(not(feature = "mysql"))]

use std::env;
use std::path::{Path, PathBuf};

use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Executor, Row, SqlitePool};

#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; run cargo test -- --ignored to execute it"]
async fn local_show_scan_writes_full_metadata_and_child_rows() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let temp = TempDir::new("ersatzrs-local-television");
    let show_dir = temp.path().join("library").join("The Expanse");
    let season_dir = show_dir.join("Season 01");
    std::fs::create_dir_all(&season_dir).unwrap();

    std::fs::write(
        show_dir.join("tvshow.nfo"),
        r#"<tvshow>
            <title>The Expanse</title>
            <sorttitle>Expanse, The</sorttitle>
            <plot>Belters vs Inners.</plot>
            <year>2015</year>
            <premiered>2015-12-14</premiered>
            <contentrating>TV-14</contentrating>
            <genre>Science Fiction</genre>
            <genre>Drama</genre>
            <tag>space</tag>
            <studio>Syfy</studio>
            <actor><name>Shohreh Aghdashloo</name><role>Avasarala</role><order>1</order></actor>
            <uniqueid type="imdb" default="true">tt3230854</uniqueid>
        </tvshow>"#,
    )
    .unwrap();

    let episode_path = season_dir.join("The Expanse - S01E01 - Dulcinea.mkv");
    std::fs::copy(fixture, &episode_path).unwrap();
    std::fs::write(
        season_dir.join("The Expanse - S01E01 - Dulcinea.nfo"),
        r#"<episodedetails>
            <title>Dulcinea</title>
            <plot>Pilot episode.</plot>
            <season>1</season>
            <episode>1</episode>
            <year>2015</year>
            <aired>2015-12-14</aired>
            <genre>Drama</genre>
            <tag>pilot</tag>
            <director>Terry McDonough</director>
            <credits>Mark Fergus</credits>
            <actor><name>Steven Strait</name><role>Holden</role><order>1</order></actor>
            <uniqueid type="imdb" default="true">tt4574334</uniqueid>
        </episodedetails>"#,
    )
    .unwrap();

    let pool = test_db().await;
    seed_local_library(
        &pool,
        temp.path().join("library").as_path(),
        /* MediaKind = */ 2, // Shows
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
    assert_eq!(count(&pool, "Show").await, 1);
    assert_eq!(count(&pool, "ShowMetadata").await, 1);
    assert_eq!(count(&pool, "Season").await, 1);
    assert_eq!(count(&pool, "SeasonMetadata").await, 1);
    assert_eq!(count(&pool, "Episode").await, 1);
    assert_eq!(count(&pool, "EpisodeMetadata").await, 1);
    assert!(count(&pool, "MediaVersion").await >= 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);

    // Show-scoped child rows.
    assert!(count_where(&pool, "Genre", r#""ShowMetadataId""#).await >= 2);
    assert!(count_where(&pool, "Tag", r#""ShowMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Studio", r#""ShowMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Actor", r#""ShowMetadataId""#).await >= 1);
    assert!(count_where(&pool, "MetadataGuid", r#""ShowMetadataId""#).await >= 1);

    // Episode-scoped child rows.
    assert!(count_where(&pool, "Genre", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Tag", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Actor", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Director", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "Writer", r#""EpisodeMetadataId""#).await >= 1);
    assert!(count_where(&pool, "MetadataGuid", r#""EpisodeMetadataId""#).await >= 1);

    // Sidecar NFO should have flipped MetadataKind to 1 (Sidecar) for both.
    let show_kind: i32 = sqlx::query(r#"SELECT "MetadataKind" FROM "ShowMetadata" LIMIT 1"#)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("MetadataKind");
    assert_eq!(show_kind, 1);

    let episode_kind: i32 = sqlx::query(r#"SELECT "MetadataKind" FROM "EpisodeMetadata" LIMIT 1"#)
        .fetch_one(&pool)
        .await
        .unwrap()
        .get("MetadataKind");
    assert_eq!(episode_kind, 1);
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
