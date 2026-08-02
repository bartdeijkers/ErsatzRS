use super::super::{
    async_trait, Db, HealthCheck, HealthCheckError, HealthCheckResult, HealthCheckStatus, Pool,
    Result, Row,
};
use chrono::{Duration, Utc};
use ersatzrs_domain::PlayoutScheduleKind;

use crate::maintenance::classic_runtime_state_is_complete;
use crate::repositories::common::decode_enum_column;
use crate::repositories::SqliteSchedulingContentRepository;

pub(crate) struct ClassicPlayoutStateHealthCheck {
    pool: Pool<Db>,
}

impl ClassicPlayoutStateHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ClassicPlayoutStateHealthCheck {
    fn title(&self) -> &'static str {
        "Classic Playout State"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT "p"."Id", "p"."ScheduleKind", "c"."Number", "c"."Name"
               FROM "Playout" AS "p"
               JOIN "Channel" AS "c" ON "c"."Id" = "p"."ChannelId"
               WHERE EXISTS (
                     SELECT 1 FROM "PlayoutItem" AS "pi" WHERE "pi"."PlayoutId" = "p"."Id"
                 )
               ORDER BY "c"."SortNumber", "p"."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|error| HealthCheckError::Database(error.to_string()))?;

        let scheduling = SqliteSchedulingContentRepository::new(self.pool.clone());
        let mut invalid = Vec::new();
        for row in rows {
            if !matches!(
                decode_enum_column::<PlayoutScheduleKind>(
                    "playout schedule_kind",
                    &row,
                    "ScheduleKind"
                ),
                Ok(PlayoutScheduleKind::Classic)
            ) {
                continue;
            }
            let playout_id = row
                .try_get::<i64, _>("Id")
                .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            let reference_data = scheduling
                .classic_reference_data(playout_id, Utc::now(), Duration::zero())
                .await
                .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            if classic_runtime_state_is_complete(&self.pool, playout_id, &reference_data)
                .await
                .map_err(|error| HealthCheckError::Database(error.to_string()))?
            {
                continue;
            }

            let number = row
                .try_get::<Option<String>, _>("Number")
                .ok()
                .flatten()
                .unwrap_or_else(|| "?".to_owned());
            let name = row
                .try_get::<Option<String>, _>("Name")
                .ok()
                .flatten()
                .unwrap_or_else(|| "Unnamed".to_owned());
            invalid.push(format!("{number} - {name}"));
        }

        if invalid.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            format!(
                "Classic playout enumerator state is incomplete for: {}. The scheduler must Refresh these playouts before they can continue safely.",
                invalid.join(", ")
            ),
            format!("{} Classic playout state error(s)", invalid.len()),
        ))
    }
}
