use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::remote_stream::{resolve_remote_stream, RemoteStreamDefinition};
use std::path::Path;

pub(crate) struct RemoteStreamHealthCheck {
    pool: Pool<Db>,
}

impl RemoteStreamHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for RemoteStreamHealthCheck {
    fn title(&self) -> &'static str {
        "Remote Streams"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"SELECT rs."Id", rs."Url", rs."Script", rs."Duration",
                      rs."FallbackQuery", rs."IsLive",
                      rsm."Title", mf."Path", mv."Duration" AS "ProbedDuration"
               FROM "RemoteStream" rs
               LEFT JOIN "RemoteStreamMetadata" rsm ON rsm."RemoteStreamId" = rs."Id"
               LEFT JOIN "MediaVersion" mv ON mv."RemoteStreamId" = rs."Id"
               LEFT JOIN "MediaFile" mf ON mf."MediaVersionId" = mv."Id"
               ORDER BY rs."Id""#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;

        if rows.is_empty() {
            return Ok(HealthCheckResult::not_applicable(self.title()));
        }

        let mut invalid = Vec::new();
        let mut warnings = Vec::new();
        for row in rows {
            let id = row.try_get::<i64, _>("Id").unwrap_or_default();
            let title = row
                .try_get::<Option<String>, _>("Title")
                .ok()
                .flatten()
                .filter(|title| !title.trim().is_empty())
                .unwrap_or_else(|| format!("Remote stream {id}"));
            let definition = RemoteStreamDefinition {
                url: row.try_get("Url").ok().flatten(),
                script: row.try_get("Script").ok().flatten(),
                duration: row.try_get("Duration").ok().flatten(),
                fallback_query: row.try_get("FallbackQuery").ok().flatten(),
                is_live: row.try_get::<i64, _>("IsLive").unwrap_or_default() != 0,
                title: None,
                plot: None,
                year: None,
                content_rating: None,
            };
            let definition_path = row
                .try_get::<Option<String>, _>("Path")
                .ok()
                .flatten()
                .unwrap_or_else(|| format!("remote-stream-{id}.yml"));
            let Ok(resolved) = resolve_remote_stream(&definition, Path::new(&definition_path))
            else {
                invalid.push(title);
                continue;
            };
            let probed_duration = row
                .try_get::<Option<String>, _>("ProbedDuration")
                .ok()
                .flatten();
            let has_duration = resolved
                .configured_duration
                .as_deref()
                .or(probed_duration.as_deref())
                .is_some_and(|duration| {
                    let duration = duration.trim();
                    !duration.is_empty()
                        && duration != "00:00:00"
                        && !duration.starts_with("00:00:00.")
                });
            if !resolved.is_live && !has_duration {
                warnings.push(title);
            }
        }

        if !invalid.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Fail,
                format!(
                    "{} Remote Stream definitions are invalid: {}",
                    invalid.len(),
                    invalid.into_iter().take(5).collect::<Vec<_>>().join(", ")
                ),
                "One or more Remote Stream definitions are invalid",
            )
            .with_link("/media/remote/streams"));
        }
        if !warnings.is_empty() {
            return Ok(HealthCheckResult::new(
                self.title(),
                HealthCheckStatus::Warning,
                format!(
                    "{} on-demand Remote Streams have no usable duration: {}",
                    warnings.len(),
                    warnings.into_iter().take(5).collect::<Vec<_>>().join(", ")
                ),
                "One or more Remote Streams have no usable duration",
            )
            .with_link("/media/remote/streams"));
        }
        Ok(HealthCheckResult::pass(self.title()))
    }
}
