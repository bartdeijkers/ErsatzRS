//! Actual Local scan routes with a pinned probe and redistributable AV/200/888.
//! Descriptor reports are not observed pages or playback-selection acceptance.

use std::path::{Path, PathBuf};

use chrono::Utc;
use ersatzrs_core::repositories::{ManagedLibraryOperationRepository, ManagedLibraryScanSpec};
use ersatzrs_domain::{
    ManagedLibraryOperationKind, MediaSourceKind, SourceTeletextBinding, SourceTeletextDescriptors,
    SourceTeletextObservedPages, SourceTeletextPid, SourceTeletextRevision,
};
use ersatzrs_infra::ffprobe::LocalProbeCleanupScope;
use ersatzrs_infra::repositories::SqliteManagedLibraryOperationRepository;
use ersatzrs_infra::teletext_source;
use ersatzrs_scanner::{scan_local_library, LocalScanConfig};
use sqlx::{sqlite::SqlitePoolOptions, Executor as _, SqlitePool};

#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in native test runtime and synthetic fixture, never product configuration"
)]
fn inputs() -> (String, PathBuf) {
    (
        std::env::var("FFPROBE_PATH").expect("set pinned FFprobe 9"),
        std::env::var_os("ERSATZRS_TELETEXT_SCAN_FIXTURE")
            .expect("set synthetic two-audio AV/200/888 source")
            .into(),
    )
}

async fn database(root: &Path, kind: i32) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys=ON").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(
        r#"
        INSERT INTO "MediaSource" ("Id") VALUES (1);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (1,'Synthetic',1,1);
        INSERT INTO "LocalLibrary" ("Id") VALUES (1);
    "#,
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query(r#"UPDATE "Library" SET "MediaKind"=? WHERE "Id"=1"#)
        .bind(kind)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id","LibraryId","Path") VALUES (1,1,?)"#)
        .bind(root.to_str().unwrap())
        .execute(&pool)
        .await
        .unwrap();
    pool
}

async fn config(pool: &SqlitePool, probe: &str) -> LocalScanConfig {
    let repo = SqliteManagedLibraryOperationRepository(pool.clone());
    let operation = repo
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            1,
            ManagedLibraryScanSpec {
                provider: MediaSourceKind::Local,
                show_id: None,
                deep_scan: true,
            },
            false,
        )
        .await
        .unwrap()
        .unwrap();
    LocalScanConfig {
        library_id: 1,
        deep_scan: true,
        ffprobe_path: probe.to_owned(),
        managed_claim: Some(
            repo.claim_operation(operation.id, Utc::now())
                .await
                .unwrap()
                .unwrap(),
        ),
    }
}

async fn scan(pool: &SqlitePool, config: &LocalScanConfig) {
    let scope = LocalProbeCleanupScope::new();
    let outcome = Box::pin(scope.scope(scan_local_library(pool, config))).await;
    scope.close();
    scope.wait_cleanup().await.unwrap();
    outcome.unwrap();
    assert_eq!(scope.cleanup_status(), Ok(true));
}

#[derive(Debug, PartialEq, sqlx::FromRow)]
struct Track {
    version_id: i64,
    stream_index: i64,
    kind: i64,
    codec: String,
    language: Option<String>,
    default_flag: i64,
    forced: i64,
}

