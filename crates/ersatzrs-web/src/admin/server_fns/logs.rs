use leptos::prelude::*;
use serde::{Deserialize, Serialize};

const LOGS_PAGE_SIZE_KEY: &str = "pages.logs.page_size";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminLogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdminLogSortField {
    #[default]
    Timestamp,
    Level,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminLogSnapshot {
    pub total_count: usize,
    pub page: u32,
    pub page_size: usize,
    pub entries: Vec<AdminLogEntry>,
}

#[server(prefix = "/admin-api")]
pub async fn get_logs_page_size() -> Result<u32, ServerFnError> {
    use crate::host::state::AppState;

    let state = expect_context::<AppState>();
    let saved = state
        .application
        .get_i32_setting(LOGS_PAGE_SIZE_KEY, 10)
        .await
        .map_err(ServerFnError::new)?;
    Ok(u32::try_from(saved).ok().unwrap_or(10))
}

#[server(prefix = "/admin-api")]
#[allow(clippy::unused_async, clippy::too_many_lines)]
pub async fn load_recent_log_entries(
    filter: String,
    page: u32,
    page_size: u32,
    sort_field: AdminLogSortField,
    sort_descending: Option<bool>,
) -> Result<AdminLogSnapshot, ServerFnError> {
    use crate::host::state::AppState;
    use chrono::Local;
    use std::path::PathBuf;

    fn newest_log_file() -> Option<PathBuf> {
        let log_dir = ersatzrs_infra::logging::default_log_dir();
        let mut files = std::fs::read_dir(log_dir)
            .ok()?
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.is_file())
            .collect::<Vec<_>>();
        files.sort();
        files.pop()
    }

    fn parse_serilog_line(line: &str) -> Option<ParsedEntry> {
        let open = line.find('[')?;
        let close = line[open..].find(']')? + open;
        let timestamp =
            chrono::DateTime::parse_from_str(line[..open].trim(), "%Y-%m-%d %H:%M:%S%.f %z")
                .or_else(|_| chrono::DateTime::parse_from_rfc3339(line[..open].trim()))
                .ok()?
                .with_timezone(&Local);
        let level = match &line[open + 1..close] {
            "DBG" => "Debug",
            "INF" => "Information",
            "WRN" => "Warning",
            "ERR" => "Error",
            "FTL" => "Fatal",
            _ => return None,
        };

        Some(ParsedEntry {
            timestamp,
            level,
            message: line[close + 1..].trim().to_owned(),
        })
    }

    fn parse_tracing_line(line: &str) -> Option<ParsedEntry> {
        let timestamp_end = line.find(char::is_whitespace)?;
        let timestamp = line[..timestamp_end].trim();
        let rest = line[timestamp_end..].trim_start();
        let level_end = rest.find(char::is_whitespace)?;
        let level = match &rest[..level_end] {
            "DEBUG" => "Debug",
            "INFO" => "Information",
            "WARN" => "Warning",
            "ERROR" => "Error",
            _ => return None,
        };
        let timestamp = chrono::DateTime::parse_from_rfc3339(timestamp)
            .ok()?
            .with_timezone(&Local);

        Some(ParsedEntry {
            timestamp,
            level,
            message: rest[level_end..].trim().to_owned(),
        })
    }

    let state = expect_context::<AppState>();
    crate::admin::server_fns::settings::save_config_value(
        state.application.ports().admin_settings(),
        LOGS_PAGE_SIZE_KEY,
        &page_size.to_string(),
    )
    .await?;

    let Some(log_file) = newest_log_file() else {
        return Ok(AdminLogSnapshot {
            page,
            page_size: usize::try_from(page_size).unwrap_or(usize::MAX),
            ..AdminLogSnapshot::default()
        });
    };

    let entries = std::fs::read_to_string(log_file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| parse_serilog_line(line).or_else(|| parse_tracing_line(line)))
        .collect::<Vec<_>>();
    let (total_count, entries) = sort_filter_page_entries(
        entries,
        &filter,
        page,
        page_size,
        sort_field,
        sort_descending,
    );

    let settings = state.application.ports().admin_settings();
    let language = crate::admin::server_fns::settings::load_ui_language_for_api(settings).await?;
    let notation = crate::admin::datetime::load_date_time_preferences(settings).await?;
    let entries = entries
        .into_iter()
        .map(|entry| AdminLogEntry {
            timestamp: crate::admin::datetime::format_date_time(
                &entry.timestamp,
                language,
                notation,
            ),
            level: entry.level.to_owned(),
            message: entry.message,
        })
        .collect();

    Ok(AdminLogSnapshot {
        total_count,
        page,
        page_size: usize::try_from(page_size).unwrap_or(usize::MAX),
        entries,
    })
}

