use super::{profile, temporal, ExpressionError, Result};
use chrono::{Duration, NaiveDateTime};
use std::cmp::Ordering;

#[derive(Clone, Debug, PartialEq)]
pub(super) enum Value {
    Null,
    Boolean(bool),
    Int(i32),
    Long(i64),
    Unsigned(u64),
    Real(f64),
    Text(String),
    List(Vec<Self>),
    Date(NaiveDateTime),
    Time(Duration),
    Guid(uuid::Uuid),
}

impl Value {
    pub(super) fn integer(value: i64) -> Self {
        i32::try_from(value).map_or(Self::Long(value), Self::Int)
    }

    pub(super) fn number(text: &str) -> Result<Self> {
        let text = text.strip_prefix('+').unwrap_or(text);
        for (prefix, radix) in [("0x", 16), ("0o", 8), ("0b", 2)] {
            if let Some(digits) = text.strip_prefix(prefix) {
                // Convert.ToInt64(radix) accepts a full two's-complement
                // 64-bit literal, unlike decimal Int64.Parse.
                return u64::from_str_radix(digits, radix)
                    .map(|value| Self::integer(i64::from_ne_bytes(value.to_ne_bytes())))
                    .map_err(|_| ExpressionError);
            }
        }
        if !text.contains(['.', 'e', 'E']) {
            if let Ok(value) = text.parse::<i64>() {
                return Ok(Self::integer(value));
            }
        }
        text.parse::<f64>()
            .map(Self::Real)
            .map_err(|_| ExpressionError)
    }

    pub(super) fn boolean(&self) -> Result<bool> {
        match self {
            Self::Null => Ok(false),
            Self::Boolean(value) => Ok(*value),
            Self::Int(value) => Ok(*value != 0),
            Self::Long(value) => Ok(*value != 0),
            Self::Unsigned(value) => Ok(*value != 0),
            Self::Real(value) => Ok(*value != 0.0),
            Self::Text(value) if value.trim().eq_ignore_ascii_case("true") => Ok(true),
            Self::Text(value) if value.trim().eq_ignore_ascii_case("false") => Ok(false),
            _ => Err(ExpressionError),
        }
    }

    #[allow(clippy::cast_precision_loss)] // Mirrors .NET Convert.ToDouble.
    pub(super) fn real(&self) -> Result<f64> {
        match self {
            Self::Null => Ok(0.0),
            Self::Boolean(value) => Ok(f64::from(u8::from(*value))),
            Self::Int(value) => Ok(f64::from(*value)),
            Self::Long(value) => Ok(*value as f64),
            Self::Unsigned(value) => Ok(*value as f64),
            Self::Real(value) => Ok(*value),
            Self::Text(value) => profile::real(value),
            _ => Err(ExpressionError),
        }
    }

    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // Checked conversion, banker's rounding matches Convert.ToUInt64.
    pub(super) fn unsigned(&self) -> Result<u64> {
        match self {
            Self::Unsigned(value) => Ok(*value),
            Self::Int(value) => u64::try_from(*value).map_err(|_| ExpressionError),
            Self::Long(value) => u64::try_from(*value).map_err(|_| ExpressionError),
            Self::Text(value) => profile::numeric_text(value)
                .parse()
                .map_err(|_| ExpressionError),
            _ => {
                let value = self.real()?.round_ties_even();
                if (0.0..18_446_744_073_709_551_616.0).contains(&value) {
                    Ok(value as u64)
                } else {
                    Err(ExpressionError)
                }
            }
        }
    }

    #[allow(clippy::cast_possible_truncation)] // Range checked before conversion.
    pub(super) fn int32(&self) -> Result<i32> {
        if let Self::Text(value) = self {
            return profile::numeric_text(value)
                .parse()
                .map_err(|_| ExpressionError);
        }
        let value = self.real()?.round_ties_even();
        if (f64::from(i32::MIN)..=f64::from(i32::MAX)).contains(&value) {
            Ok(value as i32)
        } else {
            Err(ExpressionError)
        }
    }

    pub(super) fn text(&self) -> String {
        match self {
            Self::Null => String::new(),
            Self::Boolean(true) => "True".into(),
            Self::Boolean(false) => "False".into(),
            Self::Int(v) => v.to_string(),
            Self::Long(v) => v.to_string(),
            Self::Unsigned(v) => v.to_string(),
            Self::Real(v) => v.to_string(),
            Self::Text(v) => v.clone(),
            Self::Guid(v) => v.to_string(),
            Self::Date(v) => v.format("%m/%d/%Y %H:%M:%S").to_string(),
            Self::Time(v) => v.num_seconds().to_string(),
            Self::List(_) => "System.Collections.Generic.List`1[System.Object]".into(),
        }
    }

