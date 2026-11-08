//! Row195 application acceptance: a persisted Local deep-scan request must
//! reach the native TV scanner after restart, without changing source files.

use std::path::{Path, PathBuf};

use ersatzrs_app::commands::{
    ExecuteManagedLibraryOperationOutcome, RequestLibraryScan, RequestScanOutcome, ScanRequest,
    ScanWorkRequest,
};
use ersatzrs_core::locking::EntityLocker;
use ersatzrs_domain::{ManagedLibraryOperationState, MediaSourceKind};
use ersatzrs_web::host::state::{AppState, IptvAccessTokenVerifier, SystemStartup};
use futures_util::FutureExt as _;
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::{Executor as _, SqlitePool};
use tokio::sync::mpsc::{self, UnboundedReceiver};
use uuid::Uuid;

#[tokio::test]
#[ignore = "requires published FFmpeg v2.0.6 paths; generates its own international TV fixture"]
async fn native_durable_local_deep_scan_repairs_unchanged_tv_after_restart() {
    let ffmpeg = required_runtime("ERSATZRS_NEXT_TEST_FFMPEG");
    let ffprobe = required_runtime("ERSATZRS_NEXT_TEST_FFPROBE");
    assert_eq!(PathBuf::from(ersatzrs_ffmpeg::ffmpeg_path()), ffmpeg);
    assert_eq!(PathBuf::from(ersatzrs_ffmpeg::ffprobe_path()), ffprobe);
    let root = tempfile::tempdir().expect("private durable deep-scan fixture");
    let proof = std::panic::AssertUnwindSafe(async {
        let library = root.path().join("library");
        let season = library.join("Synthetic Show").join("Season 01");
        std::fs::create_dir_all(&season).unwrap();
        let episode = season.join("Synthetic.Show.S01E01.mkv");
        generate_deep_scan_episode(ffmpeg.to_str().unwrap(), root.path(), &episode).await;
        let nfo = episode.with_extension("nfo");
        std::fs::write(&nfo, "<episodedetails><title>Fresh episode</title><season>1</season><episode>1</episode><genre>Drama</genre><tag>deep-scan</tag></episodedetails>").unwrap();
        let originals = [&episode, &nfo].map(|path| (
            path.clone(), std::fs::read(path).unwrap(),
            std::fs::metadata(path).unwrap().modified().unwrap(),
        ));
        prove_durable_refresh(&root.path().join("scan.sqlite"), &library, &season).await;
        for (path, bytes, modified) in originals {
            assert_eq!(std::fs::read(&path).unwrap(), bytes, "source bytes unchanged");
            assert_eq!(std::fs::metadata(path).unwrap().modified().unwrap(), modified, "source mtime unchanged");
        }
    }).catch_unwind().await;
    if let Err(panic) = proof {
        eprintln!(
            "retained synthetic durable deep-scan fixture: {}",
            root.keep().display()
        );
        std::panic::resume_unwind(panic);
    }
    let path = root.path().to_path_buf();
    root.close().unwrap();
    assert!(!path.exists(), "successful fixture cleanup");
}

#[expect(
    clippy::disallowed_methods,
    reason = "Explicit mandatory runtime acceptance inputs, not production configuration"
)]
fn required_runtime(name: &str) -> PathBuf {
    let path = PathBuf::from(std::env::var_os(name).expect("set the exact published runtime path"));
    assert!(
        path.is_file(),
        "{name} must identify a real published executable"
    );
    path
}

