//! Explicit output-channel caption delivery, stored with the channel UUID.

use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{
    channel_source_teletext_delivery_key, EntityId, SourceTeletextCaptionDelivery,
};
use sqlx::{SqliteConnection, SqlitePool};
use uuid::Uuid;

use crate::repositories::SqlxResultExt as _;

pub(crate) fn invalid() -> RepositoryError {
    RepositoryError::Validation {
        field: "source_teletext_delivery",
        message: "Invalid source teletext caption delivery".into(),
    }
}

/// Parse the canonical persisted enum; malformed values never become Burn.
/// # Errors
/// Rejects every value other than the literal integers `0` and `1`.
pub fn parse_source_teletext_delivery(value: &str) -> Result<SourceTeletextCaptionDelivery> {
    match value {
        "0" => Ok(SourceTeletextCaptionDelivery::Burn),
        "1" => Ok(SourceTeletextCaptionDelivery::DvbSubtitles),
        _ => Err(invalid()),
    }
}

pub(crate) async fn channel_uuid(connection: &mut SqliteConnection, id: EntityId) -> Result<Uuid> {
    let value: String = sqlx::query_scalar(r#"SELECT "UniqueId" FROM "Channel" WHERE "Id" = ?"#)
        .bind(id)
        .fetch_optional(connection)
        .await
        .repository()?
        .ok_or(RepositoryError::NotFound {
            kind: "channel",
            id,
        })?;
    Uuid::parse_str(&value).map_err(|_| invalid())
}

pub(crate) async fn load_for_uuid(
    connection: &mut SqliteConnection,
    channel: &Uuid,
) -> Result<SourceTeletextCaptionDelivery> {
    let row = sqlx::query(r#"SELECT "Value" FROM "ConfigElement" WHERE "Key" = ?"#)
        .bind(channel_source_teletext_delivery_key(channel))
        .fetch_optional(connection)
        .await
        .repository()?;
    match row {
        None => Ok(SourceTeletextCaptionDelivery::Burn),
        Some(row) => parse_source_teletext_delivery(
            &crate::repositories::common::decode_string_column(&row, "Value")?,
        ),
    }
}

/// Resolve delivery from the actual output channel, never its mirrored source.
/// Missing settings preserve the existing Burn behavior.
///
/// # Errors
/// Returns an error for absent channels, invalid stored values or storage failure.
pub async fn load_channel_source_teletext_delivery(
    pool: &SqlitePool,
    channel_id: EntityId,
) -> Result<SourceTeletextCaptionDelivery> {
    // A read-only transaction keeps the output UUID and its setting consistent.
    let mut tx = pool.begin().await.repository()?;
    let uuid = channel_uuid(&mut tx, channel_id).await?;
    let delivery = load_for_uuid(&mut tx, &uuid).await?;
    tx.commit().await.repository()?;
    Ok(delivery)
}
