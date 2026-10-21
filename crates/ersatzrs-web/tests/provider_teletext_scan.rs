//! Real host-managed provider scan must bind native source teletext independently.
use std::path::{Path, PathBuf};

use chrono::Utc;
use ersatzrs_app::ScanRuntime;
use ersatzrs_core::repositories::{ManagedLibraryOperationRepository, ManagedLibraryScanSpec};
use ersatzrs_domain::{
    ManagedLibraryOperationKind, MediaSourceKind, MediaStreamKind, SourceTeletextBinding,
    SourceTeletextDescriptors, SourceTeletextObservedPages, SourceTeletextPid,
};
use ersatzrs_infra::ffprobe::{FfprobeExtractor, LocalMediaProbe, LocalProbeCleanupScope};
use ersatzrs_infra::repositories::SqliteManagedLibraryOperationRepository;
use ersatzrs_infra::teletext_source;
use ersatzrs_web::host::scanning::NativeScanRuntime;
use sha2::{Digest as _, Sha256};
use sqlx::SqlitePool;

#[path = "provider_teletext_scan/fixture.rs"]
mod fixture;

#[allow(
    clippy::disallowed_methods,
    reason = "Explicit opt-in native test tool and redistributable synthetic source only"
)]
fn inputs() -> (String, PathBuf) {
    (
        std::env::var("FFPROBE_PATH").expect("set pinned FFprobe 9"),
        std::env::var_os("ERSATZRS_TELETEXT_SCAN_FIXTURE")
            .expect("set synthetic two-audio AV/200/888 source")
            .into(),
    )
}

async fn qualified_source(probe: &str, source: &Path) -> LocalMediaProbe {
    use std::fmt::Write as _;
    assert_eq!(ersatzrs_ffmpeg::ffprobe_path(), probe);
    let mut hash = String::with_capacity(64);
    for byte in Sha256::digest(std::fs::read(source).unwrap()) {
        write!(hash, "{byte:02x}").unwrap();
    }
    assert_eq!(
        hash, "472f563d11ba15c774161647dd7d4abbfa810ac7e50e5bdabf047810a7993794",
        "only the independently qualified synthetic AV/200/888 source is accepted"
    );
    let cleanup = LocalProbeCleanupScope::new();
    let extractor = FfprobeExtractor::new(probe);
    let outcome = cleanup
        .scope(extractor.extract_local_with_teletext(source))
        .await;
    cleanup.close();
    cleanup.wait_cleanup().await.unwrap();
    assert_eq!(cleanup.cleanup_status(), Ok(true));
    let source = outcome.expect("real native fixture must qualify before provider sync");
    assert_eq!(source.media.streams.len(), 4);
    assert_eq!(
        source
            .media
            .streams
            .iter()
            .map(|stream| (stream.index, stream.kind))
            .collect::<Vec<_>>(),
        [
            (0, MediaStreamKind::Video),
            (1, MediaStreamKind::Audio),
            (2, MediaStreamKind::Audio),
            (3, MediaStreamKind::Subtitle)
        ]
    );
    assert_eq!(source.media.streams[1].language.as_deref(), Some("eng"));
    assert_eq!(source.media.streams[2].language.as_deref(), Some("nld"));
    assert_eq!(
        source
            .media
            .streams
            .iter()
            .map(|stream| stream.codec.as_deref())
            .collect::<Vec<_>>(),
        [
            Some("mpeg2video"),
            Some("aac"),
            Some("aac"),
            Some("dvb_teletext")
        ]
    );
    let acquisition = source
        .teletext
        .as_ref()
        .expect("native teletext association");
    let headers = acquisition
        .headers()
        .for_stream(3)
        .expect("actual bounded stream3 headers");
    assert_eq!(headers.packet_count, 303);
    assert_eq!(headers.complete_pes, 14);
    assert!(headers.ended_at_eof && !headers.unobserved_prefix);
    source
}

