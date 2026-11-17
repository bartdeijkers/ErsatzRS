//! Jiff-backed time values for the application and add-on contracts.
//!
//! This crate retains the established serialized time formats and bounds at
//! compatibility boundaries. Jiff supplies Gregorian calendar calculations,
//! timezone resolution, formatting and signed elapsed durations.

mod calendar;
mod duration;
mod instant;
mod wire;

pub use calendar::{Datelike, Months, NaiveDate, NaiveDateTime, NaiveTime, Timelike, Weekday};
pub use duration::{Duration, TimeDelta};
pub use instant::{DateTime, FixedOffset, Local, LocalResult, Offset, TimeZone, Utc};
pub use wire::SecondsFormat;

/// A civil time or instant could not be represented in the requested format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("invalid or out-of-range date and time")]
pub struct TimeError;
