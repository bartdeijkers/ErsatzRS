use std::{fmt, ops, str::FromStr};

use jiff::{civil, fmt::strtime::BrokenDownTime, SignedDuration};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::{DateTime, Duration, TimeError, Utc};

pub(crate) const DAY: i64 = 86_400;
pub(crate) const BILLION: u32 = 1_000_000_000;
pub(crate) const MIN_SECOND: i64 = -8_334_601_228_800;
pub(crate) const MAX_SECOND: i64 = 8_210_266_876_799;
const CYCLE_DAYS: i32 = 146_097;
const EPOCH: civil::Date = civil::date(1970, 1, 1);

/// Gregorian calendar fields for a civil or offset-aware date.
pub trait Datelike {
    fn year(&self) -> i32;
    fn month(&self) -> u32;
    fn day(&self) -> u32;
    fn ordinal(&self) -> u32;
    fn weekday(&self) -> Weekday;
    fn month0(&self) -> u32 {
        self.month() - 1
    }
    fn day0(&self) -> u32 {
        self.day() - 1
    }
}

/// Civil clock fields. An explicitly encoded leap second has an extended
/// nanosecond field at second 59.
pub trait Timelike: Sized {
    fn hour(&self) -> u32;
    fn minute(&self) -> u32;
    fn second(&self) -> u32;
    fn nanosecond(&self) -> u32;
    fn with_hour(&self, hour: u32) -> Option<Self>;
    fn with_minute(&self, minute: u32) -> Option<Self>;
    fn with_second(&self, second: u32) -> Option<Self>;
    fn with_nanosecond(&self, nanos: u32) -> Option<Self>;

    fn num_seconds_from_midnight(&self) -> u32 {
        self.hour() * 3600 + self.minute() * 60 + self.second()
    }
}

/// Day of the Gregorian week.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Weekday {
    Mon,
    Tue,
    Wed,
    Thu,
    Fri,
    Sat,
    Sun,
}

impl Weekday {
    #[must_use]
    pub const fn num_days_from_monday(self) -> u32 {
        self as u32
    }

    #[must_use]
    pub const fn num_days_from_sunday(self) -> u32 {
        (self.num_days_from_monday() + 1) % 7
    }

    #[must_use]
    pub const fn number_from_monday(self) -> u32 {
        self.num_days_from_monday() + 1
    }

    #[must_use]
    pub const fn number_from_sunday(self) -> u32 {
        self.num_days_from_sunday() + 1
    }
}

impl From<civil::Weekday> for Weekday {
    fn from(value: civil::Weekday) -> Self {
        match value {
            civil::Weekday::Monday => Self::Mon,
            civil::Weekday::Tuesday => Self::Tue,
            civil::Weekday::Wednesday => Self::Wed,
            civil::Weekday::Thursday => Self::Thu,
            civil::Weekday::Friday => Self::Fri,
            civil::Weekday::Saturday => Self::Sat,
            civil::Weekday::Sunday => Self::Sun,
        }
    }
}

/// An unsigned number of calendar months.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Months(u32);

impl Months {
    #[must_use]
    pub const fn new(months: u32) -> Self {
        Self(months)
    }
}

/// A Gregorian date without a time zone.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NaiveDate(i32);

impl NaiveDate {
    pub const MIN: Self = Self(-96_465_292);
    pub const MAX: Self = Self(95_026_236);

    #[must_use]
    pub fn from_ymd_opt(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(-262_143..=262_142).contains(&year) {
            return None;
        }
        // Gregorian dates and weekdays repeat every 400 years. Only extended
        // years need a cycle shift into Jiff's range.
        let cycle = if (-9999..=9999).contains(&year) {
            0
        } else {
            (year - 1970).div_euclid(400)
        };
        let value = civil::Date::new(
            (year - cycle * 400).try_into().ok()?,
            month.try_into().ok()?,
            day.try_into().ok()?,
        )
        .ok()?;
        let days = i32::try_from(value.duration_since(EPOCH).as_secs() / DAY).ok()?;
        Some(Self(days + cycle * CYCLE_DAYS))
    }

    pub(crate) fn from_civil(value: civil::Date) -> Self {
        let days = value.duration_since(EPOCH).as_secs() / DAY;
        Self(i32::try_from(days).expect("Jiff date range fits i32 days"))
    }

    pub(crate) fn civil_cycle(self) -> (civil::Date, i32) {
        if let Ok(date) = EPOCH.checked_add(SignedDuration::from_secs(i64::from(self.0) * DAY)) {
            return (date, 0);
        }
        let cycle = self.0.div_euclid(CYCLE_DAYS);
        let reduced = i64::from(self.0.rem_euclid(CYCLE_DAYS));
        let date = EPOCH
            .checked_add(SignedDuration::from_secs(reduced * DAY))
            .expect("one calendar cycle fits Jiff");
        (date, cycle)
    }

