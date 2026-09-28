//! External JSON channel timelines, matching ErsatzTV's `ExternalJsonChannel`
//! and `ExternalJsonPlayoutItemProvider`. These items are never scheduler rows.

use std::{path::Path, time::Duration as StdDuration};

use chrono::{DateTime, Duration, NaiveDateTime, Utc};
use serde::Deserialize;
use tokio::io::AsyncReadExt as _;

const MAX_SCHEDULE_BYTES: u64 = 32 * 1024 * 1024;
const READ_TIMEOUT: StdDuration = StdDuration::from_secs(5);

pub(crate) mod guide;
pub mod playback;

/// Failure categories deliberately exclude input values, paths and provider URLs.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ExternalJsonError {
    #[error("external JSON schedule could not be read")]
    Read,
    #[error("external JSON schedule exceeds the 32 MiB limit")]
    TooLarge,
    #[error("external JSON schedule is not valid channel JSON")]
    Json,
    #[error("external JSON schedule has an invalid start time")]
    StartTime,
    #[error("external JSON schedule time exceeds the supported range")]
    TimeRange,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ChannelDocument {
    start_time: Option<String>,
    #[serde(default)]
    guide_minimum_duration_seconds: i32,
    programs: Vec<ExternalJsonProgram>,
}

/// Upstream wire fields. No `Debug`: paths and Plex keys may be private.
#[derive(Clone, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExternalJsonProgram {
    pub title: Option<String>,
    pub show_title: Option<String>,
    pub season: i32,
    pub episode: i32,
    pub key: Option<String>,
    pub rating_key: Option<String>,
    pub icon: Option<String>,
    pub year: Option<i32>,
    #[serde(rename = "type")]
    pub kind: Option<String>,
    /// Milliseconds, as in ExternalJsonProgram.Duration.
    pub duration: i32,
    pub plex_file: Option<String>,
    pub file: Option<String>,
    pub server_key: Option<String>,
}

/// One ephemeral programme and its absolute timeline interval.
pub struct ExternalJsonItem {
    pub start: DateTime<Utc>,
    pub finish: DateTime<Utc>,
    pub program: ExternalJsonProgram,
}

/// A finite external timeline; there is no implicit repeating cycle.
pub struct ExternalJsonSchedule {
    pub guide_minimum_duration_seconds: i32,
    pub items: Vec<ExternalJsonItem>,
}

impl ExternalJsonSchedule {
    /// Match upstream's first item whose finish is later than the playback time.
    /// An item before the declared start is retained, rather than cycling.
    #[must_use]
    pub fn active_item(&self, now: DateTime<Utc>) -> Option<&ExternalJsonItem> {
        self.items.iter().find(|item| item.finish > now)
    }
}

/// Read the current file on demand, so atomic file replacement is visible.
/// Missing files and JSON `null` have no programme, matching upstream.
///
/// # Errors
/// Returns redacted read, size, JSON or timeline validation failures. A regular
/// file and bounded read are required to keep request work finite.
pub async fn load(path: &Path) -> Result<Option<ExternalJsonSchedule>, ExternalJsonError> {
    tokio::time::timeout(READ_TIMEOUT, read(path))
        .await
        .map_err(|_| ExternalJsonError::Read)?
}

async fn read(path: &Path) -> Result<Option<ExternalJsonSchedule>, ExternalJsonError> {
    let metadata = match tokio::fs::metadata(path).await {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(ExternalJsonError::Read),
    };
    if !metadata.is_file() {
        return Err(ExternalJsonError::Read);
    }
    if metadata.len() > MAX_SCHEDULE_BYTES {
        return Err(ExternalJsonError::TooLarge);
    }
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| ExternalJsonError::Read)?;
    let mut bytes = Vec::new();
    file.take(MAX_SCHEDULE_BYTES + 1)
        .read_to_end(&mut bytes)
        .await
        .map_err(|_| ExternalJsonError::Read)?;
    if bytes.len() as u64 > MAX_SCHEDULE_BYTES {
        return Err(ExternalJsonError::TooLarge);
    }
    parse(&bytes)
}

