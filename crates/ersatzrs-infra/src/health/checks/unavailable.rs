use super::super::{
    async_trait, media_state_sample, sqlx_error, Db, HealthCheck, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row, STATUS_UNAVAILABLE,
};

/// Media items backed by a Remote Stream: the stream itself and any projected
/// episode sharing its media version. Their `Unavailable` state comes from the
/// provider's declared availability, not from a missing file.
const REMOTE_BACKED: &str = r#"AND EXISTS (
    SELECT 1 FROM "MediaVersion" AS remote
    WHERE remote."RemoteStreamId" IS NOT NULL
      AND (remote."RemoteStreamId" = mi."Id" OR remote."EpisodeId" = mi."Id"))"#;
const NOT_REMOTE_BACKED: &str = r#"AND NOT EXISTS (
    SELECT 1 FROM "MediaVersion" AS remote
    WHERE remote."RemoteStreamId" IS NOT NULL
      AND (remote."RemoteStreamId" = mi."Id" OR remote."EpisodeId" = mi."Id"))"#;

pub(crate) struct UnavailableHealthCheck {
    pool: Pool<Db>,
}

impl UnavailableHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for UnavailableHealthCheck {
    fn title(&self) -> &'static str {
        "Unavailable"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let (local_count, local_items) =
            media_state_sample(&self.pool, STATUS_UNAVAILABLE, NOT_REMOTE_BACKED).await?;
        let (stream_count, stream_items) = unavailable_remote_streams(&self.pool).await?;
        if local_count == 0 && stream_count == 0 {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        // Local and media-server items use the ErsatzRS product name. A
        // Remote Stream is unavailable because its provider says so; its
        // definition file is present, so "cannot find them on disk" would be
        // false for it.
        let mut messages = Vec::new();
        let mut briefs = Vec::new();
        if local_count > 0 {
            messages.push(format!(
                "There are {local_count} items that are unavailable because ErsatzRS cannot find them on disk, including the following: {}",
                local_items.join(", ")
            ));
            briefs.push(format!("{local_count} items"));
        }
        if stream_count > 0 {
            messages.push(format!(
                "There are {stream_count} Remote Streams that are unavailable because their provider reports them as not playable, including the following: {}",
                stream_items.join(", ")
            ));
            briefs.push(format!("{stream_count} Remote Streams"));
        }
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            messages.join(" "),
            format!("There are {} that are unavailable", briefs.join(" and ")),
        )
        .with_link("search?query=state%3aUnavailable"))
    }
}

/// Count unavailable Remote Streams once each, even when a television
/// presentation also projects them as episodes, and name up to five by path.
async fn unavailable_remote_streams(pool: &Pool<Db>) -> Result<(i64, Vec<String>)> {
    let from = format!(
        r#"FROM "MediaItem" AS mi
           JOIN "MediaVersion" AS mv ON mv."RemoteStreamId" IS NOT NULL
                AND (mv."RemoteStreamId" = mi."Id" OR mv."EpisodeId" = mi."Id")
           WHERE mi."State" = ? {REMOTE_BACKED}"#
    );
    let count = sqlx::query_scalar::<_, i64>(crate::db::assert_sql_safe(format!(
        r#"SELECT COUNT(DISTINCT mv."RemoteStreamId") {from}"#
    )))
    .bind(STATUS_UNAVAILABLE)
    .fetch_one(pool)
    .await
    .map_err(sqlx_error)?;
    if count == 0 {
        return Ok((0, Vec::new()));
    }
    let rows = sqlx::query(crate::db::assert_sql_safe(format!(
        r#"SELECT mv."RemoteStreamId" AS "StreamId",
                  (SELECT file."Path" FROM "MediaFile" AS file
                   WHERE file."MediaVersionId" = mv."Id"
                   ORDER BY file."Id" LIMIT 1) AS "Path"
           {from}
           GROUP BY mv."RemoteStreamId"
           ORDER BY MIN(mi."Id")
           LIMIT 5"#
    )))
    .bind(STATUS_UNAVAILABLE)
    .fetch_all(pool)
    .await
    .map_err(sqlx_error)?;
    let items = rows
        .iter()
        .map(|row| {
            row.try_get::<Option<String>, _>("Path")
                .ok()
                .flatten()
                .unwrap_or_else(|| {
                    format!(
                        "RemoteStream {}",
                        row.try_get::<i64, _>("StreamId").unwrap_or_default()
                    )
                })
        })
        .collect();
    Ok((count, items))
}

