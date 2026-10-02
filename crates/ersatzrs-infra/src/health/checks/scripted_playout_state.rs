use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};
use crate::repositories::common::decode_enum_column;
use ersatzrs_domain::PlayoutScheduleKind;

pub(crate) struct ScriptedPlayoutStateHealthCheck {
    pool: Pool<Db>,
}

impl ScriptedPlayoutStateHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ScriptedPlayoutStateHealthCheck {
    fn title(&self) -> &'static str {
        "Scripted Playout State"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT p."Id", p."ScheduleKind", c."Number", c."Name", b."Message"
               FROM "Playout" p
               JOIN "Channel" c ON c."Id" = p."ChannelId"
               JOIN "PlayoutBuildStatus" b ON b."PlayoutId" = p."Id"
               WHERE b."Success" = 0 ORDER BY c."SortNumber", p."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let mut failures = Vec::new();
        for row in rows {
            let kind = decode_enum_column::<PlayoutScheduleKind>(
                "playout schedule_kind",
                &row,
                "ScheduleKind",
            )
            .map_err(|error| HealthCheckError::Database(error.to_string()))?;
            if kind != PlayoutScheduleKind::Scripted {
                continue;
            }
            let id: i64 = row.try_get("Id").map_err(sqlx_error)?;
            let number: Option<String> = row.try_get("Number").map_err(sqlx_error)?;
            let name: Option<String> = row.try_get("Name").map_err(sqlx_error)?;
            let message: Option<String> = row.try_get("Message").map_err(sqlx_error)?;
            failures.push(format!(
                "{} - {} (playout {id}): {}",
                number.unwrap_or_default(),
                name.unwrap_or_default(),
                message.unwrap_or_default()
            ));
        }
        if failures.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            failures.join("\n"),
            format!("{} Scripted build failure(s)", failures.len()),
        ))
    }
}
