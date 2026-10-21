//! Actual Local scan routes with a pinned probe and redistributable AV/200/888.
//! Actual bounded header observations still do not establish page bodies or selection.

use std::path::{Path, PathBuf};

use chrono::Utc;
use ersatzrs_core::repositories::{ManagedLibraryOperationRepository, ManagedLibraryScanSpec};
use ersatzrs_domain::{
    ManagedLibraryOperationKind, MediaSourceKind, SourceTeletextBinding, SourceTeletextDescriptors,
    SourceTeletextObservedPages, SourceTeletextPid, SourceTeletextRevision,
};
use ersatzrs_infra::ffprobe::LocalProbeCleanupScope;
use ersatzrs_infra::playback::source_teletext::runtime::{self, RuntimeSelectionContext};
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
        let SourceTeletextObservedPages::HeadersObserved(headers) = report.observed_pages else {
            panic!("native scan must persist actual bounded headers");
        };
        assert_eq!(headers.packet_count, 303);
        assert_eq!(headers.complete_pes, 14);
        assert!(headers.ended_at_eof);
        assert!(!headers.unobserved_prefix);
        assert_eq!(headers.programs.len(), 1);
        assert_eq!(
            (
                headers.programs[0].program_number,
                headers.programs[0].pmt_pid,
                headers.programs[0].pcr_pid,
                headers.programs[0].stream_type
            ),
            (1, 0x1000, 0x100, 6)
        );
        assert_eq!(
            headers
                .headers
                .iter()
                .map(|h| (h.wire_magazine, h.wire_page))
                .collect::<Vec<_>>(),
            [
                (2, 0),
                (2, 1),
                (0, 0x88),
                (0, 0x99),
                (2, 0),
                (2, 1),
                (0, 0x88),
                (0, 0x99)
            ]
        );
        assert_ne!(headers.psi_fingerprint, [0; 32]);
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

async fn caption_selection_fixture(probe: &str, fixture: &Path) -> (tempfile::TempDir, SqlitePool) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Synthetic.ts");
    std::fs::copy(fixture, &path).unwrap();
    std::fs::write(
        path.with_extension("nld.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nSynthetic ordinary caption\n",
    )
    .unwrap();
    let pool = database(root.path(), 4).await;
    scan(&pool, &config(&pool, probe).await).await;
    (root, pool)
}

