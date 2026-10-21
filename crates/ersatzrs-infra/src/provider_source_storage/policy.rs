//! Dedicated source-spool policy; unrelated add-on storage limits do not apply.
use ersatzrs_core::repositories::ConfigRepository as _;

use super::StorageError;
use crate::{db::DbPool, repositories::SqliteConfigRepository};

pub const TOTAL_BYTES_KEY: &str = "teletext.source_storage.total_bytes";
pub const MAXIMUM_SOURCE_BYTES_KEY: &str = "teletext.source_storage.maximum_source_bytes";
pub const MINIMUM_FREE_BYTES_KEY: &str = "teletext.source_storage.minimum_free_bytes";
const GIB: u64 = 1024 * 1024 * 1024;

/// Admission limits for complete provider sources and all their retained copies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Policy {
    pub total_bytes: u64,
    pub maximum_source_bytes: u64,
    pub minimum_free_bytes: u64,
}
impl Default for Policy {
    fn default() -> Self {
        Self {
            total_bytes: 20 * GIB,
            maximum_source_bytes: 8 * GIB,
            minimum_free_bytes: 2 * GIB,
        }
    }
}
impl Policy {
    /// Read current typed configuration; changes govern subsequent admissions.
    /// # Errors
    /// Rejects unavailable configuration, malformed limits and source > total.
    pub async fn load(pool: &DbPool) -> Result<Self, StorageError> {
        let configuration = SqliteConfigRepository(pool.clone());
        let defaults = Self::default();
        let value = Self {
            total_bytes: read(&configuration, TOTAL_BYTES_KEY, defaults.total_bytes).await?,
            maximum_source_bytes: read(
                &configuration,
                MAXIMUM_SOURCE_BYTES_KEY,
                defaults.maximum_source_bytes,
            )
            .await?,
            minimum_free_bytes: read(
                &configuration,
                MINIMUM_FREE_BYTES_KEY,
                defaults.minimum_free_bytes,
            )
            .await?,
        };
        value.validate()?;
        Ok(value)
    }
    pub(super) fn validate(self) -> Result<(), StorageError> {
        if self.total_bytes == 0
            || self.maximum_source_bytes == 0
            || self.maximum_source_bytes > self.total_bytes
        {
            return Err(StorageError::Policy);
        }
        Ok(())
    }
}
async fn read(
    configuration: &SqliteConfigRepository,
    key: &str,
    default: u64,
) -> Result<u64, StorageError> {
    let Some(value) = configuration
        .get(key)
        .await
        .map_err(|_| StorageError::Unavailable)?
    else {
        return Ok(default);
    };
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(StorageError::Policy);
    }
    value.parse().map_err(|_| StorageError::Policy)
}
