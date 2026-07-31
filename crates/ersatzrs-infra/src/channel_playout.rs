//! ErsatzTV-compatible generated and mirror-channel playout resolution.

use chrono::{DateTime, Duration, Utc};
use ersatzrs_core::repositories::{RepositoryError, Result};
use ersatzrs_domain::{Channel, ChannelPlayoutSource, EntityId};
use sqlx::Row;

use crate::{
    db::DbPool,
    repositories::{
        common::{decode_enum_column, try_decode_signed_duration_text},
        SqlxResultExt as _,
    },
};

/// The source channel and signed time shift used to resolve one target channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChannelPlayoutContext {
    pub requested_channel_id: EntityId,
    pub source_channel_id: EntityId,
    pub offset: Duration,
}

impl ChannelPlayoutContext {
    /// Build the effective context from an already-loaded target channel.
    #[must_use]
    pub fn for_channel(channel: &Channel) -> Self {
        if channel.playout_source == ChannelPlayoutSource::Mirror {
            Self {
                requested_channel_id: channel.id,
                source_channel_id: channel.mirror_source_channel_id.unwrap_or(channel.id),
                offset: channel.playout_offset.unwrap_or_default(),
            }
        } else {
            Self {
                requested_channel_id: channel.id,
                source_channel_id: channel.id,
                offset: Duration::zero(),
            }
        }
    }

    /// Translate a target-channel wall-clock time to the source playout time.
    ///
    /// # Errors
    ///
    /// Returns a decode error when applying the offset would overflow.
    pub fn source_time(self, target_time: DateTime<Utc>) -> Result<DateTime<Utc>> {
        target_time
            .checked_sub_signed(self.offset)
            .ok_or_else(|| RepositoryError::Decode {
                kind: "channel playout offset",
                value: "source time overflows supported range".to_owned(),
            })
    }

    /// Project a source playout timestamp onto the target channel timeline.
    ///
    /// # Errors
    ///
    /// Returns a decode error when applying the offset would overflow.
    pub fn target_time(self, source_time: DateTime<Utc>) -> Result<DateTime<Utc>> {
        source_time
            .checked_add_signed(self.offset)
            .ok_or_else(|| RepositoryError::Decode {
                kind: "channel playout offset",
                value: "target time overflows supported range".to_owned(),
            })
    }
}

/// Load the effective playout context for a target channel.
///
/// # Errors
///
/// Returns a repository error when the channel row cannot be loaded or decoded.
pub async fn load_channel_playout_context(
    pool: &DbPool,
    channel_id: EntityId,
) -> Result<Option<ChannelPlayoutContext>> {
    let Some(row) = sqlx::query(
        r#"SELECT "Id", "PlayoutSource", "MirrorSourceChannelId", "PlayoutOffset"
           FROM "Channel"
           WHERE "Id" = ?"#,
    )
    .bind(channel_id)
    .fetch_optional(pool)
    .await
    .repository()?
    else {
        return Ok(None);
    };

    let playout_source = decode_enum_column::<ChannelPlayoutSource>(
        "channel playout source",
        &row,
        "PlayoutSource",
    )?;
    let offset = row
        .try_get::<Option<String>, _>("PlayoutOffset")
        .repository()?
        .as_deref()
        .map(try_decode_signed_duration_text)
        .transpose()?
        .unwrap_or_default();
    let source_channel_id = if playout_source == ChannelPlayoutSource::Mirror {
        row.try_get::<Option<EntityId>, _>("MirrorSourceChannelId")
            .repository()?
            .unwrap_or(channel_id)
    } else {
        channel_id
    };

    Ok(Some(ChannelPlayoutContext {
        requested_channel_id: channel_id,
        source_channel_id,
        offset: if playout_source == ChannelPlayoutSource::Mirror {
            offset
        } else {
            Duration::zero()
        },
    }))
}

#[cfg(test)]
mod tests {
    use super::ChannelPlayoutContext;
    use chrono::{Duration, TimeZone, Utc};

    #[test]
    fn delayed_mirror_queries_source_in_the_past_and_projects_back() {
        let wall_time = Utc.with_ymd_and_hms(2026, 7, 31, 12, 0, 0).unwrap();
        let context = ChannelPlayoutContext {
            requested_channel_id: 18,
            source_channel_id: 14,
            offset: Duration::hours(1),
        };

        let source_time = context.source_time(wall_time).unwrap();
        assert_eq!(source_time, wall_time - Duration::hours(1));
        assert_eq!(context.target_time(source_time).unwrap(), wall_time);
    }

    #[test]
    fn leading_mirror_uses_future_source_time() {
        let wall_time = Utc.with_ymd_and_hms(2026, 7, 31, 12, 0, 0).unwrap();
        let context = ChannelPlayoutContext {
            requested_channel_id: 18,
            source_channel_id: 14,
            offset: -Duration::hours(1),
        };

        assert_eq!(
            context.source_time(wall_time).unwrap(),
            wall_time + Duration::hours(1)
        );
    }
}