fn parse(bytes: &[u8]) -> Result<Option<ExternalJsonSchedule>, ExternalJsonError> {
    let Some(document) = serde_json::from_slice::<Option<ChannelDocument>>(bytes)
        .map_err(|_| ExternalJsonError::Json)?
    else {
        return Ok(None);
    };
    let text = document.start_time.as_deref().unwrap_or_default().trim();
    let mut start = DateTime::parse_from_rfc3339(text)
        .map(|value| value.with_timezone(&Utc))
        .or_else(|_| {
            NaiveDateTime::parse_from_str(text, "%Y-%m-%dT%H:%M:%S%.f").map(|value| value.and_utc())
        })
        .or_else(|_| crate::repositories::common::decode_datetime("external JSON start", text))
        .map_err(|_| ExternalJsonError::StartTime)?;
    let mut items = Vec::with_capacity(document.programs.len());
    for program in document.programs {
        let finish = start
            .checked_add_signed(Duration::milliseconds(i64::from(program.duration)))
            .ok_or(ExternalJsonError::TimeRange)?;
        items.push(ExternalJsonItem {
            start,
            finish,
            program,
        });
        start = finish;
    }
    Ok(Some(ExternalJsonSchedule {
        guide_minimum_duration_seconds: document.guide_minimum_duration_seconds,
        items,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn external_json_timeline_uses_milliseconds_and_does_not_repeat() {
        let schedule = parse(
            br#"{"startTime":"2026-09-29T12:00:00+02:00",
            "guideMinimumDurationSeconds":60,
            "programs":[{"title":"First","duration":1500},{"duration":2500}]}"#,
        )
        .unwrap()
        .unwrap();
        let start = DateTime::parse_from_rfc3339("2026-09-29T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert_eq!(schedule.items[0].start, start);
        assert_eq!(
            schedule.items[1].start,
            start + Duration::milliseconds(1500)
        );
        assert_eq!(schedule.items[1].finish, start + Duration::seconds(4));
        assert_eq!(
            schedule
                .active_item(start)
                .unwrap()
                .program
                .title
                .as_deref(),
            Some("First")
        );
        assert_eq!(
            schedule
                .active_item(start + Duration::milliseconds(1500))
                .unwrap()
                .start,
            schedule.items[1].start
        );
        assert!(schedule.active_item(start + Duration::seconds(4)).is_none());
        assert!(schedule.active_item(start - Duration::seconds(1)).is_some());
    }

    #[test]
    fn external_json_invalid_start_is_explicit_and_redacted() {
        for start in [serde_json::Value::Null, serde_json::json!("private-input")] {
            let bytes =
                serde_json::to_vec(&serde_json::json!({"startTime": start, "programs": []}))
                    .unwrap();
            assert!(matches!(parse(&bytes), Err(ExternalJsonError::StartTime)));
        }
        assert!(matches!(parse(b"{}"), Err(ExternalJsonError::Json)));
        assert!(parse(b"null").unwrap().is_none());
        assert!(!ExternalJsonError::StartTime
            .to_string()
            .contains("private-input"));
    }

    #[tokio::test]
    async fn external_json_file_updates_and_limits_are_observed() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("schedule.json");
        assert!(load(&path).await.unwrap().is_none());
        for time in ["2026-09-29T10:00:00", "2026-09-29 11:00:00"] {
            std::fs::write(
                &path,
                serde_json::to_vec(&serde_json::json!({"startTime":time,"programs":[]})).unwrap(),
            )
            .unwrap();
            assert!(load(&path).await.unwrap().is_some());
        }
        std::fs::File::create(&path)
            .unwrap()
            .set_len(MAX_SCHEDULE_BYTES + 1)
            .unwrap();
        assert!(matches!(
            load(&path).await,
            Err(ExternalJsonError::TooLarge)
        ));
        assert!(matches!(
            load(directory.path()).await,
            Err(ExternalJsonError::Read)
        ));
    }
}
