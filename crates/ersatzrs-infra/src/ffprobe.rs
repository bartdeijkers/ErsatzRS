use std::collections::BTreeMap;
use std::path::Path;
use std::process::{ExitStatus, Stdio};
use std::time::Duration as StdDuration;

use chrono::{Duration, Utc};
use ersatzrs_domain::{MediaChapter, MediaStream, MediaStreamKind, MediaVersion, VideoScanKind};
use serde::Deserialize;
use thiserror::Error;
use tokio::process::Command;

use crate::remote_stream::{
    pipe_remote_stream_producer, redacted_source_label, remote_stream_input_file,
    spawn_remote_stream_producer, RemoteStreamProducerPipe, RemoteStreamSource,
    ResolvedRemoteStream,
};

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

    /// Probe a persisted Remote Stream URL or media-producing script.
    ///
    /// Script stdout is connected directly to ffprobe stdin and both
    /// processes are bounded by one timeout/lifecycle.
    ///
    /// # Errors
    ///
    /// Returns an error when either process cannot start, the probe times out,
    /// ffprobe exits unsuccessfully, or its JSON cannot be decoded.
    pub async fn extract_remote(
        &self,
        stream: &ResolvedRemoteStream,
        ffmpeg_path: &str,
    ) -> Result<FfprobeMediaVersion> {
        self.extract_remote_with_timeout(stream, ffmpeg_path, StdDuration::from_secs(30))
            .await
    }

    async fn extract_remote_with_timeout(
        &self,
        stream: &ResolvedRemoteStream,
        ffmpeg_path: &str,
        timeout: StdDuration,
    ) -> Result<FfprobeMediaVersion> {
        let label = redacted_source_label(&stream.source);
        let (args, mut producer) = self.remote_probe_command(stream, ffmpeg_path, &label)?;
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
                pipe_remote_stream_producer(producer, consumer_stdin, label.clone()).map_err(
                    |error| FfprobeError::Spawn {
                        path: label.clone(),
                        source: std::io::Error::other(error),
                    },
                )?,
            )
        } else {
            None
        };

        let output = match tokio::time::timeout(timeout, child.wait_with_output()).await {
            Ok(Ok(output)) => output,
            Ok(Err(source)) => {
                shutdown_remote_producer(&mut producer_pipe).await;
                return Err(FfprobeError::Spawn {
                    path: label,
                    source,
                });
            }
            Err(_) => {
                shutdown_remote_producer(&mut producer_pipe).await;
                return Err(FfprobeError::Timeout { path: label });
            }
        };
        shutdown_remote_producer(&mut producer_pipe).await;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let stderr = match &stream.source {
                RemoteStreamSource::Url(url) => stderr.replace(url, &label),
                RemoteStreamSource::Script(_) => stderr.into_owned(),
            };
            return Err(FfprobeError::Process {
                path: label,
                status: output.status,
                stderr,
            });
        }
        parse_ffprobe_json(Path::new("remote-stream"), &output.stdout).map_err(|source| {
            FfprobeError::Json {
                path: label,
                source,
            }
        })
    }

    fn remote_probe_command(
        &self,
        stream: &ResolvedRemoteStream,
        ffmpeg_path: &str,
        label: &str,
    ) -> Result<(Vec<String>, Option<tokio::process::Child>)> {
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
            RemoteStreamSource::Script(command) => Some(
                spawn_remote_stream_producer(command, ffmpeg_path, Some(&self.ffprobe_path))
                    .map_err(|error| FfprobeError::Spawn {
                        path: label.to_owned(),
                        source: std::io::Error::other(error),
                    })?,
            ),
        };
        Ok((args, producer))
    }
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

    let frame_side_data: Vec<&FfprobeSideData> = probe
        .frames
        .iter()
        .flat_map(|frame| &frame.side_data_list)
        .collect();
    let streams = probe
        .streams
        .iter()
        .filter_map(|stream| stream_to_domain(stream, 0, &frame_side_data))
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
            source: RemoteStreamSource::Script(crate::remote_stream::RemoteStreamCommand {
                program: blocker.display().to_string(),
                args: Vec::new(),
                working_directory: temp.path().to_path_buf(),
            }),
            is_live: false,
            configured_duration: None,
            fallback_query: None,
            url_overrides_script: false,
        };
        let extractor = FfprobeExtractor::new(blocker.display().to_string());
        let result = tokio::time::timeout(
            StdDuration::from_secs(2),
            extractor.extract_remote_with_timeout(
                &stream,
                "ffmpeg-unused",
                StdDuration::from_millis(50),
            ),
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
}
