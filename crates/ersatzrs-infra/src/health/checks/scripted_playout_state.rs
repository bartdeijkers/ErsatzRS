use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};
use crate::repositories::common::{decode_datetime, decode_enum_column, encode_enum};
use ersatzrs_core::scheduling::validate_scripted_checkpoint;
use ersatzrs_domain::{PlayoutScheduleKind, PlayoutScriptedCheckpoint};
use std::collections::BTreeMap;

pub(crate) struct ScriptedPlayoutStateHealthCheck {
    pool: Pool<Db>,
}

impl ScriptedPlayoutStateHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }

    async fn checkpoint_failures(&self) -> Result<Vec<String>> {
        let rows = sqlx::query(
            r#"SELECT p."Id", c."Number", c."Name", i."Finish", cp."Context"
               FROM "Playout" p
               JOIN "Channel" c ON c."Id" = p."ChannelId"
               JOIN "PlayoutItem" i ON i."PlayoutId" = p."Id"
               LEFT JOIN "PlayoutScriptedCheckpoint" cp
                 ON cp."PlayoutId" = p."Id" AND cp."CheckpointDate" = i."Finish"
               WHERE p."ScheduleKind" = ?
               ORDER BY c."SortNumber", p."Id", i."Finish""#,
        )
        .bind(encode_enum(PlayoutScheduleKind::Scripted))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let mut invalid = BTreeMap::new();
        for row in rows {
            let id: i64 = row.try_get("Id").map_err(sqlx_error)?;
            if invalid.contains_key(&id) {
                continue;
            }
            let context: Option<String> = row.try_get("Context").map_err(sqlx_error)?;
            let finish: String = row.try_get("Finish").map_err(sqlx_error)?;
            let valid = context.is_some_and(|context| {
                decode_datetime("Scripted checkpoint finish", &finish).is_ok_and(
                    |checkpoint_date| {
                        validate_scripted_checkpoint(
                            &PlayoutScriptedCheckpoint {
                                playout_id: id,
                                checkpoint_date,
                                context,
                            },
                            id,
                        )
                        .is_ok()
                    },
                )
            });
            if !valid {
                let number: Option<String> = row.try_get("Number").map_err(sqlx_error)?;
                let name: Option<String> = row.try_get("Name").map_err(sqlx_error)?;
                invalid.insert(id, format!(
                    "{} - {} (playout {id}): missing or incompatible Scripted boundary checkpoint; Refresh is unsafe. Use an explicit Reset to create compatible state.",
                    number.unwrap_or_default(), name.unwrap_or_default()
                ));
            }
        }
        Ok(invalid.into_values().collect())
    }

    async fn dependency_failures(&self) -> Result<Vec<String>> {
        let rows: Vec<(i64, i64)> = sqlx::query_as(
            r#"SELECT w."LibraryId", w."PlayoutId" FROM "ScriptedSourceRefreshWork" w
               JOIN "Playout" p ON p."Id" = w."PlayoutId"
               WHERE w."Diagnostic" IS NOT NULL AND p."ScheduleKind" IN (?, 'Scripted')
               ORDER BY w."LibraryId", w."PlayoutId""#,
        )
        .bind(encode_enum(PlayoutScheduleKind::Scripted))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        Ok(rows.into_iter().map(|(library, playout)| format!(
            "Scripted source comparison failed for library {library}, playout {playout}; the published timeline is retained and comparison will retry."
        )).collect())
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
        failures.extend(self.checkpoint_failures().await?);
        failures.extend(self.dependency_failures().await?);
        if failures.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            failures.join("\n"),
            format!("{} Scripted state error(s)", failures.len()),
        ))
    }
}
