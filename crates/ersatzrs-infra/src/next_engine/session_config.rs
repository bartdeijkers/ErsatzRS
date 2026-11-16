//! Persisted session controls from upstream `ChannelConfigConverter.ToNextSession`.

use ersatzrs_core::repositories::ConfigRepository;
use ersatzrs_domain::{
    ChannelIdleBehavior, CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT,
    CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT,
};

use crate::{db::DbPool, repositories::SqliteConfigRepository};

use super::{process::ProcessError, protocol::Session};

pub(super) async fn load(
    pool: &DbPool,
    idle: ChannelIdleBehavior,
) -> Result<Session, ProcessError> {
    let repository = SqliteConfigRepository(pool.clone());
    let seconds = if idle == ChannelIdleBehavior::KeepRunning {
        0
    } else {
        let seconds = setting(&repository, CONFIG_KEY_FFMPEG_SEGMENTER_TIMEOUT, 60).await?;
        u32::try_from(seconds)
            .map_err(|_| ProcessError::Preparation("invalid session idle timeout"))?
    };
    let count = setting(&repository, CONFIG_KEY_FFMPEG_INITIAL_SEGMENT_COUNT, 1).await?;
    let ready_segment_count = match count {
        4..=9 => u8::try_from(count).ok(),
        10..=i32::MAX => Some(10),
        _ => None,
    };
    Ok(Session {
        idle_timeout_seconds: Some(seconds),
        ready_segment_count,
    })
}

async fn setting(
    repository: &SqliteConfigRepository,
    key: &str,
    default: i32,
) -> Result<i32, ProcessError> {
    Ok(repository
        .get(key)
        .await
        .map_err(|_| ProcessError::Preparation("session settings read"))?
        .and_then(|value| value.parse().ok())
        .unwrap_or(default))
}
