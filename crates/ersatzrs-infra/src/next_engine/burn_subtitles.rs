//! Bounded extraction of one selected embedded text track for Next Burn playback.
//! The caller owns the unique temporary output and its eventual atomic promotion.

use std::{ffi::OsString, path::Path, process::Stdio, time::Duration};

use tokio::{process::Command, time::Instant};

use crate::addons::{spawn_managed_addon_process, ManagedAddonChild};

use super::protocol::Source;

const MAX_OUTPUT_BYTES: u64 = 32 * 1024 * 1024;
const CLEANUP_TIMEOUT: Duration = Duration::from_secs(5);

/// Diagnostics never include media paths, source credentials or `FFmpeg` output.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub(crate) enum BurnError {
    #[error("Next subtitle extraction source is unsupported")]
    Source,
    #[error("Next subtitle extraction could not start")]
    Start,
    #[error("Next subtitle extraction failed")]
    Process,
    #[error("Next subtitle extraction output is unavailable")]
    Output,
    #[error("Next subtitle extraction exceeded its output limit")]
    OutputLimit,
    #[error("Next subtitle extraction deadline expired")]
    Deadline,
    #[error("subtitle extraction was cancelled")]
    Cancelled,
    #[error("Next subtitle extraction process cleanup is unconfirmed")]
    Cleanup,
}

/// Extract the complete selected track without rebasing its media timestamps.
///
/// Source in/out points are deliberately ignored; the returned sidecar remains
/// on the source timeline and the published track retains its original seeks.
/// ASS/SSA uses ASS output to preserve styling; other text decoders produce SRT.
/// The caller must supply an exclusively owned temporary path and must neither
/// publish nor reuse that file unless this operation succeeds. No file is promoted
/// or removed here. Cancellation drops the managed child and kills its process tree.
///
/// # Errors
/// Returns a redacted failure for unsupported input, process/output errors, a
/// 32 MiB output limit, the caller's deadline, or unconfirmed process cleanup.
/// Deadline/limit failures allow up to five seconds to confirm child cleanup.
pub(crate) async fn extract(
    source: &Source,
    stream_index: u32,
    codec: &str,
    ffmpeg: &Path,
    output: &Path,
    deadline: Instant,
) -> Result<(), BurnError> {
    extract_inner(source, stream_index, codec, ffmpeg, output, deadline, None).await
}

/// Signal cancellation and await cleanup before releasing input/output ownership.
pub(crate) async fn extract_cancellable(
    source: &Source,
    stream_index: u32,
    codec: &str,
    ffmpeg: &Path,
    output: &Path,
    deadline: Instant,
    cancellation: tokio::sync::oneshot::Receiver<()>,
) -> Result<(), BurnError> {
    extract_inner(
        source,
        stream_index,
        codec,
        ffmpeg,
        output,
        deadline,
        Some(cancellation),
    )
    .await
}

