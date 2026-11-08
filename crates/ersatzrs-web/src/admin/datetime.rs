//! Localized date and time notation for the admin UI.
//!
//! Every admin timestamp is rendered on the server in the server's time zone
//! (maintainer decision 2026-11-08) and labelled with its UTC offset. The
//! default notation follows the selected UI language, using the convention of
//! the largest group of speakers of that language (for example `nl` follows
//! the Netherlands, `es-419` follows Mexico). Operators can override the date
//! order and the 12/24-hour clock in Interface settings.

use chrono::{DateTime, Offset, TimeZone};
use serde::{Deserialize, Serialize};

use crate::admin::i18n::UiLanguage;

/// Order of day, month and year.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum DateOrder {
    /// Follow the UI language.
    #[default]
    Auto,
    /// Day-month-year.
    Dmy,
    /// Month/day/year.
    Mdy,
    /// ISO 8601 year-month-day.
    Ymd,
}

/// 12- or 24-hour clock.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ClockFormat {
    /// Follow the UI language.
    #[default]
    Auto,
    H24,
    H12,
}

impl DateOrder {
    #[must_use]
    pub fn from_config(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "dmy" => Self::Dmy,
            "mdy" => Self::Mdy,
            "ymd" => Self::Ymd,
            _ => Self::Auto,
        }
    }

    #[must_use]
    pub fn config_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Dmy => "dmy",
            Self::Mdy => "mdy",
            Self::Ymd => "ymd",
        }
    }
}

impl ClockFormat {
    #[must_use]
    pub fn from_config(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "24h" => Self::H24,
            "12h" => Self::H12,
            _ => Self::Auto,
        }
    }

    #[must_use]
    pub fn config_value(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::H24 => "24h",
            Self::H12 => "12h",
        }
    }
}

/// The operator's notation preferences.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateTimePreferences {
    pub date: DateOrder,
    pub clock: ClockFormat,
}

/// Concrete notation after resolving `Auto` against the UI language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Notation {
    date_pattern: &'static str,
    clock: ClockFormat,
}

/// Default notation per UI language (largest regional group of speakers).
fn language_notation(language: UiLanguage) -> Notation {
    let (date_pattern, clock) = match language.code() {
        "en-us" => ("%-m/%-d/%Y", ClockFormat::H12),
        "nl" => ("%d-%m-%Y", ClockFormat::H24),
        "de-de" | "pl" => ("%d.%m.%Y", ClockFormat::H24),
        "es-es" => ("%-d/%-m/%Y", ClockFormat::H24),
        // fr-FR, it-IT, pt-BR and Latin American Spanish (Mexico) all write
        // day/month/year with a 24-hour clock.
        _ => ("%d/%m/%Y", ClockFormat::H24),
    };
    Notation {
        date_pattern,
        clock,
    }
}

fn resolve(language: UiLanguage, preferences: DateTimePreferences) -> Notation {
    let default = language_notation(language);
    let date_pattern = match preferences.date {
        DateOrder::Auto => default.date_pattern,
        DateOrder::Dmy => "%d-%m-%Y",
        DateOrder::Mdy => "%m/%d/%Y",
        DateOrder::Ymd => "%Y-%m-%d",
    };
    let clock = match preferences.clock {
        ClockFormat::Auto => default.clock,
        explicit => explicit,
    };
    Notation {
        date_pattern,
        clock,
    }
}

/// Format a date and time with seconds, followed by its UTC offset label.
#[must_use]
pub fn format_date_time<Tz>(
    value: &DateTime<Tz>,
    language: UiLanguage,
    preferences: DateTimePreferences,
) -> String
where
    Tz: TimeZone,
    Tz::Offset: std::fmt::Display,
{
    let notation = resolve(language, preferences);
    let time_pattern = match notation.clock {
        ClockFormat::H12 => "%-I:%M:%S %p",
        _ => "%H:%M:%S",
    };
    format!(
        "{} {} {}",
        value.format(notation.date_pattern),
        value.format(time_pattern),
        offset_label(value)
    )
}

