//! Repair unset local metadata dates before ordinary scanning.
//! Mirrors `MetadataRepository.BackfillDateAdded` and `ScanLocalLibraryHandler`.

use std::collections::BTreeSet;
use std::fs::Metadata;
use std::time::{SystemTime, UNIX_EPOCH};

use ersatzrs_core::repositories::ManagedLibraryOperationClaim;
use ersatzrs_infra::db::{assert_sql_safe, DbPool};
use ersatzrs_time::{DateTime, Datelike, NaiveDate, NaiveDateTime, Timelike, Utc};
use sqlx::Row;

const TABLES: [(&str, &str); 6] = [
    ("MovieMetadata", "MovieId"),
    ("MusicVideoMetadata", "MusicVideoId"),
    ("OtherVideoMetadata", "OtherVideoId"),
    ("SongMetadata", "SongId"),
    ("ImageMetadata", "ImageId"),
    ("RemoteStreamMetadata", "RemoteStreamId"),
];

pub(crate) async fn backfill(
    pool: &DbPool,
    library_path: &super::LocalLibraryPath,
    claim: Option<&ManagedLibraryOperationClaim>,
) -> crate::Result<usize> {
    if !tokio::fs::metadata(&library_path.path)
        .await
        .is_ok_and(|m| m.is_dir())
    {
        return Ok(0);
    }
    let mut changed = BTreeSet::new();
    for (table, owner) in TABLES {
        backfill_table(pool, library_path.id, table, owner, claim, &mut changed).await?;
    }
    // Unlike upstream's Lucene index, search/added_inthelast reads these SQL
    // metadata dates directly. Normal successful-scan dependency resolution
    // remains owned by the caller; do not clear any scheduling checkpoint.
    Ok(changed.len())
}

async fn backfill_table(
    pool: &DbPool,
    library_path_id: i64,
    table: &str,
    owner: &str,
    claim: Option<&ManagedLibraryOperationClaim>,
    changed: &mut BTreeSet<i64>,
) -> crate::Result<()> {
    let select = format!(
        r#"SELECT m."Id", m."{owner}" AS "OwnerId", m."DateAdded",
           (SELECT f."Path" FROM "MediaVersion" v
            JOIN "MediaFile" f ON f."MediaVersionId"=v."Id"
            WHERE v."{owner}"=m."{owner}" ORDER BY v."Id", f."Id" LIMIT 1) AS "Path"
           FROM "{table}" m JOIN "MediaItem" i ON i."Id"=m."{owner}"
           WHERE i."LibraryPathId"=? AND m."Id">? AND m."DateAdded" LIKE '0001-01-01%'
           ORDER BY m."Id" LIMIT 64"#
    );
    let update = format!(
        r#"UPDATE "{table}" SET "DateAdded"=? WHERE "Id"=? AND "DateAdded"=?
           AND EXISTS(SELECT 1 FROM "MediaItem" i
                      WHERE i."Id"="{table}"."{owner}" AND i."LibraryPathId"=?)"#
    );
    let mut cursor = 0_i64;
    loop {
        let rows = sqlx::query(assert_sql_safe(&select))
            .bind(library_path_id)
            .bind(cursor)
            .fetch_all(pool)
            .await?;
        if rows.is_empty() {
            break;
        }
        let mut pending = Vec::new();
        // Filesystem work is outside the SQLite writer transaction.
        for row in rows {
            cursor = row.try_get("Id")?;
            let previous: String = row.try_get("DateAdded")?;
            if !is_unset(&previous) {
                continue;
            }
            let Some(path) = row
                .try_get::<Option<String>, _>("Path")?
                .filter(|p| !p.trim().is_empty())
            else {
                continue;
            };
            let Ok(metadata) = tokio::fs::metadata(path).await else {
                continue;
            };
            if !metadata.is_file() {
                continue;
            }
            if let Some(date) = creation_date(&metadata, Utc::now()) {
                let owner_id: i64 = row.try_get("OwnerId")?;
                pending.push((cursor, owner_id, previous, encode_date(date)));
            }
        }
        if pending.is_empty() {
            continue;
        }
        let mut tx = ersatzrs_infra::db::begin_write(pool).await?;
        super::local_scan_claim::check(&mut tx, library_path_id, claim).await?;
        let mut committed = Vec::new();
        for (id, owner_id, previous, date) in pending {
            if sqlx::query(assert_sql_safe(&update))
                .bind(date)
                .bind(id)
                .bind(previous)
                .bind(library_path_id)
                .execute(&mut *tx)
                .await?
                .rows_affected()
                != 0
            {
                committed.push(owner_id);
            }
        }
        super::local_scan_claim::check(&mut tx, library_path_id, claim).await?;
        tx.commit().await?;
        changed.extend(committed);
    }
    Ok(())
}

