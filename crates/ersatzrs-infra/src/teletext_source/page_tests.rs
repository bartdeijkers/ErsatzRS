use super::*;
use ersatzrs_domain::{SourceTeletextObservedHeader, SourceTeletextObservedPages};
use ersatzrs_ffmpeg::teletext::{
    source_header::TeletextDataUnitKind, source_observation::SuppliedTeletextHeaderObserver,
};

fn page_report(provider: MediaSourceKind) -> SourceTeletextObservation {
    let bytes =
        include_str!("../../../ersatzrs-ffmpeg/tests/fixtures/teletext-source-assembly.hex")
            .split_whitespace()
            .map(|v| u8::from_str_radix(v, 16).unwrap())
            .collect::<Vec<_>>();
    let mut observer = SuppliedTeletextHeaderObserver::new_with_pages(0x100).unwrap();
    for packet in bytes.chunks_exact(188) {
        observer.push(packet.try_into().unwrap()).unwrap();
    }
    let capture = observer.finish().unwrap();
    let mut report = observation(provider);
    let SourceTeletextObservedPages::HeadersObserved(mut headers) =
        headers::header_report().observed_pages
    else {
        unreachable!()
    };
    headers.packet_count = u32::try_from(capture.packets()).unwrap();
    headers.complete_pes = u32::try_from(capture.complete_pes()).unwrap();
    headers.unobserved_prefix = capture.has_unobserved_prefix();
    headers.ended_at_eof = true;
    headers.headers = capture
        .headers()
        .iter()
        .map(|record| {
            let header = record.header;
            let address = header.packet_address;
            SourceTeletextObservedHeader {
                wire_magazine: address.wire_magazine,
                wire_page: header.wire_page,
                subcode: header.subcode,
                controls: header.controls.raw(),
                subtitle_data_unit: address.data_unit_kind == TeletextDataUnitKind::Subtitle,
                field_parity: address.field_parity,
                line_offset: address.line_offset,
                corrected_address_bytes: address.corrected_address_bytes,
                corrected_header_bytes: header.corrected_header_bytes,
                completion_packet: u32::try_from(record.completion_packet).unwrap(),
                pes_ordinal: u32::try_from(record.pes_ordinal).unwrap(),
                unit_ordinal: u32::try_from(record.unit_ordinal).unwrap(),
                pts: record
                    .pts
                    .map(ersatzrs_ffmpeg::teletext::TeletextTimestamp::ticks),
            }
        })
        .collect();
    report.observed_pages = SourceTeletextObservedPages::PagesObserved {
        headers,
        pages: Box::new(capture.page_observations().unwrap().clone()),
    };
    report.validate().unwrap();
    report
}

