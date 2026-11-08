use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::PublicationError;

/// Explicit opt-in wire contract; ordinary upstream folder scanning cannot read it.
pub const INDEX_VERSION: &str = "ersatzrs.playout-index.v1";
/// Atomic index inside the channel's separately owned indexed directory.
pub const INDEX_FILENAME: &str = "index.json";
/// Readers hold a shared lease through index and chunk reads; reclamation is exclusive.
pub const READER_LOCK_FILENAME: &str = ".index-readers.lock";
pub(super) const MAX_DAYS: usize = 366;
pub(super) const MAX_DAY_BYTES: usize = 32 * 1024 * 1024;
pub(super) const MAX_TOTAL_BYTES: usize = 256 * 1024 * 1024;
pub(super) const MAX_INDEX_BYTES: usize = 128 * 1024;

/// One complete visible generation. Files absent from this index are invisible.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PlayoutIndex {
    pub version: String,
    pub days: Vec<DayReference>,
}

/// Half-open civil-day bounds plus an immutable content identity.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct DayReference {
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub file: String,
    pub sha256: String,
}

impl PlayoutIndex {
    /// Validate bounded, ordered, non-overlapping references without opening files.
    ///
    /// # Errors
    /// Returns a redacted error for an unsupported, excessive or unsafe index.
    pub fn validate(&self) -> Result<(), PublicationError> {
        if self.version != INDEX_VERSION || self.days.len() > MAX_DAYS {
            return Err(PublicationError::InvalidIndex);
        }
        for day in &self.days {
            if day.start >= day.finish
                || day.sha256.len() != 64
                || !day
                    .sha256
                    .bytes()
                    .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
                || day.file != format!("day-{}.json", day.sha256)
            {
                return Err(PublicationError::InvalidIndex);
            }
        }
        if self
            .days
            .windows(2)
            .any(|days| days[0].finish > days[1].start)
        {
            return Err(PublicationError::InvalidIndex);
        }
        Ok(())
    }
}
