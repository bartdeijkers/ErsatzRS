//! Persisted row290 oracle: an ordinary local scan repairs an unset date only.

use chrono::{DateTime, NaiveDateTime, Timelike, Utc};
use sqlx::{Executor, Row};

struct Fixture {
    _temp: tempfile::TempDir,
    pool: sqlx::SqlitePool,
    url: String,
    root: std::path::PathBuf,
    created: DateTime<Utc>,
}

async fn fixture() -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path().join("library");
    let other_root = temp.path().join("other-library");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::create_dir_all(&other_root).unwrap();
    let file = root.join("legacy.date-added-fixture");
    let present = root.join("present.date-added-fixture");
    let gone = root.join("gone.date-added-fixture");
    let neighbor = other_root.join("neighbor.date-added-fixture");
    for path in [&file, &present, &neighbor] {
        std::fs::write(path, b"synthetic fixture").unwrap();
    }
    // On Unix without birth time, .NET uses the earlier mtime/ctime. Give
    // this synthetic file an old mtime so that fallback has a known oracle.
    let fallback = stored_date("2020-01-01T00:00:00Z").into();
    std::fs::File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(fallback))
        .unwrap();
    let created: DateTime<Utc> = std::fs::metadata(&file)
        .unwrap()
        .created()
        .unwrap_or(fallback)
        .into();
    let created = created
        .with_nanosecond(created.nanosecond() / 100 * 100)
        .unwrap();
    let database = temp.path().join("fixture.db");
    let url = format!("sqlite:{}?mode=rwc", database.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    pool.execute(
        r#"INSERT INTO "MediaSource" ("Id") VALUES (1);
           INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
           INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId")
           VALUES (1, 'Synthetic dates', 4, 1), (2, 'Unrelated dates', 4, 1);
           INSERT INTO "LocalLibrary" ("Id") VALUES (1), (2)"#,
    )
    .await
    .unwrap();
    for (id, path) in [(1_i64, &root), (2, &other_root)] {
        sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (?, ?, ?)"#)
            .bind(id)
            .bind(id)
            .bind(path.display().to_string())
            .execute(&pool)
            .await
            .unwrap();
    }
    seed_other_videos(&pool, &file, &present, &gone, &neighbor).await;
    sqlx::query(
        r#"INSERT INTO "MediaStream" ("MediaVersionId", "Index", "MediaStreamKind", "Codec", "Language", "Channels", "Default", "Forced")
           VALUES (11, 0, ?, 'aac', 'fra', 2, 1, 0), (11, 1, ?, 'subrip', 'nld', 0, 0, 0)"#,
    )
    .bind(super::encode_stream_kind(ersatzrs_domain::MediaStreamKind::Audio))
    .bind(super::encode_stream_kind(ersatzrs_domain::MediaStreamKind::Subtitle))
    .execute(&pool).await.unwrap();

    Fixture {
        _temp: temp,
        pool,
        url,
        root,
        created,
    }
}

