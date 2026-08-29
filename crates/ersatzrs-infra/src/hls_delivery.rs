use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::DbPool;

pub(crate) const CONFIG_KEY_HLS_DELIVERY_PREFIX: &str = "health.hls_delivery.";
pub(crate) const HLS_DELIVERY_RECENT_MINUTES: i64 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum HlsDeliveryState {
    UnderRun,
    Recovered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct HlsDeliveryObservation {
    #[serde(skip)]
    pub(crate) transitioned: bool,
    pub(crate) observed_at: DateTime<Utc>,
    pub(crate) state: HlsDeliveryState,
    pub(crate) last_under_run_at: DateTime<Utc>,
    pub(crate) wall_seconds: f64,
    pub(crate) media_seconds: f64,
    pub(crate) delivery_ratio_percent: f64,
    pub(crate) no_progress_seconds: f64,
}

#[derive(Debug, Error)]
pub(crate) enum HlsDeliveryPersistenceError {
    #[error("failed to serialize HLS delivery observation: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("failed to persist HLS delivery observation: {0}")]
    Database(#[from] sqlx::Error),
}

pub(crate) async fn persist_hls_delivery_observation(
    pool: &DbPool,
    channel_id: i64,
    observation: &HlsDeliveryObservation,
) -> Result<(), HlsDeliveryPersistenceError> {
    let key = format!("{CONFIG_KEY_HLS_DELIVERY_PREFIX}{channel_id}");
    let value = serde_json::to_string(observation)?;
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    Ok(())
}