async fn claim(pool: &SqlitePool) -> ersatzrs_core::repositories::ManagedLibraryOperationClaim {
    claim_for(pool, MediaSourceKind::Jellyfin, None).await
}

async fn claim_for(
    pool: &SqlitePool,
    provider: MediaSourceKind,
    show_id: Option<i64>,
) -> ersatzrs_core::repositories::ManagedLibraryOperationClaim {
    let repository = SqliteManagedLibraryOperationRepository(pool.clone());
    let operation = repository
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            1,
            ManagedLibraryScanSpec {
                provider,
                show_id,
                deep_scan: true,
            },
            false,
        )
        .await
        .unwrap()
        .unwrap();
    repository
        .claim_operation(operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap()
}

type OrdinaryTrack = (i64, i64, Option<String>, Option<String>, i64, i64);

async fn assert_ordinary(pool: &SqlitePool, source: &LocalMediaProbe) -> (i64, i64, i64) {
    Box::pin(assert_ordinary_at(pool, source, 3)).await
}

async fn assert_ordinary_at(
    pool: &SqlitePool,
    source: &LocalMediaProbe,
    teletext_index: i32,
) -> (i64, i64, i64) {
    let tracks: Vec<OrdinaryTrack> = sqlx::query_as(
        r#"SELECT "Index","MediaStreamKind","Codec","Language","Default","Forced"
           FROM "MediaStream" ORDER BY "Index""#,
    )
    .fetch_all(pool)
    .await
    .unwrap();
    assert_eq!(
        tracks.len(),
        4,
        "ordinary provider synchronization must succeed first"
    );
    for (row, expected) in tracks.iter().zip(&source.media.streams) {
        let index = if expected.index == 3 {
            teletext_index
        } else {
            expected.index
        };
        assert_eq!(row.0, i64::from(index));
        assert_eq!(
            row.1,
            match expected.kind {
                MediaStreamKind::Video => 1,
                MediaStreamKind::Audio => 2,
                MediaStreamKind::Subtitle => 3,
                _ => unreachable!(),
            }
        );
        assert_eq!(row.2, expected.codec);
        assert_eq!(row.3, expected.language);
        assert_eq!(row.4, i64::from(expected.default));
        assert_eq!(row.5, i64::from(expected.forced));
    }
    let (path,state): (String,i64) = sqlx::query_as(
        r#"SELECT f."Path",i."State" FROM "MediaFile" f JOIN "MediaVersion" v ON v."Id"=f."MediaVersionId"
           JOIN "MediaItem" i ON i."Id"=COALESCE(v."MovieId",v."EpisodeId",v."OtherVideoId")"#,
    ).fetch_one(pool).await.unwrap();
    assert_eq!(path, fixture::PROVIDER_PATH);
    assert_eq!(state, 0, "mapped provider source must remain available");
    sqlx::query_as(
        r#"SELECT f."Id",f."MediaVersionId",s."Id" FROM "MediaFile" f
        JOIN "MediaStream" s ON s."MediaVersionId"=f."MediaVersionId" WHERE s."Index"=?"#,
    )
    .bind(teletext_index)
    .fetch_one(pool)
    .await
    .unwrap()
}

async fn scan_and_close(
    pool: &SqlitePool,
    claim: &ersatzrs_core::repositories::ManagedLibraryOperationClaim,
    server: fixture::Provider,
) -> Result<ersatzrs_app::ScanRuntimeReport, ersatzrs_app::ScanRuntimeError> {
    let runtime = NativeScanRuntime::new(pool.clone());
    let cleanup = LocalProbeCleanupScope::new();
    let outcome = cleanup
        .scope(runtime.scan_library(MediaSourceKind::Jellyfin, 1, true, &(), Some(claim)))
        .await;
    cleanup.close();
    let closed = cleanup.wait_cleanup().await;
    let requests = server.close().await;
    closed.expect("all native scan probe ownership must drain");
    assert_eq!(cleanup.cleanup_status(), Ok(true));
    assert_eq!(
        requests,
        [1, 1],
        "actual provider endpoints must both be consumed"
    );
    outcome
}

