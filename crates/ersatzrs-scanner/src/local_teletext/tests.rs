use chrono::{Duration, Utc};
use ersatzrs_core::repositories::{ManagedLibraryOperationRepository, ManagedLibraryScanSpec};
use ersatzrs_domain::ManagedLibraryOperationKind;
use ersatzrs_infra::db::{begin_write, DbPool};
use ersatzrs_infra::repositories::SqliteManagedLibraryOperationRepository;
use sqlx::{sqlite::SqlitePoolOptions, Executor as _};

use super::*;

async fn database() -> DbPool {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    pool.execute("PRAGMA foreign_keys=ON").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (11);
        INSERT INTO "LocalMediaSource" ("Id") VALUES (11);
        INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId")
            VALUES (21,'Synthetic one',1,11),(22,'Synthetic two',1,11);
        INSERT INTO "LocalLibrary" ("Id") VALUES (21),(22);
        INSERT INTO "LibraryPath" ("Id","LibraryId","Path")
            VALUES (31,21,'fixture/one'),(32,22,'fixture/two');
        INSERT INTO "MediaItem" ("Id","LibraryPathId") VALUES (101,31);
        INSERT INTO "Movie" ("Id") VALUES (101);
        INSERT INTO "MediaVersion"
            ("Id","MovieId","DateAdded","DateUpdated","Duration","Height","Width","VideoScanKind")
            VALUES (301,101,'2026-01-01 00:00:00','2026-01-01 00:00:00','00:01:00',576,720,0);
        INSERT INTO "MediaFile" ("Id","MediaVersionId","Path","PathHash")
            VALUES (501,301,'fixture/source.ts','synthetic');
        INSERT INTO "MediaStream"
            ("Id","MediaVersionId","Index","Codec","MediaStreamKind","Language","Channels","Default","Forced")
            VALUES (401,301,4,'dvb_teletext',3,'eng,eng',0,0,0),
                   (402,301,1,'aac',2,'nld',2,1,0);
    "#).execute(&pool).await.unwrap();
    pool
}

