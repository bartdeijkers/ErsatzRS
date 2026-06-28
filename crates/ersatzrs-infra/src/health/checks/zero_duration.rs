use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};

pub(crate) struct ZeroDurationHealthCheck {
    pool: Pool<Db>,
}

impl ZeroDurationHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ZeroDurationHealthCheck {
    fn title(&self) -> &'static str {
        "Zero Duration"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT mf."Path"
               FROM "MediaVersion" mv
               INNER JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
               WHERE (mv."Duration" = '00:00:00'
                  OR mv."Duration" LIKE '00:00:00.%')
                 AND mv."RemoteStreamId" IS NULL
               ORDER BY mf."Path", mf."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        let paths = rows
            .iter()
            .take(5)
            .filter_map(|row| row.try_get::<Option<String>, _>("Path").ok().flatten())
            .collect::<Vec<_>>();

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "There are {} files with zero duration, including the following: {}",
                rows.len(),
                paths.join(", ")
            ),
            format!("There are {} files with zero duration", rows.len()),
        ))
    }
}