    /// Parse a civil date using the supplied field format.
    /// # Errors
    /// Rejects invalid fields or input that does not match the format.
    pub fn parse_from_str(value: &str, format: &str) -> Result<Self, TimeError> {
        civil::Date::strptime(format, value)
            .map(Self::from_civil)
            .map_err(|_| TimeError)
    }

    #[must_use]
    pub fn succ_opt(self) -> Option<Self> {
        (self < Self::MAX).then(|| Self(self.0 + 1))
    }

    #[must_use]
    pub fn pred_opt(self) -> Option<Self> {
        (self > Self::MIN).then(|| Self(self.0 - 1))
    }

    #[must_use]
    pub fn checked_add_months(self, months: Months) -> Option<Self> {
        self.shift_months(i64::from(months.0))
    }

    #[must_use]
    pub fn checked_sub_months(self, months: Months) -> Option<Self> {
        self.shift_months(-i64::from(months.0))
    }

    fn shift_months(self, months: i64) -> Option<Self> {
        let target = i64::from(self.year()) * 12 + i64::from(self.month0()) + months;
        let year = i32::try_from(target.div_euclid(12)).ok()?;
        let month = u32::try_from(target.rem_euclid(12) + 1).ok()?;
        let first = Self::from_ymd_opt(year, month, 1)?;
        let last = u32::try_from(first.civil_cycle().0.days_in_month()).ok()?;
        Self::from_ymd_opt(year, month, self.day().min(last))
    }

    #[must_use]
    pub const fn and_time(self, time: NaiveTime) -> NaiveDateTime {
        NaiveDateTime { date: self, time }
    }

    #[must_use]
    pub fn and_hms_opt(self, hour: u32, minute: u32, second: u32) -> Option<NaiveDateTime> {
        self.and_hms_nano_opt(hour, minute, second, 0)
    }

    #[must_use]
    pub fn and_hms_nano_opt(
        self,
        hour: u32,
        minute: u32,
        second: u32,
        nanos: u32,
    ) -> Option<NaiveDateTime> {
        Some(self.and_time(NaiveTime::from_hms_nano_opt(hour, minute, second, nanos)?))
    }

    #[must_use]
    pub fn checked_add_signed(self, duration: Duration) -> Option<Self> {
        let days = i32::try_from(i64::from(self.0) + duration.num_days()).ok()?;
        let date = Self(days);
        (Self::MIN..=Self::MAX).contains(&date).then_some(date)
    }

    #[must_use]
    pub fn checked_sub_signed(self, duration: Duration) -> Option<Self> {
        self.checked_add_signed(-duration)
    }

    #[must_use]
    pub fn signed_duration_since(self, other: Self) -> Duration {
        Duration::days(i64::from(self.0) - i64::from(other.0))
    }

    #[must_use]
    pub fn format(self, format: &str) -> impl fmt::Display + '_ {
        crate::wire::format_fields(
            &BrokenDownTime::from(self.civil_cycle().0),
            format,
            self.year(),
            0,
        )
    }
}

impl Datelike for NaiveDate {
    fn year(&self) -> i32 {
        let (date, cycle) = self.civil_cycle();
        i32::from(date.year()) + cycle * 400
    }
    fn month(&self) -> u32 {
        u32::try_from(self.civil_cycle().0.month()).expect("valid month")
    }
    fn day(&self) -> u32 {
        u32::try_from(self.civil_cycle().0.day()).expect("valid day")
    }
    fn ordinal(&self) -> u32 {
        u32::try_from(self.civil_cycle().0.day_of_year()).expect("valid ordinal")
    }
    fn weekday(&self) -> Weekday {
        self.civil_cycle().0.weekday().into()
    }
}

impl fmt::Display for NaiveDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let (date, cycle) = self.civil_cycle();
        let year = i32::from(date.year()) + cycle * 400;
        if (0..=9999).contains(&year) {
            write!(f, "{year:04}")?;
        } else if year < 0 {
            write!(f, "-{:04}", year.unsigned_abs())?;
        } else {
            write!(f, "+{year}")?;
        }
        write!(f, "-{:02}-{:02}", date.month(), date.day())
    }
}

