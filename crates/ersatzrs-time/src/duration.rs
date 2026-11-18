use std::{fmt, iter::Sum, ops, time::Duration as StdDuration};

use jiff::SignedDuration;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::TimeError;

/// Signed elapsed time with nanosecond precision and an i64 millisecond bound.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Duration(SignedDuration);

/// Signed elapsed time, commonly distinguished from a process timeout.
pub type TimeDelta = Duration;

impl Duration {
    pub const ZERO: Self = Self(SignedDuration::ZERO);
    pub const MAX: Self = Self(SignedDuration::from_millis(i64::MAX));
    pub const MIN: Self = Self(SignedDuration::from_millis(-i64::MAX));

    #[must_use]
    pub fn new(seconds: i64, nanos: u32) -> Option<Self> {
        if nanos >= 1_000_000_000 {
            return None;
        }
        Self::bounded(SignedDuration::new(seconds, i32::try_from(nanos).ok()?))
    }

    #[must_use]
    pub const fn zero() -> Self {
        Self::ZERO
    }

    #[must_use]
    pub const fn try_seconds(value: i64) -> Option<Self> {
        if value < -i64::MAX / 1000 || value > i64::MAX / 1000 {
            None
        } else {
            Some(Self(SignedDuration::from_secs(value)))
        }
    }

    const fn scaled(value: i64, seconds: i64) -> Option<Self> {
        match value.checked_mul(seconds) {
            Some(value) => Self::try_seconds(value),
            None => None,
        }
    }

    #[must_use]
    pub const fn try_days(value: i64) -> Option<Self> {
        Self::scaled(value, 86_400)
    }

    #[must_use]
    pub const fn try_hours(value: i64) -> Option<Self> {
        Self::scaled(value, 3600)
    }

    #[must_use]
    pub const fn try_minutes(value: i64) -> Option<Self> {
        Self::scaled(value, 60)
    }

    /// Construct a duration in weeks.
    /// # Panics
    /// Panics when the duration is outside the signed millisecond range.
    #[must_use]
    pub const fn weeks(value: i64) -> Self {
        Self::scaled(value, 604_800).expect("duration is out of range")
    }

    /// Construct a duration in days.
    /// # Panics
    /// Panics when the duration is outside the signed millisecond range.
    #[must_use]
    pub const fn days(value: i64) -> Self {
        Self::try_days(value).expect("duration is out of range")
    }

    /// Construct a duration in hours.
    /// # Panics
    /// Panics when the duration is outside the signed millisecond range.
    #[must_use]
    pub const fn hours(value: i64) -> Self {
        Self::try_hours(value).expect("duration is out of range")
    }

    /// Construct a duration in minutes.
    /// # Panics
    /// Panics when the duration is outside the signed millisecond range.
    #[must_use]
    pub const fn minutes(value: i64) -> Self {
        Self::try_minutes(value).expect("duration is out of range")
    }

    /// Construct a duration in seconds.
    /// # Panics
    /// Panics when the duration is outside the signed millisecond range.
    #[must_use]
    pub const fn seconds(value: i64) -> Self {
        Self::try_seconds(value).expect("duration is out of range")
    }

    /// Construct a duration in milliseconds.
    /// # Panics
    /// Panics when the duration is outside the signed millisecond range.
    #[must_use]
    pub const fn milliseconds(value: i64) -> Self {
        assert!(value != i64::MIN, "duration is out of range");
        Self(SignedDuration::from_millis(value))
    }

    #[must_use]
    pub const fn microseconds(value: i64) -> Self {
        Self(SignedDuration::from_micros(value))
    }

    #[must_use]
    pub const fn nanoseconds(value: i64) -> Self {
        Self(SignedDuration::from_nanos(value))
    }

    #[must_use]
    pub const fn num_days(self) -> i64 {
        self.num_seconds() / 86_400
    }

    #[must_use]
    pub const fn num_hours(self) -> i64 {
        self.num_seconds() / 3600
    }

    #[must_use]
    pub const fn num_minutes(self) -> i64 {
        self.num_seconds() / 60
    }

    #[must_use]
    pub const fn num_seconds(self) -> i64 {
        self.0.as_secs()
    }

    #[must_use]
    pub fn num_milliseconds(self) -> i64 {
        self.num_seconds() * 1000 + i64::from(self.subsec_nanos() / 1_000_000)
    }

