use std::fmt::{self, Write as _};

use crate::{
    calendar::BILLION, DateTime, FixedOffset, NaiveDate, NaiveTime, TimeError, TimeZone, Timelike,
};

/// Fractional digits to include in a serialized UTC or offset date and time.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SecondsFormat {
    Secs,
    Millis,
    Micros,
    Nanos,
    AutoSi,
}

pub(crate) struct Rfc3339<Tz: TimeZone> {
    value: DateTime<Tz>,
    precision: SecondsFormat,
    use_z: bool,
}

impl<Tz: TimeZone> Rfc3339<Tz> {
    pub(crate) const fn new(value: DateTime<Tz>, precision: SecondsFormat, use_z: bool) -> Self {
        Self {
            value,
            precision,
            use_z,
        }
    }
}

impl<Tz: TimeZone> fmt::Display for Rfc3339<Tz> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let local = self.value.naive_local();
        write!(f, "{}T", local.date())?;
        write_clock(f, local.time(), self.precision)?;
        if self.use_z && self.value.offset().local_minus_utc() == 0 {
            f.write_char('Z')
        } else {
            write_offset(f, *self.value.offset())
        }
    }
}

pub(crate) fn write_offset(f: &mut fmt::Formatter<'_>, offset: FixedOffset) -> fmt::Result {
    let seconds = offset.local_minus_utc();
    let sign = if seconds < 0 { '-' } else { '+' };
    let minutes = (seconds.unsigned_abs() + 30) / 60;
    write!(f, "{sign}{:02}:{:02}", minutes / 60, minutes % 60)
}

pub(crate) fn write_clock(
    f: &mut fmt::Formatter<'_>,
    time: NaiveTime,
    precision: SecondsFormat,
) -> fmt::Result {
    let second = time.second() + u32::from(time.nanosecond() >= BILLION);
    write!(f, "{:02}:{:02}:{second:02}", time.hour(), time.minute())?;
    let nanos = time.nanosecond() % BILLION;
    let precision = match precision {
        SecondsFormat::AutoSi if nanos == 0 => SecondsFormat::Secs,
        SecondsFormat::AutoSi if nanos.is_multiple_of(1_000_000) => SecondsFormat::Millis,
        SecondsFormat::AutoSi if nanos.is_multiple_of(1_000) => SecondsFormat::Micros,
        SecondsFormat::AutoSi => SecondsFormat::Nanos,
        value => value,
    };
    match precision {
        SecondsFormat::Secs => Ok(()),
        SecondsFormat::Millis => write!(f, ".{:03}", nanos / 1_000_000),
        SecondsFormat::Micros => write!(f, ".{:06}", nanos / 1000),
        SecondsFormat::Nanos | SecondsFormat::AutoSi => write!(f, ".{nanos:09}"),
    }
}

pub(crate) fn parse_rfc3339(value: &str, strict: bool) -> Result<DateTime<FixedOffset>, TimeError> {
    let (date, rest) = value.split_once(['T', 't', ' ']).ok_or(TimeError)?;
    if strict
        && !matches!(
            date.as_bytes(),
            [
                b'0'..=b'9',
                b'0'..=b'9',
                b'0'..=b'9',
                b'0'..=b'9',
                b'-',
                b'0'..=b'9',
                b'0'..=b'9',
                b'-',
                b'0'..=b'9',
                b'0'..=b'9'
            ]
        )
    {
        return Err(TimeError);
    }
    let date: NaiveDate = date.parse()?;
    let (time, zone) = parse_clock_prefix(rest, strict)?;
    let offset = parse_offset(zone, strict)?;
    offset
        .resolve_local(&date.and_time(time))
        .single()
        .ok_or(TimeError)
}

pub(crate) fn parse_clock(value: &str) -> Result<NaiveTime, TimeError> {
    let (time, remaining) = parse_clock_prefix(value, false)?;
    if remaining.is_empty() {
        Ok(time)
    } else {
        Err(TimeError)
    }
}