    pub(super) fn is_blank(&self) -> bool {
        matches!(self, Self::Null) || matches!(self, Self::Text(s) if s.trim().is_empty())
    }

    pub(super) fn compare(&self, other: &Self) -> Result<Ordering> {
        match (self, other) {
            (Self::Null, Self::Null) => Ok(Ordering::Equal),
            (Self::Null, _) => Ok(Ordering::Less),
            (_, Self::Null) => Ok(Ordering::Greater),
            (Self::Text(a), Self::Text(b)) => profile::compare(a, b),
            (Self::Date(a), Self::Date(b)) => Ok(a.cmp(b)),
            (Self::Time(a), Self::Time(b)) => Ok(a.cmp(b)),
            (Self::Guid(a), Self::Guid(b)) => Ok(a.cmp(b)),
            (Self::Boolean(_), Self::Text(_) | Self::Boolean(_))
            | (Self::Text(_), Self::Boolean(_)) => Ok(self.boolean()?.cmp(&other.boolean()?)),
            (Self::Int(a), Self::Int(b)) => Ok(a.cmp(b)),
            (Self::Long(a), Self::Long(b)) => Ok(a.cmp(b)),
            (Self::Int(a), Self::Long(b)) => Ok(i64::from(*a).cmp(b)),
            (Self::Long(a), Self::Int(b)) => Ok(a.cmp(&i64::from(*b))),
            (Self::Long(a), Self::Unsigned(b)) => {
                Ok(a.cmp(&i64::try_from(*b).map_err(|_| ExpressionError)?))
            }
            (Self::Unsigned(a), Self::Long(b)) => {
                Ok(i64::try_from(*a).map_err(|_| ExpressionError)?.cmp(b))
            }
            (Self::Unsigned(a), Self::Unsigned(b)) => Ok(a.cmp(b)),
            (Self::Int(a), Self::Text(b)) => Ok(a.cmp(
                &profile::numeric_text(b)
                    .parse::<i32>()
                    .map_err(|_| ExpressionError)?,
            )),
            (Self::Text(a), Self::Int(b)) => Ok(profile::numeric_text(a)
                .parse::<i32>()
                .map_err(|_| ExpressionError)?
                .cmp(b)),
            (Self::Long(a), Self::Text(b)) => Ok(a.cmp(
                &profile::numeric_text(b)
                    .parse::<i64>()
                    .map_err(|_| ExpressionError)?,
            )),
            (Self::Text(a), Self::Long(b)) => Ok(profile::numeric_text(a)
                .parse::<i64>()
                .map_err(|_| ExpressionError)?
                .cmp(b)),
            _ => {
                let (a, b) = (self.real()?, other.real()?);
                // Double.CompareTo orders NaN below all other values, including -infinity.
                Ok(if a.is_nan() {
                    if b.is_nan() {
                        Ordering::Equal
                    } else {
                        Ordering::Less
                    }
                } else if b.is_nan() {
                    Ordering::Greater
                } else {
                    a.partial_cmp(&b).ok_or(ExpressionError)?
                })
            }
        }
    }

    pub(super) fn temporal(text: &str) -> Result<Self> {
        temporal::parse(text)
    }
}

pub(super) fn unescape(text: &str) -> Result<String> {
    let mut chars = text[1..text.len() - 1].chars().peekable();
    let mut result = Vec::<u16>::new();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            let mut buffer = [0; 2];
            result.extend_from_slice(ch.encode_utf16(&mut buffer));
            continue;
        }
        let escaped = match chars.next().ok_or(ExpressionError)? {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            'b' => '\u{8}',
            'f' => '\u{c}',
            'a' => '\u{7}',
            'v' => '\u{b}',
            '0' => '\0',
            '\\' => '\\',
            '\'' => '\'',
            '"' => '"',
            'u' => {
                let digits: String = chars.by_ref().take(4).collect();
                if digits.len() != 4 {
                    return Err(ExpressionError);
                }
                result.push(u16::from_str_radix(&digits, 16).map_err(|_| ExpressionError)?);
                continue;
            }
            'x' => {
                let mut digits = String::new();
                for _ in 0..4 {
                    if chars.peek().is_some_and(char::is_ascii_hexdigit) {
                        digits.push(chars.next().ok_or(ExpressionError)?);
                    } else {
                        break;
                    }
                }
                result.push(u16::from_str_radix(&digits, 16).map_err(|_| ExpressionError)?);
                continue;
            }
            _ => return Err(ExpressionError),
        };
        let mut buffer = [0; 2];
        result.extend_from_slice(escaped.encode_utf16(&mut buffer));
    }
    String::from_utf16(&result).map_err(|_| ExpressionError)
}
