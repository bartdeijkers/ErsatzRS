use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckError, HealthCheckResult,
    HealthCheckStatus, Pool, Result, Row,
};
use crate::repositories::common::{decode_datetime, decode_enum_column, encode_enum};
use crate::repositories::{
    SqlitePlayoutRepository, SqliteTeletextGenerationRepository, SqliteTeletextProfileRepository,
};
use chrono::{DateTime, Utc};
use ersatzrs_core::repositories::{
    PlayoutRepository as _, TeletextGenerationMutation, TeletextGenerationRepository as _,
    TeletextProfileMutation, TeletextProfileRepository as _,
};
use ersatzrs_core::scheduling::validate_scripted_checkpoint;
use ersatzrs_domain::{PlayoutScheduleKind, PlayoutScriptedCheckpoint, TeletextOverrideState};
use serde::{de::IgnoredAny, Deserialize, Deserializer};
use std::{
    collections::BTreeMap,
    time::{Duration, Instant},
};

// Match the host's bounded policy input, not the unrelated page-wire budget.
const TELETEXT_POLICY_BYTES: i64 = 65_536;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeletextPolicyProjection {
    version: u32,
    #[serde(deserialize_with = "policy_object")]
    design: TeletextDesignProjection,
    #[serde(rename = "recovery_timeout_ms")]
    _recovery_timeout_ms: IgnoredAny,
    #[serde(rename = "input")]
    _input: IgnoredAny,
    #[serde(rename = "validation")]
    _validation: IgnoredAny,
    #[serde(rename = "composition")]
    _composition: IgnoredAny,
    #[serde(rename = "encoding")]
    _encoding: IgnoredAny,
    #[serde(rename = "source")]
    _source: IgnoredAny,
    #[serde(default, rename = "live_updates")]
    _live_updates: Option<IgnoredAny>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TeletextDesignProjection {
    max_serialized_bytes: usize,
    #[serde(rename = "max_documents")]
    _max_documents: IgnoredAny,
    #[serde(rename = "max_response_bytes")]
    _max_response_bytes: IgnoredAny,
    #[serde(rename = "max_context_bytes")]
    _max_context_bytes: IgnoredAny,
    #[serde(rename = "max_context_field_bytes")]
    _max_context_field_bytes: IgnoredAny,
    #[serde(rename = "max_resource_transmissions")]
    _max_resource_transmissions: IgnoredAny,
}

fn policy_object<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> std::result::Result<T, D::Error> {
    struct Object<T>(std::marker::PhantomData<T>);
    impl<'de, T: Deserialize<'de>> serde::de::Visitor<'de> for Object<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("an object")
        }

        fn visit_map<M: serde::de::MapAccess<'de>>(
            self,
            map: M,
        ) -> std::result::Result<T, M::Error> {
            T::deserialize(serde::de::value::MapAccessDeserializer::new(map))
        }
    }
    deserializer.deserialize_map(Object(std::marker::PhantomData))
}

fn document_limit(value: &str) -> Option<usize> {
    let mut deserializer = serde_json::Deserializer::from_str(value);
    let policy: TeletextPolicyProjection = policy_object(&mut deserializer).ok()?;
    deserializer.end().ok()?;
    let maximum = policy.design.max_serialized_bytes;
    // This projection establishes only the explicit document bound. The host
    // still owns complete policy validation and native/stream readiness.
    (matches!(policy.version, 1 | 2)
        && maximum > 0
        && isize::try_from(maximum).is_ok()
        && i64::try_from(maximum).is_ok())
    .then_some(maximum)
}

pub(crate) struct ScriptedPlayoutStateHealthCheck {
    pool: Pool<Db>,
}

impl ScriptedPlayoutStateHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }

    async fn teletext_document_limit(&self) -> Result<Option<usize>> {
        let value: Option<Option<String>> = sqlx::query_scalar(
            r#"SELECT CASE WHEN TYPEOF("Value") = 'text'
                AND LENGTH(CAST("Value" AS BLOB)) <= ? THEN "Value" END
               FROM "ConfigElement" WHERE "Key" = 'teletext.output.policy'"#,
        )
        .bind(TELETEXT_POLICY_BYTES)
        .fetch_optional(&self.pool)
        .await
        .map_err(sqlx_error)?;
        Ok(value.flatten().and_then(|value| document_limit(&value)))
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

    async fn teletext_failures(&self, when: DateTime<Utc>) -> Result<Vec<String>> {
        // Actual owning output channel only; mirror source identities never
        // grant override state. Include retained Scripted intervals even when
        // the operator has changed the future scheduler kind.
        let channels: Vec<i64> = sqlx::query_scalar(
            r#"SELECT DISTINCT p."ChannelId"
            FROM "PlayoutTeletextInterval" i JOIN "Playout" p ON p."Id" = i."PlayoutId"
            WHERE i."Start" <= ? AND i."Finish" > ? ORDER BY p."ChannelId""#,
        )
        .bind(crate::repositories::common::encode_datetime(when))
        .bind(crate::repositories::common::encode_datetime(when))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let mut failures = Vec::new();
        for channel in channels {
            // Reuse SQLite's existing read/busy bound once for the complete
            // channel projection. Later configuration and confirmation reads
            // never renew it, including the stored-original association check.
            let deadline = Instant::now()
                + Duration::from_millis(u64::from(crate::db::SQLITE_BUSY_TIMEOUT_MS));
            let reason = tokio::time::timeout_at(
                tokio::time::Instant::from_std(deadline),
                self.teletext_channel_failure(channel, when, deadline),
            )
            .await
            .unwrap_or(Some("active override is unavailable"));
            if let Some(reason) = reason {
                failures.push(format!("Scripted teletext interval for channel {channel}: {reason}; the published timeline is retained."));
            }
        }
        Ok(failures)
    }

    async fn teletext_channel_failure(
        &self,
        channel: i64,
        when: DateTime<Utc>,
        deadline: Instant,
    ) -> Option<&'static str> {
        const UNAVAILABLE: &str = "active override is unavailable";
        const PENDING: &str = "scoped native generation is not available";
        let Ok(intervals) = SqlitePlayoutRepository(self.pool.clone())
            .published_teletext_intervals_at(channel, when)
            .await
        else {
            return Some(UNAVAILABLE);
        };
        if !intervals
            .iter()
            .any(|interval| matches!(interval.state, TeletextOverrideState::Enabled(_)))
        {
            // Off is safe without healthy profiles/packages or an output policy.
            return None;
        }
        let maximum = match self.teletext_document_limit().await {
            Ok(Some(maximum)) => maximum,
            Ok(None) => return Some(PENDING),
            Err(_) => return Some(UNAVAILABLE),
        };
        let Ok(TeletextProfileMutation::Saved(selection)) =
            SqliteTeletextProfileRepository(self.pool.clone())
                .effective_channel_snapshot(channel, when)
                .await
        else {
            return Some(UNAVAILABLE);
        };
        let generations = SqliteTeletextGenerationRepository(self.pool.clone());
        let generation = match generations.current_effective(channel).await {
            Ok(Some(generation)) => generation,
            Ok(None) => return Some(PENDING),
            Err(_) => return Some(UNAVAILABLE),
        };
        match generations
            .confirm_current_effective(&generation, &selection, maximum, deadline)
            .await
        {
            // This verifies durable pointer/original/current-scope association,
            // not native artifact reconstruction, stream admission or sink use.
            Ok(TeletextGenerationMutation::Accepted(())) => None,
            Ok(TeletextGenerationMutation::Rejected(_)) => Some(PENDING),
            Err(_) => Some(UNAVAILABLE),
        }
    }
}

impl ScriptedPlayoutStateHealthCheck {
    async fn check_at(&self, when: DateTime<Utc>) -> Result<HealthCheckResult> {
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
        failures.extend(self.teletext_failures(when).await?);
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

#[async_trait]
impl HealthCheck for ScriptedPlayoutStateHealthCheck {
    fn title(&self) -> &'static str {
        "Scripted Playout State"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        self.check_at(Utc::now()).await
    }
}

#[cfg(test)]
#[path = "scripted_playout_teletext_tests.rs"]
mod teletext_tests;
