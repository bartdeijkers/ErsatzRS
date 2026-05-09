use leptos::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminLogEntry {
    pub timestamp: String,
    pub level: String,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminLogSnapshot {
    pub total_count: usize,
    pub page_size: usize,
    pub entries: Vec<AdminLogEntry>,
}

#[server]
pub async fn load_recent_log_entries() -> Result<AdminLogSnapshot, ServerFnError> {
    use chrono::{DateTime, Local};
    use std::path::PathBuf;

    #[derive(Debug)]
    struct ParsedEntry {
        timestamp: DateTime<Local>,
        level: &'static str,
        message: String,
    }

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

    let Some(log_file) = newest_log_file() else {
        return Ok(AdminLogSnapshot {
            page_size: 10,
            ..AdminLogSnapshot::default()
        });
    };

    let mut entries = std::fs::read_to_string(log_file)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| parse_serilog_line(line).or_else(|| parse_tracing_line(line)))
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));

    let total_count = entries.len();
    let entries = entries
        .into_iter()
        .take(10)
        .map(|entry| AdminLogEntry {
            timestamp: entry
                .timestamp
                .format("%-m/%-d/%Y %-I:%M:%S %p")
                .to_string(),
            level: entry.level.to_owned(),
            message: entry.message,
        })
        .collect();

    Ok(AdminLogSnapshot {
        total_count,
        page_size: 10,
        entries,
    })
}