async fn extract_inner(
    source: &Source,
    stream_index: u32,
    codec: &str,
    ffmpeg: &Path,
    output: &Path,
    deadline: Instant,
    mut cancellation: Option<tokio::sync::oneshot::Receiver<()>>,
) -> Result<(), BurnError> {
    if cancellation.as_mut().is_some_and(|signal| {
        !matches!(
            signal.try_recv(),
            Err(tokio::sync::oneshot::error::TryRecvError::Empty)
        )
    }) {
        return Err(BurnError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(BurnError::Deadline);
    }
    let args = arguments(source, stream_index, codec, output)?;
    let mut command = Command::new(ffmpeg);
    command
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let mut child = spawn_managed_addon_process(command).map_err(|_| BurnError::Start)?;
    let mut monitor = tokio::time::interval(Duration::from_millis(50));
    monitor.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let outcome = loop {
        tokio::select! {
            biased;
            () = cancelled(&mut cancellation) => break Err(BurnError::Cancelled),
            () = tokio::time::sleep_until(deadline) => break Err(BurnError::Deadline),
            status = child.wait() => {
                break match status {
                    Ok(status) if status.success() => output_size(output, true).await,
                    Ok(_) => Err(BurnError::Process),
                    Err(_) => Err(BurnError::Cleanup),
                };
            }
            _ = monitor.tick() => {
                if let Err(error) = output_size(output, false).await {
                    break Err(error);
                }
            }
        }
    };
    if outcome.is_err() {
        cleanup(&mut child).await?;
    }
    outcome
}

async fn cancelled(signal: &mut Option<tokio::sync::oneshot::Receiver<()>>) {
    if let Some(signal) = signal {
        let _ = signal.await;
    } else {
        std::future::pending::<()>().await;
    }
}

async fn cleanup(child: &mut ManagedAddonChild) -> Result<(), BurnError> {
    tokio::time::timeout(CLEANUP_TIMEOUT, child.kill())
        .await
        .map_err(|_| BurnError::Cleanup)?
        .map_err(|_| BurnError::Cleanup)
}

async fn output_size(path: &Path, complete: bool) -> Result<(), BurnError> {
    match tokio::fs::metadata(path).await {
        Ok(metadata) if metadata.len() > MAX_OUTPUT_BYTES => Err(BurnError::OutputLimit),
        Ok(metadata) if !metadata.is_file() || (complete && metadata.len() == 0) => {
            Err(BurnError::Output)
        }
        Ok(_) => Ok(()),
        Err(error) if !complete && error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(BurnError::Output),
    }
}

fn arguments(
    source: &Source,
    stream_index: u32,
    codec: &str,
    output: &Path,
) -> Result<Vec<OsString>, BurnError> {
    let mut args: Vec<OsString> = [
        "-nostdin",
        "-hide_banner",
        "-loglevel",
        "error",
        "-y",
        "-copyts",
    ]
    .into_iter()
    .map(Into::into)
    .collect();
    append_input(&mut args, source)?;
    let format = if codec.eq_ignore_ascii_case("ass") || codec.eq_ignore_ascii_case("ssa") {
        "ass"
    } else {
        "srt"
    };
    args.extend([
        "-map".into(),
        format!("0:{stream_index}").into(),
        "-c:s".into(),
        format.into(),
        "-f".into(),
        format.into(),
        // FFmpeg's own limit supplements monitoring. Crossing the threshold is
        // an error; a truncated file must never be treated as successful output.
        "-fs".into(),
        (MAX_OUTPUT_BYTES + 1).to_string().into(),
        output.as_os_str().to_owned(),
    ]);
    Ok(args)
}

fn append_input(args: &mut Vec<OsString>, source: &Source) -> Result<(), BurnError> {
    match source {
        Source::Local { path, .. } if !path.trim().is_empty() => {
            args.extend([OsString::from("-i"), path.into()]);
        }
        Source::Http {
            uri,
            headers,
            user_agent,
            reconnect,
            reconnect_delay_max,
            timeout_us,
            ..
        } => {
            let parsed = url::Url::parse(uri).map_err(|_| BurnError::Source)?;
            if !matches!(parsed.scheme(), "http" | "https")
                || parsed.host_str().is_none()
                || !parsed.username().is_empty()
                || parsed.password().is_some()
                || headers.iter().any(|header| header.contains(['\r', '\n']))
            {
                return Err(BurnError::Source);
            }
            // Match Next's HttpInputSource transport options. The managed deadline
            // additionally bounds retries and whole-file demuxing, not just sockets.
            if reconnect.unwrap_or(true) {
                args.extend(
                    [
                        "-reconnect",
                        "1",
                        "-reconnect_on_network_error",
                        "1",
                        "-reconnect_streamed",
                        "1",
                    ]
                    .map(Into::into),
                );
                if let Some(delay) = reconnect_delay_max {
                    args.extend(["-reconnect_delay_max".into(), delay.to_string().into()]);
                }
            }
            if let Some(timeout) = timeout_us {
                args.extend(["-timeout".into(), timeout.to_string().into()]);
            }
            if let Some(agent) = user_agent {
                args.extend(["-user_agent".into(), agent.into()]);
            }
            if !headers.is_empty() {
                args.extend([
                    "-headers".into(),
                    format!("{}\r\n", headers.join("\r\n")).into(),
                ]);
            }
            args.extend([
                "-protocol_whitelist".into(),
                "file,http,https,tcp,tls,crypto".into(),
                "-i".into(),
                uri.into(),
            ]);
        }
        _ => return Err(BurnError::Source),
    }
    Ok(())
}

#[cfg(test)]
#[path = "burn_subtitles_tests.rs"]
mod tests;
