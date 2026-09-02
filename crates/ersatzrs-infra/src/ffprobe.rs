use std::collections::BTreeMap;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use ersatzrs_domain::{
    DolbyVisionMetadataState, MediaChapter, MediaStream, MediaStreamKind, MediaVersion,
    VideoScanKind,
};
use ersatzrs_ffmpeg::DolbyVisionConfiguration;
use serde::Deserialize;
use thiserror::Error;
use tokio::io::AsyncReadExt as _;
use tokio::process::Command;

use crate::remote_stream::{
    command_has_file_backed_secret, pipe_remote_stream_producer, read_stderr_tail,
    redact_remote_stream_output, redacted_source_label, remote_stream_environment_secrets,
    remote_stream_input_file, spawn_remote_stream_producer, RemoteStreamProducerPipe,
    RemoteStreamSource, RemoteStreamSpawnContext, ResolvedRemoteStream,
};

const MAX_REMOTE_FFPROBE_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const REMOTE_FFPROBE_PIPE_SHUTDOWN_TIMEOUT: StdDuration = StdDuration::from_secs(1);

#[derive(Debug, Clone, PartialEq)]
pub struct FfprobeMediaVersion {
    pub version: MediaVersion,
    pub streams: Vec<MediaStream>,
    pub chapters: Vec<MediaChapter>,
    /// Container-level `FFprobe` tags keyed case-insensitively in lowercase.
    pub format_metadata: BTreeMap<String, String>,
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
    #[error("ffprobe timed out for {path}")]
    Timeout { path: String },
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
        let mut process = Command::new(&command.program);
        process.args(&command.args).kill_on_drop(true);
        let output = process
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

    /// Probe a persisted Remote Stream URL or media-producing add-on.
    ///
    /// Add-on stdout is connected directly to ffprobe stdin and both
    /// processes are bounded by one timeout/lifecycle.
    ///
    /// # Errors
    ///
    /// Returns an error when either process cannot start, the probe times out,
    /// ffprobe exits unsuccessfully, or its JSON cannot be decoded.
    pub async fn extract_remote(
        &self,
        stream: &ResolvedRemoteStream,
    ) -> Result<FfprobeMediaVersion> {
        self.extract_remote_with_timeout(stream, StdDuration::from_secs(30))
            .await
    }

    async fn extract_remote_with_timeout(
        &self,
        stream: &ResolvedRemoteStream,
        timeout: StdDuration,
    ) -> Result<FfprobeMediaVersion> {
        let label = redacted_source_label(&stream.source);
        let (args, mut producer) = Self::remote_probe_command(stream, &label)?;
        let mut process = Command::new(&self.ffprobe_path);
        process
            .args(&args)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        if producer.is_some() {
            process.stdin(Stdio::piped());
        } else {
            process.stdin(Stdio::null());
        }
        let mut child = process.spawn().map_err(|source| FfprobeError::Spawn {
            path: label.clone(),
            source,
        })?;

        let mut producer_pipe = if let Some(mut producer) = producer.take() {
            let Some(consumer_stdin) = child.stdin.take() else {
                let _ = producer.start_kill();
                return Err(FfprobeError::Spawn {
                    path: label,
                    source: std::io::Error::other("ffprobe did not expose stdin"),
                });
            };
            Some(
                pipe_remote_stream_producer(
                    producer,
                    consumer_stdin,
                    label.clone(),
                    RemoteStreamSpawnContext::probe(),
                    Some("0".to_owned()),
                    match &stream.source {
                        RemoteStreamSource::Addon(command) => {
                            remote_stream_environment_secrets(command)
                        }
                        RemoteStreamSource::Url(_) => Vec::new(),
                    },
                    match &stream.source {
                        RemoteStreamSource::Addon(command) => {
                            command_has_file_backed_secret(command)
                        }
                        RemoteStreamSource::Url(_) => false,
                    },
                )
                .map_err(|error| FfprobeError::Spawn {
                    path: label.clone(),
                    source: std::io::Error::other(error),
                })?,
            )
        } else {
            None
        };

        let stdout = child.stdout.take().ok_or_else(|| FfprobeError::Spawn {
            path: label.clone(),
            source: std::io::Error::other("ffprobe did not expose stdout"),
        })?;
        let stderr = child.stderr.take().ok_or_else(|| FfprobeError::Spawn {
            path: label.clone(),
            source: std::io::Error::other("ffprobe did not expose stderr"),
        })?;
        let stdout_task = tokio::spawn(read_remote_ffprobe_stdout(stdout));
        let stderr_task = tokio::spawn(read_stderr_tail(stderr));
        let status = match tokio::time::timeout(timeout, child.wait()).await {
            Ok(Ok(status)) => status,
            Ok(Err(source)) => {
                shutdown_remote_producer(&mut producer_pipe).await;
                stdout_task.abort();
                stderr_task.abort();
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Err(FfprobeError::Spawn {
                    path: label,
                    source,
                });
            }
            Err(_) => {
                let _ = child.kill().await;
                shutdown_remote_producer(&mut producer_pipe).await;
                stdout_task.abort();
                stderr_task.abort();
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                return Err(FfprobeError::Timeout { path: label });
            }
        };
        shutdown_remote_producer(&mut producer_pipe).await;
        let (stdout, stderr) =
            Self::collect_remote_ffprobe_output(stdout_task, stderr_task, &label).await?;
        finish_remote_ffprobe(status, &label, &stdout, &stderr, stream)
    }