async fn seed_other_videos(
    pool: &sqlx::SqlitePool,
    file: &std::path::Path,
    present: &std::path::Path,
    gone: &std::path::Path,
    neighbor: &std::path::Path,
) {
    let missing = "0001-01-01T00:00:00Z";
    let valid = "2024-05-01T00:00:00Z";
    for (id, library_path_id, path, date) in [
        (11_i64, 1_i64, &file, missing),
        (12, 1, &present, valid),
        (13, 1, &gone, missing),
        (21, 2, &neighbor, missing),
    ] {
        sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, ?, 0)"#)
            .bind(id)
            .bind(library_path_id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "OtherVideo" ("Id") VALUES (?)"#)
            .bind(id)
            .execute(pool)
            .await
            .unwrap();
        sqlx::query(r#"INSERT INTO "OtherVideoMetadata"
                ("Id", "OtherVideoId", "MetadataKind", "Title", "OriginalTitle", "DateAdded", "DateUpdated")
                VALUES (?, ?, 3, 'Preserved title', 'Operator identity', ?, '2024-05-01T00:00:00Z')"#)
            .bind(id).bind(id).bind(date).execute(pool).await.unwrap();
        sqlx::query(r#"INSERT INTO "MediaVersion"
                ("Id", "OtherVideoId", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind")
                VALUES (?, ?, '2024-05-01T00:00:00Z', '2024-05-01T00:00:00Z', '00:02:00', 1080, 1920, 0)"#)
            .bind(id).bind(id).execute(pool).await.unwrap();
        let text = path.display().to_string();
        sqlx::query(
            r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (?, ?, ?)"#,
        )
        .bind(id)
        .bind(&text)
        .bind(super::path_hash(&text))
        .execute(pool)
        .await
        .unwrap();
    }
}

#[tokio::test]
async fn row290_scan_repairs_existing_date_and_search_without_replacing_metadata() {
    let fixture = fixture().await;
    let pool = &fixture.pool;
    let config = super::LocalScanConfig {
        library_id: 1,
        ffprobe_path: "unused-for-metadata-repair".into(),
        deep_scan: false,
        managed_claim: None,
    };
    for _ in 0..2 {
        let report = super::scan_local_library(pool, &config).await.unwrap();
        assert_eq!(report.scanned_files, 0);
        assert_eq!(report.failed_files, 0);
        assert_preserved(pool, fixture.created).await;
    }
    pool.close().await;
    let reopened = sqlx::SqlitePool::connect(&fixture.url).await.unwrap();
    let date: String =
        sqlx::query_scalar(r#"SELECT "DateAdded" FROM "OtherVideoMetadata" WHERE "Id"=11"#)
            .fetch_one(&reopened)
            .await
            .unwrap();
    assert_eq!(stored_date(&date), fixture.created);
    reopened.close().await;
}

async fn assert_preserved(pool: &sqlx::SqlitePool, created: DateTime<Utc>) {
    let missing = "0001-01-01T00:00:00Z";
    let valid = "2024-05-01T00:00:00Z";
    let rows = sqlx::query(r#"SELECT * FROM "OtherVideoMetadata" ORDER BY "Id""#)
        .fetch_all(pool)
        .await
        .unwrap();
    assert_eq!(rows.len(), 4);
    for row in &rows {
        let id: i64 = row.get("Id");
        let date: String = row.get("DateAdded");
        if id == 11 {
            assert_eq!(stored_date(&date), created);
        } else {
            assert_eq!(date, if id == 12 { valid } else { missing });
        }
        assert_eq!(row.get::<String, _>("Title"), "Preserved title");
        assert_eq!(row.get::<String, _>("OriginalTitle"), "Operator identity");
        assert_eq!(row.get::<String, _>("DateUpdated"), valid);
        assert_eq!(row.get::<i32, _>("MetadataKind"), 3);
    }
    let tracks: Vec<(i64, String)> = sqlx::query_as(
            r#"SELECT "MediaStreamKind", "Language" FROM "MediaStream" WHERE "MediaVersionId"=11 ORDER BY "Index""#,
        ).fetch_all(pool).await.unwrap();
    assert_eq!(
        tracks,
        [
            (
                i64::from(super::encode_stream_kind(
                    ersatzrs_domain::MediaStreamKind::Audio
                )),
                "fra".into()
            ),
            (
                i64::from(super::encode_stream_kind(
                    ersatzrs_domain::MediaStreamKind::Subtitle
                )),
                "nld".into()
            ),
        ]
    );
    let repository =
        ersatzrs_infra::repositories::SqliteSchedulingContentRepository::new(pool.clone());
    // A file without birth-time support deliberately falls back to its old
    // mtime. Query its actual date rather than assuming that it is recent.
    let query = format!("added_date:[{0} TO {0}]", created.format("%Y%m%d"));
    let (hits, _, _) = repository
        .search_documents_page(&query, 0, 0)
        .await
        .unwrap();
    assert_eq!(hits.first().and_then(|hit| hit.added_date), Some(created));
    assert_eq!(
        hits.into_iter()
            .map(|hit| hit.media_item_id)
            .collect::<Vec<_>>(),
        [11]
    );
}

fn stored_date(value: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(value).map_or_else(
        |_| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .unwrap()
                .and_utc()
        },
        |date| date.with_timezone(&Utc),
    )
}

fn library_path(fixture: &Fixture) -> super::LocalLibraryPath {
    super::LocalLibraryPath {
        id: 1,
        path: fixture.root.display().to_string(),
        media_kind: ersatzrs_domain::LibraryMediaKind::OtherVideos,
    }
}

async fn seed_kind(pool: &sqlx::SqlitePool, kind: &str, id: i64, path: &std::path::Path) {
    use ersatzrs_infra::db::assert_sql_safe;
    sqlx::query(r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId", "State") VALUES (?, 1, 0)"#)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    let item = if kind == "MusicVideo" {
        format!(r#"INSERT INTO "MusicVideo" ("Id", "ArtistId") VALUES ({id}, 900)"#)
    } else {
        format!(r#"INSERT INTO "{kind}" ("Id") VALUES ({id})"#)
    };
    sqlx::query(assert_sql_safe(&item))
        .execute(pool)
        .await
        .unwrap();
    let metadata = format!(
        r#"INSERT INTO "{kind}Metadata"
        ("{kind}Id", "MetadataKind", "DateAdded", "DateUpdated", "Title")
        VALUES (?, 3, '0001-01-01 00:00:00', '2024-01-01 00:00:00', 'Retained')"#
    );
    sqlx::query(assert_sql_safe(&metadata))
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    let version = format!(
        r#"INSERT INTO "MediaVersion"
        ("Id", "{kind}Id", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind")
        VALUES (?, ?, '2024-01-01 00:00:00', '2024-01-01 00:00:00', '00:01:00', 0, 0, 0)"#
    );
    sqlx::query(assert_sql_safe(&version))
        .bind(id)
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash") VALUES (?, ?, ?)"#,
    )
    .bind(id)
    .bind(path.display().to_string())
    .bind(format!("synthetic-{id}"))
    .execute(pool)
    .await
    .unwrap();
}

#[tokio::test]
async fn row290_repairs_six_kinds_in_batches_without_duplicate_owner_or_pathless_rows() {
    use ersatzrs_infra::db::assert_sql_safe;
    let fixture = fixture().await;
    let pool = &fixture.pool;
    pool.execute(
        r#"INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (900,1);
        INSERT INTO "Artist" ("Id") VALUES (900)"#,
    )
    .await
    .unwrap();
    let file = fixture.root.join("legacy.date-added-fixture");
    let kinds = [
        "Movie",
        "MusicVideo",
        "OtherVideo",
        "Song",
        "Image",
        "RemoteStream",
    ];
    for (kind, id) in kinds.into_iter().zip(100_i64..) {
        seed_kind(pool, kind, id, &file).await;
    }
    for id in 200..270 {
        seed_kind(pool, "OtherVideo", id, &file).await;
    }
    // Genuine additional versions/files must not multiply the repair count.
    pool.execute(
        r#"INSERT INTO "MediaFile" ("MediaVersionId", "Path", "PathHash")
        VALUES (100,'missing-secondary-file','secondary');
        UPDATE "MediaFile" SET "Path"='' WHERE "MediaVersionId"=105;
        INSERT INTO "MovieMetadata" ("MovieId", "MetadataKind", "DateAdded", "DateUpdated", "Title")
        VALUES (100,1,'0001-01-01T00:00:00Z','2024-01-01 00:00:00','Distinct metadata')"#,
    )
    .await
    .unwrap();
    let path = library_path(&fixture);
    assert_eq!(
        super::local_date_added::backfill(pool, &path, None)
            .await
            .unwrap(),
        76
    );
    assert_eq!(
        super::local_date_added::backfill(pool, &path, None)
            .await
            .unwrap(),
        0
    );
    let movie_rows: i64 = sqlx::query_scalar(
        r#"SELECT COUNT(*) FROM "MovieMetadata" WHERE "MovieId"=100
            AND "DateAdded" NOT LIKE '0001-01-01%'"#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    assert_eq!(movie_rows, 2);
    for kind in kinds {
        let sql = format!(
            r#"SELECT COUNT(*) FROM "{kind}Metadata" WHERE "Title"='Retained'
            AND "DateAdded" NOT LIKE '0001-01-01%' AND "DateUpdated"='2024-01-01 00:00:00'"#
        );
        let actual: i64 = sqlx::query_scalar(assert_sql_safe(&sql))
            .fetch_one(pool)
            .await
            .unwrap();
        assert_eq!(
            actual,
            match kind {
                "OtherVideo" => 71,
                "RemoteStream" => 0,
                _ => 1,
            }
        );
    }
    let absent = super::LocalLibraryPath {
        path: fixture.root.join("absent").display().to_string(),
        ..path
    };
    assert_eq!(
        super::local_date_added::backfill(pool, &absent, None)
            .await
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn row290_stale_or_wrong_library_claim_cannot_repair_dates() {
    use ersatzrs_core::repositories::{ManagedLibraryOperationRepository, ManagedLibraryScanSpec};
    use ersatzrs_domain::{ManagedLibraryOperationKind, MediaSourceKind};
    use ersatzrs_infra::repositories::SqliteManagedLibraryOperationRepository;
    let fixture = fixture().await;
    let pool = &fixture.pool;
    let repository = SqliteManagedLibraryOperationRepository(pool.clone());
    let operation = repository
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            1,
            ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: false,
            },
            false,
        )
        .await
        .unwrap()
        .unwrap();
    let claim = repository
        .claim_operation(operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    let path = library_path(&fixture);
    let mut forged = claim.clone();
    forged.operation.library_id = 2;
    assert!(
        super::local_date_added::backfill(pool, &path, Some(&forged))
            .await
            .is_err()
    );
    let replacement = repository
        .claim_next(Utc::now() + chrono::Duration::seconds(91))
        .await
        .unwrap()
        .unwrap();
    assert_ne!(claim.lease_owner, replacement.lease_owner);
    assert!(super::local_date_added::backfill(pool, &path, Some(&claim))
        .await
        .is_err());
    let date: String =
        sqlx::query_scalar(r#"SELECT "DateAdded" FROM "OtherVideoMetadata" WHERE "Id"=11"#)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(date, "0001-01-01T00:00:00Z");
    assert_eq!(
        super::local_date_added::backfill(pool, &path, Some(&replacement))
            .await
            .unwrap(),
        1
    );
}
