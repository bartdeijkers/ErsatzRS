use std::path::Path;
use std::process::ExitStatus;

use chrono::{Duration, Utc};
use ersatzrs_domain::{MediaChapter, MediaStream, MediaStreamKind, MediaVersion, VideoScanKind};
use serde::Deserialize;
use thiserror::Error;
use tokio::process::Command;

#[derive(Debug, Clone, PartialEq)]
pub struct FfprobeMediaVersion {
    pub version: MediaVersion,
    pub streams: Vec<MediaStream>,
    pub chapters: Vec<MediaChapter>,
}

#[derive(Debug, Error)]
pub enum FfprobeError {
    #[error("ffprobe failed for {path} with status {status}: {stderr}")]
    Process {
        path: String,
        status: ExitStatus,
        stderr: String,
    },
    #[error("failed to run ffprobe for {path}: {source}")]
    Spawn {
        path: String,
        source: std::io::Error,
    },
    #[error("failed to parse ffprobe JSON for {path}: {source}")]
    Json {
        path: String,
        source: serde_json::Error,
    },
}

pub type Result<T> = std::result::Result<T, FfprobeError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FfprobeCommand {
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct FfprobeExtractor {
    ffprobe_path: String,
}

impl FfprobeExtractor {
    #[must_use]
    pub fn new(ffprobe_path: impl Into<String>) -> Self {
        Self {
            ffprobe_path: ffprobe_path.into(),
        }
    }

    #[must_use]
    pub fn command_for(&self, path: impl AsRef<Path>) -> FfprobeCommand {
        FfprobeCommand {
            program: self.ffprobe_path.clone(),
            args: ffprobe_args(path.as_ref()),
        }
    }

    /// Run ffprobe and parse the JSON metadata for a media file.
    ///
    /// # Errors
    ///
    /// Returns an error when ffprobe cannot start, exits unsuccessfully, or
    /// returns invalid JSON.
    pub async fn extract(&self, path: impl AsRef<Path>) -> Result<FfprobeMediaVersion> {
        let path = path.as_ref();
        let command = self.command_for(path);
        let output = Command::new(&command.program)
            .args(&command.args)
            .output()
            .await
            .map_err(|source| FfprobeError::Spawn {
                path: path.display().to_string(),
                source,
            })?;

        if !output.status.success() {
            return Err(FfprobeError::Process {
                path: path.display().to_string(),
                status: output.status,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        parse_ffprobe_json(path, &output.stdout).map_err(|source| FfprobeError::Json {
            path: path.display().to_string(),
            source,
        })
    }
}

#[must_use]
pub fn ffprobe_args(path: &Path) -> Vec<String> {
    [
        "-hide_banner",
        "-print_format",
        "json",
        "-show_format",
        "-show_streams",
        "-show_chapters",
        "-i",
    ]
    .into_iter()
    .map(str::to_owned)
    .chain(std::iter::once(path.display().to_string()))
    .collect()
}

/// Parse ffprobe JSON output into domain metadata rows.
///
/// # Errors
///
/// Returns a serde error when the JSON does not match ffprobe's output shape.
pub fn parse_ffprobe_json(
    path: impl AsRef<Path>,
    json: &[u8],
) -> std::result::Result<FfprobeMediaVersion, serde_json::Error> {
    let probe: FfprobeOutput = serde_json::from_slice(json)?;
    Ok(project_to_media_version(path.as_ref(), &probe))
}

#[allow(clippy::too_many_lines)]
fn project_to_media_version(_path: &Path, probe: &FfprobeOutput) -> FfprobeMediaVersion {
    let now = Utc::now();
    let mut version = MediaVersion {
        name: "Main".to_owned(),
        date_added: now,
        date_updated: now,
        ..MediaVersion::default()
    };

    if let Some(duration) = probe
        .format
        .as_ref()
        .and_then(|format| format.duration.as_deref())
        .and_then(parse_seconds)
    {
        version.duration = duration;
    }

    let streams = probe
        .streams
        .iter()
        .filter_map(|stream| stream_to_domain(stream, 0))
        .collect();

    if let Some(video_stream) = probe
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .max_by_key(|stream| parse_i64(stream.bit_rate.as_deref()).unwrap_or(0))
    {
        version.sample_aspect_ratio = Some(
            video_stream
                .sample_aspect_ratio
                .clone()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "1:1".to_owned()),
        );
        version
            .display_aspect_ratio
            .clone_from(&video_stream.display_aspect_ratio);
        version.width = video_stream.width.unwrap_or_default();
        version.height = video_stream.height.unwrap_or_default();
        version.video_scan_kind = scan_kind_from_field_order(video_stream.field_order.as_deref());
        version.r_frame_rate.clone_from(&video_stream.r_frame_rate);
    }

    let mut chapters: Vec<MediaChapter> = probe
        .chapters
        .iter()
        .enumerate()
        .filter_map(|(index, chapter)| chapter_to_domain(index, chapter))
        .collect();

    if let Some(last) = chapters.last_mut() {
        if last.end_time != version.duration {
            last.end_time = version.duration;
        }
    }

    FfprobeMediaVersion {
        version,
        streams,
        chapters,
    }
}

fn stream_to_domain(stream: &FfprobeStream, media_version_id: i64) -> Option<MediaStream> {
    let kind = match stream.codec_type.as_deref()? {
        "video" => MediaStreamKind::Video,
        "audio" => MediaStreamKind::Audio,
        "subtitle" => MediaStreamKind::Subtitle,
        "attachment" => MediaStreamKind::Attachment,
        _ => return None,
    };

    let attached_pic = stream
        .disposition
        .as_ref()
        .is_some_and(|disposition| disposition.attached_pic == Some(1));
    let bits_per_raw_sample = stream
        .bits_per_raw_sample
        .as_deref()
        .and_then(|value| value.parse::<i32>().ok())
        .unwrap_or_default();
    let (file_name, mime_type) = if matches!(kind, MediaStreamKind::Attachment) {
        (
            stream.tags.as_ref().and_then(|tags| tags.filename.clone()),
            stream.tags.as_ref().and_then(|tags| tags.mimetype.clone()),
        )
    } else {
        (None, None)
    };

    Some(MediaStream {
        media_version_id,
        index: stream.index.unwrap_or_default(),
        kind,
        codec: stream.codec_name.clone(),
        profile: stream
            .profile
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        language: stream.tags.as_ref().and_then(|tags| tags.language.clone()),
        title: stream.tags.as_ref().and_then(|tags| tags.title.clone()),
        default: stream
            .disposition
            .as_ref()
            .is_some_and(|disposition| disposition.default == Some(1)),
        forced: stream
            .disposition
            .as_ref()
            .is_some_and(|disposition| disposition.forced == Some(1)),
        channels: stream.channels,
        width: stream.width,
        height: stream.height,
        pixel_format: stream
            .pix_fmt
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        color_range: stream
            .color_range
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        color_space: stream
            .color_space
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        color_transfer: stream
            .color_transfer
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        color_primaries: stream
            .color_primaries
            .as_ref()
            .map(|value| value.to_ascii_lowercase()),
        bits_per_raw_sample,
        attached_pic,
        file_name,
        mime_type,
        ..MediaStream::default()
    })
}

fn chapter_to_domain(index: usize, chapter: &FfprobeChapter) -> Option<MediaChapter> {
    let start_time = chapter.start_time.as_deref().and_then(parse_seconds)?;
    let end_time = chapter.end_time.as_deref().and_then(parse_seconds)?;
    Some(MediaChapter {
        media_version_id: 0,
        id: 0,
        chapter_id: i64::try_from(index).unwrap_or_default(),
        title: chapter
            .tags
            .as_ref()
            .and_then(|tags| tags.title.clone())
            .unwrap_or_default(),
        start_time,
        end_time,
    })
}

fn scan_kind_from_field_order(field_order: Option<&str>) -> VideoScanKind {
    match field_order.map(str::to_ascii_lowercase).as_deref() {
        Some("tt" | "bb" | "tb" | "bt") => VideoScanKind::Interlaced,
        Some("progressive") => VideoScanKind::Progressive,
        _ => VideoScanKind::Unknown,
    }
}

fn parse_seconds(value: &str) -> Option<Duration> {
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let seconds = whole.parse::<i64>().ok()?;
    let millis_text = format!("{fraction:0<3}");
    let millis = millis_text[..3].parse::<i64>().ok()?;
    Some(Duration::seconds(seconds) + Duration::milliseconds(millis))
}

fn parse_i64(value: Option<&str>) -> Option<i64> {
    value?.parse().ok()
}

#[derive(Debug, Deserialize)]
struct FfprobeOutput {
    format: Option<FfprobeFormat>,
    #[serde(default)]
    streams: Vec<FfprobeStream>,
    #[serde(default)]
    chapters: Vec<FfprobeChapter>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FfprobeDisposition {
    #[serde(default)]
    default: Option<i32>,
    #[serde(default)]
    forced: Option<i32>,
    #[serde(default)]
    attached_pic: Option<i32>,
}

#[derive(Debug, Deserialize)]
struct FfprobeStream {
    index: Option<i32>,
    codec_name: Option<String>,
    codec_type: Option<String>,
    profile: Option<String>,
    channels: Option<i32>,
    width: Option<i32>,
    height: Option<i32>,
    sample_aspect_ratio: Option<String>,
    display_aspect_ratio: Option<String>,
    field_order: Option<String>,
    r_frame_rate: Option<String>,
    bit_rate: Option<String>,
    pix_fmt: Option<String>,
    color_range: Option<String>,
    color_space: Option<String>,
    color_transfer: Option<String>,
    color_primaries: Option<String>,
    bits_per_raw_sample: Option<String>,
    disposition: Option<FfprobeDisposition>,
    tags: Option<FfprobeTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeChapter {
    start_time: Option<String>,
    end_time: Option<String>,
    tags: Option<FfprobeTags>,
}

#[derive(Debug, Deserialize)]
struct FfprobeTags {
    language: Option<String>,
    title: Option<String>,
    filename: Option<String>,
    mimetype: Option<String>,
}
