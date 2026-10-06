use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::repositories::common::{decode_datetime, encode_enum};
use ersatzrs_core::scheduling::validate_yaml_checkpoint;
use ersatzrs_domain::{FillerKind, PlayoutScheduleKind, PlayoutYamlCheckpoint};
use std::collections::BTreeMap;

pub(crate) struct YamlPlayoutStateHealthCheck {
    pool: Pool<Db>,
}

impl YamlPlayoutStateHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl HealthCheck for YamlPlayoutStateHealthCheck {
    fn title(&self) -> &'static str {
        "YAML Playout State"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        // A programme's checkpoint follows its post/mid-roll sequence. Pre-roll
        // has no independent resume point inside the following programme unit.
        let rows = sqlx::query(
            r#"SELECT p."Id", c."Number", c."Name", i."Finish", cp."Context"
               FROM "Playout" p
               JOIN "Channel" c ON c."Id" = p."ChannelId"
               JOIN "PlayoutItem" i ON i."PlayoutId" = p."Id"
               LEFT JOIN "PlayoutYamlCheckpoint" cp
                 ON cp."PlayoutId" = p."Id" AND cp."CheckpointDate" = i."Finish"
               WHERE p."ScheduleKind" IN (?, 'Sequential') AND i."FillerKind" <> ?
                 AND NOT EXISTS (
                   SELECT 1 FROM "PlayoutItem" n WHERE n."PlayoutId" = p."Id"
                     AND n."Start" = i."Finish" AND n."FillerKind" IN (?, ?))
               ORDER BY c."SortNumber", p."Id", i."Finish""#,
        )
        .bind(encode_enum(PlayoutScheduleKind::Sequential))
        .bind(encode_enum(FillerKind::PreRoll))
        .bind(encode_enum(FillerKind::MidRoll))
        .bind(encode_enum(FillerKind::PostRoll))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let mut failures = BTreeMap::new();
        for row in rows {
            let id: i64 = row.try_get("Id").map_err(sqlx_error)?;
            if failures.contains_key(&id) {
                continue;
            }
            let context: Option<String> = row.try_get("Context").map_err(sqlx_error)?;
            let finish: String = row.try_get("Finish").map_err(sqlx_error)?;
            let valid = context.is_some_and(|context| {
                decode_datetime("YAML checkpoint date", &finish).is_ok_and(|checkpoint_date| {
                    validate_yaml_checkpoint(
                        &PlayoutYamlCheckpoint {
                            playout_id: id,
                            checkpoint_date,
                            context,
                        },
                        id,
                    )
                    .is_ok()
                })
            });
            if !valid {
                let number: Option<String> = row.try_get("Number").map_err(sqlx_error)?;
                let name: Option<String> = row.try_get("Name").map_err(sqlx_error)?;
                failures.insert(id, format!(
                    "{} - {} (playout {id}): missing or incompatible YAML boundary checkpoint; Refresh is unsafe. Use an explicit Reset to create compatible state.",
                    number.unwrap_or_default(), name.unwrap_or_default(),
                ));
            }
        }
        if failures.is_empty() {
            return Ok(HealthCheckResult::pass(self.title()));
        }
        let count = failures.len();
        Ok(HealthCheckResult::new(
            self.title(),
            HealthCheckStatus::Fail,
            failures.into_values().collect::<Vec<_>>().join("\n"),
            format!("{count} YAML state error(s)"),
        ))
    }
}
