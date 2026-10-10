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
    /// Exact ephemeral scan order by fresh stream index; never persisted to SQL.
    pub field_orders: BTreeMap<i32, String>,
    pub chapters: Vec<MediaChapter>,
    /// Container-level `FFprobe` tags keyed case-insensitively in lowercase.
    pub format_metadata: BTreeMap<String, String>,
    /// Detected container name (including `FFprobe` aliases); never persisted to SQL.
    pub format_name: Option<String>,
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
    #[error("ffprobe cancelled for {path}")]
    Cancelled { path: String },
    #[error("ffprobe returned invalid metadata for {path}")]
    InvalidOutput { path: String },
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
        let deadline = tokio::time::Instant::now() + timeout;
        let label = redacted_source_label(&stream.source);
        let (args, producer) = Self::remote_probe_command(stream, &label)?;
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        self.run_remote_probe(Some(stream), args, producer, deadline, receiver)
            .await
    }

    /// Probe an owned input within the caller's bounded startup budget.
    /// Input options precede the locator; encoding filters are not applied by ffprobe.
    /// Signal cancellation and await this future before releasing input ownership.
    ///
    /// # Errors
    /// Rejects invalid budgets, cancellation, failed probes and invalid output.
    /// Errors never include the private input path or raw diagnostics.
    pub async fn extract_input_cancellable(
        &self,
        input: &ersatzrs_ffmpeg::InputFile,
        remaining_startup: StdDuration,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<FfprobeMediaVersion> {
        if remaining_startup.is_zero() || remaining_startup > StdDuration::from_secs(15) {
            return Err(FfprobeError::Timeout {
                path: "published interval media".to_owned(),
            });
        }
        let deadline = tokio::time::Instant::now() + remaining_startup;
        self.run_remote_probe(
            None,
            remote_probe_input_args(input),
            None,
            deadline,
            cancellation,
        )
        .await
    }

    /// Probe the bounded NUT header from an already-running v4 producer.
    /// Inspect only the copied prefix, including stream-info initialization:
    /// NUT headers alone omit rawvideo pixel format. The owner replays these
    /// same bytes to the encoder without reopening the producer.
    ///
    /// # Errors
    /// Rejects oversized/empty prefixes, cancellation and malformed media headers.
    pub async fn extract_retained_prefix_cancellable(
        &self,
        prefix: &[u8],
        deadline: tokio::time::Instant,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<FfprobeMediaVersion> {
        self.extract_prefix_cancellable(prefix, true, deadline, cancellation)
            .await
    }

    /// Inspect a bounded replayable prefix from an ordinary installed producer.
    /// Container detection and compressed-stream inspection read only the copied
    /// bytes, never the running producer. Incomplete metadata must be rejected by
    /// the consumer; this operation does not infer a complete stream duration.
    ///
    /// # Errors
    /// Rejects empty/oversized prefixes, cancellation, timeout and invalid media.
    pub async fn extract_producer_prefix_cancellable(
        &self,
        prefix: &[u8],
        deadline: tokio::time::Instant,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<FfprobeMediaVersion> {
        self.extract_prefix_cancellable(prefix, false, deadline, cancellation)
            .await
    }

    async fn extract_prefix_cancellable(
        &self,
        prefix: &[u8],
        retained: bool,
        deadline: tokio::time::Instant,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<FfprobeMediaVersion> {
        use std::io::Write as _;
        let label = "published interval media";
        if prefix.is_empty() || prefix.len() > 64 * 1024 {
            return Err(FfprobeError::Spawn {
                path: label.to_owned(),
                source: std::io::Error::other("invalid retained media prefix"),
            });
        }
        let mut file = tempfile::NamedTempFile::new().map_err(|source| FfprobeError::Spawn {
            path: label.to_owned(),
            source,
        })?;
        file.write_all(prefix)
            .map_err(|source| FfprobeError::Spawn {
                path: label.to_owned(),
                source,
            })?;
        let mut args = vec!["-hide_banner".into(), "-v".into(), "error".into()];
        if retained {
            args.extend(["-f".into(), "nut".into()]);
        }
        args.extend([
            "-show_streams".into(),
            "-show_format".into(),
            "-of".into(),
            "json".into(),
            "-i".into(),
            file.path().to_string_lossy().into_owned(),
        ]);
        self.run_remote_probe(None, args, None, deadline, cancellation)
            .await
    }

    async fn run_remote_probe(
        &self,
        stream: Option<&ResolvedRemoteStream>,
        args: Vec<String>,
        producer: Option<crate::remote_stream::RemoteStreamCommand>,
        deadline: tokio::time::Instant,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<FfprobeMediaVersion> {
        let label = stream.map_or_else(
            || "published interval media".to_owned(),
            |stream| redacted_source_label(&stream.source),
        );
        let (status, stdout, stderr) = self
            .run_owned_probe_output(&label, stream, args, producer, deadline, cancellation)
            .await?;
        finish_remote_ffprobe(status, &label, &stdout, &stderr, stream)
    }

    /// Count decoded frames of a local watermark under the shared startup deadline.
    /// Signal cancellation and await this future before releasing input ownership.
    ///
    /// # Errors
    /// Returns static errors for cancellation, timeout, failed probes or invalid counts.
    pub async fn count_watermark_frames_cancellable(
        &self,
        path: &str,
        deadline: tokio::time::Instant,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<u64> {
        let label = "watermark frame count";
        let args = [
            "-loglevel",
            "error",
            "-select_streams",
            "v:0",
            "-count_frames",
            "-show_entries",
            "stream=nb_read_frames",
            "-print_format",
            "csv",
            path,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let (status, stdout, _) = self
            .run_owned_probe_output(label, None, args, None, deadline, cancellation)
            .await?;
        if !status.success() {
            return Err(FfprobeError::Process {
                path: label.to_owned(),
                status,
                stderr: "watermark frame count failed".to_owned(),
            });
        }
        String::from_utf8_lossy(&stdout)
            .trim()
            .strip_prefix("stream,")
            .and_then(|count| count.parse::<u64>().ok())
            .ok_or_else(|| FfprobeError::InvalidOutput {
                path: label.to_owned(),
            })
    }

    /// Decode JPEG XL frame timestamps with the existing bounded process owner.
    /// Format duration is frequently absent for animated JPEG XL.
    ///
    /// # Errors
    /// Rejects failed decoding, nonmonotonic timing, timeout or cancellation.
    pub async fn jpeg_xl_timing_cancellable(
        &self,
        path: &str,
        deadline: tokio::time::Instant,
        cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<ersatzrs_ffmpeg::graphics::JpegXlTiming> {
        let label = "graphics JPEG XL timing";
        let args = [
            "-v",
            "error",
            // Bound decoder parallelism; serial full-frame probing otherwise
            // dominates startup even when the actual producer is fast enough.
            "-threads",
            "4",
            "-select_streams",
            "v:0",
            "-show_frames",
            "-show_entries",
            "frame=pts_time,duration_time",
            "-of",
            "json",
            path,
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        let (status, stdout, _) = self
            .run_owned_probe_output(
                label,
                None,
                args,
                None,
                deadline.min(tokio::time::Instant::now() + StdDuration::from_secs(15)),
                cancellation,
            )
            .await?;
        if !status.success() {
            return Err(FfprobeError::InvalidOutput { path: label.into() });
        }
        parse_jpeg_xl_timing(&stdout)
            .ok_or_else(|| FfprobeError::InvalidOutput { path: label.into() })
    }

    #[allow(clippy::too_many_lines)] // One owner keeps process, readers and producer cleanup ordered.
    async fn run_owned_probe_output(
        &self,
        label: &str,
        stream: Option<&ResolvedRemoteStream>,
        args: Vec<String>,
        producer: Option<crate::remote_stream::RemoteStreamCommand>,
        deadline: tokio::time::Instant,
        mut cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(ExitStatus, Vec<u8>, String)> {
        let label = label.to_owned();
        if !matches!(
            cancellation.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        ) {
            return Err(FfprobeError::Cancelled { path: label });
        }
        if tokio::time::Instant::now() >= deadline {
            return Err(FfprobeError::Timeout { path: label });
        }
        let mut producer = producer
            .as_ref()
            .map(spawn_remote_stream_producer)
            .transpose()
            .map_err(|error| FfprobeError::Spawn {
                path: label.clone(),
                source: std::io::Error::other(error),
            })?;
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
        let mut child = match process.spawn() {
            Ok(child) => child,
            Err(source) => {
                if let Some(producer) = producer.as_mut() {
                    let _ = producer.kill().await;
                }
                return Err(FfprobeError::Spawn {
                    path: label,
                    source,
                });
            }
        };

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
                    stream
                        .and_then(crate::remote_stream::remote_stream_producer)
                        .map_or_else(Vec::new, remote_stream_environment_secrets),
                    stream
                        .and_then(crate::remote_stream::remote_stream_producer)
                        .is_some_and(command_has_file_backed_secret),
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
        let waited = tokio::select! {
            biased;
            _ = &mut cancellation => None,
            result = tokio::time::timeout_at(deadline, child.wait()) => Some(result),
        };
        let status = match waited {
            Some(Ok(Ok(status))) => status,
            Some(Ok(Err(source))) => {
                let _ = child.kill().await;
                let cleanup = shutdown_remote_producer(&mut producer_pipe).await;
                stdout_task.abort();
                stderr_task.abort();
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                cleanup.map_err(|source| FfprobeError::Spawn {
                    path: label.clone(),
                    source,
                })?;
                return Err(FfprobeError::Spawn {
                    path: label,
                    source,
                });
            }
            Some(Err(_)) | None => {
                let _ = child.kill().await;
                let cleanup = shutdown_remote_producer(&mut producer_pipe).await;
                stdout_task.abort();
                stderr_task.abort();
                let _ = stdout_task.await;
                let _ = stderr_task.await;
                cleanup.map_err(|source| FfprobeError::Spawn {
                    path: label.clone(),
                    source,
                })?;
                return Err(if waited.is_none() {
                    FfprobeError::Cancelled { path: label }
                } else {
                    FfprobeError::Timeout { path: label }
                });
            }
        };
        if let Err(source) = shutdown_remote_producer(&mut producer_pipe).await {
            stdout_task.abort();
            stderr_task.abort();
            let _ = stdout_task.await;
            let _ = stderr_task.await;
            return Err(FfprobeError::Spawn {
                path: label,
                source,
            });
        }
        let (stdout, stderr) = Self::collect_remote_ffprobe_output(
            stdout_task,
            stderr_task,
            &label,
            deadline,
            cancellation,
        )
        .await?;
        Ok((status, stdout, stderr))
    }

    async fn collect_remote_ffprobe_output(
        mut stdout_task: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
        mut stderr_task: tokio::task::JoinHandle<String>,
        label: &str,
        deadline: tokio::time::Instant,
        mut cancellation: tokio::sync::oneshot::Receiver<()>,
    ) -> Result<(Vec<u8>, String)> {
        let deadline =
            deadline.min(tokio::time::Instant::now() + REMOTE_FFPROBE_PIPE_SHUTDOWN_TIMEOUT);
        let mut stdout_complete = false;
        let mut stderr_complete = false;
        let collect = async {
            let stdout = (&mut stdout_task).await;
            stdout_complete = true;
            let stdout =
                stdout.map_err(|_| std::io::Error::other("ffprobe stdout reader failed"))??;
            let stderr = (&mut stderr_task).await;
            stderr_complete = true;
            let stderr =
                stderr.map_err(|_| std::io::Error::other("ffprobe stderr reader failed"))?;
            Ok::<_, std::io::Error>((stdout, stderr))
        };
        let readers = tokio::select! {
            biased;
            _ = &mut cancellation => None,
            result = tokio::time::timeout_at(deadline, collect) => Some(result),
        };
        // A completed reader must not be polled twice. Every unfinished reader
        // is aborted and joined on cancellation, timeout or its sibling's error.
        if !stdout_complete {
            stdout_task.abort();
            let _ = stdout_task.await;
        }
        if !stderr_complete {
            stderr_task.abort();
            let _ = stderr_task.await;
        }
        match readers {
            Some(Ok(Ok(output))) => Ok(output),
            Some(Ok(Err(source))) => Err(FfprobeError::Spawn {
                path: label.to_owned(),
                source,
            }),
            Some(Err(_)) => Err(FfprobeError::Timeout {
                path: label.to_owned(),
            }),
            None => Err(FfprobeError::Cancelled {
                path: label.to_owned(),
            }),
        }
    }

    fn remote_probe_command(
        stream: &ResolvedRemoteStream,
        label: &str,
    ) -> Result<(
        Vec<String>,
        Option<crate::remote_stream::RemoteStreamCommand>,
    )> {
        let input = remote_stream_input_file(stream).map_err(|error| FfprobeError::Spawn {
            path: label.to_owned(),
            source: std::io::Error::other(error),
        })?;
        let args = remote_probe_input_args(&input);
        let producer = match &stream.source {
            RemoteStreamSource::Url(_)
            | RemoteStreamSource::Prepared(_)
            | RemoteStreamSource::RetainedIntervals(_) => None,
            RemoteStreamSource::Addon(command) => Some(command.clone()),
        };
        Ok((args, producer))
    }
}

fn remote_probe_input_args(input: &ersatzrs_ffmpeg::InputFile) -> Vec<String> {
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
    args.extend(input.input_options.iter().cloned());
    args.extend(["-i".to_owned(), input.path.clone()]);
    args
}

fn finish_remote_ffprobe(
    status: std::process::ExitStatus,
    label: &str,
    stdout: &[u8],
    stderr: &str,
    stream: Option<&ResolvedRemoteStream>,
) -> Result<FfprobeMediaVersion> {
    if !status.success() {
        return Err(FfprobeError::Process {
            path: label.to_owned(),
            status,
            stderr: stream.map_or_else(
                || "published interval media diagnostics redacted".to_owned(),
                |stream| redact_remote_stream_output(stderr, stream),
            ),
        });
    }
    parse_ffprobe_json(Path::new("remote-stream"), stdout).map_err(|source| {
        if stream.is_none() {
            // Deserialization errors can quote invalid string values from stdout.
            FfprobeError::InvalidOutput {
                path: label.to_owned(),
            }
        } else {
            FfprobeError::Json {
                path: label.to_owned(),
                source,
            }
        }
    })
}

fn parse_jpeg_xl_timing(bytes: &[u8]) -> Option<ersatzrs_ffmpeg::graphics::JpegXlTiming> {
    let document: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let frames = document.get("frames")?.as_array()?;
    let mut previous = None;
    let mut duration = 0.0;
    let mut frame_timestamps = Vec::with_capacity(frames.len());
    for frame in frames {
        let timestamp = frame.get("pts_time")?.as_str()?.parse::<f64>().ok()?;
        let frame_duration = frame.get("duration_time")?.as_str()?.parse::<f64>().ok()?;
        if !timestamp.is_finite()
            || timestamp < 0.0
            || !frame_duration.is_finite()
            || frame_duration <= 0.0
            || previous.is_some_and(|value| timestamp <= value)
            || (previous.is_none() && timestamp.abs() > 0.000_001)
        {
            return None;
        }
        previous = Some(timestamp);
        frame_timestamps.push(timestamp);
        duration = timestamp + frame_duration;
    }
    Some(ersatzrs_ffmpeg::graphics::JpegXlTiming {
        frame_count: frames.len(),
        duration_seconds: duration,
        last_frame_seconds: previous?,
        frame_timestamps,
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

async fn shutdown_remote_producer(
    producer: &mut Option<RemoteStreamProducerPipe>,
) -> std::io::Result<()> {
    if let Some(producer) = producer.as_mut() {
        producer.shutdown().await?;
    }
    producer.take();
    Ok(())
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

    if let Some(duration) = content_duration(probe).or_else(|| format_duration(probe)) {
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
        format_name: probe
            .format
            .as_ref()
            .and_then(|format| format.format_name.clone()),
        field_orders: probe
            .streams
            .iter()
            .filter_map(|stream| Some((stream.index?, stream.field_order.clone()?)))
            .collect(),
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

fn format_duration(probe: &FfprobeOutput) -> Option<Duration> {
    parse_positive_duration(probe.format.as_ref()?.duration.as_deref()?)
}

/// The selected video bounds playback; audio-only items need every audio duration.
fn content_duration(probe: &FfprobeOutput) -> Option<Duration> {
    // ErsatzTV LocalStatisticsProvider.GetContentDuration keeps the first stream
    // on bitrate ties and excludes attached cover art.
    let video = probe
        .streams
        .iter()
        .filter(|stream| {
            stream.codec_type.as_deref() == Some("video")
                && stream.disposition.as_ref().and_then(|d| d.attached_pic) != Some(1)
        })
        .reduce(|selected, candidate| {
            if content_bit_rate(candidate) > content_bit_rate(selected) {
                candidate
            } else {
                selected
            }
        });
    if let Some(video) = video {
        return stream_content_duration(video, probe);
    }

    let mut longest = None;
    for stream in probe
        .streams
        .iter()
        .filter(|stream| stream.codec_type.as_deref() == Some("audio"))
    {
        let duration = stream_content_duration(stream, probe)?;
        longest = Some(longest.map_or(duration, |current: Duration| current.max(duration)));
    }
    longest
}

fn content_bit_rate(stream: &FfprobeStream) -> i64 {
    parse_i64(stream.bit_rate.as_deref())
        .or_else(|| parse_i64(stream.tags.as_ref()?.extra.get("variant_bitrate")?.as_str()))
        .unwrap_or(0)
}

fn stream_content_duration(stream: &FfprobeStream, probe: &FfprobeOutput) -> Option<Duration> {
    let numeric = stream.duration.as_deref().and_then(parse_positive_duration);
    let tagged = stream.tags.as_ref().and_then(duration_tag);
    let matroska = probe
        .format
        .as_ref()
        .and_then(|format| format.format_name.as_deref())
        .is_some_and(|names| {
            names.split(',').any(|name| {
                name.trim().eq_ignore_ascii_case("matroska")
                    || name.trim().eq_ignore_ascii_case("webm")
            })
        });
    if let (Some(numeric), Some(tagged)) = (numeric, tagged) {
        // ADR 0117: FFmpeg 9 can fill a Matroska stream duration from the
        // container, including subtitles past content EOF. Override only that
        // demonstrated conflict, not an independently reported numeric duration.
        if matroska && Some(numeric) == format_duration(probe) && tagged < numeric {
            return Some(tagged);
        }
    }
    numeric.or(tagged)
}

fn duration_tag(tags: &FfprobeTags) -> Option<Duration> {
    let (_, value) = tags
        .extra
        .iter()
        .filter(|(key, _)| {
            key.eq_ignore_ascii_case("DURATION")
                || (key.len() > 9
                    && key
                        .get(..9)
                        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("DURATION-")))
        })
        .min_by_key(|(key, _)| key.len())?;
    let mut parts = value.as_str()?.trim().split(':');
    let hours = parse_unsigned_seconds(parts.next()?)?;
    let minutes = parse_unsigned_seconds(parts.next()?)?;
    let seconds = parse_decimal_duration(parts.next()?)?;
    if parts.next().is_some() || minutes >= 60 || seconds >= Duration::seconds(60) {
        return None;
    }
    let whole = hours
        .checked_mul(3600)?
        .checked_add(minutes.checked_mul(60)?)?;
    let duration = Duration::try_seconds(whole)?.checked_add(&seconds)?;
    (duration > Duration::zero()).then_some(duration)
}

fn parse_unsigned_seconds(value: &str) -> Option<i64> {
    if value.is_empty() || !value.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn parse_decimal_duration(value: &str) -> Option<Duration> {
    let value = value.trim();
    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    let seconds = parse_unsigned_seconds(whole)?;
    let nanos = if fraction.is_empty() {
        if value.contains('.') {
            return None;
        }
        0
    } else {
        if fraction.len() > 9 {
            return None;
        }
        parse_unsigned_seconds(fraction)?
            .checked_mul(10_i64.pow(u32::try_from(9 - fraction.len()).ok()?))?
    };
    Duration::try_seconds(seconds)?.checked_add(&Duration::nanoseconds(nanos))
}

fn parse_positive_duration(value: &str) -> Option<Duration> {
    parse_decimal_duration(value).filter(|duration| *duration > Duration::zero())
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
    format_name: Option<String>,
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
    duration: Option<String>,
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
    #[serde(flatten)]
    extra: BTreeMap<String, serde_json::Value>,
}

#[cfg(test)]
mod retained_probe_tests {
    use super::*;

    #[tokio::test]
    async fn watermark_count_rejects_cancelled_or_expired_startup_before_spawn() {
        let extractor = FfprobeExtractor::new("nonexistent-watermark-probe");
        let (sender, receiver) = tokio::sync::oneshot::channel();
        sender.send(()).unwrap();
        assert!(matches!(
            extractor
                .count_watermark_frames_cancellable(
                    "synthetic-logo.gif",
                    tokio::time::Instant::now() + StdDuration::from_secs(15),
                    receiver,
                )
                .await,
            Err(FfprobeError::Cancelled { .. })
        ));
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        assert!(matches!(
            extractor
                .count_watermark_frames_cancellable(
                    "synthetic-logo.gif",
                    tokio::time::Instant::now(),
                    receiver,
                )
                .await,
            Err(FfprobeError::Timeout { .. })
        ));
    }

    #[tokio::test]
    async fn retained_prefix_rejects_cancelled_expired_and_oversized_before_spawn() {
        let extractor = FfprobeExtractor::new("nonexistent-retained-probe");
        let (sender, receiver) = tokio::sync::oneshot::channel();
        sender.send(()).unwrap();
        assert!(matches!(
            extractor
                .extract_retained_prefix_cancellable(
                    b"synthetic",
                    tokio::time::Instant::now() + StdDuration::from_secs(15),
                    receiver,
                )
                .await,
            Err(FfprobeError::Cancelled { .. })
        ));
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        assert!(matches!(
            extractor
                .extract_retained_prefix_cancellable(
                    b"synthetic",
                    tokio::time::Instant::now(),
                    receiver,
                )
                .await,
            Err(FfprobeError::Timeout { .. })
        ));
        for prefix in [Vec::new(), vec![0; 64 * 1024 + 1]] {
            let (_sender, receiver) = tokio::sync::oneshot::channel();
            assert!(matches!(
                extractor
                    .extract_retained_prefix_cancellable(
                        &prefix,
                        tokio::time::Instant::now() + StdDuration::from_secs(15),
                        receiver,
                    )
                    .await,
                Err(FfprobeError::Spawn { .. })
            ));
        }
    }

    #[tokio::test]
    async fn retained_probe_reader_deadline_keeps_completed_reader_and_aborts_stalled_sibling() {
        let (started, ready) = tokio::sync::oneshot::channel();
        let (dropped, drop_observed) = tokio::sync::oneshot::channel::<()>();
        let stdout = tokio::spawn(async { Ok(vec![1, 2, 3]) });
        let stderr = tokio::spawn(async move {
            let _dropped = dropped;
            let _ = started.send(());
            std::future::pending::<String>().await
        });
        ready.await.unwrap();
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        let result = FfprobeExtractor::collect_remote_ffprobe_output(
            stdout,
            stderr,
            "published interval media",
            tokio::time::Instant::now() + StdDuration::from_millis(20),
            receiver,
        )
        .await;
        assert!(matches!(result, Err(FfprobeError::Timeout { .. })));
        assert!(drop_observed.await.is_err());
    }

    #[tokio::test]
    async fn retained_probe_reader_error_aborts_unfinished_sibling() {
        let (dropped, drop_observed) = tokio::sync::oneshot::channel::<()>();
        let stdout = tokio::spawn(async { Err(std::io::Error::other("bounded output limit")) });
        let stderr = tokio::spawn(async move {
            let _dropped = dropped;
            std::future::pending::<String>().await
        });
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        let result = FfprobeExtractor::collect_remote_ffprobe_output(
            stdout,
            stderr,
            "published interval media",
            tokio::time::Instant::now() + StdDuration::from_secs(1),
            receiver,
        )
        .await;
        assert!(matches!(result, Err(FfprobeError::Spawn { .. })));
        assert!(drop_observed.await.is_err());
    }
}

// Process-permission setup is Unix-specific; the shared producer lifecycle is
// also covered by the cross-platform tests in remote_stream.rs.
#[cfg(all(test, unix))]
mod tests {
    use std::os::unix::fs::PermissionsExt as _;

    use super::*;

    #[tokio::test]
    async fn watermark_count_preserves_decoded_frame_count_and_rejects_invalid_output() {
        let temporary = tempfile::tempdir().unwrap();
        let program = temporary.path().join("watermark-probe.sh");
        for (output, expected) in [
            ("stream,1", Some(1)),
            ("stream,2", Some(2)),
            ("stream,N/A", None),
        ] {
            std::fs::write(&program, format!("#!/bin/sh\nprintf '%s' '{output}'\n")).unwrap();
            std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
            let extractor = FfprobeExtractor::new(program.to_str().unwrap());
            let (_sender, receiver) = tokio::sync::oneshot::channel();
            let result = extractor
                .count_watermark_frames_cancellable(
                    "synthetic-logo.gif",
                    tokio::time::Instant::now() + StdDuration::from_secs(15),
                    receiver,
                )
                .await;
            if let Some(expected) = expected {
                assert_eq!(result.unwrap(), expected);
            } else {
                assert!(matches!(result, Err(FfprobeError::InvalidOutput { .. })));
            }
        }
    }

    #[tokio::test]
    async fn retained_probe_cancellation_reaps_child_and_redacts_errors() {
        let temporary = tempfile::tempdir().unwrap();
        let program = temporary.path().join("probe.sh");
        let pid_path = temporary.path().join("probe.sh.pid");
        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s' $$ > \"$0.pid\"\nwhile :; do :; done\n",
        )
        .unwrap();
        std::fs::set_permissions(&program, std::fs::Permissions::from_mode(0o755)).unwrap();
        let extractor = FfprobeExtractor::new(program.to_str().unwrap());
        let input = b"synthetic NUT prefix";
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let resolution = extractor.extract_retained_prefix_cancellable(
            input,
            tokio::time::Instant::now() + StdDuration::from_secs(15),
            receiver,
        );
        tokio::pin!(resolution);
        let wait_started = async {
            while !pid_path.exists() {
                tokio::time::sleep(StdDuration::from_millis(5)).await;
            }
        };
        tokio::select! {
            result = &mut resolution => panic!("probe ended before cancellation: {result:?}"),
            result = tokio::time::timeout(StdDuration::from_secs(2), wait_started) => result.unwrap(),
        }
        sender.send(()).unwrap();
        let result = tokio::time::timeout(StdDuration::from_secs(2), resolution)
            .await
            .unwrap();
        assert!(matches!(result, Err(FfprobeError::Cancelled { .. })));
        let pid: i32 = std::fs::read_to_string(&pid_path).unwrap().parse().unwrap();
        assert_eq!(
            nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None),
            Err(nix::errno::Errno::ESRCH)
        );

        std::fs::write(
            &program,
            "#!/bin/sh\nprintf '%s' 'synthetic-private-capability' >&2\nexit 1\n",
        )
        .unwrap();
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        let error = extractor
            .extract_retained_prefix_cancellable(
                input,
                tokio::time::Instant::now() + StdDuration::from_secs(1),
                receiver,
            )
            .await
            .unwrap_err();
        assert!(!error.to_string().contains("synthetic-private-capability"));
        assert!(!error.to_string().contains("private-input"));
        assert!(matches!(error, FfprobeError::Process { .. }));
        std::fs::write(&program, "#!/bin/sh\nprintf '%s' '{\"streams\":[{\"index\":\"synthetic-private-capability\"}]}'\n").unwrap();
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        let error = extractor
            .extract_retained_prefix_cancellable(
                input,
                tokio::time::Instant::now() + StdDuration::from_secs(1),
                receiver,
            )
            .await
            .unwrap_err();
        assert!(matches!(error, FfprobeError::InvalidOutput { .. }));
        assert!(!error.to_string().contains("synthetic-private-capability"));

        std::fs::write(&program, "#!/bin/sh\nprintf '%s' '{\"streams\":[{\"index\":0,\"codec_type\":\"video\",\"codec_name\":\"h264\",\"width\":640,\"height\":360}],\"format\":{\"duration\":\"10\"}}'\n").unwrap();
        let (_sender, receiver) = tokio::sync::oneshot::channel();
        let result = extractor
            .extract_retained_prefix_cancellable(
                input,
                tokio::time::Instant::now() + StdDuration::from_secs(1),
                receiver,
            )
            .await
            .unwrap();
        assert_eq!(result.streams[0].codec.as_deref(), Some("h264"));
        assert_eq!(result.version.width, 640);
    }

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
                tool_paths: Vec::new(),
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

#[cfg(test)]
mod jpeg_xl_tests {
    use super::parse_jpeg_xl_timing;
    #[test]
    fn jpeg_xl_duration_uses_decoded_variable_frame_timing() {
        let timing = parse_jpeg_xl_timing(br#"{"frames":[{"pts_time":"0.0","duration_time":"0.04"},{"pts_time":"0.04","duration_time":"0.1"}]}"#).unwrap();
        assert_eq!(timing.frame_count, 2);
        assert!((timing.duration_seconds - 0.14).abs() < f64::EPSILON);
        assert!((timing.last_frame_seconds - 0.04).abs() < f64::EPSILON);
        for source in [br#"{"frames":[]}"#.as_slice(), br#"{"frames":[{"pts_time":"0","duration_time":"0"}]}"#, br#"{"frames":[{"pts_time":"1","duration_time":"1"}]}"#, br#"{"frames":[{"pts_time":"0","duration_time":"1"},{"pts_time":"0","duration_time":"1"}]}"#] {
            assert!(parse_jpeg_xl_timing(source).is_none());
        }
    }
}