#[tokio::test]
async fn source_teletext_page_capture_roundtrips_for_all_owners_without_promoting_the_sentinel() {
    for provider in [
        MediaSourceKind::Local,
        MediaSourceKind::Jellyfin,
        MediaSourceKind::Emby,
        MediaSourceKind::Plex,
    ] {
        let (pool, claim) = fixture(provider).await;
        let before: Vec<(i64, String, i64, i64)> = sqlx::query_as(
            r#"SELECT "Id", "Codec", "Default", "Forced" FROM "MediaStream" ORDER BY "Id""#,
        )
        .fetch_all(&pool)
        .await
        .unwrap();
        let report = page_report(provider);
        store(&pool, &claim, &report).await;
        assert_eq!(read(&pool).await, report);
        assert!(!report
            .observed_pages
            .page_window()
            .unwrap()
            .units
            .is_empty());
        let shape: (i64, i64, i64) = sqlx::query_as(r#"SELECT p."ObservedPagesStatus", c."SchemaVersion", LENGTH(CAST(c."Payload" AS BLOB)) FROM "SourceTeletextMetadata" p JOIN "SourceTeletextPageWindow" c ON c."SourceTeletextMetadataId"=p."Id""#)
            .fetch_one(&pool).await.unwrap();
        assert_eq!((shape.0, shape.1), (0, 1));
        assert!((1..=2_097_152).contains(&shape.2));
        assert_eq!(
            before,
            sqlx::query_as::<_, (i64, String, i64, i64)>(
                r#"SELECT "Id", "Codec", "Default", "Forced" FROM "MediaStream" ORDER BY "Id""#,
            )
            .fetch_all(&pool)
            .await
            .unwrap()
        );
        let mut header_only = report.clone();
        header_only.observed_pages = SourceTeletextObservedPages::HeadersObserved(Box::new(
            report.observed_pages.headers().unwrap().clone(),
        ));
        store(&pool, &claim, &header_only).await;
        assert_eq!(read(&pool).await, header_only);
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextPageWindow""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn source_teletext_page_capture_cascades_with_headers_and_exact_source_parents() {
    for delete in [
        r#"DELETE FROM "MediaFile" WHERE "Id"=501"#,
        r#"DELETE FROM "MediaStream" WHERE "Id"=401"#,
        r#"DELETE FROM "MediaVersion" WHERE "Id"=301"#,
        r#"DELETE FROM "SourceTeletextHeaderWindow""#,
    ] {
        let (pool, claim) = fixture(MediaSourceKind::Local).await;
        store(&pool, &claim, &page_report(MediaSourceKind::Local)).await;
        sqlx::query(delete).execute(&pool).await.unwrap();
        let count: i64 = sqlx::query_scalar(r#"SELECT COUNT(*) FROM "SourceTeletextPageWindow""#)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0);
    }
}

#[tokio::test]
async fn source_teletext_page_capture_header_only_replacement_keeps_the_other_file_capture() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = page_report(MediaSourceKind::Local);
    store(&pool, &claim, &report).await;
    let mut other = report.clone();
    let SourceTeletextBinding::Bound(binding) = &mut other.binding else {
        unreachable!()
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
        &other
    )
    .await
    .unwrap());
    tx.commit().await.unwrap();
    let mut header_only = report;
    header_only.observed_pages = SourceTeletextObservedPages::HeadersObserved(Box::new(
        header_only.observed_pages.headers().unwrap().clone(),
    ));
    store(&pool, &claim, &header_only).await;
    let mut tx = pool.begin().await.unwrap();
    assert_eq!(load(&mut tx, 401, 501).await.unwrap(), Some(header_only));
    assert_eq!(load(&mut tx, 401, 502).await.unwrap(), Some(other));
    tx.rollback().await.unwrap();
}

#[tokio::test]
async fn source_teletext_page_capture_corruption_never_falls_back_to_header_only_or_echoes_body() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = page_report(MediaSourceKind::Local);
    store(&pool, &claim, &report).await;
    let mut tx = begin_write(&pool).await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=ON")
        .execute(&mut *tx)
        .await
        .unwrap();
    for sql in [
        r#"UPDATE "SourceTeletextPageWindow" SET "Payload"='private-page-body-must-not-appear'"#,
        r#"UPDATE "SourceTeletextPageWindow" SET "Payload"=printf('%.*c',2097153,'x')"#,
        r#"UPDATE "SourceTeletextPageWindow" SET "Payload"=X'7B7D'"#,
        r#"UPDATE "SourceTeletextPageWindow" SET "Payload"='{}', "SchemaVersion"=2"#,
    ] {
        sqlx::query(sql).execute(&mut *tx).await.unwrap();
        let error = load(&mut tx, 401, 501).await.unwrap_err();
        assert!(!error
            .to_string()
            .contains("private-page-body-must-not-appear"));
    }
    tx.rollback().await.unwrap();
    sqlx::query("PRAGMA ignore_check_constraints=OFF")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(read(&pool).await, report);
}

#[tokio::test]
async fn source_teletext_page_capture_invalid_or_expired_replacement_preserves_the_snapshot() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = page_report(MediaSourceKind::Local);
    store(&pool, &claim, &report).await;
    let mut invalid = report.clone();
    let SourceTeletextObservedPages::PagesObserved { pages, .. } = &mut invalid.observed_pages
    else {
        unreachable!()
    };
    pages.units[0].data.pop();
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(replace(&mut tx, &claim, parents(), &invalid).await.is_err());
    tx.rollback().await.unwrap();
    assert_eq!(read(&pool).await, report);
    sqlx::query(r#"UPDATE "ManagedLibraryLease" SET "LeaseExpiresAt"='2000-01-01 00:00:00'"#)
        .execute(&pool)
        .await
        .unwrap();
    let mut tx = begin_write(&pool).await.unwrap();
    assert!(!replace(
        &mut tx,
        &claim,
        parents(),
        &observation(MediaSourceKind::Local)
    )
    .await
    .unwrap());
    tx.commit().await.unwrap();
    assert_eq!(read(&pool).await, report);
}

#[tokio::test]
async fn source_teletext_page_capture_rejects_structurally_valid_wire_fact_disagreement() {
    let (pool, claim) = fixture(MediaSourceKind::Local).await;
    let report = page_report(MediaSourceKind::Local);
    store(&pool, &claim, &report).await;
    for context_only in [false, true] {
        let mut invalid = report.clone();
        let SourceTeletextObservedPages::PagesObserved { pages, .. } = &mut invalid.observed_pages
        else {
            unreachable!()
        };
        if context_only {
            pages.transmission_mode_conflict = !pages.transmission_mode_conflict;
        } else {
            let opening = usize::try_from(pages.pages[0].header_unit).unwrap();
            // Change the low page nibble to another valid Hamming codeword.
            pages.units[opening].data[6] ^= (0x15_u8 ^ 0x02_u8).reverse_bits();
        }
        invalid.validate().unwrap();
        let payload = serde_json::to_string(invalid.observed_pages.page_window().unwrap()).unwrap();
        let mut tx = begin_write(&pool).await.unwrap();
        assert!(replace(&mut tx, &claim, parents(), &invalid).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(read(&pool).await, report);
        let mut tx = begin_write(&pool).await.unwrap();
        sqlx::query(r#"UPDATE "SourceTeletextPageWindow" SET "Payload"=?"#)
            .bind(payload)
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(load(&mut tx, 401, 501).await.is_err());
        tx.rollback().await.unwrap();
        assert_eq!(read(&pool).await, report);
    }
}

#[tokio::test]
async fn source_teletext_page_capture_survives_committed_database_reopen() {
    let directory = tempfile::tempdir().unwrap();
    let options = sqlx::sqlite::SqliteConnectOptions::new()
        .filename(directory.path().join("page-capture.db"))
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(1)
        .connect_with(options.clone())
        .await
        .unwrap();
    let (pool, claim) = fixture_on_pool(pool, MediaSourceKind::Local).await;
    let report = page_report(MediaSourceKind::Local);
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
