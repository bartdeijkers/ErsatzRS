use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::db::DbPool;

pub(crate) const CONFIG_KEY_HLS_DELIVERY_PREFIX: &str = "health.hls_delivery.";
pub(crate) const CONFIG_KEY_HLS_DELIVERY_FALLBACK_PREFIX: &str = "hls.delivery_fallback.";
pub(crate) const HLS_DELIVERY_RECENT_MINUTES: i64 = 30;
pub(crate) const HLS_DELIVERY_FALLBACK_HOURS: i64 = 24;
const MAX_HLS_DELIVERY_FALLBACKS: usize = 256;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HlsDeliveryFallback {
    pub(crate) detected_at: DateTime<Utc>,
    pub(crate) verified_at: DateTime<Utc>,
    pub(crate) expires_at: DateTime<Utc>,
    pub(crate) max_video_height: u16,
    #[serde(default)]
    pub(crate) software_video_scope: Option<HlsDeliverySoftwareScope>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct HlsDeliverySoftwareScope {
    pub(crate) channel_id: i64,
    pub(crate) ffmpeg_profile_id: i64,
    pub(crate) profile_sha256: [u8; 32],
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

pub(crate) async fn load_hls_delivery_fallback(
    pool: &DbPool,
    media_item_id: i64,
    configured_software_scope: Option<HlsDeliverySoftwareScope>,
    now: DateTime<Utc>,
) -> Result<Option<HlsDeliveryFallback>, HlsDeliveryPersistenceError> {
    if let Some(configured_software_scope) = configured_software_scope {
        let scoped_key = hls_delivery_fallback_key(media_item_id, Some(configured_software_scope));
        if let Some(fallback) = load_hls_delivery_fallback_key(pool, &scoped_key, now).await? {
            if fallback.software_video_scope == Some(configured_software_scope) {
                return Ok(Some(fallback));
            }
            delete_hls_delivery_fallback_key(pool, &scoped_key).await?;
        }
    }

    let quality_key = hls_delivery_fallback_key(media_item_id, None);
    let fallback = load_hls_delivery_fallback_key(pool, &quality_key, now).await?;
    if fallback
        .as_ref()
        .is_some_and(|fallback| fallback.software_video_scope.is_some())
    {
        delete_hls_delivery_fallback_key(pool, &quality_key).await?;
        return Ok(None);
    }
    Ok(fallback)
}

async fn load_hls_delivery_fallback_key(
    pool: &DbPool,
    key: &str,
    now: DateTime<Utc>,
) -> Result<Option<HlsDeliveryFallback>, HlsDeliveryPersistenceError> {
    let value =
        sqlx::query_scalar::<_, String>(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .fetch_optional(pool)
            .await?;
    let Some(value) = value else {
        return Ok(None);
    };
    let fallback = serde_json::from_str::<HlsDeliveryFallback>(&value)?;
    if fallback.expires_at <= now {
        delete_hls_delivery_fallback_key(pool, key).await?;
        return Ok(None);
    }
    Ok(Some(fallback))
}

pub(crate) async fn persist_hls_delivery_fallback(
    pool: &DbPool,
    media_item_id: i64,
    detected_at: DateTime<Utc>,
    verified_at: DateTime<Utc>,
    max_video_height: u16,
    software_video_scope: Option<HlsDeliverySoftwareScope>,
) -> Result<(), HlsDeliveryPersistenceError> {
    let key = hls_delivery_fallback_key(media_item_id, software_video_scope);
    let fallback = HlsDeliveryFallback {
        detected_at,
        verified_at,
        expires_at: verified_at + Duration::hours(HLS_DELIVERY_FALLBACK_HOURS),
        max_video_height,
        software_video_scope,
    };
    let value = serde_json::to_string(&fallback)?;
    sqlx::query(r#"REPLACE INTO "ConfigElement" ("Key", "Value") VALUES (?, ?)"#)
        .bind(key)
        .bind(value)
        .execute(pool)
        .await?;
    prune_hls_delivery_fallbacks(pool, verified_at).await
}

pub(crate) async fn clear_hls_delivery_fallback(
    pool: &DbPool,
    media_item_id: i64,
    software_video_scope: Option<HlsDeliverySoftwareScope>,
) -> Result<(), HlsDeliveryPersistenceError> {
    let key = hls_delivery_fallback_key(media_item_id, software_video_scope);
    delete_hls_delivery_fallback_key(pool, &key).await
}

fn hls_delivery_fallback_key(
    media_item_id: i64,
    software_video_scope: Option<HlsDeliverySoftwareScope>,
) -> String {
    software_video_scope.map_or_else(
        || format!("{CONFIG_KEY_HLS_DELIVERY_FALLBACK_PREFIX}{media_item_id}"),
        |scope| {
            format!(
                "{CONFIG_KEY_HLS_DELIVERY_FALLBACK_PREFIX}{media_item_id}.channel.{}.profile.{}",
                scope.channel_id, scope.ffmpeg_profile_id
            )
        },
    )
}

async fn delete_hls_delivery_fallback_key(
    pool: &DbPool,
    key: &str,
) -> Result<(), HlsDeliveryPersistenceError> {
    sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(key)
        .execute(pool)
        .await?;
    Ok(())
}

async fn prune_hls_delivery_fallbacks(
    pool: &DbPool,
    now: DateTime<Utc>,
) -> Result<(), HlsDeliveryPersistenceError> {
    use sqlx::Row as _;

    let rows = sqlx::query(
        r#"SELECT "Key", "Value"
           FROM "ConfigElement"
           WHERE "Key" LIKE ?"#,
    )
    .bind(format!("{CONFIG_KEY_HLS_DELIVERY_FALLBACK_PREFIX}%"))
    .fetch_all(pool)
    .await?;
    let mut retained = Vec::new();
    let mut delete = Vec::new();
    for row in rows {
        let key = row.try_get::<String, _>("Key")?;
        let value = row.try_get::<String, _>("Value")?;
        match serde_json::from_str::<HlsDeliveryFallback>(&value) {
            Ok(fallback) if fallback.expires_at > now => {
                retained.push((fallback.expires_at, key));
            }
            Ok(_) | Err(_) => delete.push(key),
        }
    }
    retained.sort_unstable_by_key(|entry| std::cmp::Reverse(entry.0));
    delete.extend(
        retained
            .into_iter()
            .skip(MAX_HLS_DELIVERY_FALLBACKS)
            .map(|(_, key)| key),
    );
    for key in delete {
        sqlx::query(r#"DELETE FROM "ConfigElement" WHERE "Key" = ?"#)
            .bind(key)
            .execute(pool)
            .await?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use sqlx::sqlite::SqlitePoolOptions;

    use super::*;

    const fn software_scope(
        channel_id: i64,
        ffmpeg_profile_id: i64,
        signature_byte: u8,
    ) -> HlsDeliverySoftwareScope {
        HlsDeliverySoftwareScope {
            channel_id,
            ffmpeg_profile_id,
            profile_sha256: [signature_byte; 32],
        }
    }

    async fn test_pool() -> DbPool {
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .expect("connect in-memory sqlite");
        sqlx::query(
            r#"CREATE TABLE "ConfigElement" (
                "Key" TEXT NOT NULL PRIMARY KEY,
                "Value" TEXT NOT NULL
            )"#,
        )
        .execute(&pool)
        .await
        .expect("create ConfigElement");
        pool
    }

    #[tokio::test]
    async fn verified_fallback_expires_after_twenty_four_hours() {
        let pool = test_pool().await;
        let detected_at = Utc::now();
        let verified_at = detected_at + Duration::minutes(1);
        let scope = software_scope(5, 7, 1);
        persist_hls_delivery_fallback(&pool, 42, detected_at, verified_at, 720, Some(scope))
            .await
            .expect("persist fallback");

        let active = load_hls_delivery_fallback(
            &pool,
            42,
            Some(scope),
            verified_at + Duration::hours(HLS_DELIVERY_FALLBACK_HOURS) - Duration::seconds(1),
        )
        .await
        .expect("load active fallback")
        .expect("fallback should remain active");
        assert_eq!(active.detected_at, detected_at);
        assert_eq!(active.max_video_height, 720);
        assert_eq!(active.software_video_scope, Some(scope));

        let expired = load_hls_delivery_fallback(
            &pool,
            42,
            Some(scope),
            verified_at + Duration::hours(HLS_DELIVERY_FALLBACK_HOURS),
        )
        .await
        .expect("load expired fallback");
        assert!(expired.is_none());
    }

    #[tokio::test]
    async fn fallback_cache_is_bounded_to_most_recent_entries() {
        let pool = test_pool().await;
        let now = Utc::now();
        let max_fallbacks =
            i64::try_from(MAX_HLS_DELIVERY_FALLBACKS).expect("fallback cache bound fits in i64");
        for media_item_id in 0..=max_fallbacks {
            persist_hls_delivery_fallback(
                &pool,
                media_item_id,
                now,
                now + Duration::seconds(media_item_id),
                720,
                None,
            )
            .await
            .expect("persist fallback");
        }

        let count = sqlx::query_scalar::<_, i64>(
            r#"SELECT COUNT(*) FROM "ConfigElement" WHERE "Key" LIKE ?"#,
        )
        .bind(format!("{CONFIG_KEY_HLS_DELIVERY_FALLBACK_PREFIX}%"))
        .fetch_one(&pool)
        .await
        .expect("count fallbacks");
        assert_eq!(count, max_fallbacks);
        assert!(
            load_hls_delivery_fallback(&pool, 0, Some(software_scope(5, 7, 1)), now)
                .await
                .expect("load oldest fallback")
                .is_none()
        );
    }

    #[test]
    fn legacy_fallback_json_defaults_to_hardware_video() {
        let fallback = serde_json::from_str::<HlsDeliveryFallback>(
            r#"{"detected_at":"2026-08-29T12:00:00Z","verified_at":"2026-08-29T12:01:00Z","expires_at":"2026-08-30T12:01:00Z","max_video_height":720}"#,
        )
        .expect("decode fallback written before the software-video field existed");

        assert!(fallback.software_video_scope.is_none());
    }

    #[tokio::test]
    async fn software_fallback_isolated_by_channel_and_profile() {
        let pool = test_pool().await;
        let now = Utc::now();
        let stored = software_scope(5, 7, 1);
        persist_hls_delivery_fallback(&pool, 42, now, now, 1080, Some(stored))
            .await
            .expect("persist software fallback");

        assert!(load_hls_delivery_fallback(&pool, 42, Some(stored), now)
            .await
            .expect("load matching fallback")
            .is_some());
        assert!(
            load_hls_delivery_fallback(&pool, 42, Some(software_scope(6, 7, 1)), now)
                .await
                .expect("load fallback for another channel")
                .is_none()
        );
        assert!(
            load_hls_delivery_fallback(&pool, 42, Some(software_scope(5, 8, 1)), now)
                .await
                .expect("load fallback for another profile")
                .is_none()
        );
    }

    #[tokio::test]
    async fn software_fallback_is_invalidated_after_profile_edit() {
        let pool = test_pool().await;
        let now = Utc::now();
        let stored = software_scope(5, 7, 1);
        let edited = software_scope(5, 7, 2);
        persist_hls_delivery_fallback(&pool, 42, now, now, 1080, Some(stored))
            .await
            .expect("persist software fallback");

        assert!(load_hls_delivery_fallback(&pool, 42, Some(edited), now)
            .await
            .expect("load edited profile fallback")
            .is_none());
        assert!(load_hls_delivery_fallback(&pool, 42, Some(stored), now)
            .await
            .expect("load invalidated profile fallback")
            .is_none());
    }

    #[tokio::test]
    async fn quality_fallback_remains_available_across_profiles() {
        let pool = test_pool().await;
        let now = Utc::now();
        persist_hls_delivery_fallback(&pool, 42, now, now, 720, None)
            .await
            .expect("persist quality fallback");

        let fallback = load_hls_delivery_fallback(&pool, 42, Some(software_scope(8, 9, 3)), now)
            .await
            .expect("load quality fallback")
            .expect("quality fallback should remain media scoped");
        assert_eq!(fallback.max_video_height, 720);
        assert!(fallback.software_video_scope.is_none());
    }
}