impl fmt::Debug for NaiveDate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for NaiveDate {
    type Err = TimeError;
    fn from_str(value: &str) -> Result<Self, TimeError> {
        let (prefix, day) = value.rsplit_once('-').ok_or(TimeError)?;
        let (year, month) = prefix.rsplit_once('-').ok_or(TimeError)?;
        Self::from_ymd_opt(
            year.parse().map_err(|_| TimeError)?,
            month.parse().map_err(|_| TimeError)?,
            day.parse().map_err(|_| TimeError)?,
        )
        .ok_or(TimeError)
    }
}

impl ops::Add<Duration> for NaiveDate {
    type Output = Self;
    fn add(self, rhs: Duration) -> Self {
        self.checked_add_signed(rhs).expect("date is out of range")
    }
}

impl ops::Sub<Duration> for NaiveDate {
    type Output = Self;
    fn sub(self, rhs: Duration) -> Self {
        self.checked_sub_signed(rhs).expect("date is out of range")
    }
}

impl ops::Sub for NaiveDate {
    type Output = Duration;
    fn sub(self, rhs: Self) -> Duration {
        self.signed_duration_since(rhs)
    }
}

/// A civil time of day, including explicitly supplied leap seconds.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NaiveTime {
    seconds: u32,
    nanos: u32,
}

impl NaiveTime {
    pub const MIN: Self = Self {
        seconds: 0,
        nanos: 0,
    };
    pub const MAX: Self = Self {
        seconds: 86_399,
        nanos: BILLION - 1,
    };

    #[must_use]
    pub const fn from_num_seconds_from_midnight_opt(seconds: u32, nanos: u32) -> Option<Self> {
        if seconds >= 86_400 || nanos >= 2 * BILLION || (nanos >= BILLION && seconds % 60 != 59) {
            None
        } else {
            Some(Self { seconds, nanos })
        }
    }

    #[must_use]
    pub const fn from_hms_opt(hour: u32, minute: u32, second: u32) -> Option<Self> {
        Self::from_hms_nano_opt(hour, minute, second, 0)
    }

    #[must_use]
    pub const fn from_hms_nano_opt(
        hour: u32,
        minute: u32,
        second: u32,
        nanos: u32,
    ) -> Option<Self> {
        if hour >= 24 || minute >= 60 || second >= 60 {
            None
        } else {
            Self::from_num_seconds_from_midnight_opt(hour * 3600 + minute * 60 + second, nanos)
        }
    }

    pub(crate) fn from_civil(value: civil::Time) -> Self {
        let seconds = i32::from(value.hour()) * 3600
            + i32::from(value.minute()) * 60
            + i32::from(value.second());
        Self {
            seconds: u32::try_from(seconds).expect("valid Jiff seconds"),
            nanos: u32::try_from(value.subsec_nanosecond()).expect("valid Jiff nanoseconds"),
        }
    }

    pub(crate) fn civil(self) -> civil::Time {
        civil::Time::new(
            self.hour().try_into().expect("valid hour"),
            self.minute().try_into().expect("valid minute"),
            self.second().try_into().expect("valid second"),
            (self.nanos % BILLION).try_into().expect("valid fraction"),
        )
        .expect("valid civil time")
    }

    /// Parse a civil time using the supplied field format.
    /// # Errors
    /// Rejects invalid fields or input that does not match the format.
    pub fn parse_from_str(value: &str, format: &str) -> Result<Self, TimeError> {
        if matches!(format, "%H:%M:%S%.f" | "%T%.f" | "%H:%M:%S") {
            if format.ends_with("%S") && value.contains('.') {
                return Err(TimeError);
            }
            return value.parse();
        }
        civil::Time::strptime(format, value)
            .map(Self::from_civil)
            .map_err(|_| TimeError)
    }

    #[must_use]
    pub fn format(self, format: &str) -> impl fmt::Display + '_ {
        crate::wire::format_fields(&BrokenDownTime::from(self.civil()), format, 0, self.nanos)
    }
}

impl Timelike for NaiveTime {
    fn hour(&self) -> u32 {
        self.seconds / 3600
    }
    fn minute(&self) -> u32 {
        self.seconds / 60 % 60
    }
    fn second(&self) -> u32 {
        self.seconds % 60
    }
    fn nanosecond(&self) -> u32 {
        self.nanos
    }
    fn num_seconds_from_midnight(&self) -> u32 {
        self.seconds
    }
    fn with_hour(&self, hour: u32) -> Option<Self> {
        Self::from_hms_nano_opt(hour, self.minute(), self.second(), self.nanos)
    }
    fn with_minute(&self, minute: u32) -> Option<Self> {
        Self::from_hms_nano_opt(self.hour(), minute, self.second(), self.nanos)
    }
    fn with_second(&self, second: u32) -> Option<Self> {
        Self::from_hms_nano_opt(self.hour(), self.minute(), second, self.nanos)
    }
    fn with_nanosecond(&self, nanos: u32) -> Option<Self> {
        Self::from_num_seconds_from_midnight_opt(self.seconds, nanos)
    }
}

