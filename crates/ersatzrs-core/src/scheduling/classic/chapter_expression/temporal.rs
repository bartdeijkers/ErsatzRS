//! Closed Gregorian date/time literal profile from ADR 0098.
use super::{value::Value, ExpressionError, Result};
use chrono::{Duration, NaiveDate};

const TICKS_PER_SECOND: u64 = 10_000_000;

pub(super) fn parse(text: &str) -> Result<Value> {
    let text = text
        .strip_prefix('#')
        .and_then(|s| s.strip_suffix('#'))
        .ok_or(ExpressionError)?;
    if text.contains('/') {
        date_time(text, false)
    } else if text.contains('-') {
        date_time(text, true)
    } else {
        duration_from_ticks(time_ticks(text, HourWidth::Elapsed)?).map(Value::Time)
    }
}

fn date_time(text: &str, iso: bool) -> Result<Value> {
    let (date, time) = text
        .split_once(if iso { 'T' } else { ' ' })
        .map_or((text, None), |(d, t)| (d, Some(t)));
    let separator = if iso { '-' } else { '/' };
    let fields: Vec<_> = date.split(separator).collect();
    let [a, b, c] = fields.as_slice() else {
        return Err(ExpressionError);
    };
    let (year, month, day) = if iso {
        (digits(a, 4, 4)?, digits(b, 2, 2)?, digits(c, 2, 2)?)
    } else {
        (digits(c, 4, 4)?, digits(a, 1, 2)?, digits(b, 1, 2)?)
    };
    if !(1..=9999).contains(&year) {
        return Err(ExpressionError);
    }
    let date = NaiveDate::from_ymd_opt(
        i32::try_from(year).map_err(|_| ExpressionError)?,
        month,
        day,
    )
    .ok_or(ExpressionError)?;
    let ticks = time.map_or(Ok(0), |s| {
        time_ticks(
            s,
            if iso {
                HourWidth::Iso
            } else {
                HourWidth::Slash
            },
        )
    })?;
    let seconds = u32::try_from(ticks / TICKS_PER_SECOND).map_err(|_| ExpressionError)?;
    let nanos = u32::try_from((ticks % TICKS_PER_SECOND) * 100).map_err(|_| ExpressionError)?;
    date.and_hms_nano_opt(seconds / 3600, (seconds / 60) % 60, seconds % 60, nanos)
        .map(Value::Date)
        .ok_or(ExpressionError)
}

#[derive(Clone, Copy)]
enum HourWidth {
    Elapsed,
    Slash,
    Iso,
}

fn time_ticks(text: &str, width: HourWidth) -> Result<u64> {
    let fields: Vec<_> = text.split(':').collect();
    let [hours, minutes, seconds] = fields.as_slice() else {
        return Err(ExpressionError);
    };
    let max_hour_width = match width {
        HourWidth::Elapsed => usize::MAX,
        HourWidth::Slash | HourWidth::Iso => 2,
    };
    let min_hour_width = if matches!(width, HourWidth::Iso) {
        2
    } else {
        1
    };
    if !(min_hour_width..=max_hour_width).contains(&hours.len())
        || !hours.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(ExpressionError);
    }
    let hours = hours.parse::<u64>().map_err(|_| ExpressionError)?;
    if !matches!(width, HourWidth::Elapsed) && hours > 23 {
        return Err(ExpressionError);
    }
    let minutes = digits(minutes, 2, 2)?;
    let (seconds, fraction) = seconds
        .split_once('.')
        .map_or((*seconds, None), |(s, f)| (s, Some(f)));
    let seconds = digits(seconds, 2, 2)?;
    if minutes > 59 || seconds > 59 {
        return Err(ExpressionError);
    }
    let fraction = fraction.map_or(Ok(0), |s| {
        let value = digits(s, 1, 7)?;
        let padding = u32::try_from(7 - s.len()).map_err(|_| ExpressionError)?;
        Ok::<_, ExpressionError>(u64::from(value) * 10_u64.pow(padding))
    })?;
    let ticks = hours
        .checked_mul(3600)
        .and_then(|s| s.checked_add(u64::from(minutes) * 60 + u64::from(seconds)))
        .and_then(|s| s.checked_mul(TICKS_PER_SECOND))
        .and_then(|s| s.checked_add(fraction))
        .ok_or(ExpressionError)?;
    i64::try_from(ticks).map_err(|_| ExpressionError)?;
    Ok(ticks)
}

fn digits(text: &str, min: usize, max: usize) -> Result<u32> {
    if !(min..=max).contains(&text.len()) || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ExpressionError);
    }
    text.parse().map_err(|_| ExpressionError)
}

fn duration_from_ticks(ticks: u64) -> Result<Duration> {
    let seconds = i64::try_from(ticks / TICKS_PER_SECOND).map_err(|_| ExpressionError)?;
    let nanos = i64::try_from((ticks % TICKS_PER_SECOND) * 100).map_err(|_| ExpressionError)?;
    Duration::try_seconds(seconds)
        .and_then(|duration| duration.checked_add(&Duration::nanoseconds(nanos)))
        .ok_or(ExpressionError)
}
