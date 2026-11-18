use std::{
    cmp::Ordering,
    fmt,
    hash::{Hash, Hasher},
    marker::PhantomData,
    ops,
    str::FromStr,
};

use jiff::{
    tz::{self, AmbiguousOffset},
    Timestamp,
};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{
    calendar::BILLION, Datelike, Duration, Months, NaiveDate, NaiveDateTime, NaiveTime,
    SecondsFormat, TimeError, Timelike, Weekday,
};

/// The UTC timezone.
#[derive(Clone, Copy, Debug, Default)]
pub struct Utc;

/// The host's local timezone.
#[derive(Clone, Copy, Debug, Default)]
pub struct Local;

/// A fixed offset in seconds east of UTC, bounded to less than 24 hours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct FixedOffset(i32);

impl FixedOffset {
    #[must_use]
    pub const fn east_opt(seconds: i32) -> Option<Self> {
        if seconds > -86_400 && seconds < 86_400 {
            Some(Self(seconds))
        } else {
            None
        }
    }

    #[must_use]
    pub const fn west_opt(seconds: i32) -> Option<Self> {
        match seconds.checked_neg() {
            Some(east) => Self::east_opt(east),
            None => None,
        }
    }

    #[must_use]
    pub const fn local_minus_utc(self) -> i32 {
        self.0
    }

    pub(crate) fn jiff(self) -> tz::Offset {
        tz::Offset::from_seconds(self.0).expect("valid offset")
    }
}

impl fmt::Display for FixedOffset {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let sign = if self.0 < 0 { '-' } else { '+' };
        let seconds = self.0.unsigned_abs();
        write!(f, "{sign}{:02}:{:02}", seconds / 3600, (seconds / 60) % 60)?;
        if !seconds.is_multiple_of(60) {
            write!(f, ":{:02}", seconds % 60)?;
        }
        Ok(())
    }
}

/// The fixed offset observed at a particular instant.
pub trait Offset {
    fn fix(&self) -> FixedOffset;
}

impl Offset for FixedOffset {
    fn fix(&self) -> FixedOffset {
        *self
    }
}

/// Result of converting a civil time in a zone that can skip or repeat times.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalResult<T> {
    None,
    Single(T),
    Ambiguous(T, T),
}

impl<T> LocalResult<T> {
    pub fn single(self) -> Option<T> {
        match self {
            Self::Single(value) => Some(value),
            _ => None,
        }
    }

    pub fn earliest(self) -> Option<T> {
        match self {
            Self::None => None,
            Self::Single(value) | Self::Ambiguous(value, _) => Some(value),
        }
    }

    pub fn latest(self) -> Option<T> {
        match self {
            Self::None => None,
            Self::Single(value) | Self::Ambiguous(_, value) => Some(value),
        }
    }

    /// Return the single resolved instant.
    /// # Panics
    /// Panics when the time is ambiguous or does not exist.
    pub fn unwrap(self) -> T {
        self.single().expect("expected one unambiguous local time")
    }
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::Utc {}
    impl Sealed for super::FixedOffset {}
    impl Sealed for super::Local {}
}

/// Timezone used to interpret civil input and display an instant.
pub trait TimeZone: sealed::Sealed + Copy + 'static {
    type Offset: Offset;
    fn offset_at(&self, utc: NaiveDateTime) -> FixedOffset;
    fn zone_at(offset: FixedOffset) -> Self;
    fn offsets_for(&self, local: NaiveDateTime) -> LocalResult<FixedOffset>;

    fn resolve_local(&self, local: &NaiveDateTime) -> LocalResult<DateTime<Self>> {
        let resolve = |offset| {
            shift(*local, -FixedOffset::local_minus_utc(offset))
                .map(|utc| DateTime::at(utc, offset))
        };
        match self.offsets_for(*local) {
            LocalResult::Single(offset) => {
                resolve(offset).map_or(LocalResult::None, LocalResult::Single)
            }
            LocalResult::Ambiguous(first, second) => match (resolve(first), resolve(second)) {
                (Some(first), Some(second)) => {
                    LocalResult::Ambiguous(first.min(second), first.max(second))
                }
                _ => LocalResult::None,
            },
            LocalResult::None => LocalResult::None,
        }
    }

    fn resolve_utc(&self, utc: &NaiveDateTime) -> DateTime<Self> {
        DateTime::at(*utc, self.offset_at(*utc))
    }

    fn timestamp_opt(&self, seconds: i64, nanos: u32) -> LocalResult<DateTime<Self>> {
        match NaiveDateTime::from_epoch_seconds(seconds, nanos) {
            Some(value) => LocalResult::Single(self.resolve_utc(&value)),
            None => LocalResult::None,
        }
    }

    fn with_ymd_and_hms(
        &self,
        year: i32,
        month: u32,
        day: u32,
        hour: u32,
        minute: u32,
        second: u32,
    ) -> LocalResult<DateTime<Self>> {
        let local = NaiveDate::from_ymd_opt(year, month, day)
            .and_then(|date| date.and_hms_opt(hour, minute, second));
        local.map_or(LocalResult::None, |local| self.resolve_local(&local))
    }
}

