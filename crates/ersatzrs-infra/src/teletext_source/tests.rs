use chrono::{Duration, Utc};
use ersatzrs_core::repositories::{ManagedLibraryOperationRepository, ManagedLibraryScanSpec};
use ersatzrs_domain::{
    ManagedLibraryOperationKind, MediaSourceKind, SourceTeletextBinding,
    SourceTeletextBindingIdentity, SourceTeletextDescriptor, SourceTeletextDescriptors,
    SourceTeletextObservedPages, SourceTeletextPid, SourceTeletextProgram, SourceTeletextPrograms,
    SourceTeletextProvenance, SourceTeletextRevision, TeletextLanguage,
};
use sqlx::{sqlite::SqlitePoolOptions, Executor as _};

use super::*;
use crate::db::{begin_write, DbPool};
use crate::repositories::SqliteManagedLibraryOperationRepository;

fn parents() -> SourceTeletextParents {
    SourceTeletextParents {
        media_version_id: 301,
        media_stream_id: 401,
        media_file_id: 501,
    }
}

async fn fixture(provider: MediaSourceKind) -> (DbPool, ManagedLibraryOperationClaim) {
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    fixture_on_pool(pool, provider).await
}

async fn fixture_on_pool(
    pool: DbPool,
    provider: MediaSourceKind,
) -> (DbPool, ManagedLibraryOperationClaim) {
    pool.execute("PRAGMA foreign_keys=ON").await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    sqlx::raw_sql(r#"
        INSERT INTO "MediaSource" ("Id") VALUES (11);
        INSERT INTO "Library" ("Id", "Name", "MediaKind", "MediaSourceId") VALUES (21, 'Synthetic', 1, 11);
        INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (31, 21, 'fixture');
        INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (101, 31);
        INSERT INTO "Movie" ("Id") VALUES (101);
        INSERT INTO "MediaVersion" ("Id", "MovieId", "DateAdded", "DateUpdated", "Duration", "Height", "Width", "VideoScanKind")
            VALUES (301, 101, '2026-01-01 00:00:00', '2026-01-01 00:00:00', '00:01:00', 576, 720, 0),
                   (302, 101, '2026-01-01 00:00:00', '2026-01-01 00:00:00', '00:01:00', 576, 720, 0);
        INSERT INTO "MediaFile" ("Id", "MediaVersionId", "Path", "PathHash") VALUES
            (501,301,'fixture/one.ts','one'), (502,301,'fixture/two.ts','two'), (503,302,'fixture/other.ts','other');
        INSERT INTO "MediaStream" ("Id", "MediaVersionId", "Index", "Codec", "MediaStreamKind", "Language", "Channels", "Default", "Forced") VALUES
            (401,301,4,'dvb_teletext',3,'nld',0,0,0), (402,301,1,'aac',2,'eng',2,1,0),
            (403,301,2,'subrip',3,'nld',0,0,1), (404,302,4,'dvb_teletext',3,'eng',0,0,0);
    "#).execute(&pool).await.unwrap();
    let subtype_sql = match provider {
        MediaSourceKind::Local => {
            r#"INSERT INTO "LocalLibrary" ("Id") VALUES (21); INSERT INTO "LocalMediaSource" ("Id") VALUES (11);"#
        }
        MediaSourceKind::Plex => {
            r#"INSERT INTO "PlexLibrary" ("Id", "ShouldSyncItems") VALUES (21,1); INSERT INTO "PlexMediaSource" ("Id") VALUES (11);"#
        }
        MediaSourceKind::Jellyfin => {
            r#"INSERT INTO "JellyfinLibrary" ("Id", "ShouldSyncItems") VALUES (21,1); INSERT INTO "JellyfinMediaSource" ("Id") VALUES (11);"#
        }
        MediaSourceKind::Emby => {
            r#"INSERT INTO "EmbyLibrary" ("Id", "ShouldSyncItems") VALUES (21,1); INSERT INTO "EmbyMediaSource" ("Id") VALUES (11);"#
        }
    };
    sqlx::raw_sql(subtype_sql).execute(&pool).await.unwrap();
    let repository = SqliteManagedLibraryOperationRepository(pool.clone());
    let operation = repository
        .enqueue_scan(
            ManagedLibraryOperationKind::ManualScan,
            21,
            ManagedLibraryScanSpec {
                provider,
                show_id: None,
                deep_scan: true,
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
    (pool, claim)
}

fn observation(provider: MediaSourceKind) -> SourceTeletextObservation {
    let language = TeletextLanguage::new("nld".to_owned()).unwrap();
    let entries = [
        (0, 1, 2, 0),
        (1, 2, 0, 0x88),
        (2, 31, 2, 0xa0),
        (3, 1, 2, 0),
    ]
    .into_iter()
    .map(|(ordinal, kind, magazine, page)| {
        SourceTeletextDescriptor::new(ordinal, language.clone(), kind, magazine, page).unwrap()
    })
    .collect();
    let program = SourceTeletextProgram {
        program_id: 1,
        program_number: 1,
        pmt_pid: 4096,
        pcr_pid: 256,
    };
    SourceTeletextObservation::new(
        SourceTeletextBinding::Bound(
            SourceTeletextBindingIdentity::new(
                11,
                provider,
                301,
                501,
                401,
                SourceTeletextRevision::new("captured-fixture-revision".to_owned()).unwrap(),
            )
            .unwrap(),
        ),
        4,
        SourceTeletextProvenance::Ffprobe9PackedDescriptorProjection,
        SourceTeletextPid::Reported(258),
        SourceTeletextPrograms::Reported(vec![program, program]),
        SourceTeletextDescriptors::Reported {
            entries,
            trailing_unpaired_pairs: Some(2),
        },
    )
    .unwrap()
}

async fn store(
    pool: &DbPool,
    claim: &ManagedLibraryOperationClaim,
    report: &SourceTeletextObservation,
) {
    let mut tx = begin_write(pool).await.unwrap();
    assert!(replace(&mut tx, claim, parents(), report).await.unwrap());
    tx.commit().await.unwrap();
}

async fn read(pool: &DbPool) -> SourceTeletextObservation {
    // read-only transaction: load the report and its children on one snapshot.
    let mut tx = pool.begin().await.unwrap();
    let report = load(&mut tx, 401, 501).await.unwrap().unwrap();
    tx.rollback().await.unwrap();
    report
}

#[tokio::test]
async fn source_teletext_all_provider_bindings_roundtrip_without_changing_ordinary_tracks() {
    for provider in MediaSourceKind::ALL {
        let (pool, claim) = fixture(provider).await;
        let before: Vec<(i64, i64, String, i64, i64)> = sqlx::query_as(
            r#"SELECT "Id","Index","Codec","Default","Forced" FROM "MediaStream" ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let report = observation(provider);
        store(&pool, &claim, &report).await;
        assert_eq!(read(&pool).await, report);
        let after = sqlx::query_as::<_, (i64, i64, String, i64, i64)>(
            r#"SELECT "Id","Index","Codec","Default","Forced" FROM "MediaStream" ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        assert_eq!(after, before);
        assert_eq!(
            report.observed_pages,
            SourceTeletextObservedPages::NotAcquired
        );
        let SourceTeletextDescriptors::Reported { entries, .. } = &report.descriptors else {
            panic!("reported descriptors")
        };
        assert_eq!(entries[1].display_page().unwrap().get(), 888);
        assert!(entries[2].display_page().is_none());
        assert_eq!(entries[0].wire_page, entries[3].wire_page);
    }
}

#[tokio::test]
async fn source_teletext_unbound_and_unusable_states_replace_children_independently() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let mut report = observation(MediaSourceKind::Local);
    report.binding = SourceTeletextBinding::Unavailable;
    store(&pool, &claim, &report).await;
    assert_eq!(read(&pool).await, report);
    report.binding = SourceTeletextBinding::Unusable;
    report.pid = SourceTeletextPid::Unusable;
    report.programs = SourceTeletextPrograms::Unavailable;
    report.descriptors = SourceTeletextDescriptors::Unusable;
    store(&pool, &claim, &report).await;
    assert_eq!(read(&pool).await, report);
    let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextDescriptor""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    report.provenance = SourceTeletextProvenance::Unavailable;
    store(&pool, &claim, &report).await;
    assert_eq!(read(&pool).await, report);
}

#[tokio::test]
async fn source_teletext_exact_file_selection_never_uses_first_part() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let mut report = observation(MediaSourceKind::Local);
    let SourceTeletextBinding::Bound(binding) = &mut report.binding else {
        panic!("bound fixture")
    };
    binding.media_file_id = 502;
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(replace(
        &mut tx,
        &claim,
        SourceTeletextParents {
            media_file_id: 502,
            ..parents()
        },
        &report
    )
    .await
    .unwrap());
    assert!(load(&mut tx, 401, 501).await.unwrap().is_none());
    assert_eq!(load(&mut tx, 401, 502).await.unwrap(), Some(report));
    tx.commit().await.unwrap();
}

#[tokio::test]
async fn source_teletext_rejects_parent_index_codec_kind_and_bound_identity_mismatches() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let valid = observation(MediaSourceKind::Local);
    store(&pool, &claim, &valid).await;
    for bad_parent in [
        SourceTeletextParents {
            media_version_id: 302,
            ..parents()
        },
        SourceTeletextParents {
            media_file_id: 503,
            ..parents()
        },
        SourceTeletextParents {
            media_stream_id: 404,
            ..parents()
        },
        SourceTeletextParents {
            media_stream_id: 402,
            ..parents()
        },
    ] {
        let mut tx = begin_write(&pool).await.unwrap();
        assert!(replace(&mut tx, &claim, bad_parent, &valid).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(read(&pool).await, valid);
    }
    for change in 0..5 {
        let mut report = valid.clone();
        match change {
            0 => report.stream_index = 5,
            1 => {
                let SourceTeletextBinding::Bound(binding) = &mut report.binding else {
                    panic!("bound")
                };
                binding.provider = MediaSourceKind::Emby;
            }
            2 => {
                let SourceTeletextBinding::Bound(binding) = &mut report.binding else {
                    panic!("bound")
                };
                binding.media_source_id = 12;
            }
            3 => {
                let SourceTeletextBinding::Bound(binding) = &mut report.binding else {
                    panic!("bound")
                };
                binding.media_stream_id = 404;
            }
            _ => report.provenance = SourceTeletextProvenance::Unavailable,
        }
        let mut tx = begin_write(&pool).await.unwrap();
        assert!(replace(&mut tx, &claim, parents(), &report).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(read(&pool).await, valid);
    }
    for sql in [
        r#"UPDATE "MediaStream" SET "MediaStreamKind"=2 WHERE "Id"=401"#,
        r#"UPDATE "MediaStream" SET "Codec"='subrip' WHERE "Id"=401"#,
    ] {
        let mut tx = begin_write(&pool).await.unwrap();
        sqlx::query(sql).execute(&mut *tx).await.unwrap();
        assert!(replace(&mut tx, &claim, parents(), &valid).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(read(&pool).await, valid);
    }
}

#[tokio::test]
async fn source_teletext_rejects_missing_ambiguous_and_disagreeing_parent_sources() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = observation(MediaSourceKind::Local);
    for sql in [
        r#"DELETE FROM "LocalLibrary" WHERE "Id"=21"#,
        r#"DELETE FROM "LocalMediaSource" WHERE "Id"=11"#,
        r#"INSERT INTO "EmbyLibrary" ("Id","ShouldSyncItems") VALUES (21,1)"#,
        r#"INSERT INTO "EmbyMediaSource" ("Id") VALUES (11)"#,
        r#"DELETE FROM "LocalMediaSource" WHERE "Id"=11; INSERT INTO "EmbyMediaSource" ("Id") VALUES (11)"#,
        r#"UPDATE "MediaVersion" SET "MovieId"=NULL WHERE "Id"=301"#,
        r#"INSERT INTO "Song" ("Id") VALUES (101); UPDATE "MediaVersion" SET "SongId"=101 WHERE "Id"=301"#,
    ] {
        let mut tx = begin_write(&pool).await.unwrap();
        sqlx::raw_sql(sql).execute(&mut *tx).await.unwrap();
        assert!(
            replace(&mut tx, &claim, parents(), &report).await.is_err(),
            "{sql}"
        );
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn source_teletext_stale_and_expired_claims_leave_prior_records_unchanged() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = observation(MediaSourceKind::Local);
    store(&pool, &claim, &report).await;
    let mut replacement = report.clone();
    replacement.descriptors = SourceTeletextDescriptors::Unavailable;
    let mut stale = claim.clone();
    stale.lease_owner = "superseded-owner".to_owned();
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(!replace(&mut tx, &stale, parents(), &replacement)
        .await
        .unwrap());
    tx.commit().await.unwrap();
    assert_eq!(read(&pool).await, report);
    let expired = (Utc::now() - Duration::seconds(1))
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    sqlx::query(r#"UPDATE "ManagedLibraryLease" SET "LeaseExpiresAt"=?"#)
        .bind(expired)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(!replace(&mut tx, &claim, parents(), &replacement)
        .await
        .unwrap());
    tx.commit().await.unwrap();
    assert_eq!(read(&pool).await, report);
}

#[tokio::test]
async fn source_teletext_unbound_reports_require_actual_claim_library() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let mut report = observation(MediaSourceKind::Local);
    report.binding = SourceTeletextBinding::Unavailable;
    let mut bad = claim.clone();
    bad.operation.library_id = 99;
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(replace(&mut tx, &bad, parents(), &report).await.is_err());
    tx.rollback().await.unwrap();
    sqlx::query(r#"INSERT INTO "Library" ("Id","Name","MediaKind","MediaSourceId") VALUES (22,'Other',1,11)"#).execute(&pool).await.unwrap();
    sqlx::query(r#"UPDATE "ManagedLibraryLease" SET "LibraryId"=22"#)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(replace(&mut tx, &claim, parents(), &report).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn source_teletext_failed_child_write_rolls_back_with_ordinary_transaction() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = observation(MediaSourceKind::Local);
    store(&pool, &claim, &report).await;
    sqlx::query(r#"CREATE TRIGGER "RejectSyntheticDescriptor" BEFORE INSERT ON "SourceTeletextDescriptor" WHEN NEW."ProjectionOrdinal"=1 BEGIN SELECT RAISE(ABORT,'synthetic rejection'); END"#).execute(&pool).await.unwrap();
    let mut changed = report.clone();
    changed.pid = SourceTeletextPid::Reported(259);
    let mut tx = begin_write(&pool).await.unwrap();
    sqlx::query(r#"UPDATE "MediaStream" SET "Title"='Pending title' WHERE "Id"=402"#)
        .execute(&mut *tx)
        .await
        .unwrap();
    let error = replace(&mut tx, &claim, parents(), &changed)
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "source teletext metadata is unavailable: storage unavailable"
    );
    tx.rollback().await.unwrap();
    assert_eq!(read(&pool).await, report);
    let title: Option<String> =
        sqlx::query_scalar(r#"SELECT "Title" FROM "MediaStream" WHERE "Id"=402"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(title.is_none());
}

#[tokio::test]
async fn source_teletext_unknown_and_changed_parent_reads_fail_conservatively() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    // read-only transaction: load the report and its children on one snapshot.
    let mut tx = pool.begin().await.unwrap();
    assert!(load(&mut tx, 401, 501).await.unwrap().is_none());
    tx.rollback().await.unwrap();
    store(&pool, &claim, &observation(MediaSourceKind::Local)).await;
    for sql in [
        r#"UPDATE "SourceTeletextMetadata" SET "DescriptorStatus"=0,"TrailingUnpairedPairs"=NULL"#,
        r#"UPDATE "SourceTeletextMetadata" SET "ProgramStatus"=1"#,
        r#"UPDATE "SourceTeletextMetadata" SET "Provenance"=0"#,
        r#"UPDATE "SourceTeletextMetadata" SET "StreamIndex"=5"#,
        r#"UPDATE "SourceTeletextMetadata" SET "Provider"='Emby'"#,
    ] {
        let mut tx = begin_write(&pool).await.unwrap();
        sqlx::query(sql).execute(&mut *tx).await.unwrap();
        assert!(load(&mut tx, 401, 501).await.is_err(), "{sql}");
        tx.rollback().await.unwrap();
    }
}

#[tokio::test]
async fn source_teletext_resource_bounds_apply_before_insert_and_on_read() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = observation(MediaSourceKind::Local);
    for programs in [false, true] {
        let mut too_many = report.clone();
        if programs {
            let SourceTeletextPrograms::Reported(entries) = &mut too_many.programs else {
                panic!("programs")
            };
            entries.resize(MAX_SOURCE_TELETEXT_PROGRAMS + 1, entries[0]);
        } else {
            let SourceTeletextDescriptors::Reported { entries, .. } = &mut too_many.descriptors
            else {
                panic!("descriptors")
            };
            entries.resize(MAX_SOURCE_TELETEXT_DESCRIPTORS + 1, entries[0].clone());
        }
        let mut tx = begin_write(&pool).await.unwrap();
        let error = replace(&mut tx, &claim, parents(), &too_many)
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "invalid source teletext metadata: collection resource bound"
        );
        assert!(load(&mut tx, 401, 501).await.unwrap().is_none());
        tx.rollback().await.unwrap();
    }
    store(&pool, &claim, &report).await;
    let mut tx = begin_write(&pool).await.unwrap();
    sqlx::raw_sql(r#"WITH RECURSIVE ord(n) AS (VALUES(10) UNION ALL SELECT n+1 FROM ord WHERE n<1034)
        INSERT INTO "SourceTeletextDescriptor" SELECT (SELECT "Id" FROM "SourceTeletextMetadata"),n,'nld',1,2,0 FROM ord;"#).execute(&mut *tx).await.unwrap();
    assert!(load(&mut tx, 401, 501).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn source_teletext_source_deletion_cascades_and_leaves_other_tracks() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    store(&pool, &claim, &observation(MediaSourceKind::Local)).await;
    sqlx::query(r#"DELETE FROM "MediaFile" WHERE "Id"=501"#)
        .execute(&pool)
        .await
        .unwrap();
    // read-only transaction: load the report and its children on one snapshot.
    let mut tx = pool.begin().await.unwrap();
    assert!(load(&mut tx, 401, 501).await.unwrap().is_none());
    tx.rollback().await.unwrap();
    let count: i64 =
        sqlx::query_scalar(r#"SELECT COUNT(*) FROM "MediaStream" WHERE "MediaVersionId"=301"#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 3);
    let child_count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextDescriptor""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(child_count, 0);
}

#[tokio::test]
async fn source_teletext_expiry_during_child_write_requires_rollback() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = observation(MediaSourceKind::Local);
    store(&pool, &claim, &report).await;
    sqlx::query(
        r#"CREATE TRIGGER "ExpireSyntheticLease" AFTER INSERT ON "SourceTeletextDescriptor"
        WHEN NEW."ProjectionOrdinal"=1 BEGIN
        UPDATE "ManagedLibraryLease" SET "LeaseExpiresAt"='2000-01-01 00:00:00'; END"#,
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut changed = report.clone();
    changed.pid = SourceTeletextPid::Reported(259);
    let mut tx = begin_write(&pool).await.unwrap();
    let error = replace(&mut tx, &claim, parents(), &changed)
        .await
        .unwrap_err();
    assert_eq!(
        error.to_string(),
        "invalid source teletext metadata: lease expired during metadata replacement"
    );
    tx.rollback().await.unwrap();
    assert_eq!(read(&pool).await, report);
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(claim_is_current(&mut tx, &claim).await.unwrap());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn source_teletext_exact_resource_limits_roundtrip_and_excess_program_read_fails() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let mut report = observation(MediaSourceKind::Local);
    let SourceTeletextDescriptors::Reported { entries, .. } = &mut report.descriptors else {
        panic!("descriptors")
    };
    entries.clear();
    for ordinal in 0..MAX_SOURCE_TELETEXT_DESCRIPTORS {
        entries.push(
            SourceTeletextDescriptor::new(
                u32::try_from(ordinal).unwrap(),
                TeletextLanguage::new("nld".to_owned()).unwrap(),
                1,
                2,
                0,
            )
            .unwrap(),
        );
    }
    let SourceTeletextPrograms::Reported(entries) = &mut report.programs else {
        panic!("programs")
    };
    entries.resize(MAX_SOURCE_TELETEXT_PROGRAMS, entries[0]);
    store(&pool, &claim, &report).await;
    assert_eq!(read(&pool).await, report);
    let mut tx = begin_write(&pool).await.unwrap();
    sqlx::query(r#"INSERT INTO "SourceTeletextProgram" ("SourceTeletextMetadataId","MembershipOrdinal","ProgramId","ProgramNumber","PmtPid","PcrPid")
        SELECT "Id",128,1,1,4096,256 FROM "SourceTeletextMetadata""#).execute(&mut *tx).await.unwrap();
    assert!(load(&mut tx, 401, 501).await.is_err());
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn source_teletext_corrupt_text_and_scalar_values_fail_without_echoing_revision() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    store(&pool, &claim, &observation(MediaSourceKind::Local)).await;
    let mut tx = begin_write(&pool).await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&mut *tx)
        .await
        .unwrap();
    for sql in [
        r#"UPDATE "SourceTeletextMetadata" SET "SourceRevision"=printf('%.*c',2048,'x')"#,
        r#"UPDATE "SourceTeletextMetadata" SET "SourceRevision"='private-input-must-not-appear',"StreamIndex"=4294967300"#,
        r#"UPDATE "SourceTeletextMetadata" SET "StreamIndex"=4,"Provenance"=99"#,
    ] {
        sqlx::query(sql).execute(&mut *tx).await.unwrap();
        let error = load(&mut tx, 401, 501).await.unwrap_err();
        assert!(!error.to_string().contains("private-input-must-not-appear"));
    }
    tx.rollback().await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=OFF")
        .execute(&pool)
        .await
        .unwrap();
}

#[path = "header_tests.rs"]
mod headers;

#[path = "page_tests.rs"]
mod pages;