impl fmt::Display for NaiveTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        crate::wire::write_clock(f, *self, crate::SecondsFormat::AutoSi)
    }
}

impl fmt::Debug for NaiveTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

impl FromStr for NaiveTime {
    type Err = TimeError;
    fn from_str(value: &str) -> Result<Self, TimeError> {
        crate::wire::parse_clock(value)
    }
}

/// A Gregorian date and clock reading without a time zone.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct NaiveDateTime {
    date: NaiveDate,
    time: NaiveTime,
}

impl NaiveDateTime {
    pub const MIN: Self = NaiveDate::MIN.and_time(NaiveTime::MIN);
    pub const MAX: Self = NaiveDate::MAX.and_time(NaiveTime::MAX);
    pub const UNIX_EPOCH: Self = NaiveDate(0).and_time(NaiveTime::MIN);

    #[must_use]
    pub const fn new(date: NaiveDate, time: NaiveTime) -> Self {
        Self { date, time }
    }

    #[must_use]
    pub const fn date(self) -> NaiveDate {
        self.date
    }

    #[must_use]
    pub const fn time(self) -> NaiveTime {
        self.time
    }

    #[must_use]
    pub const fn and_utc(self) -> DateTime<Utc> {
        DateTime::from_naive_utc_and_offset(self, Utc)
    }

    pub(crate) fn from_civil(value: civil::DateTime) -> Self {
        NaiveDate::from_civil(value.date()).and_time(NaiveTime::from_civil(value.time()))
    }

    pub(crate) fn civil(self) -> civil::DateTime {
        self.date.civil_cycle().0.to_datetime(self.time.civil())
    }

    pub(crate) const fn epoch_seconds(self) -> i64 {
        self.date.0 as i64 * DAY + self.time.seconds as i64
    }

    pub(crate) fn from_epoch_seconds(seconds: i64, nanos: u32) -> Option<Self> {
        if !(MIN_SECOND..=MAX_SECOND).contains(&seconds) {
            return None;
        }
        let date = NaiveDate(i32::try_from(seconds.div_euclid(DAY)).ok()?);
        let time = NaiveTime::from_num_seconds_from_midnight_opt(
            u32::try_from(seconds.rem_euclid(DAY)).ok()?,
            nanos,
        )?;
        Some(date.and_time(time))
    }

    /// Parse a civil date and time using the supplied field format.
    /// # Errors
    /// Rejects invalid fields or input that does not match the format.
    pub fn parse_from_str(value: &str, format: &str) -> Result<Self, TimeError> {
        if matches!(
            format,
            "%Y-%m-%d %H:%M:%S%.f" | "%Y-%m-%d %H:%M:%S" | "%F %T%.f" | "%Y-%m-%dT%H:%M:%S%.f"
        ) {
            let separator = if format.contains('T') && format != "%F %T%.f" {
                'T'
            } else {
                ' '
            };
            let (_, clock) = value.split_once(separator).ok_or(TimeError)?;
            if format.ends_with("%S") && clock.contains('.') {
                return Err(TimeError);
            }
            return value.parse();
        }
        civil::DateTime::strptime(format, value)
            .map(Self::from_civil)
            .map_err(|_| TimeError)
    }

