use chrono::{DateTime, Datelike, Duration, NaiveDateTime, Timelike, Utc};
use jiff::{civil, tz::AmbiguousOffset, tz::TimeZone, Timestamp};

use super::SchedulingError;

/// Civil clock used only for Classic fixed starts. Persisted checkpoints remain UTC.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassicClock(TimeZone);

impl Default for ClassicClock {
    fn default() -> Self {
        Self(TimeZone::UTC)
    }
}

impl ClassicClock {
    /// Resolve the host clock once when loading a build's reference data.
    ///
    /// # Errors
    /// Returns an error if the operating-system time zone cannot be loaded.
    pub fn system() -> Result<Self, SchedulingError> {
        TimeZone::try_system()
            .map(Self)
            .map_err(|_| invalid_clock())
    }

    /// Select an explicit IANA clock for reproducible scheduling inputs.
    ///
    /// # Errors
    /// Returns an error if the named zone is unavailable.
    pub fn named(name: &str) -> Result<Self, SchedulingError> {
        TimeZone::get(name).map(Self).map_err(|_| invalid_clock())
    }

    pub(super) fn occurrences(
        &self,
        current: DateTime<Utc>,
        time: Duration,
    ) -> Result<(DateTime<Utc>, DateTime<Utc>), SchedulingError> {
        let local = self.local_time(current)?;
        let today = local
            .date()
            .and_hms_opt(0, 0, 0)
            .ok_or_else(invalid_clock)?
            .checked_add_signed(time)
            .ok_or_else(invalid_clock)?;
        let tomorrow = today
            .checked_add_signed(Duration::days(1))
            .ok_or_else(invalid_clock)?;
        Ok((self.resolve(today)?, self.resolve(tomorrow)?))
    }

    pub(super) fn local_time(
        &self,
        current: DateTime<Utc>,
    ) -> Result<NaiveDateTime, SchedulingError> {
        let instant = Timestamp::new(current.timestamp(), 0).map_err(|_| invalid_clock())?;
        current
            .naive_utc()
            .checked_add_signed(Duration::seconds(i64::from(
                self.0.to_offset(instant).seconds(),
            )))
            .ok_or_else(invalid_clock)
    }

    fn resolve(&self, local: NaiveDateTime) -> Result<DateTime<Utc>, SchedulingError> {
        let civil = civil::DateTime::new(
            local.year().try_into().map_err(|_| invalid_clock())?,
            local.month().try_into().map_err(|_| invalid_clock())?,
            local.day().try_into().map_err(|_| invalid_clock())?,
            local.hour().try_into().map_err(|_| invalid_clock())?,
            local.minute().try_into().map_err(|_| invalid_clock())?,
            local.second().try_into().map_err(|_| invalid_clock())?,
            local.nanosecond().try_into().map_err(|_| invalid_clock())?,
        )
        .map_err(|_| invalid_clock())?;
        let offset = match self.0.to_ambiguous_timestamp(civil).offset() {
            AmbiguousOffset::Unambiguous { offset } => offset,
            // TimeZoneInfo.GetUtcOffset uses the standard offset in DST gaps
            // and overlaps, including zones with negative DST in TZDB.
            ambiguity @ (AmbiguousOffset::Gap { before, after }
            | AmbiguousOffset::Fold { before, after }) => {
                let first = before.to_timestamp(civil).map_err(|_| invalid_clock())?;
                let second = after.to_timestamp(civil).map_err(|_| invalid_clock())?;
                if self.0.to_offset_info(first).dst() == self.0.to_offset_info(second).dst() {
                    // A civil-zone change is not a daylight-saving transition.
                    // Match GetUtcOffset's civil-date rule at these seams.
                    match ambiguity {
                        AmbiguousOffset::Gap { .. } => after,
                        _ => before,
                    }
                } else {
                    before.min(after)
                }
            }
        };
        local
            .checked_sub_signed(Duration::seconds(i64::from(offset.seconds())))
            .map(|value| value.and_utc())
            .ok_or_else(invalid_clock)
    }
}

fn invalid_clock() -> SchedulingError {
    SchedulingError::InvalidTime(
        "Classic fixed-start clock could not resolve the civil time".into(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tomorrow_resolves_its_own_dst_offset() {
        let clock = ClassicClock::named("Europe/Amsterdam").unwrap();
        for (now, today, tomorrow) in [
            (
                "2026-03-28T22:50:00Z",
                "2026-03-27T23:00:00Z",
                "2026-03-28T23:00:00Z",
            ),
            (
                "2026-03-29T21:50:00Z",
                "2026-03-28T23:00:00Z",
                "2026-03-29T22:00:00Z",
            ),
            (
                "2026-10-25T22:50:00Z",
                "2026-10-24T22:00:00Z",
                "2026-10-25T23:00:00Z",
            ),
        ] {
            let parse = |value: &str| value.parse::<DateTime<Utc>>().unwrap();
            assert_eq!(
                clock.occurrences(parse(now), Duration::zero()).unwrap(),
                (parse(today), parse(tomorrow))
            );
        }
    }

    #[test]
    fn dst_gaps_and_overlaps_match_dotnet_standard_offsets() {
        // TimeZoneInfo.GetUtcOffset(Unspecified), .NET runtime 10.0.12 oracle (SDK 10.0.112).
        for (zone, civil, offset_seconds) in [
            ("Europe/Amsterdam", "2026-03-29T02:30:00", 3600),
            ("Europe/Amsterdam", "2026-10-25T02:30:00", 3600),
            ("Europe/Dublin", "2026-03-29T01:30:00", 0),
            ("Europe/Dublin", "2026-10-25T01:30:00", 0),
            ("Australia/Lord_Howe", "2026-04-05T01:45:00", 37800),
            ("Australia/Lord_Howe", "2026-10-04T02:15:00", 37800),
            ("Africa/Casablanca", "2026-02-15T02:30:00", 0),
            ("Africa/Casablanca", "2026-03-22T02:30:00", 0),
            ("Pacific/Apia", "2011-12-30T12:00:00", 50400),
            ("Europe/Moscow", "2014-10-26T01:30:00", 14400),
        ] {
            let local = civil.parse::<NaiveDateTime>().unwrap();
            assert_eq!(
                ClassicClock::named(zone).unwrap().resolve(local).unwrap(),
                (local - Duration::seconds(offset_seconds)).and_utc(),
                "{zone} {civil}"
            );
        }
    }
}
