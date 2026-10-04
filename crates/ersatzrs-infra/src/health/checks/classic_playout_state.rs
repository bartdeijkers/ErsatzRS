use super::super::{
    async_trait, Db, HealthCheck, HealthCheckError, HealthCheckResult, HealthCheckStatus, Pool,
    Result, Row,
};
use chrono::{Duration, Utc};
use ersatzrs_core::scheduling::{
    audit_classic_refresh_checkpoint, validate_classic_timeline, PlayoutPreservation,
};
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
            let now = Utc::now();
            let mut reference_data = scheduling
                .classic_reference_data(playout_id, now, Duration::zero())
                .await
                .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            let runtime_complete =
                classic_runtime_state_is_complete(&self.pool, playout_id, &reference_data)
                    .await
                    .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            let refresh_audit = audit_classic_refresh_checkpoint(&reference_data, playout_id, now);
            let preservation_ready = if reference_data
                .existing_items
                .iter()
                .any(|item| item.start <= now && item.finish > now)
            {
                reference_data.preservation = PlayoutPreservation::KeepCurrent;
                audit_classic_refresh_checkpoint(&reference_data, playout_id, now).is_complete()
            } else {
                true
            };
            let mut intervals = reference_data
                .existing_items
                .iter()
                .map(|item| (item.start, item.finish))
                .collect::<Vec<_>>();
            intervals.sort_unstable();
            let timeline_ready = validate_classic_timeline(intervals, None).is_ok();
            let deferred_ready = crate::classic_deferred::state_is_valid(&self.pool, playout_id)
                .await
                .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            if runtime_complete
                && refresh_audit.is_complete()
                && preservation_ready
                && timeline_ready
                && deferred_ready
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
                "Classic playout state needs attention for: {}. Check programme times and saved continuation state. Refresh requires a verified programme boundary and complete checkpoint; unsafe requests leave published programmes unchanged.",
                invalid.join(", ")
            ),
            format!("{} Classic playout state error(s)", invalid.len()),
        ))
    }
}
