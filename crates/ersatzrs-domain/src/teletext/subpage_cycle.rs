//! Checked display rotation metadata, separate from semantic page contents.

use crate::{TeletextPageNumber, TELETEXT_ELEMENT_CYCLE_SECONDS};

/// Published dwell time for each subpage of one logical display page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TeletextSubpageCycle {
    page: TeletextPageNumber,
    seconds: u16,
}

/// Content-free rejection of invalid or unassociated display timing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum TeletextSubpageCycleError {
    #[error("teletext subpage cycle {seconds} seconds is outside the supported range")]
    Duration { seconds: u16 },
    #[error("teletext subpage cycle records exceed the existing display count")]
    Count,
    #[error("teletext subpage cycle repeats page {page}")]
    Duplicate { page: u16 },
    #[error("teletext subpage cycle refers to missing display page {page}")]
    MissingPage { page: u16 },
}

impl TeletextSubpageCycle {
    /// Validate the existing element cycle range; this grants no output authority.
    /// # Errors
    /// Rejects zero or a duration above the existing 600-second limit.
    pub fn new(page: TeletextPageNumber, seconds: u16) -> Result<Self, TeletextSubpageCycleError> {
        if !TELETEXT_ELEMENT_CYCLE_SECONDS.contains(&seconds) {
            return Err(TeletextSubpageCycleError::Duration { seconds });
        }
        Ok(Self { page, seconds })
    }

    #[must_use]
    pub const fn page(&self) -> TeletextPageNumber {
        self.page
    }

    #[must_use]
    pub const fn seconds(&self) -> u16 {
        self.seconds
    }
}
