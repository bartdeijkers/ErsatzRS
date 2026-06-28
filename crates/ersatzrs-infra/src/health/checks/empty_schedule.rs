use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};

pub(crate) struct EmptyScheduleHealthCheck {
    pool: Pool<Db>,
}

impl EmptyScheduleHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for EmptyScheduleHealthCheck {
    fn title(&self) -> &'static str {
        "Empty Schedule"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT ps."Name"
               FROM "ProgramSchedule" ps
               WHERE NOT EXISTS (
                   SELECT 1 FROM "ProgramScheduleItem" psi
                   WHERE psi."ProgramScheduleId" = ps."Id"
               )
               AND (
                   EXISTS (
                       SELECT 1 FROM "Playout" p
                       WHERE p."ProgramScheduleId" = ps."Id"
                   )
                   OR EXISTS (
                       SELECT 1 FROM "ProgramScheduleAlternate" psa
                       WHERE psa."ProgramScheduleId" = ps."Id"
                   )
               )
               ORDER BY ps."Name", ps."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        let names: Vec<String> = rows
            .iter()
            .map(|row| {
                row.try_get::<Option<String>, _>("Name")
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| "Unnamed".to_owned())
            })
            .collect();

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            format!(
                "There are {} empty schedules in use, which are NOT supported and WILL cause errors, including: {}",
                names.len(),
                names.join(", ")
            ),
            format!(
                "There are {} empty schedules in use, which are NOT supported and WILL cause errors",
                names.len()
            ),
        ))
    }
}
