use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use chrono::Duration;

pub(crate) struct MirrorChannelHealthCheck {
    pool: Pool<Db>,
}

impl MirrorChannelHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for MirrorChannelHealthCheck {
    fn title(&self) -> &'static str {
        "Mirror Channels"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT c."Number", c."Name", c."MirrorSourceChannelId", c."PlayoutOffset",
                      source."Id" AS "SourceId", source."PlayoutSource" AS "SourcePlayoutSource",
                      (SELECT COUNT(*) FROM "Playout" p WHERE p."ChannelId" = c."Id") AS "OwnPlayoutCount"
               FROM "Channel" c
               LEFT JOIN "Channel" source ON source."Id" = c."MirrorSourceChannelId"
               WHERE c."PlayoutSource" = 1
               ORDER BY c."SortNumber", c."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        let mut issues = Vec::new();
        for row in rows {
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
            let label = format!("{number} - {name}");
            let source_id = row.try_get::<Option<i64>, _>("SourceId").ok().flatten();
            if source_id.is_none() {
                issues.push(format!("{label}: source channel is missing"));
            } else if row
                .try_get::<Option<i64>, _>("SourcePlayoutSource")
                .ok()
                .flatten()
                != Some(0)
            {
                issues.push(format!("{label}: source channel is not Generated"));
            }

            if row.try_get::<i64, _>("OwnPlayoutCount").unwrap_or_default() > 0 {
                issues.push(format!("{label}: mirror also has its own playout"));
            }

            if let Some(offset) = row
                .try_get::<Option<String>, _>("PlayoutOffset")
                .ok()
                .flatten()
            {
                match crate::repositories::common::try_decode_signed_duration_text(&offset) {
                    Ok(offset)
                        if (Duration::hours(-12)..=Duration::hours(12)).contains(&offset) => {}
                    Ok(_) => issues.push(format!("{label}: offset is outside the 12-hour limit")),
                    Err(_) => issues.push(format!("{label}: offset is malformed")),
                }
            }
        }

        if issues.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }

        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Warning,
            issues.join("; "),
            format!("{} mirror channel configuration issue(s)", issues.len()),
        ))
    }
}