#[derive(Debug, Clone)]
struct ParsedEntry {
    timestamp: chrono::DateTime<chrono::Local>,
    level: &'static str,
    message: String,
}

fn sort_filter_page_entries(
    mut entries: Vec<ParsedEntry>,
    filter: &str,
    page: u32,
    page_size: u32,
    sort_field: AdminLogSortField,
    sort_descending: Option<bool>,
) -> (usize, Vec<ParsedEntry>) {
    let filter = filter.trim().to_lowercase();
    entries.retain(|entry| {
        if filter.is_empty() {
            return true;
        }
        entry.level.to_lowercase().contains(&filter)
            || entry.message.to_lowercase().contains(&filter)
    });

    match sort_descending {
        Some(descending) => match sort_field {
            AdminLogSortField::Timestamp => {
                if descending {
                    entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp));
                } else {
                    entries.sort_by_key(|entry| entry.timestamp);
                }
            }
            AdminLogSortField::Level => {
                entries.sort_by(|left, right| {
                    let level_order = level_rank(left.level).cmp(&level_rank(right.level));
                    let ordering = if descending {
                        level_order.reverse()
                    } else {
                        level_order
                    };
                    ordering.then_with(|| right.timestamp.cmp(&left.timestamp))
                });
            }
        },
        None => entries.sort_by_key(|entry| std::cmp::Reverse(entry.timestamp)),
    }

    let total_count = entries.len();
    let start = if page_size == 0 {
        0
    } else {
        usize::try_from(u64::from(page) * u64::from(page_size)).unwrap_or(usize::MAX)
    };
    let page_size = if page_size == 0 {
        usize::MAX
    } else {
        usize::try_from(page_size).unwrap_or(usize::MAX)
    };
    let page = entries.into_iter().skip(start).take(page_size).collect();
    (total_count, page)
}

fn level_rank(level: &str) -> u8 {
    match level {
        "Verbose" | "Trace" => 0,
        "Debug" => 1,
        "Information" => 2,
        "Warning" => 3,
        "Error" => 4,
        "Fatal" => 5,
        _ => 6,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Local, TimeZone};

    fn entry(second: u32, level: &'static str, message: &str) -> ParsedEntry {
        ParsedEntry {
            timestamp: Local.with_ymd_and_hms(2026, 6, 17, 12, 0, second).unwrap(),
            level,
            message: message.to_owned(),
        }
    }

    #[test]
    fn logs_default_to_timestamp_descending() {
        let (_, page) = sort_filter_page_entries(
            vec![
                entry(1, "Information", "first"),
                entry(3, "Warning", "third"),
                entry(2, "Error", "second"),
            ],
            "",
            0,
            10,
            AdminLogSortField::Timestamp,
            None,
        );

        let messages = page
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>();
        assert_eq!(messages, ["third", "second", "first"]);
    }

    #[test]
    fn logs_sort_level_with_timestamp_tiebreaker_and_page() {
        let (total, page) = sort_filter_page_entries(
            vec![
                entry(1, "Information", "info-old"),
                entry(4, "Information", "info-new"),
                entry(3, "Error", "error"),
                entry(2, "Debug", "debug"),
            ],
            "",
            1,
            2,
            AdminLogSortField::Level,
            Some(false),
        );

        let messages = page
            .into_iter()
            .map(|entry| entry.message)
            .collect::<Vec<_>>();
        assert_eq!(total, 4);
        assert_eq!(messages, ["info-old", "error"]);
    }

    #[test]
    fn logs_filter_level_and_message_case_insensitive() {
        let (total, page) = sort_filter_page_entries(
            vec![
                entry(1, "Information", "started"),
                entry(2, "Warning", "Missing file"),
                entry(3, "Error", "failed"),
            ],
            "warn",
            0,
            10,
            AdminLogSortField::Timestamp,
            None,
        );

        assert_eq!(total, 1);
        assert_eq!(page[0].message, "Missing file");
    }

    #[test]
    fn logs_all_rows_ignore_the_old_page_and_preserve_filter_and_sort() {
        let (total, page) = sort_filter_page_entries(
            vec![
                entry(1, "Information", "match old"),
                entry(2, "Warning", "unrelated"),
                entry(3, "Error", "match new"),
            ],
            "match",
            9,
            0,
            AdminLogSortField::Timestamp,
            Some(false),
        );
        assert_eq!(total, 2);
        assert_eq!(
            page.into_iter()
                .map(|entry| entry.message)
                .collect::<Vec<_>>(),
            ["match old", "match new"]
        );
    }
}