async fn qualify_scanned_caption(
    probe: &str,
    path: &Path,
    candidate: teletext_source::SourceTeletextCaptionCandidate,
    scope: &LocalProbeCleanupScope,
    work: &ersatzrs_core::locking::TeletextWorkGuard,
) -> ersatzrs_infra::playback::source_teletext::QualifiedSourceCaption {
    use ersatzrs_infra::ffprobe::FfprobeExtractor;
    use ersatzrs_infra::playback::source_teletext::QualifiedSourceCaption;
    use std::{sync::Arc, time::Duration};
    use tokio::time::Instant;

    assert_eq!(candidate.request.page.get(), 888);
    assert_eq!(candidate.request.stream_index, 3);
    assert_eq!(candidate.request.pid, 0x103);
    assert_eq!(candidate.request.program_number, 1);
    assert_eq!(candidate.language.as_str(), "eng");
    assert_eq!(candidate.descriptor_type, 2);
    assert!(!candidate.is_sdh());
    assert_eq!(candidate.descriptor_occurrences.len(), 1);
    assert_eq!(candidate.descriptor_occurrences[0].descriptor_tag, 0x56);

    let decoded = Arc::new(
        scope
            .scope(FfprobeExtractor::new(probe).decode_local_teletext_page(
                candidate.request.clone(),
                path.to_path_buf(),
                path.to_path_buf(),
                Instant::now() + Duration::from_secs(15),
                work.hold_work(),
            ))
            .await
            .unwrap(),
    );
    assert_eq!(decoded.window().psi_fingerprint, candidate.psi_fingerprint);
    assert_eq!(decoded.events().count, 2);
    assert_eq!(decoded.window().source_packets, 303);
    assert_eq!(decoded.window().decoder_packets, 303);
    assert!(decoded.window().ended_at_eof);
    let mut wrong_page = candidate.clone();
    wrong_page.request.page = ersatzrs_domain::TeletextPageNumber::new(200).unwrap();
    assert!(QualifiedSourceCaption::new(wrong_page, decoded.clone(), work.hold_work()).is_err());
    let mut wrong_psi = candidate.clone();
    wrong_psi.psi_fingerprint[0] ^= 1;
    assert!(QualifiedSourceCaption::new(wrong_psi, decoded.clone(), work.hold_work()).is_err());
    QualifiedSourceCaption::new(candidate, decoded, work.hold_work()).unwrap()
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic two-audio AV/200/888 input"]
async fn native_scan_caption_selection_uses_wire_language_and_exact_decoded_page() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    use ersatzrs_domain::{ChannelSubtitleMode, StreamingMode};
    use ersatzrs_infra::playback::{
        load_subtitles_for_media_item, source_teletext::select_native_playback_subtitle_for_version,
    };

    let (probe, fixture) = inputs();
    let (root, pool) = caption_selection_fixture(&probe, &fixture).await;
    let path = root.path().join("Synthetic.ts");
    let (item, version, file, original): (i64, i64, i64, String) = sqlx::query_as(
        r#"SELECT v."OtherVideoId",v."Id",f."Id",f."Path"
        FROM "MediaVersion" v JOIN "MediaFile" f ON f."MediaVersionId"=v."Id""#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let before = load_subtitles_for_media_item(&pool, item).await.unwrap();
    let before_tracks = tracks(&pool, 1).await;
    let choices = teletext_source::load_caption_candidates(&pool, item, version, file, &original)
        .await
        .unwrap();
    assert_eq!(choices.len(), 1, "observed guide 200 is not a caption");
    let candidate = choices.into_iter().next().unwrap();
    let locker = EntityLocker::new();
    let work = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let scope = LocalProbeCleanupScope::new();
    let qualified = [qualify_scanned_caption(&probe, &path, candidate, &scope, &work).await];
    let selected = select_native_playback_subtitle_for_version(
        &pool,
        item,
        version,
        &["en".into()],
        ChannelSubtitleMode::Any,
        StreamingMode::TransportStream,
        &qualified,
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(selected.subtitle().language.as_deref(), Some("eng"));
    let source = selected.source().unwrap();
    assert_eq!(source.candidate().request.page.get(), 888);
    let input = source.receipt().native_burn_input();
    assert!(input.is_image_based_subtitle);
    assert_eq!(input.streams[0].index, 3);
    assert_eq!(input.streams[0].source_teletext_page.unwrap().get(), 888);
    let original_subtitle = before.iter().find(|s| s.stream_index == 3).unwrap();
    assert_eq!(selected.subtitle().default, original_subtitle.default);
    assert_eq!(selected.subtitle().forced, original_subtitle.forced);
    assert_eq!(selected.subtitle().id, original_subtitle.id);
    let ordinary = select_native_playback_subtitle_for_version(
        &pool,
        item,
        version,
        &["nl".into()],
        ChannelSubtitleMode::Any,
        StreamingMode::TransportStream,
        &qualified,
    )
    .await
    .unwrap()
    .unwrap();
    // Existing sidecar parsing normalizes Dutch to its bibliographic ISO alias.
    assert_eq!(ordinary.subtitle().language.as_deref(), Some("dut"));
    assert!(ordinary.source().is_none());
    assert!(select_native_playback_subtitle_for_version(
        &pool,
        item,
        version,
        &["en".into()],
        ChannelSubtitleMode::None,
        StreamingMode::TransportStream,
        &qualified,
    )
    .await
    .unwrap()
    .is_none());
    assert_eq!(
        before,
        load_subtitles_for_media_item(&pool, item).await.unwrap()
    );
    assert_eq!(before_tracks, tracks(&pool, 1).await);
    scope.close();
    scope.wait_cleanup().await.unwrap();
    drop(work);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
    drop(qualified);
    assert!(TeletextWorkGuard::try_acquire(&locker, 1).is_none());
    drop(selected);
    assert_eq!(locker.active_teletext_work(), 0);
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic two-audio AV/200/888 input"]
async fn native_shared_caption_runtime_releases_preparation_and_preserves_ordinary_fallback() {
    use ersatzrs_core::locking::{EntityLocker, TeletextWorkGuard};
    use ersatzrs_domain::{ChannelSubtitleMode, StreamingMode};
    use tokio::time::Instant;

    let (probe, fixture) = inputs();
    let (root, pool) = caption_selection_fixture(&probe, &fixture).await;
    let path = root.path().join("Synthetic.ts");
    let (item, version, file, original): (i64, i64, i64, String) = sqlx::query_as(
        r#"SELECT v."OtherVideoId",v."Id",f."Id",f."Path"
        FROM "MediaVersion" v JOIN "MediaFile" f ON f."MediaVersionId"=v."Id""#,
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let before_tracks = tracks(&pool, 1).await;
    let before_subtitles = subtitles(&pool, 1).await;
    let locker = EntityLocker::new();
    let languages = ["en".to_owned()];
    let context = || RuntimeSelectionContext {
        pool: &pool,
        locker: &locker,
        media_item_id: Some(item),
        media_version_id: Some(version),
        media_file_id: Some(file),
        original_path: &original,
        resolved_path: path.to_str().unwrap(),
        languages: &languages,
        subtitle_mode: ChannelSubtitleMode::Any,
        streaming_mode: StreamingMode::TransportStream,
        source_teletext_delivery: ersatzrs_domain::SourceTeletextCaptionDelivery::Burn,
        playback_interval: None,
        deadline: Instant::now() + std::time::Duration::from_secs(20),
    };
    let selected = runtime::select(context()).await.unwrap().unwrap();
    assert_eq!(selected.subtitle.unwrap().language.as_deref(), Some("eng"));
    let source = selected.source.unwrap();
    assert_eq!(source.receipt().request().page.get(), 888);
    assert_eq!(locker.active_teletext_work(), 0);
    source.recheck(source.initial_deadline()).await.unwrap();
    assert_eq!(locker.active_teletext_work(), 0);
    let other = TeletextWorkGuard::try_acquire(&locker, 1).unwrap();
    let fallback = runtime::select(context()).await.unwrap().unwrap();
    assert!(fallback.source.is_none());
    assert!(
        fallback.subtitle.is_none(),
        "English preferences must not substitute Dutch captions"
    );
    let dutch = ["nl".to_owned()];
    let ordinary = runtime::select(RuntimeSelectionContext {
        languages: &dutch,
        ..context()
    })
    .await
    .unwrap()
    .unwrap();
    assert!(ordinary.source.is_none());
    assert_eq!(ordinary.subtitle.unwrap().language.as_deref(), Some("dut"));
    assert!(source.recheck(source.initial_deadline()).await.is_err());
    drop(other);
    assert_eq!(locker.active_teletext_work(), 0);
    let modified = path.metadata().unwrap().modified().unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new().set_modified(modified + std::time::Duration::from_secs(1)),
        )
        .unwrap();
    assert!(source
        .recheck(Instant::now() + std::time::Duration::from_secs(15))
        .await
        .is_err());
    let stale = runtime::select(context()).await.unwrap().unwrap();
    assert!(stale.source.is_none());
    assert!(
        stale.subtitle.is_none(),
        "Stale English pages cannot authorize another language"
    );
    assert_eq!(locker.active_teletext_work(), 0);
    sqlx::query(r#"DELETE FROM "SourceTeletextMetadata""#)
        .execute(&pool)
        .await
        .unwrap();
    let missing = runtime::select(context()).await.unwrap().unwrap();
    assert!(missing.source.is_none());
    assert!(missing.subtitle.is_none());
    let ordinary = runtime::select(RuntimeSelectionContext {
        languages: &dutch,
        ..context()
    })
    .await
    .unwrap()
    .unwrap();
    assert!(ordinary.source.is_none());
    assert_eq!(ordinary.subtitle.unwrap().language.as_deref(), Some("dut"));
    assert_eq!(locker.active_teletext_work(), 0);
    assert_eq!(tracks(&pool, 1).await, before_tracks);
    assert_eq!(subtitles(&pool, 1).await, before_subtitles);
    drop(source);
    pool.close().await;
}

#[tokio::test]
#[ignore = "requires pinned FFprobe 9 and synthetic two-audio AV/200/888 input"]
async fn native_rejected_source_header_clears_old_window_without_losing_ordinary_tracks() {
    let (probe, fixture) = inputs();
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("Synthetic.ts");
    std::fs::copy(fixture, &path).unwrap();
    std::fs::write(
        path.with_extension("eng.srt"),
        "1\n00:00:00,000 --> 00:00:01,000\nSynthetic caption\n",
    )
    .unwrap();
    let pool = database(root.path(), 4).await;
    let config = config(&pool, &probe).await;
    scan(&pool, &config).await;
    let before_tracks = tracks(&pool, 1).await;
    let before_subtitles = subtitles(&pool, 1).await;
    let before_reports = reports(&pool, 1).await;
    let modified = path.metadata().unwrap().modified().unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    let packet = bytes
        .chunks_exact_mut(188)
        .find(|p| (u16::from(p[1] & 31) << 8 | u16::from(p[2])) == 0x103 && p[1] & 0x40 != 0)
        .unwrap();
    assert_eq!(packet[3] & 0x30, 0x10);
    assert_eq!(&packet[4..7], &[0, 0, 1]);
    // Two flipped bits in the first protected page byte: invalid optional
    // teletext metadata, with every audio/video packet left byte-identical.
    packet[4 + 46 + 6] ^= 3;
    std::fs::write(&path, bytes).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(
            std::fs::FileTimes::new().set_modified(modified + std::time::Duration::from_secs(1)),
        )
        .unwrap();
    scan(&pool, &config).await;
    assert_eq!(tracks(&pool, 1).await, before_tracks);
    assert_eq!(subtitles(&pool, 1).await, before_subtitles);
    let (stream_id,file_id):(i64,i64)=sqlx::query_as(r#"SELECT s."Id",f."Id" FROM "MediaStream" s JOIN "MediaFile" f ON f."MediaVersionId"=s."MediaVersionId" WHERE s."Codec"='dvb_teletext'"#).fetch_one(&pool).await.unwrap();
    // read-only transaction: inspect the replacement descriptor report/window.
    let mut tx = pool.begin().await.unwrap();
    let report = teletext_source::load(&mut tx, stream_id, file_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        report.observed_pages,
        SourceTeletextObservedPages::NotAcquired
    );
    let SourceTeletextBinding::Bound(binding) = report.binding else {
        panic!("current source binding")
    };
    assert_eq!(file_id, before_reports[0].0);
    assert_ne!(binding.source_revision, before_reports[0].1);
    assert!(matches!(
        report.descriptors,
        SourceTeletextDescriptors::Reported { .. }
    ));
    tx.rollback().await.unwrap();
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextHeaderWindow""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    pool.close().await;
}