impl TimeZone for Utc {
    type Offset = FixedOffset;
    fn offset_at(&self, _: NaiveDateTime) -> FixedOffset {
        FixedOffset(0)
    }
    fn zone_at(_: FixedOffset) -> Self {
        Self
    }
    fn offsets_for(&self, _: NaiveDateTime) -> LocalResult<FixedOffset> {
        LocalResult::Single(FixedOffset(0))
    }
}

impl TimeZone for FixedOffset {
    type Offset = Self;
    fn offset_at(&self, _: NaiveDateTime) -> FixedOffset {
        *self
    }
    fn zone_at(offset: FixedOffset) -> Self {
        offset
    }
    fn offsets_for(&self, _: NaiveDateTime) -> LocalResult<FixedOffset> {
        LocalResult::Single(*self)
    }
}

impl TimeZone for Local {
    type Offset = FixedOffset;
    fn offset_at(&self, utc: NaiveDateTime) -> FixedOffset {
        Timestamp::from_second(utc.epoch_seconds())
            .ok()
            .and_then(|ts| FixedOffset::east_opt(tz::TimeZone::system().to_offset(ts).seconds()))
            .unwrap_or_default()
    }

    fn zone_at(_: FixedOffset) -> Self {
        Self
    }
    fn offsets_for(&self, local: NaiveDateTime) -> LocalResult<FixedOffset> {
        let (_, cycle) = local.date().civil_cycle();
        if cycle != 0 {
            return LocalResult::None;
        }
        let convert = |offset: tz::Offset| FixedOffset::east_opt(offset.seconds());
        match tz::TimeZone::system()
            .to_ambiguous_timestamp(local.civil())
            .offset()
        {
            AmbiguousOffset::Unambiguous { offset } => {
                convert(offset).map_or(LocalResult::None, LocalResult::Single)
            }
            AmbiguousOffset::Fold { before, after } => match (convert(before), convert(after)) {
                (Some(before), Some(after)) => LocalResult::Ambiguous(before, after),
                _ => LocalResult::None,
            },
            AmbiguousOffset::Gap { .. } => LocalResult::None,
        }
    }
}

impl Utc {
    #[must_use]
    pub fn now() -> DateTime<Self> {
        DateTime::from(Timestamp::now())
    }
}

impl From<Timestamp> for DateTime<Utc> {
    fn from(ts: Timestamp) -> Self {
        let seconds = ts.as_second();
        let nanos = ts.subsec_nanosecond();
        let epoch = seconds - i64::from(nanos < 0);
        let fraction = u32::try_from(nanos.rem_euclid(1_000_000_000)).expect("valid fraction");
        Self::from_timestamp(epoch, fraction).expect("Jiff timestamps fit application bounds")
    }
}

impl Local {
    #[must_use]
    pub fn now() -> DateTime<Self> {
        Utc::now().with_timezone(&Self)
    }
}

/// A UTC instant with the offset used to present it.
///
/// Comparison, hashing and subtraction use UTC. Formatting and calendar field
/// access use the observed offset.
#[derive(Clone, Copy)]
pub struct DateTime<Tz: TimeZone> {
    utc: NaiveDateTime,
    offset: FixedOffset,
    zone: PhantomData<Tz>,
}

