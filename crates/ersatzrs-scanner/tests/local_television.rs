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
#[allow(clippy::too_many_lines)]
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

    // Steps 3-5 parity: one show LibraryFolder + one season LibraryFolder,
    // with the season folder's ParentId pointing at the show folder. The
    // library root itself gets no row (ErsatzTV's ScanFolder only adds
    // show/season folders), so the show folder's ParentId stays NULL. The
    // show folder's Etag stays NULL (ErsatzTV only etags season folders,
    // after ScanEpisodes); the season folder's Etag is persisted after the
    // episode scan; the episode's MediaFile links to the season folder.
    assert_eq!(count(&pool, "LibraryFolder").await, 2);
    let show_path = show_dir.to_string_lossy().into_owned();
    let season_path = season_dir.to_string_lossy().into_owned();
    let show_folder =
        sqlx::query(r#"SELECT "Id", "ParentId", "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(&show_path)
            .fetch_one(&pool)
            .await
            .expect("show LibraryFolder row");
    let show_folder_id: i64 = show_folder.get("Id");
    assert_eq!(show_folder.get::<Option<i64>, _>("ParentId"), None);
    assert_eq!(show_folder.get::<Option<String>, _>("Etag"), None);

    let season_folder =
        sqlx::query(r#"SELECT "Id", "ParentId", "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(&season_path)
            .fetch_one(&pool)
            .await
            .expect("season LibraryFolder row");
    let season_folder_id: i64 = season_folder.get("Id");
    assert_eq!(
        season_folder.get::<Option<i64>, _>("ParentId"),
        Some(show_folder_id)
    );
    assert!(
        season_folder.get::<Option<String>, _>("Etag").is_some(),
        "season folder etag is persisted after the episode scan"
    );

    // Step 4 parity: the episode MediaFile links to the season LibraryFolder.
    let media_file_folder: Option<i64> =
        sqlx::query(r#"SELECT "LibraryFolderId" FROM "MediaFile" LIMIT 1"#)
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("LibraryFolderId");
    assert_eq!(media_file_folder, Some(season_folder_id));

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

/// Step 5 parity: a second scan of an unchanged season folder hits the etag
/// fast path (no episode probing, `scanned_files == 0`), while touching a
/// file under the season folder changes the etag and forces a rescan.
#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; run cargo test -- --ignored to execute it"]
async fn second_scan_skips_unchanged_season_via_etag() {
    let Some(ffprobe) = ffprobe_binary() else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let temp = TempDir::new("ersatzrs-local-television-etag");
    let season_dir = temp
        .path()
        .join("library")
        .join("The Expanse")
        .join("Season 01");
    std::fs::create_dir_all(&season_dir).unwrap();
    let episode_path = season_dir.join("The Expanse - S01E01 - Dulcinea.mkv");
    std::fs::copy(&fixture, &episode_path).unwrap();

    let pool = test_db().await;
    seed_local_library(&pool, temp.path().join("library").as_path(), 2).await;

    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: ffprobe,
    };

    // First scan probes the episode and persists the season folder etag.
    let first = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(first.scanned_files, 1);
    let etag_after_first: Option<String> =
        sqlx::query(r#"SELECT "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(season_dir.to_string_lossy().into_owned())
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("Etag");
    assert!(etag_after_first.is_some(), "season etag must be persisted");

    // Second scan with an unchanged folder skips episode work entirely.
    let second = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(
        second.scanned_files, 0,
        "unchanged season must not be re-probed"
    );
    assert_eq!(count(&pool, "Episode").await, 1);

    // Touching the episode file changes the folder etag and forces a rescan.
    let later = std::time::SystemTime::now() + std::time::Duration::from_secs(5);
    std::fs::File::options()
        .write(true)
        .open(&episode_path)
        .unwrap()
        .set_modified(later)
        .unwrap();
    let third = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(
        third.scanned_files, 1,
        "changed season etag must trigger a rescan"
    );
    let etag_after_third: Option<String> =
        sqlx::query(r#"SELECT "Etag" FROM "LibraryFolder" WHERE "Path" = ?"#)
            .bind(season_dir.to_string_lossy().into_owned())
            .fetch_one(&pool)
            .await
            .unwrap()
            .get("Etag");
    assert_ne!(
        etag_after_third, etag_after_first,
        "etag must change after the file mtime changed"
    );
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