async fn tracks(pool: &SqlitePool, files: usize) -> Vec<Track> {
    let rows: Vec<Track> = sqlx::query_as(
        r#"SELECT "MediaVersionId" AS version_id,
        "Index" AS stream_index,"MediaStreamKind" AS kind,"Codec" AS codec,
        "Language" AS language,"Default" AS default_flag,"Forced" AS forced
        FROM "MediaStream" ORDER BY "MediaVersionId","Index""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), files * 4);
    for chunk in rows.chunks_exact(4) {
        assert_eq!(
            chunk
                .iter()
                .map(|row| (row.stream_index, row.kind))
                .collect::<Vec<_>>(),
            vec![(0, 1), (1, 2), (2, 2), (3, 3)]
        );
        assert_eq!(chunk[1].language.as_deref(), Some("eng"));
        assert_eq!(chunk[2].language.as_deref(), Some("nld"));
        assert_eq!(chunk[3].language.as_deref(), Some("eng,eng"));
        assert_eq!(chunk[3].codec, "dvb_teletext");
    }
    rows
}

async fn reports(pool: &SqlitePool, files: usize) -> Vec<(i64, SourceTeletextRevision)> {
    let parents: Vec<(i64, i64, i64)> = sqlx::query_as(
        r#"SELECT f."Id",f."MediaVersionId",s."Id"
        FROM "MediaFile" f JOIN "MediaStream" s ON s."MediaVersionId"=f."MediaVersionId"
        WHERE s."Codec"='dvb_teletext' ORDER BY f."Id""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(parents.len(), files);
    // read-only transaction: load each report and its children on one snapshot.
    let mut tx = pool.begin().await.unwrap();
    let mut revisions = Vec::new();
    for (file_id, version_id, stream_id) in parents {
        let report = teletext_source::load(&mut tx, stream_id, file_id)
            .await
            .unwrap()
            .unwrap();
        let SourceTeletextBinding::Bound(binding) = report.binding else {
            panic!("managed binding")
        };
        assert_eq!(
            (binding.media_source_id, binding.provider),
            (1, MediaSourceKind::Local)
        );
        assert_eq!(
            (
                binding.media_file_id,
                binding.media_version_id,
                binding.media_stream_id
            ),
            (file_id, version_id, stream_id)
        );
        assert_eq!(report.stream_index, 3);
        assert_eq!(report.pid, SourceTeletextPid::Reported(0x103));
        assert_eq!(
            report.observed_pages,
            SourceTeletextObservedPages::NotAcquired
        );
        let SourceTeletextDescriptors::Reported {
            entries,
            trailing_unpaired_pairs,
        } = report.descriptors
        else {
            panic!("native descriptor prefix")
        };
        assert_eq!(trailing_unpaired_pairs, None);
        assert_eq!(
            entries
                .iter()
                .map(|entry| (
                    entry.projection_ordinal,
                    entry.display_page().unwrap().get(),
                    entry.descriptor_type,
                    entry.language.as_str()
                ))
                .collect::<Vec<_>>(),
            vec![(0, 200, 1, "eng"), (1, 888, 2, "eng")]
        );
        revisions.push((file_id, binding.source_revision));
    }
    tx.rollback().await.unwrap();
    revisions
}

async fn subtitles(pool: &SqlitePool, files: usize) -> Vec<(String, String, i64, i64)> {
    let rows = sqlx::query_as(
        r#"SELECT "Path","Language","Default","Forced" FROM "Subtitle"
        WHERE "Path" LIKE '%.srt' ORDER BY "Path""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(rows.len(), files);
    rows
}

async fn case(kind: i32, paths: &[&str], probe: &str, fixture: &Path) {
    let root = tempfile::tempdir().unwrap();
    for relative in paths {
        let path = root.path().join(relative);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::copy(fixture, &path).unwrap();
        std::fs::write(
            path.with_extension("eng.srt"),
            "1\n00:00:00,000 --> 00:00:01,000\nSynthetic ordinary caption\n",
        )
        .unwrap();
    }
    let pool = database(root.path(), kind).await;
    let config = config(&pool, probe).await;
    scan(&pool, &config).await;
    let before_tracks = tracks(&pool, paths.len()).await;
    let before_reports = reports(&pool, paths.len()).await;
    let before_subtitles = subtitles(&pool, paths.len()).await;
    let before_files: Vec<(i64, i64, String)> =
        sqlx::query_as(r#"SELECT "Id","MediaVersionId","Path" FROM "MediaFile" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    scan(&pool, &config).await;
    assert_eq!(before_tracks, tracks(&pool, paths.len()).await);
    assert_eq!(before_reports, reports(&pool, paths.len()).await);
    assert_eq!(before_subtitles, subtitles(&pool, paths.len()).await);
    let after_files: Vec<(i64, i64, String)> =
        sqlx::query_as(r#"SELECT "Id","MediaVersionId","Path" FROM "MediaFile" ORDER BY "Id""#)
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(before_files, after_files);
    // Replace the file, keeping its locator, length and filesystem mtime. The
    // next genuine scan must acquire a new association, without re-keying media.
    let changed = root.path().join(paths[0]);
    let original = changed.metadata().unwrap();
    let replacement = root.path().join("replacement.tmp");
    std::fs::copy(&changed, &replacement).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&replacement)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(original.modified().unwrap()))
        .unwrap();
    assert_eq!(replacement.metadata().unwrap().len(), original.len());
    assert_eq!(
        replacement.metadata().unwrap().modified().unwrap(),
        original.modified().unwrap()
    );
    std::fs::rename(replacement, &changed).unwrap();
    scan(&pool, &config).await;
    assert_eq!(before_tracks, tracks(&pool, paths.len()).await);
    assert_eq!(before_subtitles, subtitles(&pool, paths.len()).await);
    let current = reports(&pool, paths.len()).await;
    let changed_id = before_files
        .iter()
        .find(|row| row.2 == changed.to_str().unwrap())
        .unwrap()
        .0;
    for (file_id, revision) in &before_reports {
        let updated = &current.iter().find(|row| row.0 == *file_id).unwrap().1;
        if *file_id == changed_id {
            assert_ne!(revision, updated);
        } else {
            assert_eq!(revision, updated);
        }
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap(),
        i64::try_from(paths.len()).unwrap()
    );
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic two-audio AV/200/888 input"]
async fn native_local_scans_bind_reports_across_leaf_writers_without_losing_tracks() {
    let (probe, fixture) = inputs();
    case(4, &["Synthetic.ts"], &probe, &fixture).await;
    case(
        1,
        &[
            "Synthetic Movie/Synthetic Movie.ts",
            "Synthetic Movie/Synthetic Movie-trailer.ts",
        ],
        &probe,
        &fixture,
    )
    .await;
    case(
        3,
        &["Synthetic Artist/Synthetic Track.ts"],
        &probe,
        &fixture,
    )
    .await;
    case(
        2,
        &[
            "Synthetic Show/Season 1/Synthetic.S01E01.ts",
            "Synthetic Show/Featurettes/Making Of.ts",
            "Synthetic Show/Season 1/Trailers/Season Trailer.ts",
        ],
        &probe,
        &fixture,
    )
    .await;
}