impl<Tz: TimeZone> DateTime<Tz> {
    const fn at(utc: NaiveDateTime, offset: FixedOffset) -> Self {
        Self {
            utc,
            offset,
            zone: PhantomData,
        }
    }

    #[must_use]
    pub fn timezone(&self) -> Tz {
        Tz::zone_at(self.offset)
    }

    #[must_use]
    pub const fn offset(&self) -> &FixedOffset {
        &self.offset
    }

    #[must_use]
    pub const fn naive_utc(&self) -> NaiveDateTime {
        self.utc
    }

    /// Return the civil clock reading with the observed offset.
    /// # Panics
    /// Panics if an offset shifts the reading outside the supported civil range.
    #[must_use]
    pub fn naive_local(&self) -> NaiveDateTime {
        shift(self.utc, self.offset.0).expect("offset maps to a representable civil time")
    }

    #[must_use]
    pub fn date_naive(&self) -> NaiveDate {
        self.naive_local().date()
    }

    #[must_use]
    pub fn time(&self) -> NaiveTime {
        self.naive_local().time()
    }

    pub fn with_timezone<Zone: TimeZone>(&self, zone: &Zone) -> DateTime<Zone> {
        zone.resolve_utc(&self.utc)
    }

    #[must_use]
    pub const fn to_utc(&self) -> DateTime<Utc> {
        self.utc.and_utc()
    }

    #[must_use]
    pub const fn timestamp(&self) -> i64 {
        self.utc.epoch_seconds()
    }

    #[must_use]
    pub fn timestamp_subsec_nanos(&self) -> u32 {
        self.utc.nanosecond()
    }

    #[must_use]
    pub fn timestamp_subsec_millis(&self) -> u32 {
        self.timestamp_subsec_nanos() / 1_000_000
    }

    #[must_use]
    pub fn timestamp_millis(&self) -> i64 {
        self.timestamp() * 1000 + i64::from(self.timestamp_subsec_millis())
    }

    #[must_use]
    pub fn timestamp_nanos_opt(&self) -> Option<i64> {
        let nanos = i128::from(self.timestamp()) * i128::from(BILLION)
            + i128::from(self.timestamp_subsec_nanos());
        nanos.try_into().ok()
    }

    #[must_use]
    pub fn format(&self, format: &str) -> impl fmt::Display {
        self.naive_local()
            .formatted(format, Some(self.offset.jiff()))
    }

    #[must_use]
    pub fn to_rfc3339(&self) -> String {
        self.to_rfc3339_opts(SecondsFormat::AutoSi, false)
    }

    #[must_use]
    pub fn to_rfc3339_opts(&self, seconds: SecondsFormat, use_z: bool) -> String {
        crate::wire::Rfc3339::new(*self, seconds, use_z).to_string()
    }

    #[must_use]
    pub fn signed_duration_since<Zone: TimeZone>(self, other: DateTime<Zone>) -> Duration {
        self.utc.signed_duration_since(other.utc)
    }

    #[must_use]
    pub fn checked_add_signed(self, duration: Duration) -> Option<Self> {
        let utc = self.utc.checked_add_signed(duration)?;
        Some(Tz::zone_at(self.offset).resolve_utc(&utc))
    }

    #[must_use]
    pub fn checked_sub_signed(self, duration: Duration) -> Option<Self> {
        self.checked_add_signed(-duration)
    }

    #[must_use]
    pub fn checked_add_months(self, months: Months) -> Option<Self> {
        let local = self.naive_local().checked_add_months(months)?;
        Tz::zone_at(self.offset).resolve_local(&local).single()
    }

    #[must_use]
    pub fn checked_sub_months(self, months: Months) -> Option<Self> {
        let local = self.naive_local().checked_sub_months(months)?;
        Tz::zone_at(self.offset).resolve_local(&local).single()
    }

    fn change_local(
        self,
        adjust: impl FnOnce(NaiveDateTime) -> Option<NaiveDateTime>,
    ) -> Option<Self> {
        let local = adjust(self.naive_local())?;
        Tz::zone_at(self.offset).resolve_local(&local).single()
    }
}

