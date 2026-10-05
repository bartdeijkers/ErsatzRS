//! Shared value objects for bounded add-on list selection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Saved rolling selection; absent settings preserve legacy inclusion.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AddonListSelectionPolicy {
    pub age_days: Option<u32>,
    pub newest_count: Option<u32>,
    pub minimum_seconds: Option<u64>,
    pub maximum_seconds: Option<u64>,
    /// Any matching term accepts a title; matching is Unicode case-insensitive.
    pub include_terms: Vec<String>,
    /// Any matching term rejects a title.
    pub exclude_terms: Vec<String>,
}

/// Polling pause is independent of the saved cadence and selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListSelectionSettings {
    pub policy: Option<AddonListSelectionPolicy>,
    pub refresh_paused: bool,
}

/// Saved settings and the clock used for the last effective rolling selection.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListSelectionState {
    pub settings: AddonListSelectionSettings,
    pub evaluated_at: Option<DateTime<Utc>>,
    pub cutoff_at: Option<DateTime<Utc>>,
    /// Optional first-import review holds automatic publication until settings are saved.
    #[serde(default)]
    pub review_required: bool,
}

/// Cheap selection inputs, without provider calls or local media paths.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AddonListSelectionItem {
    pub id: i64,
    pub rank: i32,
    pub title: String,
    pub published_at: Option<DateTime<Utc>>,
    pub duration_seconds: Option<u64>,
    /// True pins an item; false excludes it; None follows automatic selection.
    pub manual: Option<bool>,
    pub legacy_included: bool,
}

/// Stable reason codes, translated only at the admin boundary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AddonListSelectionReason {
    Accepted,
    Pinned,
    Excluded,
    MissingDate,
    FutureDate,
    TooOld,
    MissingDuration,
    TooShort,
    TooLong,
    TitleNotIncluded,
    TitleExcluded,
    NewestLimit,
}

impl AddonListSelectionReason {
    /// Stable persisted code; contains no provider data.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Accepted => "accepted",
            Self::Pinned => "pinned",
            Self::Excluded => "excluded",
            Self::MissingDate => "missing_date",
            Self::FutureDate => "future_date",
            Self::TooOld => "too_old",
            Self::MissingDuration => "missing_duration",
            Self::TooShort => "too_short",
            Self::TooLong => "too_long",
            Self::TitleNotIncluded => "title_not_included",
            Self::TitleExcluded => "title_excluded",
            Self::NewestLimit => "newest_limit",
        }
    }
}

/// One decision; a rejection archives the item without deleting its artifacts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListSelectionDecision {
    pub item_id: i64,
    pub title: String,
    pub accepted: bool,
    pub reason: AddonListSelectionReason,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AddonListSelectionPreview {
    pub evaluated_at: DateTime<Utc>,
    pub cutoff_at: Option<DateTime<Utc>>,
    pub accepted_count: usize,
    pub decisions: Vec<AddonListSelectionDecision>,
}

impl AddonListSelectionPolicy {
    /// Reject unbounded or contradictory saved filters.
    ///
    /// # Errors
    /// Returns a bounded validation explanation for an invalid policy.
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.age_days.is_some_and(|n| n == 0 || n > 36_500)
            || self.newest_count.is_some_and(|n| n == 0 || n > 10_000)
            || self.minimum_seconds.is_some_and(|n| n > 604_800)
            || self.maximum_seconds.is_some_and(|n| n == 0 || n > 604_800)
            || self
                .minimum_seconds
                .zip(self.maximum_seconds)
                .is_some_and(|(a, b)| a > b)
        {
            return Err("invalid selection bounds");
        }
        for terms in [&self.include_terms, &self.exclude_terms] {
            if terms.len() > 32 || terms.iter().any(|t| t.trim().is_empty() || t.len() > 256) {
                return Err("selection terms must contain at most 32 nonempty values of 256 bytes");
            }
        }
        Ok(())
    }
}
