//! Upstream `NextLogger` framing with bounded, sanitized process diagnostics.

use std::{
    collections::VecDeque,
    path::Path,
    sync::{Arc, Mutex},
    time::Duration,
};

use tokio::{
    io::{AsyncRead, AsyncReadExt as _},
    process::Command,
    task::JoinSet,
};

use super::protocol::ChannelConfig;

const MAX_LINE_BYTES: usize = 8 * 1024;
const MAX_LOG_BYTES: usize = 256 * 1024;
const OMITTED: &str = "[WARN] Earlier diagnostic lines omitted at the retained-tail limit\n";

#[derive(Default)]
struct Tail {
    lines: VecDeque<String>,
    bytes: usize,
    truncated: bool,
}

impl Tail {
    fn push(&mut self, line: String) {
        self.bytes += line.len();
        self.lines.push_back(line);
        self.truncated |= self.bytes > MAX_LOG_BYTES;
        let limit = MAX_LOG_BYTES - if self.truncated { OMITTED.len() } else { 0 };
        while self.bytes > limit {
            if let Some(line) = self.lines.pop_front() {
                self.bytes -= line.len();
            }
        }
    }

    fn text(&self) -> String {
        let mut text = if self.truncated {
            OMITTED.to_owned()
        } else {
            String::new()
        };
        for line in &self.lines {
            text.push_str(line);
        }
        text
    }
}

/// Bounded decision messages for an explicitly requested troubleshooting preview.
/// Ordinary scheduled projections do not allocate or populate this log.
#[derive(Clone, Default)]
pub struct SelectionLog(Arc<Mutex<Tail>>);

impl SelectionLog {
    pub(crate) fn debug(&self, message: &str) {
        let message = if message.len() > MAX_LINE_BYTES {
            "Oversized selection diagnostic discarded"
        } else {
            message
        };
        record(
            &format!("[selection DEBUG selection] {message}"),
            &self.0,
            &[],
        );
    }

    /// Sanitized messages accumulated during preparation and provider callbacks.
    #[must_use]
    pub fn text(&self) -> String {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .text()
    }
}

pub(super) struct Diagnostics {
    readers: JoinSet<()>,
    tail: Arc<Mutex<Tail>>,
    redactions: Arc<Vec<String>>,
}

impl Diagnostics {
    pub(super) fn new(command: &Command, config: &ChannelConfig, output: &Path) -> Self {
        let mut values = vec![
            config.playout.folder.clone(),
            config.ffmpeg.ffmpeg_path.clone(),
            config.ffmpeg.ffprobe_path.clone(),
            output.to_string_lossy().into_owned(),
        ];
        values.extend(config.ffmpeg.reports_folder.clone());
        values.extend(config.normalization.subtitle.fonts_folder.clone());
        values.retain(|value| value.contains('/') || value.contains('\\'));
        if let Some(value) = command
            .as_std()
            .get_envs()
            .find(|(key, _)| *key == super::media::SESSION_ENV)
            .and_then(|(_, value)| value)
        {
            values.push(value.to_string_lossy().into_owned());
        }
        values.retain(|value| !value.is_empty());
        values.sort_unstable_by_key(|value| std::cmp::Reverse(value.len()));
        values.dedup();
        Self {
            readers: JoinSet::new(),
            tail: Arc::default(),
            redactions: Arc::new(values),
        }
    }

    pub(super) fn read(&mut self, stream: impl AsyncRead + Unpin + Send + 'static) {
        let tail = Arc::clone(&self.tail);
        let redactions = Arc::clone(&self.redactions);
        self.readers.spawn(drain(stream, tail, redactions));
    }

    pub(super) fn text(&self) -> String {
        let tail = self
            .tail
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        tail.text()
    }

    pub(super) async fn finish(&mut self) {
        // After managed tree cleanup both pipes should reach EOF. Bound reader
        // completion too; JoinSet also aborts all readers when this owner drops.
        if tokio::time::timeout(Duration::from_secs(1), async {
            while self.readers.join_next().await.is_some() {}
        })
        .await
        .is_err()
        {
            self.readers.abort_all();
            while self.readers.join_next().await.is_some() {}
        }
    }
}