    async fn collect_remote_ffprobe_output(
        mut stdout_task: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
        mut stderr_task: tokio::task::JoinHandle<String>,
        label: &str,
    ) -> Result<(Vec<u8>, String)> {
        let readers = tokio::time::timeout(REMOTE_FFPROBE_PIPE_SHUTDOWN_TIMEOUT, async {
            let stdout = (&mut stdout_task)
                .await
                .map_err(|_| std::io::Error::other("ffprobe stdout reader failed"))??;
            let stderr = (&mut stderr_task)
                .await
                .map_err(|_| std::io::Error::other("ffprobe stderr reader failed"))?;
            Ok::<_, std::io::Error>((stdout, stderr))
        })
        .await;
        match readers {
            Ok(Ok(output)) => Ok(output),
            Ok(Err(source)) => Err(FfprobeError::Spawn {
                path: label.to_owned(),
                source,
            }),
            Err(_) => {
                stdout_task.abort();
                stderr_task.abort();
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                Err(FfprobeError::Spawn {
                    path: label.to_owned(),
                    source: std::io::Error::other("ffprobe output pipes did not close"),
                })
            }
        }
    }

    fn remote_probe_command(
        stream: &ResolvedRemoteStream,
        label: &str,
    ) -> Result<(Vec<String>, Option<crate::addons::ManagedAddonChild>)> {
        let mut args = [
            "-hide_banner",
            "-print_format",
            "json",
            "-show_format",
            "-show_streams",
            "-show_chapters",
            // One frame only: Matroska carries HDR static metadata as frame
            // side data, and a full frame walk would be ruinous on a scan.
            "-read_intervals",
            "%+#1",
            "-show_frames",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect::<Vec<_>>();
        let input = remote_stream_input_file(stream);
        args.extend(input.input_options);
        args.extend(["-i".to_owned(), input.path]);
        let producer = match &stream.source {
            RemoteStreamSource::Url(_) => None,
            RemoteStreamSource::Addon(command) => {
                Some(spawn_remote_stream_producer(command).map_err(|error| {
                    FfprobeError::Spawn {
                        path: label.to_owned(),
                        source: std::io::Error::other(error),
                    }
                })?)
            }
        };
        Ok((args, producer))
    }
}

fn finish_remote_ffprobe(
    status: std::process::ExitStatus,
    label: &str,
    stdout: &[u8],
    stderr: &str,
    stream: &ResolvedRemoteStream,
) -> Result<FfprobeMediaVersion> {
    if !status.success() {
        return Err(FfprobeError::Process {
            path: label.to_owned(),
            status,
            stderr: redact_remote_stream_output(stderr, stream),
        });
    }
    parse_ffprobe_json(Path::new("remote-stream"), stdout).map_err(|source| FfprobeError::Json {
        path: label.to_owned(),
        source,
    })
}

async fn read_remote_ffprobe_stdout(
    stdout: tokio::process::ChildStdout,
) -> std::io::Result<Vec<u8>> {
    let mut output = Vec::new();
    stdout
        .take(MAX_REMOTE_FFPROBE_OUTPUT_BYTES as u64 + 1)
        .read_to_end(&mut output)
        .await?;
    if output.len() > MAX_REMOTE_FFPROBE_OUTPUT_BYTES {
        return Err(std::io::Error::other(
            "ffprobe stdout exceeded its size limit",
        ));
    }
    Ok(output)
}

async fn shutdown_remote_producer(producer: &mut Option<RemoteStreamProducerPipe>) {
    if let Some(mut producer) = producer.take() {
        producer.shutdown().await;
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
        // See above: one frame, for HDR static metadata.
        "-read_intervals",
        "%+#1",
        "-show_frames",
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

    // Only one video stream? ffprobe sometimes omits `stream_index` on frames,
    // and with a single video stream there is no ambiguity about who they
    // belong to. With more than one, an unindexed frame is dropped rather than
    // attributed to the wrong stream.
    let single_video_stream = probe
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("video"))
        .count()
        == 1;
    let streams = probe
        .streams
        .iter()
        .filter_map(|stream| {
            let frame_side_data = frame_side_data_for_stream(probe, stream, single_video_stream);
            stream_to_domain(stream, 0, &frame_side_data)
        })
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
        format_metadata: probe
            .format
            .as_ref()
            .map(|format| {
                format
                    .tags
                    .iter()
                    .map(|(key, value)| (key.trim().to_ascii_lowercase(), value.clone()))
                    .filter(|(key, value)| !key.is_empty() && !value.trim().is_empty())
                    .collect()
            })
            .unwrap_or_default(),
    }
}