    #[must_use]
    pub fn format(self, format: &str) -> impl fmt::Display + '_ {
        self.formatted(format, None)
    }

    pub(crate) fn formatted(self, format: &str, offset: Option<jiff::tz::Offset>) -> String {
        let mut parts = BrokenDownTime::from(self.civil());
        parts.set_offset(offset);
        crate::wire::format_fields(&parts, format, self.year(), self.nanosecond())
    }

    #[must_use]
    pub fn checked_add_signed(self, duration: Duration) -> Option<Self> {
        let start = self.epoch_seconds();
        let nanos = self.time.nanos;
        if nanos >= BILLION {
            let fraction = i64::from(nanos) + i64::from(duration.subsec_nanos());
            if duration.num_seconds() == 0 && fraction < 2 * i64::from(BILLION) {
                return Self::from_epoch_seconds(start, fraction.try_into().ok()?);
            }
            let base = start.checked_add(i64::from(duration.num_seconds() < 0))?;
            return Self::add_normal(base, nanos - BILLION, duration);
        }
        Self::add_normal(start, nanos, duration)
    }

    fn add_normal(seconds: i64, nanos: u32, duration: Duration) -> Option<Self> {
        let elapsed = SignedDuration::new(seconds, i32::try_from(nanos).ok()?)
            .checked_add(duration.jiff())?;
        let total = elapsed.as_nanos();
        let scale = i128::from(BILLION);
        Self::from_epoch_seconds(
            i64::try_from(total.div_euclid(scale)).ok()?,
            u32::try_from(total.rem_euclid(scale)).ok()?,
        )
    }

    #[must_use]
    pub fn checked_sub_signed(self, duration: Duration) -> Option<Self> {
        self.checked_add_signed(-duration)
    }

    #[must_use]
    pub fn signed_duration_since(self, rhs: Self) -> Duration {
        let correction = if self.time.seconds > rhs.time.seconds && rhs.time.nanos >= BILLION {
            1
        } else {
            -i64::from(self.time.seconds < rhs.time.seconds && self.time.nanos >= BILLION)
        };
        let seconds = self.epoch_seconds() - rhs.epoch_seconds() + correction;
        let fraction = i64::from(self.time.nanos) - i64::from(rhs.time.nanos);
        Duration::seconds(seconds) + Duration::nanoseconds(fraction)
    }

    #[must_use]
    pub fn checked_add_months(self, months: Months) -> Option<Self> {
        Some(self.date.checked_add_months(months)?.and_time(self.time))
    }

    #[must_use]
    pub fn checked_sub_months(self, months: Months) -> Option<Self> {
        Some(self.date.checked_sub_months(months)?.and_time(self.time))
    }
}

impl Datelike for NaiveDateTime {
    fn year(&self) -> i32 {
        self.date.year()
    }
    fn month(&self) -> u32 {
        self.date.month()
    }
    fn day(&self) -> u32 {
        self.date.day()
    }
    fn ordinal(&self) -> u32 {
        self.date.ordinal()
    }
    fn weekday(&self) -> Weekday {
        self.date.weekday()
    }
}

impl Timelike for NaiveDateTime {
    fn hour(&self) -> u32 {
        self.time.hour()
    }
    fn minute(&self) -> u32 {
        self.time.minute()
    }
    fn second(&self) -> u32 {
        self.time.second()
    }
    fn nanosecond(&self) -> u32 {
        self.time.nanosecond()
    }
    fn with_hour(&self, hour: u32) -> Option<Self> {
        Some(self.date.and_time(self.time.with_hour(hour)?))
    }
    fn with_minute(&self, minute: u32) -> Option<Self> {
        Some(self.date.and_time(self.time.with_minute(minute)?))
    }
    fn with_second(&self, second: u32) -> Option<Self> {
        Some(self.date.and_time(self.time.with_second(second)?))
    }
    fn with_nanosecond(&self, nanos: u32) -> Option<Self> {
        Some(self.date.and_time(self.time.with_nanosecond(nanos)?))
    }
}

impl FromStr for NaiveDateTime {
    type Err = TimeError;
    fn from_str(value: &str) -> Result<Self, TimeError> {
        let (date, time) = value.split_once(['T', 't', ' ']).ok_or(TimeError)?;
        Ok(Self::new(date.parse()?, time.parse()?))
    }
}

impl fmt::Display for NaiveDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.date, self.time)
    }
}

impl fmt::Debug for NaiveDateTime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}T{}", self.date, self.time)
    }
}

impl ops::Add<Duration> for NaiveDateTime {
    type Output = Self;
    fn add(self, rhs: Duration) -> Self {
        self.checked_add_signed(rhs)
            .expect("date and time is out of range")
    }
}

impl ops::Sub<Duration> for NaiveDateTime {
    type Output = Self;
    fn sub(self, rhs: Duration) -> Self {
        self.checked_sub_signed(rhs)
            .expect("date and time is out of range")
    }
}

impl ops::Sub for NaiveDateTime {
    type Output = Duration;
    fn sub(self, rhs: Self) -> Duration {
        self.signed_duration_since(rhs)
    }
}

macro_rules! serialize_civil {
    ($value:ty, $formatter:literal) => {
        impl Serialize for $value {
            fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
                serializer.collect_str(&format_args!($formatter, self))
            }
        }
        impl<'de> Deserialize<'de> for $value {
            fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                String::deserialize(deserializer)?
                    .parse()
                    .map_err(serde::de::Error::custom)
            }
        }
    };
}

serialize_civil!(NaiveDate, "{}");
serialize_civil!(NaiveTime, "{}");
serialize_civil!(NaiveDateTime, "{:?}");