impl DateTime<Utc> {
    pub const MIN_UTC: Self = NaiveDateTime::MIN.and_utc();
    pub const MAX_UTC: Self = NaiveDateTime::MAX.and_utc();
    pub const UNIX_EPOCH: Self = NaiveDateTime::UNIX_EPOCH.and_utc();

    #[must_use]
    pub const fn from_naive_utc_and_offset(utc: NaiveDateTime, _: Utc) -> Self {
        Self::at(utc, FixedOffset(0))
    }

    #[must_use]
    pub fn from_timestamp(seconds: i64, nanos: u32) -> Option<Self> {
        Some(NaiveDateTime::from_epoch_seconds(seconds, nanos)?.and_utc())
    }

    #[must_use]
    pub fn from_timestamp_millis(millis: i64) -> Option<Self> {
        let nanos = u32::try_from(millis.rem_euclid(1000)).ok()? * 1_000_000;
        Self::from_timestamp(millis.div_euclid(1000), nanos)
    }
}

impl From<std::time::SystemTime> for DateTime<Utc> {
    fn from(value: std::time::SystemTime) -> Self {
        let elapsed = match value.duration_since(std::time::UNIX_EPOCH) {
            Ok(value) => Duration::from_std(value),
            Err(error) => Duration::from_std(error.duration()).map(std::ops::Neg::neg),
        }
        .expect("system time fits application bounds");
        Self::UNIX_EPOCH + elapsed
    }
}

impl<Tz: TimeZone> From<DateTime<Tz>> for std::time::SystemTime {
    fn from(value: DateTime<Tz>) -> Self {
        let seconds = value.timestamp();
        let elapsed = std::time::Duration::from_secs(seconds.unsigned_abs());
        let time = if seconds < 0 {
            std::time::UNIX_EPOCH - elapsed
        } else {
            std::time::UNIX_EPOCH + elapsed
        };
        time + std::time::Duration::from_nanos(u64::from(value.timestamp_subsec_nanos()))
    }
}

impl Default for DateTime<Utc> {
    fn default() -> Self {
        Self::UNIX_EPOCH
    }
}

impl DateTime<FixedOffset> {
    /// Parse a timestamp with an explicit RFC 3339 offset.
    /// # Errors
    /// Rejects invalid dates, times and offsets.
    pub fn parse_from_rfc3339(value: &str) -> Result<Self, TimeError> {
        crate::wire::parse_rfc3339(value, true)
    }

    /// Parse an offset-aware timestamp with the supplied field format.
    /// # Errors
    /// Rejects invalid fields, missing offsets and out-of-range timestamps.
    pub fn parse_from_str(value: &str, format: &str) -> Result<Self, TimeError> {
        if format == "%Y-%m-%dT%H:%M:%S%.f%z" && value.contains('T') {
            return crate::wire::parse_rfc3339(value, false);
        }
        let parts =
            jiff::fmt::strtime::BrokenDownTime::parse(format, value).map_err(|_| TimeError)?;
        let offset = parts
            .offset()
            .and_then(|offset| FixedOffset::east_opt(offset.seconds()))
            .ok_or(TimeError)?;
        let local = NaiveDateTime::from_civil(parts.to_datetime().map_err(|_| TimeError)?);
        offset.resolve_local(&local).single().ok_or(TimeError)
    }
}

fn shift(value: NaiveDateTime, seconds: i32) -> Option<NaiveDateTime> {
    if seconds == 0 {
        return Some(value);
    }
    NaiveDateTime::from_epoch_seconds(
        value.epoch_seconds() + i64::from(seconds),
        value.nanosecond(),
    )
}

impl<Tz: TimeZone> Datelike for DateTime<Tz> {
    fn year(&self) -> i32 {
        self.date_naive().year()
    }
    fn month(&self) -> u32 {
        self.date_naive().month()
    }
    fn day(&self) -> u32 {
        self.date_naive().day()
    }
    fn ordinal(&self) -> u32 {
        self.date_naive().ordinal()
    }
    fn weekday(&self) -> Weekday {
        self.date_naive().weekday()
    }
}