async fn assert_report(pool: &SqlitePool, source: &LocalMediaProbe, ids: (i64, i64, i64)) {
    assert_report_for(pool, source, ids, MediaSourceKind::Jellyfin).await;
}

async fn assert_report_for(
    pool: &SqlitePool,
    source: &LocalMediaProbe,
    ids: (i64, i64, i64),
    provider: MediaSourceKind,
) {
    let mut tx = pool.begin().await.unwrap();
    let report = teletext_source::load(&mut tx, ids.2, ids.0)
        .await
        .unwrap()
        .expect("managed provider scan must persist its source-qualified teletext report");
    let SourceTeletextBinding::Bound(binding) = report.binding else {
        panic!("managed provider binding")
    };
    assert_eq!(binding.provider, provider);
    assert_eq!(binding.media_source_id, 1);
    assert_eq!(
        (
            binding.media_file_id,
            binding.media_version_id,
            binding.media_stream_id
        ),
        ids
    );
    assert_eq!(
        &binding.source_revision,
        source.teletext.as_ref().unwrap().revision()
    );
    assert_eq!(report.stream_index, 3);
    assert_eq!(report.pid, SourceTeletextPid::Reported(0x103));
    let SourceTeletextDescriptors::Reported {
        entries,
        trailing_unpaired_pairs,
    } = report.descriptors
    else {
        panic!("real native source descriptors")
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
        [(0, 200, 1, "eng"), (1, 888, 2, "eng")]
    );
    let SourceTeletextObservedPages::HeadersObserved(headers) = report.observed_pages else {
        panic!("real bounded source header window")
    };
    assert_eq!(
        headers.as_ref(),
        source
            .teletext
            .as_ref()
            .unwrap()
            .headers()
            .for_stream(3)
            .unwrap()
    );
    tx.rollback().await.unwrap();
}

#[tokio::test]
#[ignore = "requires FFprobe9 and ERSATZRS_TELETEXT_SCAN_FIXTURE synthetic AV/200/888"]
async fn managed_jellyfin_scan_preserves_audio_and_binds_native_teletext_report() {
    let (probe, input) = inputs();
    let root = tempfile::tempdir().unwrap();
    let local = root.path().join("movie.ts");
    std::fs::copy(input, &local).unwrap();
    let source = Box::pin(qualified_source(&probe, &local)).await;
    let server = fixture::Provider::start(&source).await;
    let pool = fixture::database(root.path(), &server.address).await;
    let claim = claim(&pool).await;
    Box::pin(scan_and_close(&pool, &claim, server))
        .await
        .expect("ordinary host provider scan must finish successfully");
    let ids = Box::pin(assert_ordinary(&pool, &source)).await;
    Box::pin(assert_report(&pool, &source, ids)).await;

    // Same bytes and provider identity, but a genuinely new file revision must
    // replace the previous report instead of preserving its stale association.
    let changed =
        std::fs::metadata(&local).unwrap().modified().unwrap() + std::time::Duration::from_mins(2);
    std::fs::File::options()
        .write(true)
        .open(&local)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(changed))
        .unwrap();
    let refreshed = Box::pin(qualified_source(&probe, &local)).await;
    assert_ne!(
        source.teletext.as_ref().unwrap().revision(),
        refreshed.teletext.as_ref().unwrap().revision()
    );
    let server = fixture::Provider::start(&refreshed).await;
    sqlx::query(r#"UPDATE "JellyfinConnection" SET "Address"=?"#)
        .bind(&server.address)
        .execute(&pool)
        .await
        .unwrap();
    Box::pin(scan_and_close(&pool, &claim, server))
        .await
        .expect("deep rescan must replace the source-qualified report");
    let refreshed_ids = Box::pin(assert_ordinary(&pool, &refreshed)).await;
    assert_eq!(
        (ids.0, ids.1),
        (refreshed_ids.0, refreshed_ids.1),
        "deep rescan preserves file and version identity"
    );
    Box::pin(assert_report(&pool, &refreshed, refreshed_ids)).await;
    let reports: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reports, 1, "deep rescan leaves exactly the fresh binding");
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires FFprobe9 and ERSATZRS_TELETEXT_SCAN_FIXTURE synthetic AV/200/888"]
async fn managed_jellyfin_scan_refuses_mismatched_declarations_without_losing_tracks() {
    let (probe, input) = inputs();
    let root = tempfile::tempdir().unwrap();
    let local = root.path().join("movie.ts");
    std::fs::copy(input, &local).unwrap();
    let source = Box::pin(qualified_source(&probe, &local)).await;
    for (declaration, index) in [
        (fixture::Declaration::SelectedPathMismatch, 3),
        (fixture::Declaration::NativeIndexMismatch, 4),
    ] {
        let server = fixture::Provider::with_declaration(&source, declaration).await;
        let pool = fixture::database(root.path(), &server.address).await;
        let claim = claim(&pool).await;
        Box::pin(scan_and_close(&pool, &claim, server))
            .await
            .expect("an unqualified report must not damage ordinary provider sync");
        let ids = Box::pin(assert_ordinary_at(&pool, &source, index)).await;
        let mut tx = pool.begin().await.unwrap();
        assert!(
            teletext_source::load(&mut tx, ids.2, ids.0)
                .await
                .unwrap()
                .is_none(),
            "a mismatched selected source or native index cannot bind a report"
        );
        tx.rollback().await.unwrap();
        let reports: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            reports, 0,
            "no alternate or unbound report may hide a mismatch"
        );
        pool.close().await;
    }
}