/// Frame side data belonging to one stream.
fn frame_side_data_for_stream<'a>(
    probe: &'a FfprobeOutput,
    stream: &FfprobeStream,
    single_video_stream: bool,
) -> Vec<&'a FfprobeSideData> {
    probe
        .frames
        .iter()
        .filter(|frame| match (frame.stream_index, stream.index) {
            (Some(frame_index), Some(stream_index)) => frame_index == stream_index,
            // Unindexed frames are usable only when there is nothing to confuse
            // them with.
            (None, _) => single_video_stream,
            (Some(_), None) => false,
        })
        .flat_map(|frame| &frame.side_data_list)
        .collect()
}

fn stream_to_domain(
    stream: &FfprobeStream,
    media_version_id: i64,
    frame_side_data: &[&FfprobeSideData],
) -> Option<MediaStream> {
    let kind = match stream.codec_type.as_deref()? {
        "video" => MediaStreamKind::Video,
        "audio" => MediaStreamKind::Audio,
        "subtitle" => MediaStreamKind::Subtitle,
        "attachment" => MediaStreamKind::Attachment,
        _ => return None,
    };

    // HDR static metadata describes the picture, so it is only meaningful on a
    // video stream even when a container attaches side data elsewhere.
    let is_video = matches!(kind, MediaStreamKind::Video);
    let content_light = stream.content_light(frame_side_data);
    let dolby_vision = is_video
        .then(|| stream.dolby_vision(frame_side_data))
        .flatten();
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
        rotation: stream.rotation_degrees(),
        mastering_display: is_video
            .then(|| stream.mastering_display(frame_side_data))
            .flatten(),
        max_cll: is_video.then_some(content_light.0).flatten(),
        max_fall: is_video.then_some(content_light.1).flatten(),
        dolby_vision_profile: dolby_vision.map(|config| i32::from(config.profile)),
        dolby_vision_bl_compatibility_id: dolby_vision
            .map(|config| i32::from(config.bl_signal_compatibility_id)),
        dolby_vision_bl_present: dolby_vision.map(|config| config.bl_present),
        dolby_vision_el_present: dolby_vision.map(|config| config.el_present),
        dolby_vision_rpu_present: dolby_vision.map(|config| config.rpu_present),
        dolby_vision_metadata_state: if dolby_vision.is_some() {
            DolbyVisionMetadataState::Complete
        } else {
            DolbyVisionMetadataState::NotDetected
        },
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
    /// First decoded frame, probed only for HDR static metadata.
    ///
    /// Matroska carries mastering display and content light as *frame* side
    /// data rather than stream side data, so a stream-only probe misses it
    /// entirely. Measured: adding one frame to the existing probe costs no
    /// measurable time.
    #[serde(default)]
    frames: Vec<FfprobeFrame>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFrame {
    /// Stream this frame belongs to.
    ///
    /// Without it, a file with two video streams would have the first stream's
    /// mastering display and Dolby Vision record broadcast onto the second.
    stream_index: Option<i32>,
    #[serde(default)]
    side_data_list: Vec<FfprobeSideData>,
}

#[derive(Debug, Deserialize)]
struct FfprobeFormat {
    duration: Option<String>,
    #[serde(default)]
    tags: BTreeMap<String, String>,
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
    /// Stream side data, which is where the display matrix lives.
    ///
    /// `FFmpeg` 9 autorotates from this and ignores the legacy `rotate` tag, so
    /// the tag is deliberately not read.
    side_data_list: Option<Vec<FfprobeSideData>>,
}

#[derive(Debug, Deserialize)]
struct FfprobeSideData {
    /// Counter-clockwise rotation in degrees from a display matrix.
    rotation: Option<f64>,
    /// SMPTE ST 2086 chromaticity and luminance, each reported by ffprobe as a
    /// `"numerator/denominator"` string in the standard's own units.
    red_x: Option<String>,
    red_y: Option<String>,
    green_x: Option<String>,
    green_y: Option<String>,
    blue_x: Option<String>,
    blue_y: Option<String>,
    white_point_x: Option<String>,
    white_point_y: Option<String>,
    max_luminance: Option<String>,
    min_luminance: Option<String>,
    /// CTA-861.3 content light level, in cd/m².
    max_content: Option<i32>,
    max_average: Option<i32>,
    /// Dolby Vision configuration record fields (ADR 0064). Field names match
    /// `ffprobe`'s `DOVI configuration record` output.
    dv_profile: Option<u8>,
    dv_bl_signal_compatibility_id: Option<u8>,
    rpu_present_flag: Option<i32>,
    el_present_flag: Option<i32>,
    bl_present_flag: Option<i32>,
}

/// Parse an ffprobe rational such as `"34000/50000"` into its numerator.
///
/// ffprobe always reports these with the standard's own denominator (50000 for
/// chromaticity, 10000 for luminance), which is exactly the unit `FFmpeg`'s
/// `-mastering_display` option expects, so the numerator is stored as-is and
/// never re-scaled.
fn side_data_numerator(value: Option<&String>) -> Option<u32> {
    let value = value?.trim();
    let numerator = value
        .split_once('/')
        .map_or(value, |(numerator, _)| numerator);
    numerator.parse().ok()
}

impl FfprobeStream {
    /// SMPTE ST 2086 mastering display in `FFmpeg`'s `-mastering_display`
    /// syntax, when the stream carries it.
    fn mastering_display(&self, frame_side_data: &[&FfprobeSideData]) -> Option<String> {
        let side_data = self
            .side_data_list
            .iter()
            .flatten()
            .chain(frame_side_data.iter().copied())
            .find(|entry| entry.max_luminance.is_some() && entry.green_x.is_some())?;

        Some(format!(
            "G({},{})B({},{})R({},{})WP({},{})L({},{})",
            side_data_numerator(side_data.green_x.as_ref())?,
            side_data_numerator(side_data.green_y.as_ref())?,
            side_data_numerator(side_data.blue_x.as_ref())?,
            side_data_numerator(side_data.blue_y.as_ref())?,
            side_data_numerator(side_data.red_x.as_ref())?,
            side_data_numerator(side_data.red_y.as_ref())?,
            side_data_numerator(side_data.white_point_x.as_ref())?,
            side_data_numerator(side_data.white_point_y.as_ref())?,
            side_data_numerator(side_data.max_luminance.as_ref())?,
            side_data_numerator(side_data.min_luminance.as_ref())?,
        ))
    }

    /// CTA-861.3 `MaxCLL` / `MaxFALL`, when the stream carries them.
    fn content_light(&self, frame_side_data: &[&FfprobeSideData]) -> (Option<i32>, Option<i32>) {
        self.side_data_list
            .iter()
            .flatten()
            .chain(frame_side_data.iter().copied())
            .find(|entry| entry.max_content.is_some() || entry.max_average.is_some())
            .map_or((None, None), |entry| (entry.max_content, entry.max_average))
    }

    /// Dolby Vision configuration record, when the stream carries one.
    fn dolby_vision(
        &self,
        frame_side_data: &[&FfprobeSideData],
    ) -> Option<DolbyVisionConfiguration> {
        let side_data = self
            .side_data_list
            .iter()
            .flatten()
            .chain(frame_side_data.iter().copied())
            .find(|entry| entry.dv_profile.is_some())?;

        Some(DolbyVisionConfiguration {
            profile: side_data.dv_profile?,
            bl_signal_compatibility_id: side_data.dv_bl_signal_compatibility_id.unwrap_or_default(),
            rpu_present: side_data.rpu_present_flag.unwrap_or_default() != 0,
            el_present: side_data.el_present_flag.unwrap_or_default() != 0,
            bl_present: side_data.bl_present_flag.unwrap_or_default() != 0,
        })
    }

    /// Display-matrix rotation in degrees, normalized to 0/90/180/270.
    fn rotation_degrees(&self) -> Option<i32> {
        let rotation = self
            .side_data_list
            .as_ref()?
            .iter()
            .find_map(|side_data| side_data.rotation)?;
        // ffprobe reports counter-clockwise degrees and can emit -90.
        #[allow(clippy::cast_possible_truncation)]
        let normalized = (rotation.round() as i64).rem_euclid(360) as i32;
        Some(normalized)
    }
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

// Process-permission setup is Unix-specific; the shared producer lifecycle is
// also covered by the cross-platform tests in remote_stream.rs.
#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[tokio::test]
    async fn remote_probe_timeout_cancels_stalled_producer() {
        let temp = tempfile::TempDir::new().expect("temp dir");
        let blocker = temp.path().join("block-forever.sh");
        std::fs::write(&blocker, "#!/bin/sh\nwhile :; do :; done\n").expect("blocker script");
        let mut permissions = std::fs::metadata(&blocker)
            .expect("blocker metadata")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&blocker, permissions).expect("blocker permissions");

        let stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Addon(crate::remote_stream::RemoteStreamCommand {
                program: blocker.display().to_string(),
                args: Vec::new(),
                working_directory: temp.path().to_path_buf(),
                environment: std::collections::BTreeMap::new(),
            }),
            liveness: ersatzrs_domain::RemoteStreamLiveness::Finite,
            is_live: false,
            configured_duration: None,
            fallback_query: None,
            uses_addon: true,
        };
        let extractor = FfprobeExtractor::new(blocker.display().to_string());
        let result = tokio::time::timeout(
            StdDuration::from_secs(2),
            extractor.extract_remote_with_timeout(&stream, StdDuration::from_millis(50)),
        )
        .await
        .expect("producer cancellation timed out");

        assert!(matches!(result, Err(FfprobeError::Timeout { .. })));
    }

    /// Real-`ffprobe` gate for ADR 0064 source metadata. Ignored by default.
    ///
    /// Run with:
    /// `ERSATZRS_FFMPEG=<path to ffmpeg> ./run.sh test -p ersatzrs-infra \
    ///  --run-ignored ignored-only source_hdr_metadata`
    // Test-only opt-in path; the configuration layer is for the application.
    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    #[ignore = "requires a real ffmpeg/ffprobe binary"]
    async fn source_hdr_metadata_is_parsed_from_a_real_hdr10_file() {
        let Some(ffmpeg) = std::env::var_os("ERSATZRS_FFMPEG") else {
            eprintln!("set ERSATZRS_FFMPEG to run this gate");
            return;
        };
        let ffmpeg = std::path::PathBuf::from(&ffmpeg);
        let ffprobe = ffmpeg.parent().expect("ffmpeg directory").join("ffprobe");
        let directory = tempfile::tempdir().expect("temp dir");
        let source = directory.path().join("hdr10.mkv");

        // A BT.2020/PQ HEVC file carrying ST 2086 and CTA-861.3 metadata.
        let status = tokio::process::Command::new(&ffmpeg)
            .args(["-nostdin", "-hide_banner", "-loglevel", "error", "-y"])
            .args(["-f", "lavfi", "-i", "testsrc2=size=64x64:duration=1:rate=5"])
            .args([
                "-vf",
                "format=yuv420p10le,setparams=color_primaries=bt2020:color_trc=smpte2084:colorspace=bt2020nc:range=tv",
                "-c:v",
                "libx265",
                "-profile:v",
                "main10",
                "-pix_fmt",
                "yuv420p10le",
                "-x265-params",
                "master-display=G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50):max-cll=1000,400:hdr-opt=1:repeat-headers=1:log-level=error",
            ])
            .arg(&source)
            .status()
            .await
            .expect("run ffmpeg");
        assert!(status.success(), "failed to build the HDR10 fixture");

        let output = tokio::process::Command::new(&ffprobe)
            .args(super::ffprobe_args(&source))
            .output()
            .await
            .expect("run ffprobe");
        assert!(output.status.success(), "ffprobe failed");

        let probed =
            super::parse_ffprobe_json(&source, &output.stdout).expect("parse ffprobe output");

        let video = probed
            .streams
            .iter()
            .find(|stream| stream.kind == ersatzrs_domain::MediaStreamKind::Video)
            .expect("video stream");

        // Matroska carries these as frame side data, which a stream-only probe
        // would miss entirely.
        assert_eq!(
            video.mastering_display.as_deref(),
            Some("G(13250,34500)B(7500,3000)R(34000,16000)WP(15635,16450)L(10000000,50)"),
            "mastering display must round-trip in FFmpeg's own units"
        );
        assert_eq!(video.max_cll, Some(1_000));
        assert_eq!(video.max_fall, Some(400));
    }

    #[test]
    fn dolby_vision_configuration_is_parsed_from_ffprobe_stream_side_data() {
        // Field names taken from the shipped ffprobe binary's own writer:
        // dv_profile, dv_level, rpu_present_flag, el_present_flag,
        // bl_present_flag, dv_bl_signal_compatibility_id.
        let json = br#"{
            "streams": [{
                "index": 0,
                "codec_type": "video",
                "codec_name": "hevc",
                "side_data_list": [{
                    "side_data_type": "DOVI configuration record",
                    "dv_version_major": 1,
                    "dv_version_minor": 0,
                    "dv_profile": 8,
                    "dv_level": 6,
                    "rpu_present_flag": 1,
                    "el_present_flag": 0,
                    "bl_present_flag": 1,
                    "dv_bl_signal_compatibility_id": 1
                }]
            }]
        }"#;

        let probed = super::parse_ffprobe_json("dv81.mkv", json).expect("parse");
        let video = probed.streams.first().expect("stream");

        assert_eq!(video.dolby_vision_profile, Some(8));
        assert_eq!(video.dolby_vision_bl_compatibility_id, Some(1));
        assert_eq!(video.dolby_vision_bl_present, Some(true));
        assert_eq!(video.dolby_vision_rpu_present, Some(true));
        assert_eq!(video.dolby_vision_el_present, Some(false));
    }

    #[test]
    fn a_profile_7_dual_layer_record_is_parsed_with_its_enhancement_layer() {
        let json = br#"{
            "streams": [{
                "index": 0,
                "codec_type": "video",
                "codec_name": "hevc",
                "side_data_list": [{
                    "side_data_type": "DOVI configuration record",
                    "dv_profile": 7,
                    "rpu_present_flag": 1,
                    "el_present_flag": 1,
                    "bl_present_flag": 1,
                    "dv_bl_signal_compatibility_id": 0
                }]
            }]
        }"#;

        let probed = super::parse_ffprobe_json("dv7.mkv", json).expect("parse");
        let video = probed.streams.first().expect("stream");

        assert_eq!(video.dolby_vision_profile, Some(7));
        assert_eq!(video.dolby_vision_bl_present, Some(true));
        assert_eq!(video.dolby_vision_el_present, Some(true));
    }

    #[test]
    fn a_stream_without_dolby_vision_reports_none() {
        let json = br#"{
            "streams": [{
                "index": 0,
                "codec_type": "video",
                "codec_name": "hevc"
            }]
        }"#;

        let probed = super::parse_ffprobe_json("plain.mkv", json).expect("parse");
        let video = probed.streams.first().expect("stream");

        assert_eq!(video.dolby_vision_profile, None);
        assert_eq!(video.dolby_vision_bl_present, None);
        assert_eq!(video.dolby_vision_rpu_present, None);
    }

    /// Real-file gate for the Dolby Vision configuration record.
    ///
    /// Point `ERSATZRS_DOLBY_VISION_FIXTURE` at any Dolby Vision file. The path
    /// is supplied by the operator and never committed: `CONTRIBUTING.md`
    /// forbids private media paths in repository content.
    ///
    /// Run with:
    /// `ERSATZRS_FFMPEG=<ffmpeg> ERSATZRS_DOLBY_VISION_FIXTURE=<file> \
    ///  ./run.sh test -p ersatzrs-infra --run-ignored ignored-only real_dolby_vision`
    // Test-only opt-in path; the configuration layer is for the application.
    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    #[ignore = "requires a real Dolby Vision file"]
    async fn real_dolby_vision_file_is_parsed_and_classified() {
        let (Some(ffmpeg), Some(fixture)) = (
            std::env::var_os("ERSATZRS_FFMPEG"),
            std::env::var_os("ERSATZRS_DOLBY_VISION_FIXTURE"),
        ) else {
            eprintln!("set ERSATZRS_FFMPEG and ERSATZRS_DOLBY_VISION_FIXTURE to run this gate");
            return;
        };
        let ffprobe = std::path::Path::new(&ffmpeg)
            .parent()
            .expect("ffmpeg directory")
            .join("ffprobe");
        let fixture = std::path::PathBuf::from(&fixture);

        let output = tokio::process::Command::new(&ffprobe)
            .args(super::ffprobe_args(&fixture))
            .output()
            .await
            .expect("run ffprobe");
        assert!(output.status.success(), "ffprobe failed");

        let probed =
            super::parse_ffprobe_json(&fixture, &output.stdout).expect("parse ffprobe output");
        let video = probed
            .streams
            .iter()
            .find(|stream| stream.kind == ersatzrs_domain::MediaStreamKind::Video)
            .expect("video stream");

        let profile = video
            .dolby_vision_profile
            .expect("the fixture must actually be Dolby Vision");
        let configuration = ersatzrs_ffmpeg::DolbyVisionConfiguration {
            profile: u8::try_from(profile).expect("profile fits"),
            bl_signal_compatibility_id: u8::try_from(
                video.dolby_vision_bl_compatibility_id.unwrap_or_default(),
            )
            .expect("compatibility id fits"),
            rpu_present: video.dolby_vision_rpu_present.unwrap_or_default(),
            el_present: video.dolby_vision_el_present.unwrap_or_default(),
            bl_present: true,
        };

        println!(
            "profile={} compatibility_id={} rpu={} el={} -> {}",
            configuration.profile,
            configuration.bl_signal_compatibility_id,
            configuration.rpu_present,
            configuration.el_present,
            configuration.evidence()
        );

        // The parse must produce a usable record, not silently default.
        assert!(
            configuration.rpu_present,
            "a Dolby Vision stream has an RPU"
        );

        // Profile 8.1 can use its declared HDR10-compatible base layer.
        // Profile 7 can use its HDR10 base layer only when the complete
        // dual-layer record and exact HEVC Main 10 stream shape are present;
        // the command boundary forces direct software decode.
        if configuration.profile == 8 && configuration.bl_signal_compatibility_id == 1 {
            assert!(configuration.is_playable());
            assert!(!configuration.el_present, "profile 8 is single layer");
            // The HDR10 base layer must carry real HDR10 signalling.
            assert_eq!(video.color_transfer.as_deref(), Some("smpte2084"));
            assert_eq!(video.color_primaries.as_deref(), Some("bt2020"));
            assert!(
                video.mastering_display.is_some(),
                "an HDR10-compatible base layer should carry mastering display"
            );
        } else if configuration.profile == 7 {
            assert!(
                configuration.el_present,
                "profile 7 needs an enhancement layer"
            );
            assert!(configuration.is_playable(), "{}", configuration.evidence());
            assert_eq!(video.color_transfer.as_deref(), Some("smpte2084"));
            assert_eq!(video.color_primaries.as_deref(), Some("bt2020"));
        } else {
            assert!(!configuration.is_playable(), "{}", configuration.evidence());
        }
    }

    /// Real-file gate for `FFmpeg` 9 profile-7 base-layer separation.
    ///
    /// The fixture is operator-supplied and never committed. This test does
    /// not write an output file; it seeks into the source, applies the exact
    /// selected-stream input bitstream filter emitted by the pipeline, and
    /// requires one decoded frame.
    // Test-only opt-in paths; the configuration layer is for the application.
    #[allow(clippy::disallowed_methods)]
    #[tokio::test]
    #[ignore = "requires a real Dolby Vision profile 7 file and FFmpeg 9"]
    async fn real_profile_7_base_layer_is_decodable() {
        let (Some(ffmpeg), Some(fixture)) = (
            std::env::var_os("ERSATZRS_FFMPEG"),
            std::env::var_os("ERSATZRS_DOLBY_VISION_PROFILE7_FIXTURE"),
        ) else {
            eprintln!(
                "set ERSATZRS_FFMPEG and ERSATZRS_DOLBY_VISION_PROFILE7_FIXTURE to run this gate"
            );
            return;
        };

        let output = tokio::process::Command::new(ffmpeg)
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-ss",
                "3",
                "-bsf:0",
                "dovi_split=mode=bl",
                "-i",
            ])
            .arg(fixture)
            .args(["-frames:v", "1", "-f", "null", "-"])
            .output()
            .await
            .expect("run FFmpeg profile 7 base-layer decode");

        assert!(
            output.status.success(),
            "profile 7 base-layer decode failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    #[test]
    fn frame_side_data_is_scoped_to_its_own_video_stream() {
        // Two video streams with distinct HDR and Dolby Vision metadata. Before
        // scoping, the first stream's data was broadcast onto both, so pinning
        // the second stream produced another stream's colour metadata.
        let json = br#"{
            "streams": [
                {"index": 0, "codec_type": "video", "codec_name": "hevc"},
                {"index": 1, "codec_type": "video", "codec_name": "hevc"}
            ],
            "frames": [
                {"stream_index": 0, "side_data_list": [
                    {"side_data_type": "Mastering display metadata",
                     "red_x": "34000/50000", "red_y": "16000/50000",
                     "green_x": "13250/50000", "green_y": "34500/50000",
                     "blue_x": "7500/50000", "blue_y": "3000/50000",
                     "white_point_x": "15635/50000", "white_point_y": "16450/50000",
                     "max_luminance": "10000000/10000", "min_luminance": "50/10000"},
                    {"side_data_type": "Content light level metadata",
                     "max_content": 1000, "max_average": 400},
                    {"side_data_type": "DOVI configuration record",
                     "dv_profile": 8, "rpu_present_flag": 1, "el_present_flag": 0,
                     "bl_present_flag": 1, "dv_bl_signal_compatibility_id": 1}
                ]},
                {"stream_index": 1, "side_data_list": [
                    {"side_data_type": "Content light level metadata",
                     "max_content": 4000, "max_average": 600}
                ]}
            ]
        }"#;

        let probed = super::parse_ffprobe_json("two-video-streams.mkv", json).expect("parse");
        let first = &probed.streams[0];
        let second = &probed.streams[1];

        assert_eq!(first.max_cll, Some(1_000));
        assert_eq!(first.dolby_vision_profile, Some(8));
        assert!(first.mastering_display.is_some());

        // The second stream keeps only its own values.
        assert_eq!(second.max_cll, Some(4_000));
        assert_eq!(second.max_fall, Some(600));
        assert_eq!(
            second.mastering_display, None,
            "the first stream's mastering display must not leak"
        );
        assert_eq!(
            second.dolby_vision_profile, None,
            "the first stream's Dolby Vision record must not leak"
        );
    }

    #[test]
    fn unindexed_frames_are_used_only_when_there_is_one_video_stream() {
        // ffprobe can omit stream_index. With a single video stream that is
        // unambiguous; with two it would be a guess, so the data is dropped.
        let single = br#"{
            "streams": [{"index": 0, "codec_type": "video", "codec_name": "hevc"}],
            "frames": [{"side_data_list": [
                {"side_data_type": "Content light level metadata",
                 "max_content": 1000, "max_average": 400}
            ]}]
        }"#;
        let probed = super::parse_ffprobe_json("single.mkv", single).expect("parse");
        assert_eq!(probed.streams[0].max_cll, Some(1_000));

        let multiple = br#"{
            "streams": [
                {"index": 0, "codec_type": "video", "codec_name": "hevc"},
                {"index": 1, "codec_type": "video", "codec_name": "hevc"}
            ],
            "frames": [{"side_data_list": [
                {"side_data_type": "Content light level metadata",
                 "max_content": 1000, "max_average": 400}
            ]}]
        }"#;
        let probed = super::parse_ffprobe_json("multiple.mkv", multiple).expect("parse");
        for stream in &probed.streams {
            assert_eq!(
                stream.max_cll, None,
                "an unindexed frame must not be attributed by guesswork"
            );
        }
    }
}