    #[must_use]
    pub fn num_nanoseconds(self) -> Option<i64> {
        self.0.as_nanos().try_into().ok()
    }

    #[must_use]
    pub const fn subsec_nanos(self) -> i32 {
        self.0.subsec_nanos()
    }

    #[must_use]
    pub const fn is_zero(self) -> bool {
        self.0.is_zero()
    }

    #[must_use]
    pub const fn abs(self) -> Self {
        Self(self.0.abs())
    }

    #[must_use]
    pub fn as_seconds_f64(self) -> f64 {
        self.0.as_secs_f64()
    }

    /// Convert from a monotonic clock or timeout duration.
    /// # Errors
    /// Rejects durations outside the application's signed range.
    pub fn from_std(value: StdDuration) -> Result<Self, TimeError> {
        Self::bounded(value.try_into().map_err(|_| TimeError)?).ok_or(TimeError)
    }

    /// Convert to a monotonic clock or timeout duration.
    /// # Errors
    /// Rejects negative values.
    pub fn to_std(self) -> Result<StdDuration, TimeError> {
        self.0.try_into().map_err(|_| TimeError)
    }

    #[must_use]
    pub fn checked_add(self, other: &Self) -> Option<Self> {
        Self::bounded(self.0.checked_add(other.0)?)
    }

    #[must_use]
    pub fn checked_sub(self, other: &Self) -> Option<Self> {
        Self::bounded(self.0.checked_sub(other.0)?)
    }

    #[must_use]
    pub fn checked_mul(self, factor: i32) -> Option<Self> {
        Self::bounded(self.0.checked_mul(factor)?)
    }

    #[must_use]
    pub fn checked_div(self, divisor: i32) -> Option<Self> {
        Self::bounded(self.0.checked_div(divisor)?)
    }

    pub(crate) fn bounded(value: SignedDuration) -> Option<Self> {
        (Self::MIN.0..=Self::MAX.0)
            .contains(&value)
            .then_some(Self(value))
    }

    pub(crate) const fn jiff(self) -> SignedDuration {
        self.0
    }
}

impl ops::Add for Duration {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.checked_add(&other).expect("duration is out of range")
    }
}

impl ops::Sub for Duration {
    type Output = Self;

    fn sub(self, other: Self) -> Self {
        self.checked_sub(&other).expect("duration is out of range")
    }
}

impl ops::AddAssign for Duration {
    fn add_assign(&mut self, other: Self) {
        *self = *self + other;
    }
}

impl ops::SubAssign for Duration {
    fn sub_assign(&mut self, other: Self) {
        *self = *self - other;
    }
}

impl ops::Neg for Duration {
    type Output = Self;

    fn neg(self) -> Self {
        Self(-self.0)
    }
}

impl ops::Mul<i32> for Duration {
    type Output = Self;

    fn mul(self, factor: i32) -> Self {
        self.checked_mul(factor).expect("duration is out of range")
    }
}

impl ops::Div<i32> for Duration {
    type Output = Self;

    fn div(self, divisor: i32) -> Self {
        self.checked_div(divisor)
            .expect("invalid duration division")
    }
}

impl Sum for Duration {
    fn sum<I: Iterator<Item = Self>>(values: I) -> Self {
        values.fold(Self::ZERO, ops::Add::add)
    }
}

impl fmt::Display for Duration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_zero() {
            return f.write_str("P0D");
        }
        let sign = if self.0.is_negative() { "-" } else { "" };
        let magnitude = self.abs();
        write!(f, "{sign}PT{}", magnitude.num_seconds())?;
        if magnitude.subsec_nanos() != 0 {
            let fraction = format!("{:09}", magnitude.subsec_nanos());
            write!(f, ".{}", fraction.trim_end_matches('0'))?;
        }
        f.write_str("S")
    }
}

// Persisted schedules use a signed seconds / positive nanos tuple.
impl Serialize for Duration {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let nanos = self.subsec_nanos();
        let seconds = self.num_seconds() - i64::from(nanos < 0);
        (seconds, nanos.rem_euclid(1_000_000_000)).serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Duration {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let (seconds, nanos) = <(i64, u32)>::deserialize(deserializer)?;
        Self::new(seconds, nanos).ok_or_else(|| serde::de::Error::custom(TimeError))
    }
}
