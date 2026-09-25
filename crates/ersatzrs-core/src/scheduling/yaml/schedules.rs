//! Date selection for Sequential YAML `schedules`, matching
//! `AlternateScheduleSelector` and `YamlPlayoutScheduleItem` in ErsatzTV.

use chrono::{DateTime, Datelike, Duration, Local, NaiveDate, Utc};

use super::YamlPlayoutScheduleItem;

pub(super) fn select_schedule(
    schedules: &[YamlPlayoutScheduleItem],
    time: DateTime<Utc>,
) -> Option<&YamlPlayoutScheduleItem> {
    let date = time.with_timezone(&Local).date_naive();
    let mut selected: Option<&YamlPlayoutScheduleItem> = None;
    for schedule in schedules {
        if schedule_matches(schedule, date)
            && selected.is_none_or(|current| schedule.priority > current.priority)
        {
            selected = Some(schedule);
        }
    }
    selected
}

fn schedule_matches(schedule: &YamlPlayoutScheduleItem, date: NaiveDate) -> bool {
    let (Some(start), Some(end)) = (
        parse_schedule_date(&schedule.start_date),
        parse_schedule_date(&schedule.end_date),
    ) else {
        return false;
    };

    let (start_year, end_year, reverse) = match (start.year, end.year) {
        (Some(start_year), Some(end_year)) => (start_year, end_year, false),
        _ => (
            date.year(),
            date.year(),
            (start.month, start.day) > (end.month, end.day),
        ),
    };
    let Some(start_date) = start_range_date(start_year, start.month, start.day) else {
        return false;
    };
    let Some(end_date) = end_range_date(end_year, end.month, end.day) else {
        return false;
    };
    if reverse {
        date <= end_date || date >= start_date
    } else {
        date >= start_date && date <= end_date
    }
}

struct ParsedScheduleDate {
    year: Option<i32>,
    month: u32,
    day: u32,
}

fn parse_schedule_date(value: &str) -> Option<ParsedScheduleDate> {
    let value = value.trim();
    if value.len() == 10 {
        let date = NaiveDate::parse_from_str(value, "%Y-%m-%d").ok()?;
        return Some(ParsedScheduleDate {
            year: Some(date.year()),
            month: date.month(),
            day: date.day(),
        });
    }
    let (month, day) = value.split_once('-')?;
    let month = month.parse::<u32>().ok()?;
    let day = day.parse::<u32>().ok()?;
    (1..=12).contains(&month).then_some(())?;
    (day >= 1).then_some(())?;
    Some(ParsedScheduleDate {
        year: None,
        month,
        day,
    })
}

fn start_range_date(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day).or_else(|| {
        let first = NaiveDate::from_ymd_opt(year, month, 1)?;
        first.checked_add_signed(Duration::days(i64::from(days_in_month(year, month))))
    })
}

fn end_range_date(year: i32, month: u32, day: u32) -> Option<NaiveDate> {
    NaiveDate::from_ymd_opt(year, month, day)
        .or_else(|| NaiveDate::from_ymd_opt(year, month, days_in_month(year, month)))
}

fn days_in_month(year: i32, month: u32) -> u32 {
    (28..=31)
        .rev()
        .find(|day| NaiveDate::from_ymd_opt(year, month, *day).is_some())
        .unwrap_or(28)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn item(name: &str, start: &str, end: &str, priority: i32) -> YamlPlayoutScheduleItem {
        YamlPlayoutScheduleItem {
            name: name.to_owned(),
            start_date: start.to_owned(),
            end_date: end.to_owned(),
            priority,
            ..YamlPlayoutScheduleItem::default()
        }
    }

    fn selected_name(
        schedules: &[YamlPlayoutScheduleItem],
        year: i32,
        month: u32,
        day: u32,
    ) -> Option<&str> {
        select_schedule(
            schedules,
            Local
                .with_ymd_and_hms(year, month, day, 12, 0, 0)
                .single()
                .expect("test date")
                .with_timezone(&Utc),
        )
        .map(|item| item.name.as_str())
    }

    #[test]
    fn selects_annual_specific_and_wrapping_ranges() {
        let april = item("April", "04-01", "04-30", 0);
        assert_eq!(
            selected_name(std::slice::from_ref(&april), 2025, 4, 1),
            Some("April")
        );
        assert_eq!(selected_name(&[april], 2030, 5, 1), None);

        let christmas = item("Christmas", "2025-12-25", "2025-12-25", 0);
        assert_eq!(
            selected_name(std::slice::from_ref(&christmas), 2025, 12, 25),
            Some("Christmas")
        );
        assert_eq!(selected_name(&[christmas], 2026, 12, 25), None);

        let winter = item("Winter", "12-01", "01-15", 0);
        assert_eq!(
            selected_name(std::slice::from_ref(&winter), 2025, 12, 20),
            Some("Winter")
        );
        assert_eq!(
            selected_name(std::slice::from_ref(&winter), 2026, 1, 10),
            Some("Winter")
        );
        assert_eq!(selected_name(&[winter], 2026, 6, 1), None);
    }

    #[test]
    fn higher_priority_wins_and_ties_keep_definition_order() {
        let december = item("December", "12-01", "12-31", 0);
        let christmas = item("Christmas", "12-25", "12-25", 10);
        let same_priority = item("Later", "12-25", "12-25", 10);
        let schedules = [december, christmas, same_priority];
        assert_eq!(selected_name(&schedules, 2025, 12, 25), Some("Christmas"));
        assert_eq!(selected_name(&schedules, 2025, 12, 20), Some("December"));
    }

    #[test]
    fn invalid_dates_never_select_and_annual_overflow_matches_upstream() {
        for (start, end) in [
            ("not-a-date", "04-30"),
            ("00-01", "04-30"),
            ("04-00", "04-30"),
            ("2025-02-29", "03-01"),
        ] {
            assert_eq!(
                selected_name(&[item("Invalid", start, end, 0)], 2025, 4, 15),
                None
            );
        }
        let leap = item("Leap", "02-29", "03-01", 0);
        assert_eq!(
            selected_name(std::slice::from_ref(&leap), 2024, 2, 29),
            Some("Leap")
        );
        assert_eq!(selected_name(&[leap], 2025, 3, 1), Some("Leap"));
    }
}