fn parse_date(value: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|d| d.with_timezone(&Utc))
        .or_else(|| {
            NaiveDateTime::parse_from_str(value, "%Y-%m-%d %H:%M:%S%.f")
                .ok()
                .map(|d| d.and_utc())
        })
        .or_else(|| {
            NaiveDate::parse_from_str(value, "%Y-%m-%d")
                .ok()?
                .and_hms_opt(0, 0, 0)
                .map(|d| d.and_utc())
        })
}

fn is_unset(value: &str) -> bool {
    parse_date(value).is_some_and(|d| {
        d.year() == 1
            && d.ordinal() == 1
            && d.num_seconds_from_midnight() == 0
            && d.nanosecond() == 0
    })
}

fn encode_date(date: DateTime<Utc>) -> String {
    // Match repositories::common's canonical UTC/.NET 100ns storage codec.
    let base = date.naive_utc().format("%Y-%m-%d %H:%M:%S").to_string();
    let ticks = date.nanosecond() / 100;
    if ticks == 0 {
        base
    } else {
        format!("{base}.{ticks:07}")
    }
}

fn creation_date(metadata: &Metadata, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    normalize_creation_date(
        metadata
            .created()
            .ok()
            .and_then(system_time)
            .or_else(|| creation_fallback(metadata)),
        now,
    )
}

fn normalize_creation_date(
    date: Option<DateTime<Utc>>,
    now: DateTime<Utc>,
) -> Option<DateTime<Utc>> {
    date.filter(|d| (1..=9999).contains(&d.year()))
        .map(|d| d.min(now))
}

// .NET10 CreationTimeUtc uses min(mtime, ctime) when Unix birth time is absent.
// Windows uses its actual creation time; an unavailable timestamp is skipped.
#[cfg(unix)]
fn creation_fallback(metadata: &Metadata) -> Option<DateTime<Utc>> {
    use std::os::unix::fs::MetadataExt;
    let changed =
        DateTime::from_timestamp(metadata.ctime(), u32::try_from(metadata.ctime_nsec()).ok()?)?;
    let modified = system_time(metadata.modified().ok()?)?;
    Some(changed.min(modified))
}

#[cfg(not(unix))]
fn creation_fallback(_metadata: &Metadata) -> Option<DateTime<Utc>> {
    None
}

fn system_time(time: SystemTime) -> Option<DateTime<Utc>> {
    match time.duration_since(UNIX_EPOCH) {
        Ok(d) => DateTime::from_timestamp(i64::try_from(d.as_secs()).ok()?, d.subsec_nanos()),
        Err(e) => {
            let d = e.duration();
            let seconds = i64::try_from(d.as_secs()).ok()?.checked_neg()?;
            if d.subsec_nanos() == 0 {
                DateTime::from_timestamp(seconds, 0)
            } else {
                DateTime::from_timestamp(seconds.checked_sub(1)?, 1_000_000_000 - d.subsec_nanos())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn row290_unset_means_exact_minimum_and_storage_preserves_ticks() {
        for value in [
            "0001-01-01T00:00:00Z",
            "0001-01-01 00:00:00.0000000",
            "0001-01-01",
        ] {
            assert!(is_unset(value));
        }
        for value in [
            "0001-01-01 00:00:00.0000001",
            "0001-01-02 00:00:00",
            "invalid",
        ] {
            assert!(!is_unset(value));
        }
        let value = parse_date("2020-04-03T01:02:03.123456789Z").unwrap();
        assert_eq!(encode_date(value), "2020-04-03 01:02:03.1234567");
        let now = parse_date("2026-01-01T00:00:00Z").unwrap();
        assert_eq!(normalize_creation_date(Some(value), now), Some(value));
        assert_eq!(
            normalize_creation_date(Some(now + ersatzrs_time::Duration::days(1)), now),
            Some(now)
        );
        assert_eq!(normalize_creation_date(None, now), None);
    }

    #[test]
    fn row290_system_time_handles_both_sides_of_epoch() {
        for (time, expected) in [
            (UNIX_EPOCH, "1970-01-01T00:00:00Z"),
            (
                UNIX_EPOCH - Duration::from_millis(1),
                "1969-12-31T23:59:59.999Z",
            ),
            (UNIX_EPOCH - Duration::from_secs(1), "1969-12-31T23:59:59Z"),
            (
                UNIX_EPOCH + Duration::from_millis(1),
                "1970-01-01T00:00:00.001Z",
            ),
        ] {
            assert_eq!(system_time(time), parse_date(expected));
        }
    }

    // .NET's Unix fallback is retained independently of birth-time availability.
    #[cfg(unix)]
    #[test]
    fn row290_unix_fallback_uses_earlier_modified_or_changed_time() {
        let file = tempfile::NamedTempFile::new().unwrap();
        let modified = parse_date("2020-01-01T00:00:00Z").unwrap().into();
        file.as_file()
            .set_times(std::fs::FileTimes::new().set_modified(modified))
            .unwrap();
        assert_eq!(
            creation_fallback(&file.as_file().metadata().unwrap()),
            system_time(modified)
        );
    }
}