async fn prove_durable_refresh(database: &Path, library: &Path, season: &Path) {
    let pool = open_database(database, true).await;
    seed_local_library(&pool, library).await;
    let (state, mut queue) = application_state(pool.clone());
    let initial = admit(&state, &mut queue, false).await;
    execute(&state, initial, false).await;
    let baseline = deep_scan_projection(&pool).await;
    assert_international_projection(&baseline);
    eprintln!("initial native TV projection: {baseline:?}");
    let etag = season_etag(&pool, season).await;
    pool.execute(r#"UPDATE "EpisodeMetadata" SET "Title"='Stale projection'"#)
        .await
        .unwrap();
    pool.execute(
        r#"UPDATE "MediaVersion" SET "Duration"='00:00:20' WHERE "EpisodeId" IS NOT NULL"#,
    )
    .await
    .unwrap();
    pool.execute(r#"UPDATE "MediaStream" SET "Language"='und', "Codec"='mp3' WHERE "MediaStreamKind"=2 AND "Index"=2"#).await.unwrap();
    pool.execute(r#"UPDATE "MediaStream" SET "Language"='und' WHERE "MediaStreamKind"=3"#)
        .await
        .unwrap();
    pool.execute(r#"UPDATE "Subtitle" SET "Language"='und'"#)
        .await
        .unwrap();
    let outdated_projection = deep_scan_projection(&pool).await;
    assert_ne!(outdated_projection, baseline);
    let ordinary = admit(&state, &mut queue, false).await;
    execute(&state, ordinary, false).await;
    assert_eq!(
        deep_scan_projection(&pool).await,
        outdated_projection,
        "admitted ordinary scan preserves unchanged-season cache"
    );
    let deep = admit(&state, &mut queue, true).await;
    let persisted = state
        .application
        .managed_library_operation(deep)
        .await
        .unwrap()
        .unwrap();
    assert!(persisted.scan.unwrap().deep_scan);
    assert_eq!(persisted.state, ManagedLibraryOperationState::Queued);
    eprintln!(
        "durable queued deep operation: id={}, scan={:?}",
        persisted.id, persisted.scan
    );
    drop(queue);
    drop(state);
    pool.close().await;

    // Reassemble production repositories and NativeScanRuntime after losing the
    // process-local wakeup token; durable intent alone determines the scan.
    let reopened = open_database(database, false).await;
    let (restarted, mut queue) = application_state(reopened.clone());
    assert_eq!(
        restarted
            .application
            .managed_library_operation(deep)
            .await
            .unwrap()
            .unwrap(),
        persisted
    );
    execute(&restarted, deep, true).await;
    let restored = deep_scan_projection(&reopened).await;
    eprintln!("restarted native TV projection: {restored:?}");
    assert_eq!(
        restored, baseline,
        "durable deep scan restores full metadata/tracks and stable IDs"
    );
    assert_eq!(season_etag(&reopened, season).await, etag);
    let repeated = admit(&restarted, &mut queue, true).await;
    execute(&restarted, repeated, true).await;
    assert_eq!(
        deep_scan_projection(&reopened).await,
        baseline,
        "repeated durable deep scan is idempotent"
    );
    let ordinary = admit(&restarted, &mut queue, false).await;
    execute(&restarted, ordinary, false).await;
    assert_eq!(deep_scan_projection(&reopened).await, baseline);
    assert!(
        restarted
            .application
            .claim_managed_library_operation()
            .await
            .unwrap()
            .is_none(),
        "no outstanding operation remains"
    );
    drop(queue);
    drop(restarted);
    reopened.close().await;
}

fn assert_international_projection(baseline: &DeepScanProjection) {
    assert_eq!(baseline.duration, "00:00:01");
    assert_eq!(baseline.title, "Fresh episode");
    assert_eq!(baseline.streams.len(), 4);
    assert_eq!(baseline.streams[1].3.as_deref(), Some("eng"));
    assert_eq!(baseline.streams[2].3.as_deref(), Some("nld"));
    assert_eq!(baseline.streams[3].0, 3);
    assert_eq!(baseline.streams[3].1, 3);
    assert_eq!(baseline.streams[3].3.as_deref(), Some("nld"));
    assert_eq!(baseline.subtitles.len(), 1);
}

async fn admit(
    state: &AppState,
    queue: &mut UnboundedReceiver<ScanWorkRequest>,
    deep: bool,
) -> i64 {
    let outcome = state
        .application
        .request_library_scan(RequestLibraryScan::new(1, deep))
        .await;
    let RequestScanOutcome::Accepted { operation_id } = outcome else {
        panic!("production scan admission refused: {outcome:?}")
    };
    let wakeup = queue
        .try_recv()
        .expect("native scan queue receives accepted token");
    assert_eq!(wakeup.operation_id, operation_id);
    assert_eq!(
        wakeup.request,
        ScanRequest::Library {
            provider: MediaSourceKind::Local,
            library_id: 1,
            deep_scan: deep
        }
    );
    i64::try_from(operation_id.0).unwrap()
}

async fn execute(state: &AppState, id: i64, deep: bool) {
    let claim = state
        .application
        .claim_managed_library_operation_by_id(id)
        .await
        .unwrap()
        .expect("claim exact persisted operation");
    assert_eq!(claim.operation.scan.unwrap().deep_scan, deep);
    assert_eq!(
        claim.operation.scan.unwrap().provider,
        MediaSourceKind::Local
    );
    assert_eq!(
        state
            .application
            .execute_managed_library_operation(claim)
            .await,
        ExecuteManagedLibraryOperationOutcome::Completed
    );
    assert_eq!(
        state
            .application
            .managed_library_operation(id)
            .await
            .unwrap()
            .unwrap()
            .state,
        ManagedLibraryOperationState::Completed
    );
}

fn application_state(pool: SqlitePool) -> (AppState, UnboundedReceiver<ScanWorkRequest>) {
    let (playout_tx, _playout_rx) = mpsc::unbounded_channel();
    let (scan_tx, scan_rx) = mpsc::unbounded_channel();
    let state = AppState::new_with_scan_queue(
        pool,
        ersatzrs_infra::db::DatabaseBackend::Sqlite,
        SystemStartup {
            database_ready: true,
            schema_status: ersatzrs_infra::db::DatabaseSchemaStatus::default(),
        },
        ersatzrs_ffmpeg::FfmpegRuntimeReport::for_tests(),
        Uuid::nil(),
        1,
        IptvAccessTokenVerifier::disabled(),
        playout_tx,
        scan_tx,
        EntityLocker::new(),
        None,
    );
    (state, scan_rx)
}

async fn open_database(path: &Path, create: bool) -> SqlitePool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(
            SqliteConnectOptions::new()
                .filename(path)
                .create_if_missing(create)
                .foreign_keys(true),
        )
        .await
        .unwrap();
    if create {
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    }
    pool
}

async fn seed_local_library(pool: &SqlitePool, root: &Path) {
    pool.execute(r#"INSERT INTO "MediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "LocalMediaSource" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    pool.execute(r#"INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (1, 'Synthetic TV', 2, 1)"#).await.unwrap();
    pool.execute(r#"INSERT INTO "LocalLibrary" ("Id") VALUES (1)"#)
        .await
        .unwrap();
    sqlx::query(r#"INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, ?)"#)
        .bind(root.to_string_lossy().as_ref())
        .execute(pool)
        .await
        .unwrap();
}

async fn season_etag(pool: &SqlitePool, season: &Path) -> String {
    sqlx::query_scalar(r#"SELECT "Etag" FROM "LibraryFolder" WHERE "Path"=?"#)
        .bind(season.to_string_lossy().as_ref())
        .fetch_one(pool)
        .await
        .expect("completed season etag")
}

async fn count(pool: &SqlitePool, table: &str) -> i64 {
    let sql = format!(r#"SELECT COUNT(*) FROM "{table}""#);
    sqlx::query_scalar(ersatzrs_infra::db::assert_sql_safe(&sql))
        .fetch_one(pool)
        .await
        .unwrap()
}
type TrackProjection = (
    i32,
    i32,
    Option<String>,
    Option<String>,
    Option<String>,
    bool,
    bool,
);

#[derive(Debug, PartialEq, Eq)]
struct DeepScanProjection {
    identities: (i64, i64, i64, i64, i64, i64),
    duration: String,
    title: String,
    streams: Vec<TrackProjection>,
    subtitles: Vec<(i32, Option<String>, Option<String>)>,
    related_counts: Vec<(String, i64)>,
}

async fn deep_scan_projection(pool: &SqlitePool) -> DeepScanProjection {
    let (episode, version, file, metadata, season, show, duration, title): (
        i64,
        i64,
        i64,
        i64,
        i64,
        i64,
        String,
        String,
    ) = sqlx::query_as(
        r#"SELECT e."Id", v."Id", f."Id", m."Id", e."SeasonId", s."ShowId", v."Duration", m."Title"
           FROM "Episode" e JOIN "Season" s ON s."Id"=e."SeasonId"
           JOIN "MediaVersion" v ON v."EpisodeId"=e."Id"
           JOIN "MediaFile" f ON f."MediaVersionId"=v."Id"
           JOIN "EpisodeMetadata" m ON m."EpisodeId"=e."Id"
        "#,
    )
    .fetch_one(pool)
    .await
    .unwrap();
    let streams = sqlx::query_as(r#"SELECT "Index", "MediaStreamKind", "Codec", "Language", "Title", "Default", "Forced" FROM "MediaStream" ORDER BY "Index""#).fetch_all(pool).await.unwrap();
    let subtitles = sqlx::query_as(
        r#"SELECT "StreamIndex", "Language", "Codec" FROM "Subtitle" ORDER BY "StreamIndex""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut related_counts = Vec::new();
    for table in [
        "Show",
        "ShowMetadata",
        "Season",
        "SeasonMetadata",
        "Episode",
        "EpisodeMetadata",
        "MediaVersion",
        "MediaFile",
        "MediaStream",
        "Subtitle",
        "Genre",
        "Tag",
    ] {
        related_counts.push((table.to_owned(), count(pool, table).await));
    }
    DeepScanProjection {
        identities: (episode, version, file, metadata, season, show),
        duration,
        title,
        streams,
        subtitles,
        related_counts,
    }
}

async fn generate_deep_scan_episode(ffmpeg: &str, root: &Path, episode: &Path) {
    let subtitle = root.join("source.nl.srt");
    std::fs::write(
        &subtitle,
        "1\n00:00:00,000 --> 00:00:01,000\nSynthetic Dutch caption\n",
    )
    .unwrap();
    let mut command = tokio::process::Command::new(ffmpeg);
    command
        .args([
            "-v",
            "error",
            "-nostdin",
            "-f",
            "lavfi",
            "-i",
            "color=blue:s=64x64:r=25:d=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:sample_rate=48000:duration=1",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=880:sample_rate=48000:duration=1",
            "-i",
        ])
        .arg(&subtitle)
        .args([
            "-map",
            "0:v",
            "-map",
            "1:a",
            "-map",
            "2:a",
            "-map",
            "3:s",
            "-c:v",
            "libx264",
            "-threads",
            "1",
            "-c:a",
            "aac",
            "-ac",
            "2",
            "-c:s",
            "srt",
            "-metadata:s:a:0",
            "language=eng",
            "-metadata:s:a:0",
            "title=English",
            "-metadata:s:a:1",
            "language=nld",
            "-metadata:s:a:1",
            "title=Dutch",
            "-metadata:s:s:0",
            "language=nld",
            "-metadata:s:s:0",
            "title=Dutch",
        ])
        .arg(episode);
    run_deep_scan_generator(&mut command).await;
}

async fn run_deep_scan_generator(command: &mut tokio::process::Command) {
    use tokio::io::AsyncReadExt as _;
    let mut child = command
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut stderr = child.stderr.take().unwrap().take(128 * 1024 + 1);
    let reader = tokio::spawn(async move {
        let mut bytes = Vec::new();
        stderr.read_to_end(&mut bytes).await.unwrap();
        bytes
    });
    let waited = tokio::time::timeout(std::time::Duration::from_secs(30), child.wait()).await;
    if !matches!(&waited, Ok(Ok(_))) {
        tokio::time::timeout(std::time::Duration::from_secs(5), child.kill())
            .await
            .expect("synthetic generator cleanup deadline")
            .expect("synthetic generator must be reaped before removing source assets");
    }
    let errors = reader.await.unwrap();
    assert!(errors.len() <= 128 * 1024, "bounded fixture stderr");
    assert!(
        waited
            .expect("bounded synthetic generation")
            .unwrap()
            .success(),
        "{}",
        String::from_utf8_lossy(&errors)
    );
}
