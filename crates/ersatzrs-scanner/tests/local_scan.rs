#![cfg(not(feature = "mysql"))]
use std::env;
use std::path::{Path, PathBuf};

use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::sqlite::SqlitePoolOptions;
use sqlx::{Executor, Row, SqlitePool};

#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; run cargo test -- --ignored to execute it"]
async fn local_scan_populates_media_rows_from_test_mp4() {
    let temp = TempDir::new("ersatzrs-local-scan");
    let Some(ffprobe) = ffprobe_binary(temp.path()) else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let media_path = temp.path().join("library").join("movie.mp4");
    std::fs::create_dir_all(media_path.parent().unwrap()).unwrap();
    std::fs::copy(fixture, &media_path).unwrap();

    let pool = test_db().await;
    seed_local_library(&pool, media_path.parent().unwrap(), 4).await;

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
    assert_eq!(count(&pool, "MediaItem").await, 1);
    assert_eq!(count(&pool, "OtherVideo").await, 1);
    assert_eq!(count(&pool, "MediaVersion").await, 1);
    assert_eq!(count(&pool, "MediaFile").await, 1);
    assert!(count(&pool, "MediaStream").await >= 1);
    assert!(library_last_scan(&pool).await.is_some());
}

#[tokio::test]
#[ignore = "requires ffprobe and the Big Buck Bunny fixture; run cargo test -- --ignored to execute it"]
#[allow(clippy::too_many_lines)]
async fn local_movie_scan_enriches_sidecars_artwork_and_is_idempotent() {
    let temp = TempDir::new("ersatzrs-local-movie-scan");
    let Some(ffprobe) = ffprobe_binary(temp.path()) else {
        eprintln!("skipping: ffprobe not found and ERSATZRS_FFPROBE is unset");
        return;
    };
    let Some(fixture) = big_buck_bunny_fixture() else {
        eprintln!("skipping: Big Buck Bunny testdata fixture not found");
        return;
    };

    let library = temp.path().join("library");
    let sidecar_folder = library.join("sidecar");
    let artwork_only_folder = library.join("artwork-only");
    let malformed_folder = library.join("malformed");
    std::fs::create_dir_all(&sidecar_folder).unwrap();
    std::fs::create_dir_all(&artwork_only_folder).unwrap();
    std::fs::create_dir_all(&malformed_folder).unwrap();

    let sidecar_movie = sidecar_folder.join("Film (2020).mp4");
    let artwork_only_movie = artwork_only_folder.join("Artwork Only.mp4");
    let malformed_movie = malformed_folder.join("Broken Film.mp4");
    link_or_copy_fixture(&fixture, &sidecar_movie);
    link_or_copy_fixture(&fixture, &artwork_only_movie);
    link_or_copy_fixture(&fixture, &malformed_movie);

    std::fs::write(
        sidecar_folder.join("Film (2020).nfo"),
        r#"<movie>
            <title>The Film</title>
            <sorttitle>Film</sorttitle>
            <outline>Short summary.</outline>
            <plot>Detailed plot.</plot>
            <year>2020</year>
            <mpaa>PG-13</mpaa>
            <premiered>2020-02-03</premiered>
            <genre>Drama</genre>
            <tag>Festival</tag>
            <studio>Studio One</studio>
            <actor><name>Ada Actor</name><role>Lead</role><order>1</order></actor>
            <director>Dana Director</director>
            <credits>Will Writer</credits>
            <uniqueid type="imdb" default="true">tt1234567</uniqueid>
        </movie>"#,
    )
    .unwrap();
    std::fs::write(sidecar_folder.join("poster.jpg"), b"poster").unwrap();
    std::fs::write(sidecar_folder.join("fanart.jpg"), b"fanart").unwrap();
    std::fs::write(artwork_only_folder.join("poster.jpg"), b"poster").unwrap();
    std::fs::write(artwork_only_folder.join("fanart.jpg"), b"fanart").unwrap();
    std::fs::write(
        malformed_folder.join("Broken Film.nfo"),
        b"<movie><title>Broken",
    )
    .unwrap();

    let pool = test_db().await;
    seed_local_library(&pool, &library, 1).await;
    let config = LocalScanConfig {
        library_id: 1,
        ffprobe_path: ffprobe,
    };

    let first_report = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(first_report.scanned_files, 3);
    let sidecar_metadata_id = movie_metadata_id(&pool, "The Film").await;

    let second_report = scan_local_library(&pool, &config).await.unwrap();
    assert_eq!(second_report.scanned_files, 3);
    assert_eq!(
        movie_metadata_id(&pool, "The Film").await,
        sidecar_metadata_id
    );

    let sidecar = sqlx::query(
        r#"SELECT "MetadataKind", "Year", "Plot", "ContentRating"
           FROM "MovieMetadata" WHERE "Id" = ?"#,
    )
    .bind(sidecar_metadata_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(sidecar.get::<i32, _>("MetadataKind"), 1);
    assert_eq!(sidecar.get::<Option<i32>, _>("Year"), Some(2020));
    assert_eq!(
        sidecar.get::<Option<String>, _>("Plot").as_deref(),
        Some("Detailed plot.")
    );
    assert_eq!(
        sidecar.get::<Option<String>, _>("ContentRating").as_deref(),
        Some("PG-13")
    );

    assert_eq!(
        count_for_metadata(&pool, "Artwork", sidecar_metadata_id).await,
        2
    );
    assert_eq!(
        count_for_metadata(&pool, "Genre", sidecar_metadata_id).await,
        1
    );
    assert_eq!(
        count_for_metadata(&pool, "Actor", sidecar_metadata_id).await,
        1
    );
    assert_eq!(
        count_for_metadata(&pool, "Director", sidecar_metadata_id).await,
        1
    );
    assert_eq!(
        count_for_metadata(&pool, "Writer", sidecar_metadata_id).await,
        1
    );
    assert_eq!(
        count_for_metadata(&pool, "MetadataGuid", sidecar_metadata_id).await,
        1
    );

    let artwork_only_id = movie_metadata_id(&pool, "Artwork Only").await;
    let artwork_only = sqlx::query(r#"SELECT "MetadataKind" FROM "MovieMetadata" WHERE "Id" = ?"#)
        .bind(artwork_only_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(artwork_only.get::<i32, _>("MetadataKind"), 0);
    assert_eq!(
        count_for_metadata(&pool, "Artwork", artwork_only_id).await,
        2
    );

    let malformed_id = movie_metadata_id(&pool, "Broken Film").await;
    let malformed =
        sqlx::query(r#"SELECT "MetadataKind", "Plot" FROM "MovieMetadata" WHERE "Id" = ?"#)
            .bind(malformed_id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(malformed.get::<i32, _>("MetadataKind"), 0);
    assert!(malformed.get::<Option<String>, _>("Plot").is_none());
    assert_eq!(count(&pool, "MovieMetadata").await, 3);
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

async fn movie_metadata_id(pool: &SqlitePool, title: &str) -> i64 {
    sqlx::query(r#"SELECT "Id" FROM "MovieMetadata" WHERE "Title" = ?"#)
        .bind(title)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Id")
}

async fn count_for_metadata(pool: &SqlitePool, table: &str, metadata_id: i64) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}" WHERE "MovieMetadataId" = ?"#);
    sqlx::query(&sql)
        .bind(metadata_id)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) AS "Count" FROM "{table}""#);
    sqlx::query(&sql)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("Count")
}

async fn library_last_scan(pool: &SqlitePool) -> Option<String> {
    sqlx::query(r#"SELECT "LastScan" FROM "Library" WHERE "Id" = 1"#)
        .fetch_one(pool)
        .await
        .unwrap()
        .get("LastScan")
}

#[allow(clippy::disallowed_methods)]
fn ffprobe_binary(wrapper_dir: &Path) -> Option<String> {
    env::var("ERSATZRS_FFPROBE")
        .ok()
        .or_else(|| find_on_path("ffprobe").map(|path| ffprobe_command(&path, wrapper_dir)))
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

#[cfg(unix)]
fn ffprobe_command(path: &Path, wrapper_dir: &Path) -> String {
    use std::os::unix::fs::PermissionsExt;

    let Some(root) = path.parent().and_then(Path::parent) else {
        return path.display().to_string();
    };
    let lib_dir = root.join("lib");
    if !lib_dir.join("libavdevice.so.62").is_file() {
        return path.display().to_string();
    }

    let wrapper = wrapper_dir.join("ffprobe-wrapper.sh");
    let script = format!(
        "#!/bin/sh\nLD_LIBRARY_PATH={}${{LD_LIBRARY_PATH:+:$LD_LIBRARY_PATH}} exec {} \"$@\"\n",
        shell_quote(&lib_dir.display().to_string()),
        shell_quote(&path.display().to_string())
    );
    if std::fs::write(&wrapper, script).is_err() {
        return path.display().to_string();
    }
    if let Ok(metadata) = std::fs::metadata(&wrapper) {
        let mut permissions = metadata.permissions();
        permissions.set_mode(0o755);
        if std::fs::set_permissions(&wrapper, permissions).is_err() {
            return path.display().to_string();
        }
    }
    wrapper.display().to_string()
}

#[cfg(not(unix))]
fn ffprobe_command(path: &Path, _wrapper_dir: &Path) -> String {
    path.display().to_string()
}

#[cfg(unix)]
fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn big_buck_bunny_fixture() -> Option<PathBuf> {
    let crate_root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let workspace_root = crate_root.join("../..");
    [
        "testdata/bbb_sunflower_1080p_30fps_normal.mp4",
        "testdata/bbb_sunflower_1080p_60fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_30fps_normal.mp4",
        "testdata/bbb_sunflower_2160p_60fps_normal.mp4",
    ]
    .into_iter()
    .flat_map(|path| [PathBuf::from(path), workspace_root.join(path)])
    .find(|path| path.is_file())
}

fn link_or_copy_fixture(source: &Path, destination: &Path) {
    if std::fs::hard_link(source, destination).is_err() {
        std::fs::copy(source, destination).unwrap();
    }
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