async fn drain(
    mut stream: impl AsyncRead + Unpin,
    tail: Arc<Mutex<Tail>>,
    redactions: Arc<Vec<String>>,
) {
    let mut buffer = [0; 4096];
    let mut line = Vec::new();
    let mut oversized = false;
    loop {
        let length = match stream.read(&mut buffer).await {
            Ok(0) => break,
            Ok(length) => length,
            Err(_) => {
                record("Next diagnostic stream read failed", &tail, &redactions);
                return;
            }
        };
        for byte in &buffer[..length] {
            if *byte == b'\n' {
                record_line(&line, oversized, &tail, &redactions);
                line.clear();
                oversized = false;
            } else if line.len() < MAX_LINE_BYTES {
                line.push(*byte);
            } else {
                oversized = true;
            }
        }
    }
    if !line.is_empty() || oversized {
        record_line(&line, oversized, &tail, &redactions);
    }
}

fn record_line(line: &[u8], oversized: bool, tail: &Mutex<Tail>, redactions: &[String]) {
    // Do not retain fragments that could split a credential/path across the cap.
    if oversized {
        record("Oversized Next diagnostic line discarded", tail, redactions);
    } else {
        record(
            String::from_utf8_lossy(line).trim_end_matches('\r'),
            tail,
            redactions,
        );
    }
}

fn record(line: &str, tail: &Mutex<Tail>, redactions: &[String]) {
    if line.is_empty() {
        return;
    }
    let json = parse_json(line);
    let (level, target, message) = json.as_ref().map_or_else(
        || parse(line),
        |(level, target, message)| (*level, target.as_str(), message.as_str()),
    );
    let target = sanitize(target, redactions);
    let message = sanitize(message, redactions);
    match level {
        "ERROR" => tracing::error!(next_target = %target, "{message}"),
        "WARN" => tracing::warn!(next_target = %target, "{message}"),
        "INFO" => tracing::info!(next_target = %target, "{message}"),
        "TRACE" => tracing::trace!(next_target = %target, "{message}"),
        _ => tracing::debug!(next_target = %target, "{message}"),
    }
    let time = chrono::Utc::now().format("%H:%M:%S");
    tail.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .push(format!("[{time} {level}] [{target}] {message}\n"));
}

fn parse(line: &str) -> (&str, &str, &str) {
    if let Some((prefix, message)) = line
        .strip_prefix('[')
        .and_then(|line| line.split_once("] "))
    {
        let mut parts = prefix.splitn(3, ' ');
        if let (Some(timestamp), Some(level), Some(target)) =
            (parts.next(), parts.next(), parts.next())
        {
            if !timestamp.is_empty()
                && !timestamp.chars().any(char::is_whitespace)
                && !target.is_empty()
                && !target.contains(']')
                && matches!(level, "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR")
            {
                return (level, target, message);
            }
        }
    }
    ("DEBUG", "next", line)
}

/// Upstream `NextLogger` JSON framing, retaining older text-runtime compatibility.
fn parse_json(line: &str) -> Option<(&'static str, String, String)> {
    if !line.trim_start().starts_with('{') {
        return None;
    }
    let value: serde_json::Value = serde_json::from_str(line).ok()?;
    let message = value.get("message")?.as_str()?.to_owned();
    let level = match value.get("level")?.as_str()? {
        "ERROR" => "ERROR",
        "WARN" => "WARN",
        "INFO" => "INFO",
        "TRACE" => "TRACE",
        _ => "DEBUG",
    };
    let channel = value.get("span").and_then(|span| span.get("number"));
    let channel = match channel {
        Some(serde_json::Value::String(number)) if !number.is_empty() => Some(number.clone()),
        Some(serde_json::Value::Number(number)) => Some(number.to_string()),
        _ => None,
    };
    let ffmpeg = value.get("target").and_then(serde_json::Value::as_str) == Some("ffmpeg");
    let target = match (channel, ffmpeg) {
        (Some(channel), true) => format!("next::channel {channel}::ffmpeg"),
        (Some(channel), false) => format!("next::channel {channel}"),
        (None, true) => "next::ffmpeg".to_owned(),
        (None, false) => "next".to_owned(),
    };
    Some((level, target, message))
}

fn sanitize(value: &str, redactions: &[String]) -> String {
    let value = redactions.iter().fold(value.to_owned(), |value, private| {
        value.replace(private, "[redacted]")
    });
    crate::remote_stream::sanitize_remote_stream_diagnostic(&value)
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .collect()
}

#[cfg(test)]
#[path = "diagnostics_tests.rs"]
mod tests;
