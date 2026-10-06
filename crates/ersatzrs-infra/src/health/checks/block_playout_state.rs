use super::super::{
    async_trait, sqlx_error, Db, HealthCheck, HealthCheckResult, HealthCheckStatus, Pool, Result,
    Row,
};
use crate::repositories::common::{decode_datetime, encode_enum};
use ersatzrs_core::scheduling::{validate_block_history_checkpoint, SchedulingSmartShuffleState};
use ersatzrs_domain::{FillerKind, PlayoutScheduleKind};
use std::collections::BTreeMap;

pub(crate) struct BlockPlayoutStateHealthCheck {
    pool: Pool<Db>,
}

impl BlockPlayoutStateHealthCheck {
    pub(crate) fn new(pool: Pool<Db>) -> Self {
        Self { pool }
    }
}

#[derive(serde::Deserialize)]
struct Anchor {
    #[serde(default)]
    version: Option<u32>,
    #[serde(default, alias = "cooldown")]
    smart_shuffle: SchedulingSmartShuffleState,
    #[serde(default)]
    history_start: Option<chrono::DateTime<chrono::Utc>>,
}

fn ready(start: &str, context: Option<&str>, details: Option<&str>) -> bool {
    if let Some(context) = context {
        let Ok(anchor) = serde_json::from_str::<Anchor>(context) else {
            return false;
        };
        if anchor.version.is_some_and(|version| version != 1)
            || anchor.smart_shuffle.version != SchedulingSmartShuffleState::default().version
        {
            return false;
        }
        if let Some(floor) = anchor.history_start {
            let Ok(start) = decode_datetime("Block programme start", start) else {
                return false;
            };
            // Explicit Reset keeps the old programme but starts fresh state at
            // its end. Retained pre-Reset rows are not restoration checkpoints.
            if start < floor {
                return true;
            }
        }
    }
    details.is_some_and(|value| validate_block_history_checkpoint(value).is_ok())
}

#[async_trait]
impl HealthCheck for BlockPlayoutStateHealthCheck {
    fn title(&self) -> &'static str {
        "Block Playout State"
    }

    async fn check(&self) -> Result<HealthCheckResult> {
        let rows = sqlx::query(
            r#"
            SELECT p."Id", c."Number", c."Name", i."Start", h."Details", a."Context"
            FROM "Playout" p JOIN "Channel" c ON c."Id" = p."ChannelId"
            JOIN "PlayoutItem" i ON i."PlayoutId" = p."Id"
            LEFT JOIN "PlayoutAnchor" a ON a."PlayoutId" = p."Id"
            LEFT JOIN "PlayoutHistory" h ON h."PlayoutId" = p."Id"
                AND h."When" = i."Start" AND h."Finish" = i."Finish"
                AND h."Key" LIKE '%"BlockId"%'
            WHERE p."ScheduleKind" IN (?, 'Block')
                AND i."FillerKind" IN (?, ?, 'None', 'GuideMode')
            ORDER BY c."SortNumber", p."Id", i."Start"
        "#,
        )
        .bind(encode_enum(PlayoutScheduleKind::Block))
        .bind(encode_enum(FillerKind::None))
        .bind(encode_enum(FillerKind::GuideMode))
        .fetch_all(&self.pool)
        .await
        .map_err(sqlx_error)?;
        let mut failures = BTreeMap::new();
        for row in rows {
            let id: i64 = row.try_get("Id").map_err(sqlx_error)?;
            if failures.contains_key(&id) {
                continue;
            }
            let start: String = row.try_get("Start").map_err(sqlx_error)?;
            let context: Option<String> = row.try_get("Context").map_err(sqlx_error)?;
            let details: Option<String> = row.try_get("Details").map_err(sqlx_error)?;
            if !ready(&start, context.as_deref(), details.as_deref()) {
                let number: Option<String> = row.try_get("Number").map_err(sqlx_error)?;
                let name: Option<String> = row.try_get("Name").map_err(sqlx_error)?;
                failures.insert(id, format!("{} - {} (playout {id}): missing or incompatible Block boundary checkpoint; Refresh is unsafe. Use an explicit Reset to create compatible state.", number.unwrap_or_default(), name.unwrap_or_default()));
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
            format!("{count} Block state error(s)"),
        ))
    }
}