/// `UTC`, `UTC+1`, `UTC+5:30` or `UTC-3` for the value's offset.
fn offset_label<Tz>(value: &DateTime<Tz>) -> String
where
    Tz: TimeZone,
{
    let seconds = value.offset().fix().local_minus_utc();
    if seconds == 0 {
        return "UTC".to_owned();
    }
    let sign = if seconds < 0 { '-' } else { '+' };
    let minutes = seconds.unsigned_abs() / 60;
    let (hours, minutes) = (minutes / 60, minutes % 60);
    if minutes == 0 {
        format!("UTC{sign}{hours}")
    } else {
        format!("UTC{sign}{hours}:{minutes:02}")
    }
}

/// Load the stored notation preferences.
#[cfg(feature = "ssr")]
pub async fn load_date_time_preferences(
    repository: &dyn ersatzrs_core::repositories::AdminSettingsRepository,
) -> DateTimePreferences {
    let date = ersatzrs_app::queries::get_string_setting(
        ersatzrs_domain::CONFIG_KEY_PAGES_DATE_FORMAT,
        "auto",
        repository,
    )
    .await
    .unwrap_or_default();
    let clock = ersatzrs_app::queries::get_string_setting(
        ersatzrs_domain::CONFIG_KEY_PAGES_TIME_FORMAT,
        "auto",
        repository,
    )
    .await
    .unwrap_or_default();
    DateTimePreferences {
        date: DateOrder::from_config(&date),
        clock: ClockFormat::from_config(&clock),
    }
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, TimeZone, Utc};

    use super::{format_date_time, ClockFormat, DateOrder, DateTimePreferences};
    use crate::admin::i18n::UiLanguage;

    fn sample() -> chrono::DateTime<FixedOffset> {
        FixedOffset::east_opt(3600)
            .unwrap()
            .with_ymd_and_hms(2026, 11, 8, 23, 5, 9)
            .unwrap()
    }

    #[test]
    fn language_defaults_follow_the_largest_regional_group() {
        let auto = DateTimePreferences::default();
        let cases = [
            ("nl", "08-11-2026 23:05:09 UTC+1"),
            ("en-us", "11/8/2026 11:05:09 PM UTC+1"),
            ("de-de", "08.11.2026 23:05:09 UTC+1"),
            ("fr-fr", "08/11/2026 23:05:09 UTC+1"),
            ("es-es", "8/11/2026 23:05:09 UTC+1"),
            ("es-419", "08/11/2026 23:05:09 UTC+1"),
            ("pt-br", "08/11/2026 23:05:09 UTC+1"),
            ("pl", "08.11.2026 23:05:09 UTC+1"),
        ];
        for (code, expected) in cases {
            let language = UiLanguage::from_config(code);
            assert_eq!(
                format_date_time(&sample(), language, auto),
                expected,
                "{code}"
            );
        }
    }

    #[test]
    fn explicit_preferences_override_the_language() {
        let preferences = DateTimePreferences {
            date: DateOrder::Ymd,
            clock: ClockFormat::H12,
        };
        assert_eq!(
            format_date_time(&sample(), UiLanguage::from_config("nl"), preferences),
            "2026-11-08 11:05:09 PM UTC+1"
        );
    }

    #[test]
    fn utc_and_fractional_offsets_are_labelled() {
        let auto = DateTimePreferences::default();
        let utc = Utc.with_ymd_and_hms(2026, 1, 2, 3, 4, 5).unwrap();
        assert_eq!(
            format_date_time(&utc, UiLanguage::from_config("nl"), auto),
            "02-01-2026 03:04:05 UTC"
        );
        let india = FixedOffset::east_opt(19_800)
            .unwrap()
            .with_ymd_and_hms(2026, 1, 2, 3, 4, 5)
            .unwrap();
        assert!(
            format_date_time(&india, UiLanguage::from_config("nl"), auto).ends_with("UTC+5:30")
        );
    }

    #[test]
    fn unknown_config_values_fall_back_to_auto() {
        assert_eq!(DateOrder::from_config("bogus"), DateOrder::Auto);
        assert_eq!(ClockFormat::from_config(""), ClockFormat::Auto);
        assert_eq!(DateOrder::from_config("YMD"), DateOrder::Ymd);
        assert_eq!(
            ClockFormat::from_config(ClockFormat::H12.config_value()),
            ClockFormat::H12
        );
    }
}