fn parse_clock_prefix(value: &str, strict: bool) -> Result<(NaiveTime, &str), TimeError> {
    let (hour, value) = value.split_once(':').ok_or(TimeError)?;
    let (minute, value) = value.split_once(':').ok_or(TimeError)?;
    let count = value.bytes().take_while(u8::is_ascii_digit).count();
    let (second, rest) = value.split_at(count);
    let (hour, minute, second) = (
        number(hour, strict)?,
        number(minute, strict)?,
        number(second, strict)?,
    );
    let (mut fraction, zone) = if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return Err(TimeError);
        }
        let significant = digits.min(9);
        let nanos = fraction[..significant]
            .parse::<u32>()
            .map_err(|_| TimeError)?
            * 10_u32.pow(u32::try_from(9 - significant).map_err(|_| TimeError)?);
        (nanos, &fraction[digits..])
    } else {
        (0, rest)
    };
    let second = if second == 60 {
        fraction += BILLION;
        59
    } else {
        second
    };
    let time = NaiveTime::from_hms_nano_opt(hour, minute, second, fraction).ok_or(TimeError)?;
    Ok((time, zone))
}

fn number(text: &str, strict: bool) -> Result<u32, TimeError> {
    if (strict && text.len() != 2) || !text.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(TimeError);
    }
    text.parse().map_err(|_| TimeError)
}

fn parse_offset(value: &str, strict: bool) -> Result<FixedOffset, TimeError> {
    let value = if strict { value } else { value.trim() };
    if matches!(value, "Z" | "z") || (!strict && value.eq_ignore_ascii_case("UTC")) {
        return FixedOffset::east_opt(0).ok_or(TimeError);
    }
    let (sign, digits) = if let Some(bare) = value.strip_prefix('+') {
        (1, bare)
    } else {
        (-1, value.strip_prefix(['-', '−']).ok_or(TimeError)?)
    };
    let (hours, minutes) = if let Some(pair) = digits.split_once(':') {
        pair
    } else if !strict && digits.len() == 4 {
        digits.split_at(2)
    } else {
        return Err(TimeError);
    };
    let hours = number(hours, true)?;
    let minutes = number(minutes, true)?;
    if hours > 23 || minutes > 59 {
        return Err(TimeError);
    }
    let seconds = i32::try_from(hours * 3600 + minutes * 60).map_err(|_| TimeError)?;
    FixedOffset::east_opt(sign * seconds).ok_or(TimeError)
}

/// Let Jiff render the fields, with the established byte-level exceptions for
/// automatic fractions, leap seconds and years outside Jiff's civil range.
pub(crate) fn format_fields(
    fields: &jiff::fmt::strtime::BrokenDownTime,
    pattern: &str,
    year: i32,
    nanos: u32,
) -> String {
    let leap = nanos >= BILLION;
    let extended_year = !(-9999..=9999).contains(&year);
    if !leap && !extended_year && !pattern.contains("%.f") && !pattern.contains("%f") {
        return fields.to_string(pattern).expect("valid time format");
    }
    let mut adapted = String::with_capacity(pattern.len() + 12);
    let mut tail = pattern;
    while let Some((prefix, directive)) = tail.split_once('%') {
        adapted.push_str(prefix);
        let (consumed, replacement) = match directive.as_bytes().first() {
            Some(b'%') => (1, "%%"),
            Some(b'f') => (1, "%9f"),
            Some(b'S') if leap => (1, "60"),
            Some(b'T') if leap => (1, "%H:%M:60"),
            Some(b'Y') if extended_year => {
                write!(adapted, "{year:+}").expect("writing to String");
                (1, "")
            }
            Some(b'F') if extended_year => {
                write!(adapted, "{year:+}-%m-%d").expect("writing to String");
                (1, "")
            }
            Some(b'.') if directive.starts_with(".f") => {
                let fraction = nanos % BILLION;
                let replacement = match fraction {
                    0 => "",
                    n if n.is_multiple_of(1_000_000) => "%.3f",
                    n if n.is_multiple_of(1000) => "%.6f",
                    _ => "%.9f",
                };
                (2, replacement)
            }
            _ => (0, "%"),
        };
        adapted.push_str(replacement);
        tail = &directive[consumed..];
    }
    adapted.push_str(tail);
    fields.to_string(&adapted).expect("valid time format")
}
