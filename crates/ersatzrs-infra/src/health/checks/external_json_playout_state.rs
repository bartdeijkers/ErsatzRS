use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::external_json::snapshot::{self, SnapshotError};

pub(crate) struct ExternalJsonPlayoutStateHealthCheck {
    pool: Pool<Db>,
}

impl ExternalJsonPlayoutStateHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for ExternalJsonPlayoutStateHealthCheck {
    fn title(&self) -> &'static str {
        "External JSON Playout State"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT p."Id", p."ScheduleFile", c."Number", c."Name",
            EXISTS(SELECT 1 FROM "Channel" n WHERE
                (n."Id"=c."Id" OR (n."PlayoutSource"=1 AND n."MirrorSourceChannelId"=c."Id"))
                AND n."StreamingEngine"=1) AS "UsesNext"
            FROM "Playout" p JOIN "Channel" c ON c."Id"=p."ChannelId"
            WHERE p."ScheduleKind" IN (20, 'ExternalJson') ORDER BY p."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let now = chrono::Utc::now();
        let mut messages = Vec::new();
        let mut severity = 0;
        for row in rows {
            let id: i64 = row.try_get("Id").map_err(sqlx_error)?;
            let path: Option<String> = row.try_get("ScheduleFile").map_err(sqlx_error)?;
            let issue = if row.try_get::<bool, _>("UsesNext").map_err(sqlx_error)? {
                Some((3, "Next does not consume external planning."))
            } else {
                match snapshot::accepted(&self.pool, id).await {
                    Ok(None) => Some((1, "Direct file updates; no accepted planning baseline.")),
                    Ok(Some(saved)) if Some(saved.schedule_file.as_str()) != path.as_deref() => {
                        Some((2, "The source path differs from the accepted planning."))
                    }
                    Ok(Some(saved))
                        if saved
                            .schedule
                            .items
                            .last()
                            .is_none_or(|item| item.finish <= now) =>
                    {
                        Some((2, "The accepted planning has no future programmes."))
                    }
                    Ok(Some(_)) => None,
                    Err(SnapshotError::Database) => {
                        return Err(ersatzrs_core::health::HealthCheckError::Database(
                            "external JSON accepted planning could not be checked".into(),
                        ))
                    }
                    Err(_) => Some((3, "The accepted planning is invalid or unsupported.")),
                }
            };
            if let Some((level, reason)) = issue {
                severity = severity.max(level);
                let number: Option<String> = row.try_get("Number").map_err(sqlx_error)?;
                let name: Option<String> = row.try_get("Name").map_err(sqlx_error)?;
                messages.push(format!(
                    "{} - {}: {reason}",
                    number.unwrap_or_default(),
                    name.unwrap_or_default()
                ));
            }
        }
        if messages.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }
        let status = match severity {
            3 => HealthCheckStatus::Fail,
            2 => HealthCheckStatus::Warning,
            _ => HealthCheckStatus::Info,
        };
        Ok(HealthCheckResult::new(
            self.title(),
            status,
            messages.join("\n"),
            "External JSON planning needs attention",
        )
        .with_link("/playouts"))
    }
}