async fn scan_claim(pool: &DbPool) -> ManagedLibraryOperationClaim {
    let repository = SqliteManagedLibraryOperationRepository(pool.clone());
    let operation = repository
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            21,
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
    repository
        .claim_operation(operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap()
}

fn context(claim: ManagedLibraryOperationClaim) -> LocalTeletextWrite {
    LocalTeletextWrite {
        claim: Some(claim),
        ..LocalTeletextWrite::default()
    }
}

async fn ordinary_mutation(tx: &mut Transaction<'_, Db>) {
    sqlx::query(r#"UPDATE "MediaVersion" SET "Width"=800 WHERE "Id"=301"#)
        .execute(&mut **tx)
        .await
        .unwrap();
}

async fn persisted_counts(pool: &DbPool) -> (i64, i64) {
    let width = sqlx::query_scalar(r#"SELECT "Width" FROM "MediaVersion" WHERE "Id"=301"#)
        .fetch_one(pool)
        .await
        .unwrap();
    let reports = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
        .fetch_one(pool)
        .await
        .unwrap();
    (width, reports)
}

#[tokio::test]
async fn local_teletext_claim_matches_actual_leaf_and_rejects_forged_library() {
    let pool = database().await;
    let claim = scan_claim(&pool).await;
    let valid = context(claim.clone());
    let mut tx = begin_write(&pool).await.unwrap();
    valid.check_claim(&mut tx, 31).await.unwrap();
    assert!(valid.check_claim(&mut tx, 32).await.is_err());
    let mut forged = claim;
    forged.operation.library_id = 22;
    assert!(context(forged).check_claim(&mut tx, 32).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(persisted_counts(&pool).await, (720, 0));
}

#[tokio::test]
async fn local_teletext_reclaimed_owner_cannot_authorize_ordinary_mutation() {
    let pool = database().await;
    let claim = scan_claim(&pool).await;
    let repository = SqliteManagedLibraryOperationRepository(pool.clone());
    let fresh = repository
        .claim_next(Utc::now() + Duration::seconds(91))
        .await
        .unwrap()
        .unwrap();
    assert_eq!(fresh.operation.id, claim.operation.id);
    assert_ne!(fresh.lease_owner, claim.lease_owner);
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(context(claim).check_claim(&mut tx, 31).await.is_err());
    context(fresh).check_claim(&mut tx, 31).await.unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(persisted_counts(&pool).await, (720, 0));
}

#[tokio::test]
async fn local_teletext_claim_loss_after_ordinary_write_requires_whole_leaf_rollback() {
    let pool = database().await;
    let writer = context(scan_claim(&pool).await);
    let mut tx = begin_write(&pool).await.unwrap();
    writer.check_claim(&mut tx, 31).await.unwrap();
    ordinary_mutation(&mut tx).await;
    // Deterministic expiry fault inside the disposable leaf transaction. This
    // never creates admission authority; the actual claim came from the repo.
    sqlx::query(r#"UPDATE "ManagedLibraryLease" SET "LeaseExpiresAt"='2000-01-01T00:00:00Z'"#)
        .execute(&mut *tx)
        .await
        .unwrap();
    assert!(writer.check_claim(&mut tx, 31).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(persisted_counts(&pool).await, (720, 0));
    let mut tx = begin_write(&pool).await.unwrap();
    writer.check_claim(&mut tx, 31).await.unwrap();
    tx.rollback().await.unwrap();
}

async fn publication_child(pool: &DbPool) -> ManagedLibraryOperationClaim {
    sqlx::raw_sql(r#"
        INSERT INTO "AddonInstallation"
            ("AddonId","Version","Enabled","PackageSha256","ContentSha256","ManifestJson","InstallPath","InstalledAt","UpdatedAt")
            VALUES ('org.example.claim','1.0.0',1,'package','content','{}','addons/claim','2026-01-01T00:00:00Z','2026-01-01T00:00:00Z');
        INSERT INTO "AddonMediaList"
            ("Id","AddonId","ProviderId","SourceUrl","Name","DraftRevision","PublishedRevision")
            VALUES (1,'org.example.claim','list','https://example.invalid/list','Synthetic',1,0);
    "#).execute(pool).await.unwrap();
    let repository = SqliteManagedLibraryOperationRepository(pool.clone());
    let queued = repository.enqueue_publication(1, 21, 1, 1).await.unwrap();
    let parent = repository
        .claim_operation(queued.operation.id, Utc::now())
        .await
        .unwrap()
        .unwrap();
    let operation = repository
        .ensure_publication_child_scan(&parent, 1)
        .await
        .unwrap();
    assert_eq!(operation.parent_operation_id, Some(parent.operation.id));
    assert_eq!(operation.owner_token, parent.operation.owner_token);
    ManagedLibraryOperationClaim {
        operation,
        lease_owner: parent.lease_owner,
    }
}

#[tokio::test]
async fn local_teletext_real_publication_child_uses_parent_lease_without_second_lease() {
    let pool = database().await;
    let child = publication_child(&pool).await;
    let leases: Vec<i64> = sqlx::query_scalar(r#"SELECT "OperationId" FROM "ManagedLibraryLease""#)
        .fetch_all(&pool)
        .await
        .unwrap();
    assert_eq!(leases, vec![child.operation.parent_operation_id.unwrap()]);
    let writer = context(child);
    let mut tx = begin_write(&pool).await.unwrap();
    writer.check_claim(&mut tx, 31).await.unwrap();
    ordinary_mutation(&mut tx).await;
    writer.check_claim(&mut tx, 31).await.unwrap();
    tx.commit().await.unwrap();
    assert_eq!(persisted_counts(&pool).await, (800, 0));
}

// Shell acquisition exercises the real bounded local owner on Unix. Portable
// claim/transaction tests above also run on Windows; native Windows FFprobe/job
// acceptance is separate and is not inferred from these shell fixtures.
#[cfg(unix)]
mod unix {
    use super::*;
    use ersatzrs_domain::SourceTeletextObservedPages;
    use ersatzrs_infra::ffprobe::LocalProbeCleanupScope;
    use std::os::unix::fs::PermissionsExt as _;

    const PRIMARY: &str = r#"{"format":{"format_name":"mpegts","duration":"60"},"streams":[{"index":1,"id":"0x101","codec_type":"audio","codec_name":"aac","tags":{"language":"nld"}},{"index":4,"id":"0x102","codec_type":"subtitle","codec_name":"dvb_teletext","tags":{"language":"eng,eng"}}]}"#;
    const AUXILIARY: &str = r#"{"format":{"format_name":"mpegts"},"streams":[{"index":4,"id":"0x102","codec_type":"subtitle","codec_name":"dvb_teletext","extradata":"CgAQiA==","extradata_size":4,"tags":{"language":"eng,eng"}}],"programs":[{"program_id":1,"program_num":1,"pmt_pid":4096,"pcr_pid":256,"streams":[{"index":4}]}]}"#;

    struct Source {
        _directory: tempfile::TempDir,
        path: PathBuf,
        extractor: FfprobeExtractor,
    }

    impl Source {
        fn new() -> Self {
            Self::with_auxiliary(AUXILIARY)
        }

        fn with_auxiliary(auxiliary: &str) -> Self {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("source.ts");
            std::fs::write(&path, b"first").unwrap();
            let program = directory.path().join("probe");
            let script = format!("#!/bin/sh\naux=false\nfor arg in \"$@\"; do\nif [ \"$arg\" = -select_streams ]; then aux=true; fi\ndone\nif $aux; then\nprintf '%s' '{auxiliary}'\nelse\nprintf '%s' '{PRIMARY}'\nfi\n");
            std::fs::write(&program, script).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o700)).unwrap();
            Self {
                path,
                extractor: FfprobeExtractor::new(program.to_string_lossy()),
                _directory: directory,
            }
        }

        fn replace(&self) {
            let metadata = self.path.metadata().unwrap();
            let replacement = self.path.with_extension("replacement");
            std::fs::write(&replacement, b"other").unwrap();
            std::fs::File::options()
                .write(true)
                .open(&replacement)
                .unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(metadata.modified().unwrap()))
                .unwrap();
            std::fs::rename(replacement, &self.path).unwrap();
            assert_eq!(self.path.metadata().unwrap().len(), metadata.len());
            assert_eq!(
                self.path.metadata().unwrap().modified().unwrap(),
                metadata.modified().unwrap()
            );
        }

        async fn acquire(&self, claim: Option<&ManagedLibraryOperationClaim>) -> LocalScanProbe {
            let scope = LocalProbeCleanupScope::new();
            let acquired = scope
                .scope(acquire(&self.extractor, &self.path, claim))
                .await
                .unwrap();
            assert!(acquired.teletext.acquisition.is_some());
            scope.close();
            scope.wait_cleanup().await.unwrap();
            acquired
        }

        async fn bind(&self, pool: &DbPool) {
            sqlx::query(r#"UPDATE "MediaFile" SET "Path"=? WHERE "Id"=501"#)
                .bind(self.path.display().to_string())
                .execute(pool)
                .await
                .unwrap();
        }
    }

    async fn report(pool: &DbPool, stream: i64, file: i64) -> SourceTeletextObservation {
        // read-only transaction: load the report and children on one snapshot.
        let mut tx = pool.begin().await.unwrap();
        let report = teletext_source::load(&mut tx, stream, file)
            .await
            .unwrap()
            .unwrap();
        tx.rollback().await.unwrap();
        report
    }

    #[tokio::test]
    async fn local_teletext_failed_auxiliary_preserves_managed_ordinary_commit() {
        let pool = database().await;
        let source = Source::with_auxiliary("malformed");
        source.bind(&pool).await;
        let claim = scan_claim(&pool).await;
        let scope = LocalProbeCleanupScope::new();
        let mut probe = scope
            .scope(acquire(&source.extractor, &source.path, Some(&claim)))
            .await
            .unwrap();
        assert!(probe.teletext.acquisition.is_none());
        assert_eq!(
            probe
                .media
                .streams
                .iter()
                .map(|stream| stream.index)
                .collect::<Vec<_>>(),
            vec![1, 4]
        );
        let mut tx = begin_write(&pool).await.unwrap();
        probe.teletext.check_claim(&mut tx, 31).await.unwrap();
        ordinary_mutation(&mut tx).await;
        probe
            .teletext
            .persist(&mut tx, 31, 301, &source.path)
            .await
            .unwrap();
        probe.teletext.check_claim(&mut tx, 31).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(persisted_counts(&pool).await, (800, 0));
        scope.close();
        scope.wait_cleanup().await.unwrap();
    }

    #[tokio::test]
    async fn local_teletext_unmanaged_acquisition_commits_ordinary_rows_without_report() {
        let pool = database().await;
        let source = Source::new();
        source.bind(&pool).await;
        let mut probe = source.acquire(None).await;
        let mut tx = begin_write(&pool).await.unwrap();
        probe.teletext.check_claim(&mut tx, 31).await.unwrap();
        ordinary_mutation(&mut tx).await;
        probe
            .teletext
            .persist(&mut tx, 31, 301, &source.path)
            .await
            .unwrap();
        probe.teletext.check_claim(&mut tx, 31).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(persisted_counts(&pool).await, (800, 0));
    }

    #[tokio::test]
    async fn local_teletext_replacement_before_report_preserves_ordinary_commit() {
        let pool = database().await;
        let source = Source::new();
        source.bind(&pool).await;
        let claim = scan_claim(&pool).await;
        let mut probe = source.acquire(Some(&claim)).await;
        let mut tx = begin_write(&pool).await.unwrap();
        probe.teletext.check_claim(&mut tx, 31).await.unwrap();
        ordinary_mutation(&mut tx).await;
        source.replace();
        probe
            .teletext
            .persist(&mut tx, 31, 301, &source.path)
            .await
            .unwrap();
        probe.teletext.check_claim(&mut tx, 31).await.unwrap();
        tx.commit().await.unwrap();
        assert_eq!(persisted_counts(&pool).await, (800, 0));
    }

    #[tokio::test]
    async fn local_teletext_replacement_after_report_prevents_commit_of_entire_leaf() {
        let pool = database().await;
        let source = Source::new();
        source.bind(&pool).await;
        let claim = scan_claim(&pool).await;
        let mut probe = source.acquire(Some(&claim)).await;
        let mut tx = begin_write(&pool).await.unwrap();
        probe.teletext.check_claim(&mut tx, 31).await.unwrap();
        ordinary_mutation(&mut tx).await;
        probe
            .teletext
            .persist(&mut tx, 31, 301, &source.path)
            .await
            .unwrap();
        let staged: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextMetadata""#)
            .fetch_one(&mut *tx)
            .await
            .unwrap();
        assert_eq!(staged, 1);
        source.replace();
        assert!(probe.teletext.check_claim(&mut tx, 31).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(persisted_counts(&pool).await, (720, 0));
        let parents: Vec<(i64, i64)> =
            sqlx::query_as(r#"SELECT "Id","Index" FROM "MediaStream" ORDER BY "Id""#)
                .fetch_all(&pool)
                .await
                .unwrap();
        assert_eq!(parents, vec![(401, 4), (402, 1)]);
    }

    async fn replace_parents(pool: &DbPool) {
        sqlx::raw_sql(r#"
            DELETE FROM "MediaStream" WHERE "MediaVersionId"=301;
            INSERT INTO "MediaStream"
                ("Id","MediaVersionId","Index","Codec","MediaStreamKind","Language","Channels","Default","Forced")
                VALUES (411,301,4,'dvb_teletext',3,'eng,eng',0,0,0),(412,301,1,'aac',2,'nld',2,1,0);
            UPDATE "MediaFile" SET "Id"=511 WHERE "Id"=501;
        "#).execute(pool).await.unwrap();
    }

    #[tokio::test]
    async fn local_teletext_rescan_binds_fresh_stream_and_file_parents_without_stale_reports() {
        let pool = database().await;
        let source = Source::new();
        source.bind(&pool).await;
        let claim = scan_claim(&pool).await;
        let mut first = source.acquire(Some(&claim)).await;
        let mut tx = begin_write(&pool).await.unwrap();
        first
            .teletext
            .persist(&mut tx, 31, 301, &source.path)
            .await
            .unwrap();
        first.teletext.check_claim(&mut tx, 31).await.unwrap();
        tx.commit().await.unwrap();
        let old = report(&pool, 401, 501).await;
        source.replace();
        replace_parents(&pool).await;
        assert_eq!(persisted_counts(&pool).await, (720, 0));
        let mut next = source.acquire(Some(&claim)).await;
        let mut tx = begin_write(&pool).await.unwrap();
        next.teletext
            .persist(&mut tx, 31, 301, &source.path)
            .await
            .unwrap();
        next.teletext.check_claim(&mut tx, 31).await.unwrap();
        tx.commit().await.unwrap();
        let fresh = report(&pool, 411, 511).await;
        let SourceTeletextBinding::Bound(identity) = &fresh.binding else {
            panic!("bound report")
        };
        let SourceTeletextBinding::Bound(old_identity) = old.binding else {
            panic!("bound old report")
        };
        assert_eq!(
            (identity.media_stream_id, identity.media_file_id),
            (411, 511)
        );
        assert_ne!(identity.source_revision, old_identity.source_revision);
        assert_eq!(
            fresh.observed_pages,
            SourceTeletextObservedPages::NotAcquired
        );
        assert_eq!(fresh.descriptors, old.descriptors);
        assert_eq!(fresh.programs, old.programs);
        assert_eq!(persisted_counts(&pool).await, (720, 1));
    }
}
