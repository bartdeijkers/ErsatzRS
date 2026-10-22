use super::*;
use ersatzrs_domain::{
    SourceTeletextHeaderProgram, SourceTeletextHeaderWindow, SourceTeletextObservedHeader,
};

pub(super) fn header_report() -> SourceTeletextObservation {
    let mut report = observation(MediaSourceKind::Local);
    report.observed_pages =
        SourceTeletextObservedPages::HeadersObserved(Box::new(SourceTeletextHeaderWindow {
            transport_stream_id: 1,
            pat_version: 2,
            psi_fingerprint: [3; 32],
            packet_count: 40,
            complete_pes: 2,
            unobserved_prefix: true,
            ended_at_eof: false,
            programs: vec![SourceTeletextHeaderProgram {
                program_number: 1,
                pmt_pid: 4096,
                pcr_pid: 256,
                pmt_version: 4,
                stream_type: 6,
                descriptors: vec![0x56, 5, b'e', b'n', b'g', 0x10, 0x88],
            }],
            headers: vec![SourceTeletextObservedHeader {
                wire_magazine: 0,
                wire_page: 0x88,
                subcode: 0,
                controls: 0x100,
                subtitle_data_unit: true,
                field_parity: false,
                line_offset: 7,
                corrected_address_bytes: 0,
                corrected_header_bytes: 1,
                completion_packet: 39,
                pes_ordinal: 1,
                unit_ordinal: 0,
                pts: Some(90_000),
            }],
        }));
    report
}

#[tokio::test]
async fn source_teletext_header_window_roundtrip_keeps_legacy_sentinel_and_ordinary_tracks() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let before: Vec<(i64, String, i64, i64)> = sqlx::query_as(
        r#"SELECT "Id", "Codec", "Default", "Forced" FROM "MediaStream" ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let report = header_report();
    store(&pool, &claim, &report).await;
    assert_eq!(read(&pool).await, report);
    let sentinel: i64 =
        sqlx::query_scalar(r#"SELECT "ObservedPagesStatus" FROM "SourceTeletextMetadata""#)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(sentinel, 0);
    let shape: (i64, i64) = sqlx::query_as(r#"SELECT "SchemaVersion", LENGTH(CAST("Payload" AS BLOB)) FROM "SourceTeletextHeaderWindow""#)
        .fetch_one(&pool).await.unwrap();
    assert_eq!(shape.0, 1);
    assert!((1..=1_048_576).contains(&shape.1));
    let after = sqlx::query_as::<_, (i64, String, i64, i64)>(
        r#"SELECT "Id", "Codec", "Default", "Forced" FROM "MediaStream" ORDER BY "Id""#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    assert_eq!(before, after);
    store(&pool, &claim, &observation(MediaSourceKind::Local)).await;
    assert_eq!(
        read(&pool).await.observed_pages,
        SourceTeletextObservedPages::NotAcquired
    );
    let children: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextHeaderWindow""#)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(children, 0);
}

#[tokio::test]
async fn source_teletext_header_window_source_parent_deletion_cascades() {
    for delete in [
        r#"DELETE FROM "MediaFile" WHERE "Id"=501"#,
        r#"DELETE FROM "MediaStream" WHERE "Id"=401"#,
        r#"DELETE FROM "MediaVersion" WHERE "Id"=301"#,
    ] {
        let (pool, claim) = fixture(MediaSourceKind::Local).await;
        store(&pool, &claim, &header_report()).await;
        sqlx::query(delete).execute(&pool).await.unwrap();
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextHeaderWindow""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn source_teletext_header_window_corruption_fails_bounded_without_echoing_input() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    store(&pool, &claim, &header_report()).await;
    let mut tx = begin_write(&pool).await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&mut *tx)
        .await
        .unwrap();
    for sql in [
        r#"UPDATE "SourceTeletextHeaderWindow" SET "Payload"='private-input-must-not-appear'"#,
        r#"UPDATE "SourceTeletextHeaderWindow" SET "Payload"=printf('%.*c',1048577,'x')"#,
        r#"UPDATE "SourceTeletextHeaderWindow" SET "Payload"=X'7B7D'"#,
        r#"UPDATE "SourceTeletextHeaderWindow" SET "Payload"='{}', "SchemaVersion"=2"#,
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
    assert_eq!(read(&pool).await, header_report());
}

#[tokio::test]
async fn source_teletext_header_window_rejects_unbound_and_stale_claim_without_mutation() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = header_report();
    store(&pool, &claim, &report).await;
    let mut unbound = report.clone();
    unbound.binding = SourceTeletextBinding::Unavailable;
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(replace(&mut tx, &claim, parents(), &unbound).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(read(&pool).await, report);
    sqlx::query(r#"UPDATE "ManagedLibraryLease" SET "LeaseExpiresAt"='2000-01-01 00:00:00'"#)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(!replace(&mut tx, &claim, parents(), &report).await.unwrap());
    tx.commit().await.unwrap();
    assert_eq!(read(&pool).await, report);
}

#[tokio::test]
async fn source_teletext_header_window_committed_file_reopens_with_genuine_claim_binding() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("header-window.db");
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(&path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    // Migrate and seed this actual file, then acquire its real repository lease.
    let (pool, claim) = fixture_on_pool(pool, MediaSourceKind::Local).await;
    let report = header_report();
    store(&pool, &claim, &report).await;
    pool.close().await;
    let reopened = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    assert_eq!(read(&reopened).await, report);
    reopened.close().await;
}