#[cfg(test)]
mod tests {
    use ersatzrs_core::health::{HealthCheck, HealthCheckStatus};
    use sqlx::sqlite::SqlitePoolOptions;
    use sqlx::Executor;

    use super::UnavailableHealthCheck;

    async fn fixture_pool() -> sqlx::Pool<crate::db::Db> {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("in-memory sqlite");
        for table in [
            "MediaItem\" (\"Id\" INTEGER PRIMARY KEY, \"State\" INTEGER NOT NULL",
            "Movie\" (\"Id\" INTEGER PRIMARY KEY",
            "Episode\" (\"Id\" INTEGER PRIMARY KEY",
            "MusicVideo\" (\"Id\" INTEGER PRIMARY KEY",
            "OtherVideo\" (\"Id\" INTEGER PRIMARY KEY",
            "Song\" (\"Id\" INTEGER PRIMARY KEY",
            "Image\" (\"Id\" INTEGER PRIMARY KEY",
            "RemoteStream\" (\"Id\" INTEGER PRIMARY KEY",
            "MediaVersion\" (\"Id\" INTEGER PRIMARY KEY, \"MovieId\" INTEGER, \"EpisodeId\" INTEGER, \"MusicVideoId\" INTEGER, \"OtherVideoId\" INTEGER, \"SongId\" INTEGER, \"ImageId\" INTEGER, \"RemoteStreamId\" INTEGER",
            "MediaFile\" (\"Id\" INTEGER PRIMARY KEY, \"MediaVersionId\" INTEGER, \"Path\" TEXT",
        ] {
            let sql = format!("CREATE TABLE \"{table})");
            sqlx::raw_sql(crate::db::assert_sql_safe(sql))
                .execute(&pool)
                .await
                .expect("fixture table");
        }
        pool
    }

    #[tokio::test]
    async fn remote_streams_are_counted_once_and_not_reported_missing_from_disk() {
        let pool = fixture_pool().await;
        pool.execute(
            r#"INSERT INTO "MediaItem" VALUES (10, 2), (11, 2), (12, 0);
               INSERT INTO "RemoteStream" VALUES (10), (12);
               INSERT INTO "Episode" VALUES (11);
               INSERT INTO "MediaVersion" ("Id", "EpisodeId", "RemoteStreamId")
                   VALUES (100, 11, 10), (102, NULL, 12);
               INSERT INTO "MediaFile" VALUES (1000, 100, 'streams/Title-abc.yml'),
                   (1002, 102, 'streams/Other-def.yml');"#,
        )
        .await
        .expect("remote fixture");

        let result = UnavailableHealthCheck::new(pool.clone())
            .check()
            .await
            .expect("health result");
        assert_eq!(result.status, HealthCheckStatus::Warning);
        assert_eq!(
            result.brief_message,
            "There are 1 Remote Streams that are unavailable"
        );
        assert!(result.message.contains(
            "1 Remote Streams that are unavailable because their provider reports them as not playable, including the following: streams/Title-abc.yml"
        ));
        assert!(!result.message.contains("cannot find them on disk"));

        pool.execute(
            r#"INSERT INTO "MediaItem" VALUES (20, 2);
               INSERT INTO "Movie" VALUES (20);
               INSERT INTO "MediaVersion" ("Id", "MovieId") VALUES (200, 20);
               INSERT INTO "MediaFile" VALUES (2000, 200, '/media/Movie.mkv');"#,
        )
        .await
        .expect("local fixture");
        let result = UnavailableHealthCheck::new(pool)
            .check()
            .await
            .expect("health result");
        assert_eq!(
            result.brief_message,
            "There are 1 items and 1 Remote Streams that are unavailable"
        );
        assert!(result.message.starts_with(
            "There are 1 items that are unavailable because ErsatzRS cannot find them on disk, including the following: /media/Movie.mkv"
        ));
    }
}
