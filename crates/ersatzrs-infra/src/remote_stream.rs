use std::collections::VecDeque;
use std::ops::{Deref, DerefMut};
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};

use serde::Deserialize;
use thiserror::Error;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;

use ersatzrs_ffmpeg::{FfmpegCommand, InputFile};

const REMOTE_INPUT_TIMEOUT_MICROSECONDS: &str = "30000000";
const STDERR_TAIL_LIMIT: usize = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RemoteStreamDefinition {
    pub url: Option<String>,
    pub script: Option<String>,
    pub duration: Option<String>,
    pub fallback_query: Option<String>,
    pub is_live: bool,
    pub title: Option<String>,
    pub plot: Option<String>,
    pub year: Option<i32>,
    pub content_rating: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteStreamCommand {
    pub program: String,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoteStreamSource {
    Url(String),
    Script(RemoteStreamCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRemoteStream {
    pub source: RemoteStreamSource,
    pub is_live: bool,
    pub configured_duration: Option<String>,
    pub fallback_query: Option<String>,
    pub url_overrides_script: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RemoteStreamError {
    #[error("remote stream definition is invalid: {0}")]
    Definition(String),
    #[error("remote stream script command is invalid: {0}")]
    Command(String),
    #[error("remote stream producer failed to start: {0}")]
    Spawn(String),
    #[error("remote stream producer did not expose stdout")]
    MissingStdout,
}

/// Consumer process paired with an optional Remote Stream producer.
///
/// Waiting for, killing, or dropping the consumer also cancels the producer.
#[derive(Debug)]
pub struct RemoteStreamProcess {
    child: Child,
    producer_pipe: Option<RemoteStreamProducerPipe>,
}

impl RemoteStreamProcess {
    /// Wait for the consumer and then stop any producer that is still running.
    ///
    /// # Errors
    ///
    /// Returns an error when waiting for the consumer fails.
    pub async fn wait(&mut self) -> std::io::Result<ExitStatus> {
        let result = self.child.wait().await;
        self.shutdown_producer().await;
        result
    }

    /// Kill the consumer and its producer, waiting for both to stop.
    ///
    /// # Errors
    ///
    /// Returns an error when killing or waiting for the consumer fails.
    pub async fn kill(&mut self) -> std::io::Result<()> {
        self.cancel_producer();
        let result = self.child.kill().await;
        self.shutdown_producer().await;
        result
    }

    /// Start killing the consumer and independently signal its producer.
    ///
    /// # Errors
    ///
    /// Returns an error when the consumer kill signal cannot be sent.
    pub fn start_kill(&mut self) -> std::io::Result<()> {
        self.cancel_producer();
        self.child.start_kill()
    }

    fn cancel_producer(&mut self) {
        if let Some(pipe) = self.producer_pipe.as_mut() {
            pipe.cancel();
        }
    }

    async fn shutdown_producer(&mut self) {
        if let Some(mut pipe) = self.producer_pipe.take() {
            pipe.shutdown().await;
        }
    }
}

impl Deref for RemoteStreamProcess {
    type Target = Child;

    fn deref(&self) -> &Self::Target {
        &self.child
    }
}

impl DerefMut for RemoteStreamProcess {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.child
    }
}

#[derive(Debug)]
pub(crate) struct RemoteStreamProducerPipe {
    cancel: Option<oneshot::Sender<()>>,
    task: Option<JoinHandle<()>>,
}

impl RemoteStreamProducerPipe {
    fn cancel(&mut self) {
        if let Some(cancel) = self.cancel.take() {
            let _ = cancel.send(());
        }
    }

    pub(crate) async fn shutdown(&mut self) {
        self.cancel();
        if let Some(task) = self.task.take() {
            let _ = task.await;
        }
    }
}

impl Drop for RemoteStreamProducerPipe {
    fn drop(&mut self) {
        self.cancel();
    }
}

/// Parse and validate one Remote Stream YAML definition.
///
/// # Errors
///
/// Returns an error when the YAML shape or a configured value is invalid.
pub fn parse_remote_stream_definition(
    yaml: &str,
) -> Result<RemoteStreamDefinition, RemoteStreamError> {
    let definition: RemoteStreamDefinition = serde_yaml::from_str(yaml)
        .map_err(|error| RemoteStreamError::Definition(error.to_string()))?;
    validate_definition(&definition)?;
    Ok(definition)
}

/// Resolve the selected source and its definition-relative working directory.
///
/// # Errors
///
/// Returns an error when the definition or script command is invalid.
pub fn resolve_remote_stream(
    definition: &RemoteStreamDefinition,
    definition_path: &Path,
) -> Result<ResolvedRemoteStream, RemoteStreamError> {
    validate_definition(definition)?;
    let url = trimmed(definition.url.as_ref());
    let script = trimmed(definition.script.as_ref());
    let url_overrides_script = url.is_some() && script.is_some();
    let source = if let Some(url) = url {
        RemoteStreamSource::Url(url.to_owned())
    } else {
        let script = script.ok_or_else(|| {
            RemoteStreamError::Definition("either url or script is required".to_owned())
        })?;
        RemoteStreamSource::Script(parse_script_command(
            script,
            definition_path.parent().unwrap_or_else(|| Path::new(".")),
        )?)
    };
    let is_live = definition.is_live
        || matches!(&source, RemoteStreamSource::Url(url)
            if url_scheme(url).is_some_and(|scheme| scheme.eq_ignore_ascii_case("rtsp")));
    Ok(ResolvedRemoteStream {
        source,
        is_live,
        configured_duration: definition.duration.clone(),
        fallback_query: definition.fallback_query.clone(),
        url_overrides_script,
    })
}

/// Split a platform-native script command without invoking a shell.
///
/// # Errors
///
/// Returns an error for an empty command or malformed quoting.
pub fn parse_script_command(
    value: &str,
    working_directory: &Path,
) -> Result<RemoteStreamCommand, RemoteStreamError> {
    let parts = split_command_line(value)?;
    let (program, args) = parts.split_first().ok_or_else(|| {
        RemoteStreamError::Command("the command must include an executable".to_owned())
    })?;
    if program.trim().is_empty() {
        return Err(RemoteStreamError::Command(
            "the executable must not be empty".to_owned(),
        ));
    }
    Ok(RemoteStreamCommand {
        program: program.clone(),
        args: args.to_vec(),
        working_directory: working_directory.to_path_buf(),
    })
}

#[cfg(not(windows))]
fn split_command_line(value: &str) -> Result<Vec<String>, RemoteStreamError> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quote = None;
    let mut escaped = false;
    let mut token_started = false;
    for character in value.chars() {
        if escaped {
            current.push(character);
            escaped = false;
            token_started = true;
            continue;
        }
        match (quote, character) {
            (Some('\''), '\'') | (Some('"'), '"') => quote = None,
            (Some('"') | None, '\\') => escaped = true,
            (Some('\'' | '"'), _) => current.push(character),
            (None, '\'' | '"') => {
                quote = Some(character);
                token_started = true;
            }
            (None, character) if character.is_whitespace() => {
                if token_started {
                    result.push(std::mem::take(&mut current));
                    token_started = false;
                }
            }
            (None, character) => {
                current.push(character);
                token_started = true;
            }
            _ => unreachable!(),
        }
    }
    if quote.is_some() || escaped {
        return Err(RemoteStreamError::Command(
            "the command contains unmatched quotes or an incomplete escape".to_owned(),
        ));
    }
    if token_started {
        result.push(current);
    }
    Ok(result)
}

#[cfg(windows)]
fn split_command_line(value: &str) -> Result<Vec<String>, RemoteStreamError> {
    split_windows_command_line(value)
}

#[cfg(windows)]
fn split_windows_command_line(value: &str) -> Result<Vec<String>, RemoteStreamError> {
    let mut result = Vec::new();
    let mut current = String::new();
    let mut quoted = false;
    let mut backslashes = 0usize;
    let mut token_started = false;
    for character in value.chars() {
        if character == '\\' {
            backslashes += 1;
            token_started = true;
            continue;
        }
        if character == '"' {
            current.extend(std::iter::repeat_n('\\', backslashes / 2));
            if backslashes % 2 == 0 {
                quoted = !quoted;
            } else {
                current.push('"');
            }
            backslashes = 0;
            token_started = true;
            continue;
        }
        current.extend(std::iter::repeat_n('\\', backslashes));
        backslashes = 0;
        if character.is_whitespace() && !quoted {
            if token_started {
                result.push(std::mem::take(&mut current));
                token_started = false;
            }
        } else {
            current.push(character);
            token_started = true;
        }
    }
    current.extend(std::iter::repeat_n('\\', backslashes));
    if quoted {
        return Err(RemoteStreamError::Command(
            "the command contains unmatched quotes".to_owned(),
        ));
    }
    if token_started {
        result.push(current);
    }
    Ok(result)
}

#[must_use]
pub fn remote_url_input_options(url: &str, is_live: bool) -> Vec<String> {
    let mut options = vec![
        "-rw_timeout".to_owned(),
        REMOTE_INPUT_TIMEOUT_MICROSECONDS.to_owned(),
    ];
    if url_scheme(url).is_some_and(|scheme| {
        scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
    }) {
        options.extend([
            "-reconnect".to_owned(),
            "1".to_owned(),
            "-reconnect_streamed".to_owned(),
            "1".to_owned(),
            "-reconnect_on_network_error".to_owned(),
            "1".to_owned(),
            "-reconnect_on_http_error".to_owned(),
            "4xx,5xx".to_owned(),
            "-reconnect_delay_max".to_owned(),
            "10".to_owned(),
        ]);
        if is_live {
            options.extend(["-reconnect_at_eof".to_owned(), "1".to_owned()]);
        }
    }
    options
}

#[must_use]
pub fn remote_stream_input_file(stream: &ResolvedRemoteStream) -> InputFile {
    match &stream.source {
        RemoteStreamSource::Url(url) => {
            let input = InputFile::video(url.clone())
                .with_input_options(remote_url_input_options(url, stream.is_live));
            if stream.is_live {
                input.without_readrate()
            } else {
                input
            }
        }
        RemoteStreamSource::Script(_) => {
            // ErsatzTV forwards producer stdout through its internal endpoint
            // without adding an FFmpeg input format. Keep the direct pipe
            // equally opaque so FFmpeg can probe streamable MP4, WebM, MPEG-TS,
            // and other supported containers from their bytes.
            let input = InputFile::video("pipe:0").with_seek_after_input();
            if stream.is_live {
                input.without_readrate()
            } else {
                input
            }
        }
    }
}

#[must_use]
pub fn remote_stream_producer(stream: &ResolvedRemoteStream) -> Option<&RemoteStreamCommand> {
    match &stream.source {
        RemoteStreamSource::Script(command) => Some(command),
        RemoteStreamSource::Url(_) => None,
    }
}

#[must_use]
pub fn redacted_source_label(source: &RemoteStreamSource) -> String {
    match source {
        RemoteStreamSource::Url(url) => {
            let scheme = url_scheme(url).unwrap_or("unknown");
            let authority = url
                .split_once("://")
                .map(|(_, remainder)| remainder)
                .and_then(|remainder| remainder.split(['/', '?', '#']).next())
                .and_then(|authority| authority.rsplit('@').next())
                .unwrap_or("remote");
            format!("{scheme}://{authority}")
        }
        RemoteStreamSource::Script(command) => Path::new(&command.program)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("script")
            .to_owned(),
    }
}

#[must_use]
pub fn redact_remote_stream_output(value: &str, stream: &ResolvedRemoteStream) -> String {
    match &stream.source {
        RemoteStreamSource::Url(url) => value.replace(url, &redacted_source_label(&stream.source)),
        RemoteStreamSource::Script(_) => value.to_owned(),
    }
}

/// Start a configured Remote Stream script producer.
///
/// # Errors
///
/// Returns an error when the producer process cannot be started.
pub fn spawn_remote_stream_producer(
    command: &RemoteStreamCommand,
    ffmpeg_path: &str,
    ffprobe_path: Option<&str>,
) -> Result<Child, RemoteStreamError> {
    let mut process = platform_script_process(command);
    process
        .current_dir(&command.working_directory)
        .env("FFMPEG_BIN", ffmpeg_path)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if let Some(ffprobe_path) = ffprobe_path {
        process.env("FFPROBE_BIN", ffprobe_path);
    }
    process
        .spawn()
        .map_err(|error| RemoteStreamError::Spawn(error.to_string()))
}

/// Start ffmpeg and directly connect an optional producer to its standard input.
///
/// # Errors
///
/// Returns an error when either process cannot be started or the pipe cannot
/// be established.
pub fn spawn_ffmpeg_with_remote_input(
    command: &FfmpegCommand,
    working_directory: Option<&Path>,
    producer: Option<&RemoteStreamCommand>,
) -> Result<RemoteStreamProcess, RemoteStreamError> {
    let mut producer_child = producer
        .map(|producer| spawn_remote_stream_producer(producer, &command.executable, None))
        .transpose()?;
    let mut process = Command::new(&command.executable);
    process
        .args(&command.arguments)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if producer.is_some() {
        process.stdin(Stdio::piped());
    } else {
        process.stdin(Stdio::null());
    }
    if let Some(working_directory) = working_directory {
        process.current_dir(working_directory);
    }
    for variable in &command.environment {
        process.env(&variable.name, &variable.value);
    }
    let mut consumer = process
        .spawn()
        .map_err(|error| RemoteStreamError::Spawn(error.to_string()))?;

    let producer_pipe = if let Some(mut producer_child) = producer_child.take() {
        let Some(consumer_stdin) = consumer.stdin.take() else {
            let _ = producer_child.start_kill();
            return Err(RemoteStreamError::Spawn(
                "ffmpeg did not expose stdin for the remote stream producer".to_owned(),
            ));
        };
        let label = producer.map_or_else(
            || "script".to_owned(),
            |producer| {
                Path::new(&producer.program)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("script")
                    .to_owned()
            },
        );
        Some(pipe_remote_stream_producer(
            producer_child,
            consumer_stdin,
            label,
        )?)
    } else {
        None
    };
    Ok(RemoteStreamProcess {
        child: consumer,
        producer_pipe,
    })
}

pub(crate) fn pipe_remote_stream_producer(
    mut producer: Child,
    consumer_stdin: ChildStdin,
    label: String,
) -> Result<RemoteStreamProducerPipe, RemoteStreamError> {
    let stdout = producer
        .stdout
        .take()
        .ok_or(RemoteStreamError::MissingStdout)?;
    let stderr_task = producer
        .stderr
        .take()
        .map(|stderr| tokio::spawn(read_stderr_tail(stderr)));
    let (cancel, cancellation) = oneshot::channel();
    let task = tokio::spawn(run_remote_stream_producer(
        producer,
        stdout,
        consumer_stdin,
        stderr_task,
        cancellation,
        label,
    ));
    Ok(RemoteStreamProducerPipe {
        cancel: Some(cancel),
        task: Some(task),
    })
}

async fn run_remote_stream_producer(
    mut producer: Child,
    stdout: ChildStdout,
    consumer_stdin: ChildStdin,
    stderr_task: Option<JoinHandle<String>>,
    mut cancellation: oneshot::Receiver<()>,
    label: String,
) {
    let copy_result = tokio::select! {
        result = pipe_remote_producer_to_stdin(stdout, consumer_stdin) => Some(result),
        _ = &mut cancellation => None,
    };
    let mut cancelled = copy_result.is_none();
    if cancelled || copy_result.as_ref().is_some_and(Result::is_err) {
        let _ = producer.start_kill();
    }

    let status = if cancelled {
        producer.wait().await
    } else {
        let wait_result = tokio::select! {
            result = producer.wait() => Some(result),
            _ = &mut cancellation => None,
        };
        if let Some(result) = wait_result {
            result
        } else {
            cancelled = true;
            let _ = producer.start_kill();
            producer.wait().await
        }
    };
    let stderr = match stderr_task {
        Some(task) => task.await.unwrap_or_default(),
        None => String::new(),
    };
    match status {
        Ok(status) if status.success() && copy_result.is_some_and(|result| result.is_ok()) => {}
        Ok(status) if cancelled => tracing::debug!(
            producer = %label,
            %status,
            stderr_bytes = stderr.len(),
            "remote stream producer cancelled"
        ),
        Ok(status) => tracing::warn!(
            producer = %label,
            %status,
            stderr_bytes = stderr.len(),
            "remote stream producer stopped"
        ),
        Err(error) => tracing::warn!(
            producer = %label,
            %error,
            stderr_bytes = stderr.len(),
            "failed to wait for remote stream producer"
        ),
    }
}

#[cfg(not(windows))]
fn platform_script_process(command: &RemoteStreamCommand) -> Command {
    let mut process = Command::new(&command.program);
    process.args(&command.args);
    process
}

#[cfg(windows)]
fn platform_script_process(command: &RemoteStreamCommand) -> Command {
    use std::os::windows::process::CommandExt as _;

    let extension = Path::new(&command.program)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if extension.eq_ignore_ascii_case("bat") || extension.eq_ignore_ascii_case("cmd") {
        let mut process = Command::new("cmd.exe");
        process
            .as_std_mut()
            .raw_arg(windows_batch_raw_arguments(command));
        process
    } else {
        let mut process = Command::new(&command.program);
        process.args(&command.args);
        process
    }
}

#[cfg(any(windows, test))]
fn windows_batch_raw_arguments(command: &RemoteStreamCommand) -> String {
    format!("/D /S /C \"{}\"", quote_windows_batch_command(command))
}

#[cfg(any(windows, test))]
fn quote_windows_batch_command(command: &RemoteStreamCommand) -> String {
    std::iter::once(&command.program)
        .chain(command.args.iter())
        .map(|value| format!("\"{}\"", value.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Copy producer output into a consumer and close the input at EOF.
///
/// # Errors
///
/// Returns an error when producer stdout is unavailable or copying fails.
pub async fn pipe_remote_producer_to_stdin(
    mut stdout: ChildStdout,
    mut consumer_stdin: tokio::process::ChildStdin,
) -> Result<(), RemoteStreamError> {
    tokio::io::copy(&mut stdout, &mut consumer_stdin)
        .await
        .map_err(|error| RemoteStreamError::Spawn(error.to_string()))?;
    consumer_stdin
        .shutdown()
        .await
        .map_err(|error| RemoteStreamError::Spawn(error.to_string()))
}

pub async fn read_stderr_tail(stderr: tokio::process::ChildStderr) -> String {
    let mut stderr = stderr;
    let mut tail = RollingStderrTail::default();
    let mut chunk = [0_u8; 4096];
    loop {
        match stderr.read(&mut chunk).await {
            Ok(0) | Err(_) => break,
            Ok(read) => tail.push(&chunk[..read]),
        }
    }
    String::from_utf8_lossy(&tail.into_bytes()).into_owned()
}

#[derive(Debug, Default)]
struct RollingStderrTail {
    bytes: VecDeque<u8>,
}

impl RollingStderrTail {
    fn push(&mut self, chunk: &[u8]) {
        if chunk.len() >= STDERR_TAIL_LIMIT {
            self.bytes.clear();
            self.bytes
                .extend(chunk[chunk.len() - STDERR_TAIL_LIMIT..].iter().copied());
            return;
        }
        let overflow = self
            .bytes
            .len()
            .saturating_add(chunk.len())
            .saturating_sub(STDERR_TAIL_LIMIT);
        self.bytes.drain(..overflow);
        self.bytes.extend(chunk.iter().copied());
    }

    fn into_bytes(self) -> Vec<u8> {
        self.bytes.into_iter().collect()
    }
}

fn validate_definition(definition: &RemoteStreamDefinition) -> Result<(), RemoteStreamError> {
    if trimmed(definition.url.as_ref()).is_none() && trimmed(definition.script.as_ref()).is_none() {
        return Err(RemoteStreamError::Definition(
            "either url or script is required".to_owned(),
        ));
    }
    if let Some(duration) = trimmed(definition.duration.as_ref()) {
        validate_duration(duration)?;
    }
    Ok(())
}

fn validate_duration(value: &str) -> Result<(), RemoteStreamError> {
    let mut parts = value.split(':');
    let Some(hours) = parts.next() else {
        return invalid_duration();
    };
    let Some(minutes) = parts.next() else {
        return invalid_duration();
    };
    let Some(seconds) = parts.next() else {
        return invalid_duration();
    };
    if parts.next().is_some() || hours.parse::<u64>().is_err() {
        return invalid_duration();
    }
    let Ok(minutes) = minutes.parse::<u8>() else {
        return invalid_duration();
    };
    let (whole_seconds, fraction) = seconds.split_once('.').unwrap_or((seconds, ""));
    let Ok(seconds) = whole_seconds.parse::<u8>() else {
        return invalid_duration();
    };
    if minutes >= 60
        || seconds >= 60
        || (!fraction.is_empty() && !fraction.chars().all(|character| character.is_ascii_digit()))
    {
        return invalid_duration();
    }
    Ok(())
}

fn invalid_duration() -> Result<(), RemoteStreamError> {
    Err(RemoteStreamError::Definition(
        "duration must use HH:MM:SS[.fraction]".to_owned(),
    ))
}

fn trimmed(value: Option<&String>) -> Option<&str> {
    value
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
}

fn url_scheme(value: &str) -> Option<&str> {
    value
        .split_once(':')
        .map(|(scheme, _)| scheme)
        .filter(|scheme| !scheme.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(not(windows))]
    fn producer_and_consumer() -> (RemoteStreamCommand, FfmpegCommand) {
        (
            RemoteStreamCommand {
                program: "sh".to_owned(),
                args: vec!["-c".to_owned(), "printf remote-bytes".to_owned()],
                working_directory: PathBuf::from("."),
            },
            FfmpegCommand {
                executable: "sh".to_owned(),
                environment: Vec::new(),
                arguments: vec!["-c".to_owned(), "cat".to_owned()],
            },
        )
    }

    #[cfg(not(windows))]
    fn stalled_producer_and_consumer() -> (RemoteStreamCommand, FfmpegCommand) {
        (
            RemoteStreamCommand {
                program: "sh".to_owned(),
                args: vec!["-c".to_owned(), "while :; do :; done".to_owned()],
                working_directory: PathBuf::from("."),
            },
            FfmpegCommand {
                executable: "sh".to_owned(),
                environment: Vec::new(),
                arguments: vec!["-c".to_owned(), "cat".to_owned()],
            },
        )
    }

    #[cfg(windows)]
    fn stalled_producer_and_consumer() -> (RemoteStreamCommand, FfmpegCommand) {
        (
            RemoteStreamCommand {
                program: "cmd.exe".to_owned(),
                args: vec![
                    "/D".to_owned(),
                    "/S".to_owned(),
                    "/C".to_owned(),
                    "for /L %i in (1,0,2) do @rem".to_owned(),
                ],
                working_directory: PathBuf::from("."),
            },
            FfmpegCommand {
                executable: "cmd.exe".to_owned(),
                environment: Vec::new(),
                arguments: vec![
                    "/D".to_owned(),
                    "/S".to_owned(),
                    "/C".to_owned(),
                    "more".to_owned(),
                ],
            },
        )
    }

    #[cfg(windows)]
    fn producer_and_consumer() -> (RemoteStreamCommand, FfmpegCommand) {
        (
            RemoteStreamCommand {
                program: "cmd.exe".to_owned(),
                args: vec![
                    "/D".to_owned(),
                    "/S".to_owned(),
                    "/C".to_owned(),
                    "<nul set /p=remote-bytes".to_owned(),
                ],
                working_directory: PathBuf::from("."),
            },
            FfmpegCommand {
                executable: "cmd.exe".to_owned(),
                environment: Vec::new(),
                arguments: vec![
                    "/D".to_owned(),
                    "/S".to_owned(),
                    "/C".to_owned(),
                    "more".to_owned(),
                ],
            },
        )
    }

    #[test]
    fn parses_complete_definition_and_prefers_url() {
        let definition = parse_remote_stream_definition(
            r#"
url: https://user:secret@example.test/live.m3u8?token=hidden
script: '"tools/stream producer.sh" "https://example.test/watch/1"'
is_live: true
duration: 03:41:00
title: Example
plot: Plot
year: 1994
content_rating: PG
"#,
        )
        .expect("definition");
        let resolved =
            resolve_remote_stream(&definition, Path::new("streams/example.yml")).expect("source");
        assert!(resolved.url_overrides_script);
        assert!(resolved.is_live);
        assert_eq!(
            redacted_source_label(&resolved.source),
            "https://example.test"
        );
    }

    #[test]
    fn parses_quoted_script_without_shell_expansion() {
        let command = parse_script_command(
            r#""tools/Remote Producer.sh" "https://example.test/watch?id=1&x=2""#,
            Path::new("streams"),
        )
        .expect("command");
        assert_eq!(command.program, "tools/Remote Producer.sh");
        assert_eq!(command.args, ["https://example.test/watch?id=1&x=2"]);

        let empty_argument =
            parse_script_command(r#""producer" """#, Path::new("streams")).expect("empty argument");
        assert_eq!(empty_argument.args, [""]);
    }

    #[test]
    fn formats_windows_batch_command_as_raw_cmd_arguments() {
        let command = RemoteStreamCommand {
            program: r"C:\Program Files\ErsatzRS\Remote Producer.bat".to_owned(),
            args: vec![
                "https://example.test/watch?id=1&x=2".to_owned(),
                String::new(),
            ],
            working_directory: PathBuf::from("."),
        };

        assert_eq!(
            windows_batch_raw_arguments(&command),
            "/D /S /C \"\"C:\\Program Files\\ErsatzRS\\Remote Producer.bat\" \
             \"https://example.test/watch?id=1&x=2\" \"\"\""
        );
    }

    #[test]
    fn beeldengeluid_extractors_emit_one_video_audio_pair() {
        let shell = include_str!("../../../assets/extractors/beeldengeluid/beeldengeluid.sh");
        let batch = include_str!("../../../assets/extractors/beeldengeluid/beeldengeluid.bat");

        assert!(shell.contains("-map 0:v:0?"));
        assert!(shell.contains("-map 0:a:0?"));
        assert!(!shell.contains("-map 0:v?"));
        assert!(!shell.contains("-map 0:a?"));
        assert!(batch.contains("-map 0:v:0? -map 0:a:0?"));
        assert!(!batch.contains("-map 0:v? -map 0:a?"));
        assert!(!shell.contains("-vsync"));
        assert!(!batch.contains("-vsync"));
        assert!(shell.contains("-c copy"));
        assert!(batch.contains("-c copy -f mpegts pipe:1"));
    }

    #[cfg(windows)]
    #[tokio::test]
    async fn launches_windows_batch_producer_with_raw_cmd_arguments() {
        let root = tempfile::Builder::new()
            .prefix("remote stream producer ")
            .tempdir()
            .expect("temp directory");
        let batch = root.path().join("emit arguments.bat");
        std::fs::write(&batch, "@echo off\r\n<nul set /p=\"%~1|%~2\"\r\n")
            .expect("write batch producer");
        let command = RemoteStreamCommand {
            program: batch.to_string_lossy().into_owned(),
            args: vec![
                "https://example.test/watch?id=1&x=2".to_owned(),
                String::new(),
            ],
            working_directory: root.path().to_path_buf(),
        };

        let mut child = spawn_remote_stream_producer(&command, "unused-ffmpeg.exe", None)
            .expect("spawn batch producer");
        let mut stdout = child.stdout.take().expect("producer stdout");
        let mut output = String::new();
        stdout
            .read_to_string(&mut output)
            .await
            .expect("read producer output");
        let status = child.wait().await.expect("producer status");

        assert!(status.success());
        assert_eq!(output, "https://example.test/watch?id=1&x=2|");
    }

    #[test]
    fn rejects_missing_source_unknown_fields_and_invalid_duration() {
        assert!(parse_remote_stream_definition("is_live: false").is_err());
        assert!(parse_remote_stream_definition(
            "url: https://example.test\nis_live: false\nextra: x"
        )
        .is_err());
        assert!(parse_remote_stream_definition(
            "url: https://example.test\nis_live: false\nduration: 00:99:00"
        )
        .is_err());
    }

    #[test]
    fn rtsp_is_always_live_and_http_live_reconnects_at_eof() {
        let rtsp = RemoteStreamDefinition {
            url: Some("rtsp://camera.test/live".to_owned()),
            script: None,
            duration: None,
            fallback_query: None,
            is_live: false,
            title: None,
            plot: None,
            year: None,
            content_rating: None,
        };
        assert!(
            resolve_remote_stream(&rtsp, Path::new("camera.yml"))
                .expect("source")
                .is_live
        );
        assert!(remote_url_input_options("https://example.test/live", true)
            .windows(2)
            .any(|pair| pair == ["-reconnect_at_eof", "1"]));
        assert!(!remote_url_input_options("https://example.test/vod", false)
            .windows(2)
            .any(|pair| pair == ["-reconnect_at_eof", "1"]));
    }

    #[test]
    fn script_input_uses_container_autodetection() {
        let stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Script(RemoteStreamCommand {
                program: "producer".to_owned(),
                args: Vec::new(),
                working_directory: PathBuf::from("streams"),
            }),
            is_live: false,
            configured_duration: None,
            fallback_query: None,
            url_overrides_script: false,
        };

        let input = remote_stream_input_file(&stream);

        assert_eq!(input.path, "pipe:0");
        assert!(input.input_options.is_empty());
        assert!(input.seek_after_input);
        assert!(input.allow_readrate);
    }

    #[test]
    fn redacts_credentials_and_query_parameters_from_ffmpeg_output() {
        let stream = ResolvedRemoteStream {
            source: RemoteStreamSource::Url(
                "https://user:secret@example.test/live.m3u8?token=hidden".to_owned(),
            ),
            is_live: true,
            configured_duration: None,
            fallback_query: None,
            url_overrides_script: false,
        };
        let stderr = redact_remote_stream_output(
            "Error opening https://user:secret@example.test/live.m3u8?token=hidden",
            &stream,
        );

        assert_eq!(stderr, "Error opening https://example.test");
        assert!(!stderr.contains("secret"));
        assert!(!stderr.contains("token"));
    }

    #[tokio::test]
    async fn pipes_producer_stdout_directly_to_consumer_stdin() {
        use tokio::io::AsyncReadExt as _;

        let (producer, consumer) = producer_and_consumer();
        let mut process = spawn_ffmpeg_with_remote_input(&consumer, None, Some(&producer))
            .expect("spawn producer and consumer");
        let mut stdout = process.stdout.take().expect("consumer stdout");
        let output_task = tokio::spawn(async move {
            let mut output = Vec::new();
            stdout.read_to_end(&mut output).await.expect("read output");
            output
        });
        let status = process.wait().await.expect("consumer status");
        let output = output_task.await.expect("output task");

        assert!(status.success());
        assert!(
            String::from_utf8_lossy(&output).contains("remote-bytes"),
            "{output:?}"
        );
    }

    #[tokio::test]
    async fn killing_consumer_cancels_stalled_producer_without_stdout() {
        let (producer, consumer) = stalled_producer_and_consumer();
        let mut process = spawn_ffmpeg_with_remote_input(&consumer, None, Some(&producer))
            .expect("spawn stalled producer and consumer");

        tokio::time::timeout(std::time::Duration::from_secs(2), process.kill())
            .await
            .expect("producer cancellation timed out")
            .expect("kill consumer and producer");
    }

    #[test]
    fn stderr_tail_stays_bounded_while_accepting_long_running_output() {
        let mut tail = RollingStderrTail::default();
        for value in 0_u8..32 {
            tail.push(&vec![value; 1024]);
            assert!(tail.bytes.len() <= STDERR_TAIL_LIMIT);
        }

        let bytes = tail.into_bytes();
        assert_eq!(bytes.len(), STDERR_TAIL_LIMIT);
        assert_eq!(bytes.first(), Some(&16));
        assert_eq!(bytes.last(), Some(&31));
    }
}