#[tokio::test]
#[ignore = "requires FFprobe9 and ERSATZRS_TELETEXT_SCAN_FIXTURE synthetic AV/200/888"]
async fn managed_jellyfin_scan_expired_lease_refuses_leaf_writes() {
    let (probe, input) = inputs();
    let root = tempfile::tempdir().unwrap();
    let local = root.path().join("movie.ts");
    std::fs::copy(input, &local).unwrap();
    let source = Box::pin(qualified_source(&probe, &local)).await;
    let server = fixture::Provider::start(&source).await;
    let pool = fixture::database(root.path(), &server.address).await;
    let claim = claim(&pool).await;
    let changed = sqlx::query(r#"UPDATE "ManagedLibraryLease" SET "LeaseExpiresAt"='2000-01-01 00:00:00' WHERE "OperationId"=?"#)
        .bind(claim.operation.id).execute(&pool).await.unwrap();
    assert_eq!(
        changed.rows_affected(),
        1,
        "expire the actual claimed library lease"
    );
    let outcome = Box::pin(scan_and_close(&pool, &claim, server)).await;
    let error = outcome.expect_err("an expired real claim must refuse its leaf transaction");
    assert!(error
        .to_string()
        .contains("mapped source scan claim is no longer current"));
    let counts: Vec<(String, i64)> = sqlx::query_as(
        r#"
        SELECT 'Movie', COUNT(*) FROM "Movie"
        UNION ALL SELECT 'MovieMetadata', COUNT(*) FROM "MovieMetadata"
        UNION ALL SELECT 'MediaItem', COUNT(*) FROM "MediaItem"
        UNION ALL SELECT 'MediaVersion', COUNT(*) FROM "MediaVersion"
        UNION ALL SELECT 'MediaFile', COUNT(*) FROM "MediaFile"
        UNION ALL SELECT 'MediaStream', COUNT(*) FROM "MediaStream"
        UNION ALL SELECT 'SourceTeletextMetadata', COUNT(*) FROM "SourceTeletextMetadata"
    "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(counts.len(), 7);
    for (table, count) in counts {
        assert_eq!(count, 0, "expired claim wrote {table}");
    }
    pool.close().await;
}

#[path = "provider_teletext_scan/actions.rs"]
mod actions;
