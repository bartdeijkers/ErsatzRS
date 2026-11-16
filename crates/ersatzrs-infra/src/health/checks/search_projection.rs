use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
};
use chrono::{Datelike, Timelike};
use sqlx::Row;

use crate::repositories::common::decode_datetime;

pub(crate) struct SearchProjectionHealthCheck {
    pool: Pool<Db>,
}

impl SearchProjectionHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for SearchProjectionHealthCheck {
    fn title(&self) -> &'static str {
        "Search Projection"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        for sql in [
            r#"SELECT "ContentKind", "SubKind", "ParentMediaItemId" FROM "MediaItem" LIMIT 1"#,
            r#"SELECT "Title" FROM "MediaStream" LIMIT 1"#,
            r#"SELECT "Title" FROM "Subtitle" LIMIT 1"#,
        ] {
            sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map_err(sqlx_error)?;
        }

        let missing_dates = self.missing_local_dates().await?;
        if missing_dates != 0 {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!("{missing_dates} local media items have an unset date added. Rescan their libraries to recover file creation dates; unavailable files remain unchanged. Recent-date searches can miss these items until repaired."),
                "Local date-added repair needed",
            ));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Pass,
            "Search projection includes content variant, parent linkage, and stream-title fields.",
            "Search projection ready",
        ))
    }
}

impl SearchProjectionHealthCheck {
    async fn missing_local_dates(&self) -> Result<usize> {
        let mut missing = std::collections::BTreeSet::new();
        for kind in [
            "Movie",
            "MusicVideo",
            "OtherVideo",
            "Song",
            "Image",
            "RemoteStream",
        ] {
            let sql = format!(
                r#"SELECT metadata."{kind}Id" AS "OwnerId", metadata."DateAdded"
                FROM "{kind}Metadata" metadata
                JOIN "MediaItem" item ON item."Id"=metadata."{kind}Id"
                JOIN "LibraryPath" path ON path."Id"=item."LibraryPathId"
                JOIN "LocalLibrary" local ON local."Id"=path."LibraryId"
                WHERE metadata."DateAdded" LIKE '0001-01-01%'"#
            );
            let rows = sqlx::query(crate::db::assert_sql_safe(&sql))
                .fetch_all(&self.pool)
                .await
                .map_err(sqlx_error)?;
            for row in rows {
                let value: String = row.try_get("DateAdded").map_err(sqlx_error)?;
                if decode_datetime("DateAdded", &value).is_ok_and(|date| {
                    date.year() == 1
                        && date.ordinal() == 1
                        && date.num_seconds_from_midnight() == 0
                        && date.nanosecond() == 0
                }) {
                    missing.insert(row.try_get::<i64, _>("OwnerId").map_err(sqlx_error)?);
                }
            }
        }
        Ok(missing.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::Executor;

    #[tokio::test]
    async fn row290_search_health_counts_local_owners_and_recovers_after_repair() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
        pool.execute(
            r#"INSERT INTO "MediaSource" ("Id") VALUES (1);
            INSERT INTO "LocalMediaSource" ("Id") VALUES (1);
            INSERT INTO "Library" ("Id", "MediaKind", "MediaSourceId", "Name")
                VALUES (1, 1, 1, 'Synthetic health'), (2, 1, 1, 'Nonlocal fixture');
            INSERT INTO "LocalLibrary" ("Id") VALUES (1);
            INSERT INTO "LibraryPath" ("Id", "LibraryId", "Path") VALUES (1, 1, 'synthetic'), (2, 2, 'nonlocal');
            INSERT INTO "MediaItem" ("Id", "LibraryPathId") VALUES (1,1),(2,1),(3,2);
            INSERT INTO "Movie" ("Id") VALUES (1),(2),(3);
            INSERT INTO "MovieMetadata" ("MovieId", "MetadataKind", "DateAdded", "DateUpdated")
                VALUES (1,3,'0001-01-01T00:00:00Z','2024-01-01'),
                       (1,1,'0001-01-01 00:00:00.0000000','2024-01-01'),
                       (2,3,'0001-01-01 00:00:00.0000001','2024-01-01'),
                       (3,3,'0001-01-01T00:00:00Z','2024-01-01');"#,
        )
        .await
        .unwrap();
        let check = SearchProjectionHealthCheck::new(pool.clone());
        let result = check.check().await.unwrap();
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert!(result.message.starts_with("1 local media items"));
        pool.execute(
            r#"UPDATE "MovieMetadata" SET "DateAdded"='2024-01-01'
            WHERE "MovieId"=1"#,
        )
        .await
        .unwrap();
        assert_eq!(check.check().await.unwrap().status, HealthCheckStatus::Pass);
    }
}