impl<Tz: TimeZone> Timelike for DateTime<Tz> {
    fn hour(&self) -> u32 {
        self.time().hour()
    }
    fn minute(&self) -> u32 {
        self.time().minute()
    }
    fn second(&self) -> u32 {
        self.time().second()
    }
    fn nanosecond(&self) -> u32 {
        self.time().nanosecond()
    }
    fn with_hour(&self, hour: u32) -> Option<Self> {
        self.change_local(|local| local.with_hour(hour))
    }
    fn with_minute(&self, minute: u32) -> Option<Self> {
        self.change_local(|local| local.with_minute(minute))
    }
    fn with_second(&self, second: u32) -> Option<Self> {
        self.change_local(|local| local.with_second(second))
    }
    fn with_nanosecond(&self, nanos: u32) -> Option<Self> {
        self.change_local(|local| local.with_nanosecond(nanos))
    }
}

impl<Tz: TimeZone, Other: TimeZone> PartialEq<DateTime<Other>> for DateTime<Tz> {
    fn eq(&self, other: &DateTime<Other>) -> bool {
        self.utc == other.utc
    }
}
impl<Tz: TimeZone> Eq for DateTime<Tz> {}
impl<Tz: TimeZone, Other: TimeZone> PartialOrd<DateTime<Other>> for DateTime<Tz> {
    fn partial_cmp(&self, other: &DateTime<Other>) -> Option<Ordering> {
        Some(self.utc.cmp(&other.utc))
    }
}
impl<Tz: TimeZone> Ord for DateTime<Tz> {
    fn cmp(&self, other: &Self) -> Ordering {
        self.utc.cmp(&other.utc)
    }
}
impl<Tz: TimeZone> Hash for DateTime<Tz> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.utc.hash(state);
    }
}

impl<Tz: TimeZone> ops::Add<Duration> for DateTime<Tz> {
    type Output = Self;
    fn add(self, duration: Duration) -> Self {
        self.checked_add_signed(duration)
            .expect("date and time is out of range")
    }
}
impl<Tz: TimeZone> ops::Sub<Duration> for DateTime<Tz> {
    type Output = Self;
    fn sub(self, duration: Duration) -> Self {
        self.checked_sub_signed(duration)
            .expect("date and time is out of range")
    }
}
impl<Tz: TimeZone> ops::AddAssign<Duration> for DateTime<Tz> {
    fn add_assign(&mut self, duration: Duration) {
        *self = *self + duration;
    }
}
impl<Tz: TimeZone> ops::SubAssign<Duration> for DateTime<Tz> {
    fn sub_assign(&mut self, duration: Duration) {
        *self = *self - duration;
    }
}
impl<Tz: TimeZone, Other: TimeZone> ops::Sub<DateTime<Other>> for DateTime<Tz> {
    type Output = Duration;
    fn sub(self, other: DateTime<Other>) -> Duration {
        self.signed_duration_since(other)
    }
}

impl<Tz: TimeZone> fmt::Display for DateTime<Tz> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ", self.naive_local())?;
        if std::any::TypeId::of::<Tz>() == std::any::TypeId::of::<Utc>() {
            f.write_str("UTC")
        } else {
            self.offset.fmt(f)
        }
    }
}
impl<Tz: TimeZone> fmt::Debug for DateTime<Tz> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(
            &crate::wire::Rfc3339::new(*self, SecondsFormat::AutoSi, true),
            f,
        )
    }
}
impl<Tz: TimeZone> FromStr for DateTime<Tz> {
    type Err = TimeError;
    fn from_str(value: &str) -> Result<Self, TimeError> {
        let parsed = crate::wire::parse_rfc3339(value, false)?;
        Ok(parsed.with_timezone(&Tz::zone_at(*parsed.offset())))
    }
}
impl<Tz: TimeZone> Serialize for DateTime<Tz> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.collect_str(&crate::wire::Rfc3339::new(
            *self,
            SecondsFormat::AutoSi,
            true,
        ))
    }
}
impl<'de, Tz: TimeZone> Deserialize<'de> for DateTime<Tz> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
